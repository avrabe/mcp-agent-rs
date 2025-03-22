use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rand::Rng;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use mcp_agent::llm::types::LlmClient;
use mcp_agent::telemetry::{init_telemetry, TelemetryConfig};
use mcp_agent::workflow::signal::DefaultSignalHandler;
use mcp_agent::workflow::{task, WorkflowEngine, WorkflowResult, WorkflowSignal, WorkflowState};
use mcp_agent::{Completion, CompletionRequest, LlmConfig};

/// Represents a chunk of data to be processed
#[derive(Debug, Clone)]
struct DataChunk {
    id: String,
    data: Vec<u32>,
    timestamp: DateTime<Utc>,
}

impl DataChunk {
    fn new(id: &str, data: Vec<u32>) -> Self {
        Self {
            id: id.to_string(),
            data,
            timestamp: Utc::now(),
        }
    }
}

/// Worker configuration for processing data chunks
#[derive(Debug, Clone)]
struct WorkerConfig {
    id: String,
    processing_delay_ms: u64,
    error_rate: f32, // 0.0 to 1.0
}

impl WorkerConfig {
    fn new(id: &str, processing_delay_ms: u64, error_rate: f32) -> Self {
        Self {
            id: id.to_string(),
            processing_delay_ms,
            error_rate: error_rate.clamp(0.0, 1.0),
        }
    }
}

/// Result of processing a data chunk
#[derive(Debug, Clone)]
struct ProcessingResult {
    chunk_id: String,
    worker_id: String,
    result: Option<u32>,
    error: Option<String>,
    processing_time_ms: u64,
}

/// Aggregation operation to apply to worker results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AggregationOperation {
    Sum,
    Average,
    Minimum,
    Maximum,
    Count,
    Median,
}

impl AggregationOperation {
    fn apply(&self, values: &[u32]) -> Result<f64> {
        if values.is_empty() {
            return Err(anyhow!("Cannot aggregate empty values"));
        }

        match self {
            Self::Sum => Ok(values.iter().map(|&x| x as f64).sum()),
            Self::Average => {
                Ok(values.iter().map(|&x| x as f64).sum::<f64>() / values.len() as f64)
            }
            Self::Minimum => Ok(*values.iter().min().unwrap() as f64),
            Self::Maximum => Ok(*values.iter().max().unwrap() as f64),
            Self::Count => Ok(values.len() as f64),
            Self::Median => {
                let mut sorted = values.to_vec();
                sorted.sort();

                if values.len() % 2 == 0 {
                    // Even number of elements, take average of middle two
                    let mid = values.len() / 2;
                    Ok((sorted[mid - 1] as f64 + sorted[mid] as f64) / 2.0)
                } else {
                    // Odd number of elements, take middle one
                    Ok(sorted[values.len() / 2] as f64)
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Sum => "Sum",
            Self::Average => "Average",
            Self::Minimum => "Minimum",
            Self::Maximum => "Maximum",
            Self::Count => "Count",
            Self::Median => "Median",
        }
    }
}

/// Task result status
#[derive(Debug, Clone, PartialEq)]
enum TaskResultStatus {
    Success,
    Failure,
}

/// A fan-out/fan-in workflow for parallel data processing
struct FanOutFanInWorkflow {
    /// State of the workflow
    state: WorkflowState,
    /// Engine to execute workflow tasks
    engine: WorkflowEngine,
    /// LLM client
    _llm_client: Arc<MockLlmClient>,
    /// Data chunks to process
    data_chunks: Vec<DataChunk>,
    /// Worker configurations
    workers: Vec<WorkerConfig>,
    /// Aggregation operations
    aggregation_operations: Vec<AggregationOperation>,
    /// Processing results
    processing_results: Arc<Mutex<Vec<ProcessingResult>>>,
    /// Maximum number of retries
    max_retries: usize,
}

impl FanOutFanInWorkflow {
    fn new(
        engine: WorkflowEngine,
        llm_client: Arc<MockLlmClient>,
        data_chunks: Vec<DataChunk>,
        workers: Vec<WorkerConfig>,
        aggregation_operations: Vec<AggregationOperation>,
        max_retries: usize,
    ) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("chunk_count".to_string(), json!(data_chunks.len()));
        metadata.insert("worker_count".to_string(), json!(workers.len()));

        Self {
            state: WorkflowState::new(Some("FanOutFanInWorkflow".to_string()), Some(metadata)),
            engine,
            _llm_client: llm_client,
            data_chunks,
            workers,
            aggregation_operations,
            processing_results: Arc::new(Mutex::new(Vec::new())),
            max_retries,
        }
    }

    /// Create a task for a worker to process a data chunk
    fn create_processing_task(
        &self,
        worker: &WorkerConfig,
        chunk: &DataChunk,
    ) -> task::WorkflowTask<String> {
        let worker_clone = worker.clone();
        let chunk_clone = chunk.clone();

        task::task(
            &format!("process_chunk_{}_by_{}", chunk.id, worker.id),
            move || async move {
                info!(
                    "Worker {} processing chunk {}",
                    worker_clone.id, chunk_clone.id
                );

                let start_time = std::time::Instant::now();

                // Simulate processing time
                tokio::time::sleep(Duration::from_millis(worker_clone.processing_delay_ms)).await;

                // Simulate random failures based on error rate
                let mut rng = rand::thread_rng();
                if rng.gen_range(0.0..1.0) < worker_clone.error_rate {
                    error!(
                        "Worker {} failed to process chunk {}",
                        worker_clone.id, chunk_clone.id
                    );
                    return Err(anyhow!(
                        "Worker {} failed to process chunk {}",
                        worker_clone.id,
                        chunk_clone.id
                    ));
                }

                // Calculate sum of values (simple processing task)
                let sum: u32 = chunk_clone.data.iter().sum();

                let processing_time = start_time.elapsed().as_millis() as u64;
                info!(
                    "Worker {} completed processing chunk {} in {}ms",
                    worker_clone.id, chunk_clone.id, processing_time
                );

                // Return result with metadata
                Ok(format!("{}", sum))
            },
        )
    }

    /// Distribute data chunks among workers (fan-out phase)
    async fn fan_out_phase(&mut self) -> Result<()> {
        info!("Starting fan-out phase");
        self.state
            .set_metadata("current_phase", "fan_out".to_string());

        // Track retry attempts for each chunk
        let mut retry_counts: HashMap<String, usize> = HashMap::new();

        // Continue until all chunks are processed or max retries reached
        let mut chunks_to_process: Vec<DataChunk> = self.data_chunks.clone();

        while !chunks_to_process.is_empty() {
            info!("Processing batch of {} chunks", chunks_to_process.len());

            // Create a task for each chunk-worker pair
            let mut tasks = Vec::new();
            for chunk in &chunks_to_process {
                // For simplicity, we'll use a single worker per chunk
                // In a real-world scenario, you might have more complex assignment logic
                let worker_index = rand::thread_rng().gen_range(0..self.workers.len());
                let worker = &self.workers[worker_index];

                let task = self.create_processing_task(worker, chunk);
                tasks.push(task);
            }

            // Execute tasks in parallel
            match self.engine.execute_task_group(tasks).await {
                Ok(results) => {
                    let mut processing_results = self.processing_results.lock().await;
                    let mut successful_chunks = Vec::new();
                    let mut failed_chunks = Vec::new();

                    for (i, result_string) in results.iter().enumerate() {
                        if i < chunks_to_process.len() {
                            let chunk = &chunks_to_process[i];

                            // Basic success/failure parsing - assume success if we can parse the number
                            let parse_result = result_string.parse::<u32>();

                            let worker_id = "worker".to_string(); // Default worker id
                            let processing_time = 0; // Default processing time

                            if let Ok(numeric_result) = parse_result {
                                info!("Successfully processed chunk {}", chunk.id);

                                processing_results.push(ProcessingResult {
                                    chunk_id: chunk.id.clone(),
                                    worker_id,
                                    result: Some(numeric_result),
                                    error: None,
                                    processing_time_ms: processing_time,
                                });

                                successful_chunks.push(chunk.id.clone());
                            } else {
                                error!(
                                    "Failed to process chunk {}: Unable to parse result",
                                    chunk.id
                                );

                                // Increment retry count
                                let retry_count = retry_counts.entry(chunk.id.clone()).or_insert(0);
                                *retry_count += 1;

                                if *retry_count >= self.max_retries {
                                    warn!("Max retries reached for chunk {}, giving up", chunk.id);

                                    processing_results.push(ProcessingResult {
                                        chunk_id: chunk.id.clone(),
                                        worker_id,
                                        result: None,
                                        error: Some("Failed to parse result".to_string()),
                                        processing_time_ms: processing_time,
                                    });

                                    successful_chunks.push(chunk.id.clone()); // Mark as "done" even though it failed
                                } else {
                                    failed_chunks.push(chunk.id.clone());
                                }
                            }
                        }
                    }

                    // Remove successfully processed chunks
                    chunks_to_process.retain(|chunk| !successful_chunks.contains(&chunk.id));

                    // Update workflow state
                    self.state.set_metadata(
                        "chunks_processed",
                        (self.data_chunks.len() - chunks_to_process.len()).to_string(),
                    );
                    self.state
                        .set_metadata("chunks_remaining", chunks_to_process.len().to_string());

                    if chunks_to_process.is_empty() {
                        info!("All chunks processed successfully");
                    } else {
                        info!("{} chunks remaining to process", chunks_to_process.len());
                    }
                }
                Err(e) => {
                    error!("Error executing task group: {}", e);
                    return Err(e);
                }
            }

            // Brief pause between retries
            if !chunks_to_process.is_empty() {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }

        info!("Fan-out phase completed");
        Ok(())
    }

    /// Aggregate results from all workers (fan-in phase)
    async fn fan_in_phase(&mut self) -> Result<HashMap<String, f64>> {
        info!("Starting fan-in phase");
        self.state
            .set_metadata("current_phase", "fan_in".to_string());

        let processing_results = self.processing_results.lock().await;

        // Extract successful numeric results
        let successful_results: Vec<&ProcessingResult> = processing_results
            .iter()
            .filter(|r| r.result.is_some())
            .collect();

        if successful_results.is_empty() {
            return Err(anyhow!("No successful results to aggregate"));
        }

        let values: Vec<u32> = successful_results.iter().filter_map(|r| r.result).collect();

        // Apply each aggregation operation
        let mut aggregation_results = HashMap::new();

        for op in &self.aggregation_operations {
            match op.apply(&values) {
                Ok(result) => {
                    info!("{} aggregation: {}", op.name(), result);
                    aggregation_results.insert(op.name().to_string(), result);
                }
                Err(e) => {
                    error!("Failed to apply {} aggregation: {}", op.name(), e);
                }
            }
        }

        info!(
            "Fan-in phase completed with {} aggregation results",
            aggregation_results.len()
        );
        Ok(aggregation_results)
    }

    /// Generate a summary report of the workflow execution
    fn generate_summary_report(
        &self,
        processing_results: &[ProcessingResult],
        aggregation_results: &HashMap<String, f64>,
    ) -> String {
        let mut summary = String::new();

        summary.push_str("# Fan-Out Fan-In Workflow Processing Summary\n\n");

        // Overall statistics
        let total_chunks = self.data_chunks.len();
        let successful_chunks = processing_results
            .iter()
            .filter(|r| r.result.is_some())
            .count();
        let failed_chunks = total_chunks - successful_chunks;

        summary.push_str("## Overall Statistics\n\n");
        summary.push_str(&format!("- Total Data Chunks: {}\n", total_chunks));
        summary.push_str(&format!(
            "- Successfully Processed: {} ({:.1}%)\n",
            successful_chunks,
            if total_chunks > 0 {
                (successful_chunks as f64 / total_chunks as f64) * 100.0
            } else {
                0.0
            }
        ));
        summary.push_str(&format!(
            "- Failed to Process: {} ({:.1}%)\n",
            failed_chunks,
            if total_chunks > 0 {
                (failed_chunks as f64 / total_chunks as f64) * 100.0
            } else {
                0.0
            }
        ));

        // Worker statistics
        summary.push_str("\n## Worker Statistics\n\n");

        let mut worker_stats: HashMap<String, (usize, usize, u64)> = HashMap::new(); // (total, success, time)

        for result in processing_results {
            let entry = worker_stats
                .entry(result.worker_id.clone())
                .or_insert((0, 0, 0));

            entry.0 += 1; // total
            if result.result.is_some() {
                entry.1 += 1; // success
            }
            entry.2 += result.processing_time_ms; // time
        }

        for (worker_id, (total, success, time)) in worker_stats {
            summary.push_str(&format!("### Worker: {}\n", worker_id));
            summary.push_str(&format!("- Chunks Processed: {}\n", total));
            summary.push_str(&format!(
                "- Successful: {} ({:.1}%)\n",
                success,
                if total > 0 {
                    (success as f64 / total as f64) * 100.0
                } else {
                    0.0
                }
            ));
            summary.push_str(&format!(
                "- Average Processing Time: {:.2} ms\n",
                if total > 0 {
                    time as f64 / total as f64
                } else {
                    0.0
                }
            ));
            summary.push('\n');
        }

        // Aggregation results
        summary.push_str("## Aggregation Results\n\n");

        for (op_name, result) in aggregation_results {
            summary.push_str(&format!("- {}: {:.4}\n", op_name, result));
        }

        // Individual chunk results
        summary.push_str("\n## Individual Chunk Results\n\n");

        for result in processing_results {
            let status = if result.result.is_some() {
                "✅ SUCCESS"
            } else {
                "❌ FAILED"
            };
            summary.push_str(&format!("### Chunk: {} ({})\n", result.chunk_id, status));
            summary.push_str(&format!("- Worker: {}\n", result.worker_id));
            summary.push_str(&format!(
                "- Processing Time: {} ms\n",
                result.processing_time_ms
            ));

            if let Some(value) = result.result {
                summary.push_str(&format!("- Result: {}\n", value));
            }

            if let Some(error) = &result.error {
                summary.push_str(&format!("- Error: {}\n", error));
            }

            summary.push('\n');
        }

        summary
    }

    /// Generate sample data chunks for demonstration
    fn generate_sample_data(chunk_count: usize, values_per_chunk: usize) -> Vec<DataChunk> {
        let mut chunks = Vec::with_capacity(chunk_count);
        let mut rng = rand::thread_rng();

        for i in 0..chunk_count {
            let mut data = Vec::with_capacity(values_per_chunk);

            for _ in 0..values_per_chunk {
                data.push(rng.gen_range(1..100));
            }

            chunks.push(DataChunk::new(&format!("chunk_{}", i + 1), data));
        }

        chunks
    }

    /// Generate sample workers with varying performance characteristics
    fn generate_sample_workers(worker_count: usize) -> Vec<WorkerConfig> {
        let mut workers = Vec::with_capacity(worker_count);
        let mut rng = rand::thread_rng();

        for i in 0..worker_count {
            // Create workers with varying speeds and reliability
            let processing_delay = match i % 3 {
                0 => rng.gen_range(50..100),  // Fast workers
                1 => rng.gen_range(100..200), // Medium workers
                _ => rng.gen_range(200..400), // Slow workers
            };

            let error_rate = match i % 4 {
                0 => 0.0,                      // Reliable workers
                1 => rng.gen_range(0.05..0.1), // Mostly reliable workers
                2 => rng.gen_range(0.1..0.2),  // Somewhat unreliable workers
                _ => rng.gen_range(0.2..0.3),  // Unreliable workers
            };

            workers.push(WorkerConfig::new(
                &format!("worker_{}", i + 1),
                processing_delay,
                error_rate,
            ));
        }

        workers
    }

    /// Run the fan-out/fan-in workflow
    async fn run(&mut self) -> Result<WorkflowResult> {
        info!("Starting Fan-out/Fan-in workflow");

        // Update workflow state to processing
        self.state.set_status("processing");

        // Execute the fan-out phase
        let fan_out_result = self.fan_out_phase().await;
        if let Err(e) = fan_out_result {
            error!("Fan-out phase failed: {}", e);
            self.state.set_status("failed");
            self.state.set_error(format!("Fan-out phase failed: {}", e));
            return Ok(WorkflowResult::failed(e.to_string()));
        }

        // Execute the fan-in phase
        let aggregation_result = match self.fan_in_phase().await {
            Ok(results) => results,
            Err(e) => {
                error!("Fan-in phase failed: {}", e);
                self.state.set_status("failed");
                self.state.set_error(format!("Fan-in phase failed: {}", e));
                return Ok(WorkflowResult::failed(e.to_string()));
            }
        };

        // Format the final output
        let mut output = String::new();
        output.push_str("Fan-out/Fan-in Workflow Results:\n\n");

        for (operation, result) in &aggregation_result {
            output.push_str(&format!("{}: {}\n", operation, result));
        }

        // Mark workflow as completed
        self.state.set_status("completed");

        // Return success result with the output
        Ok(WorkflowResult::success(output))
    }
}

/// A mock LLM client for testing
struct MockLlmClient {
    config: LlmConfig,
}

impl MockLlmClient {
    fn new(config: LlmConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn complete(&self, _request: CompletionRequest) -> Result<Completion> {
        // Simulate a response
        Ok(Completion {
            content: "This is a mock response.".to_string(),
            model: Some(self.config.model.clone()),
            prompt_tokens: Some(10),
            completion_tokens: Some(10),
            total_tokens: Some(20),
            metadata: None,
        })
    }

    async fn is_available(&self) -> Result<bool> {
        Ok(true)
    }

    fn config(&self) -> &LlmConfig {
        &self.config
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize telemetry
    let telemetry_config = TelemetryConfig::default();
    let _ = init_telemetry(telemetry_config);

    // Create LLM client
    let llm_config = LlmConfig {
        model: "llama2".to_string(),
        api_url: "http://localhost:11434".to_string(),
        api_key: None,
        max_tokens: Some(1000),
        temperature: Some(0.7),
        top_p: Some(0.9),
        parameters: HashMap::new(),
    };

    let llm_client = Arc::new(MockLlmClient::new(llm_config));

    // Create workflow engine with signal handling
    let signal_handler = DefaultSignalHandler::new_with_signals(vec![
        WorkflowSignal::INTERRUPT,
        WorkflowSignal::TERMINATE,
    ]);
    let workflow_engine = WorkflowEngine::new(signal_handler);

    // Generate sample data and workers
    let data_chunks = FanOutFanInWorkflow::generate_sample_data(20, 100);
    let workers = FanOutFanInWorkflow::generate_sample_workers(8);

    // Define aggregation operations
    let aggregation_operations = vec![
        AggregationOperation::Sum,
        AggregationOperation::Average,
        AggregationOperation::Minimum,
        AggregationOperation::Maximum,
        AggregationOperation::Count,
        AggregationOperation::Median,
    ];

    // Create and run fan-out/fan-in workflow
    let mut workflow = FanOutFanInWorkflow::new(
        workflow_engine,
        llm_client,
        data_chunks,
        workers,
        aggregation_operations,
        3, // Max retries for failed chunks
    );

    info!("Starting fan-out/fan-in workflow...");
    let result = workflow.run().await?;

    if result.is_success() {
        info!("Fan-out/fan-in workflow completed successfully");
        println!("\n{}\n", result.output());
    } else {
        error!(
            "Fan-out/fan-in workflow failed: {}",
            result.error().unwrap_or_default()
        );
    }

    Ok(())
}
