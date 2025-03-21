use anyhow::{Result, anyhow};
use async_trait::async_trait;
use rand::Rng;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex as TokioMutex;
use tracing::{debug, error, info, warn};

use mcp_agent::llm::types::LlmClient;
use mcp_agent::telemetry::{TelemetryConfig, init_telemetry};
use mcp_agent::workflow::{
    AsyncSignalHandler, Workflow, WorkflowEngine, WorkflowResult, WorkflowSignal, WorkflowState,
    execute_workflow, task,
};
use mcp_agent::{Completion, CompletionRequest, LlmConfig};

#[derive(Debug, Clone, Copy, PartialEq)]
enum RetryStrategy {
    /// Fixed interval between retries
    Fixed { interval_ms: u64 },

    /// Exponential backoff with configurable base
    ExponentialBackoff {
        base_ms: u64,
        factor: f64,
        max_ms: Option<u64>,
    },

    /// Linear backoff with configurable increment
    LinearBackoff {
        initial_ms: u64,
        increment_ms: u64,
        max_ms: Option<u64>,
    },

    /// Random jitter within a range
    Jitter { min_ms: u64, max_ms: u64 },
}

impl RetryStrategy {
    fn calculate_delay(&self, attempt: usize) -> Duration {
        match self {
            Self::Fixed { interval_ms } => Duration::from_millis(*interval_ms),

            Self::ExponentialBackoff {
                base_ms,
                factor,
                max_ms,
            } => {
                let delay = *base_ms as f64 * factor.powf(attempt as f64);
                let delay = delay as u64;

                if let Some(max) = max_ms {
                    Duration::from_millis(delay.min(*max))
                } else {
                    Duration::from_millis(delay)
                }
            }

            Self::LinearBackoff {
                initial_ms,
                increment_ms,
                max_ms,
            } => {
                let delay = *initial_ms + (*increment_ms * attempt as u64);

                if let Some(max) = max_ms {
                    Duration::from_millis(delay.min(*max))
                } else {
                    Duration::from_millis(delay)
                }
            }

            Self::Jitter { min_ms, max_ms } => {
                let range = *max_ms - *min_ms;
                let jitter = rand::thread_rng().gen_range(0..=range);
                Duration::from_millis(*min_ms + jitter)
            }
        }
    }
}

/// A mock flaky service that randomly fails
struct FlakyService {
    failure_rate: f64,
    min_delay_ms: u64,
    max_delay_ms: u64,
}

impl FlakyService {
    fn new(failure_rate: f64, min_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            failure_rate,
            min_delay_ms,
            max_delay_ms,
        }
    }

    /// Process a request, with potential for failure
    async fn process_request(&self, request: &str) -> Result<String> {
        // Simulate processing time
        let processing_time = rand::thread_rng().gen_range(self.min_delay_ms..=self.max_delay_ms);
        tokio::time::sleep(Duration::from_millis(processing_time)).await;

        // Randomly fail based on failure rate
        if rand::thread_rng().gen_range(0.0..1.0) < self.failure_rate {
            Err(anyhow!("Service error: Request processing failed"))
        } else {
            // Successful response
            Ok(format!("Processed: {}", request))
        }
    }
}

/// Retry workflow that handles retrying failed operations
struct RetryWorkflow {
    state: WorkflowState,
    engine: WorkflowEngine,
    flaky_service: Arc<FlakyService>,
    requests: Vec<String>,
    retry_strategy: RetryStrategy,
    max_retries: usize,
    results: Arc<TokioMutex<Vec<(String, Option<String>, Option<String>)>>>,
}

impl RetryWorkflow {
    fn new(
        engine: WorkflowEngine,
        flaky_service: Arc<FlakyService>,
        requests: Vec<String>,
        retry_strategy: RetryStrategy,
        max_retries: usize,
    ) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("request_count".to_string(), json!(requests.len()));
        metadata.insert("max_retries".to_string(), json!(max_retries));

        Self {
            state: WorkflowState::new(Some("RetryWorkflow".to_string()), Some(metadata)),
            engine,
            flaky_service,
            requests,
            retry_strategy,
            max_retries,
            results: Arc::new(TokioMutex::new(Vec::new())),
        }
    }

    /// Create a task to process a request with retries
    fn create_retry_task(
        &self,
        request: String,
        request_index: usize,
    ) -> task::WorkflowTask<String> {
        let flaky_service = self.flaky_service.clone();
        let retry_strategy = self.retry_strategy;
        let max_retries = self.max_retries;

        task::task(
            &format!("process_request_{}", request_index),
            move || async move {
                info!("Processing request: {}", request);

                let mut attempt = 0;
                let mut last_error = None;

                loop {
                    if attempt >= max_retries {
                        warn!(
                            "Max retries ({}) reached for request: {}",
                            max_retries, request
                        );
                        break;
                    }

                    debug!("Attempt {} for request: {}", attempt + 1, request);
                    let start_time = Instant::now();

                    match flaky_service.process_request(&request).await {
                        Ok(_response) => {
                            let elapsed = start_time.elapsed();
                            info!(
                                "Request succeeded on attempt {}: {} (took {:?})",
                                attempt + 1,
                                request,
                                elapsed
                            );

                            // Return success with attempt count
                            return Ok(format!(
                                "{{\"success\":true,\"attempts\":{}}}",
                                attempt + 1
                            ));
                        }
                        Err(e) => {
                            let elapsed = start_time.elapsed();
                            error!(
                                "Request failed on attempt {}: {} (took {:?}): {}",
                                attempt + 1,
                                request,
                                elapsed,
                                e
                            );

                            last_error = Some(e.to_string());

                            // Calculate delay before next retry
                            let delay = retry_strategy.calculate_delay(attempt);
                            info!(
                                "Retrying in {:?} (attempt {}/{})",
                                delay,
                                attempt + 1,
                                max_retries
                            );

                            tokio::time::sleep(delay).await;
                            attempt += 1;
                        }
                    }
                }

                let error_msg = last_error.unwrap_or_else(|| "Unknown error".to_string());
                Ok(format!(
                    "{{\"success\":false,\"attempts\":{},\"error\":\"{}\"}}",
                    attempt, error_msg
                ))
            },
        )
    }
}

#[async_trait]
impl Workflow for RetryWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        self.state.set_status("starting");

        // Create a task for each request
        let mut tasks = Vec::new();
        for (i, request) in self.requests.iter().enumerate() {
            let task = self.create_retry_task(request.clone(), i);
            tasks.push(task);
        }

        // Execute all tasks
        self.state.set_status("processing");
        let results = self.engine.execute_task_group(tasks).await?;

        // Process results
        let mut success_count = 0;
        let mut failure_count = 0;
        let mut total_attempts = 0;

        let mut result_data = self.results.lock().await;

        for (i, result) in results.iter().enumerate() {
            if i < self.requests.len() {
                let request = &self.requests[i];

                // Parse the JSON result
                if let Ok(json_result) = serde_json::from_str::<serde_json::Value>(result) {
                    let success = json_result
                        .get("success")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let attempts = json_result
                        .get("attempts")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    let error = json_result
                        .get("error")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    if success {
                        success_count += 1;
                        info!("Request {} succeeded after {} attempts", request, attempts);
                        result_data.push((
                            request.clone(),
                            Some(format!("Success after {} attempts", attempts)),
                            None,
                        ));
                    } else {
                        failure_count += 1;
                        warn!(
                            "Request {} failed after {} attempts: {}",
                            request,
                            attempts,
                            error.clone().unwrap_or_default()
                        );
                        result_data.push((request.clone(), None, error));
                    }

                    total_attempts += attempts as usize;
                }
            }
        }

        // Update workflow state with summary
        self.state
            .set_metadata("success_count", json!(success_count));
        self.state
            .set_metadata("failure_count", json!(failure_count));
        self.state
            .set_metadata("total_attempts", json!(total_attempts));

        // Create workflow result
        let mut result = WorkflowResult::new();

        // Add summary data to result
        result.value = Some(json!({
            "requests": self.requests.len(),
            "successful": success_count,
            "failed": failure_count,
            "total_attempts": total_attempts,
            "results": result_data.iter().map(|(req, success, error)| {
                json!({
                    "request": req,
                    "success": success.is_some(),
                    "result": success,
                    "error": error
                })
            }).collect::<Vec<_>>()
        }));

        // Update the final state of the result
        if failure_count == 0 {
            self.state.set_status("completed");
            Ok(WorkflowResult::success(
                serde_json::to_string(&result.value.unwrap()).unwrap_or_default(),
            ))
        } else {
            self.state.set_status("completed_with_errors");
            let error_msg = "Some requests failed after retries".to_string();
            Ok(WorkflowResult::failed(&error_msg))
        }
    }

    fn state(&self) -> &WorkflowState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut WorkflowState {
        &mut self.state
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
    let telemetry_config = TelemetryConfig {
        service_name: "retry_workflow".to_string(),
        enable_console: true,
        ..TelemetryConfig::default()
    };

    // Handle the Result<(), Error> from init_telemetry without using ?
    if let Err(e) = init_telemetry(telemetry_config) {
        eprintln!("Failed to initialize telemetry: {}", e);
        // Continue anyway
    }

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

    // Create a flaky service for testing
    let flaky_service = Arc::new(FlakyService::new(
        0.7, // 70% failure rate
        100, // 100ms min delay
        500, // 500ms max delay
    ));

    // Create workflow engine with signal handling
    let signal_handler = AsyncSignalHandler::new_with_signals(vec![
        WorkflowSignal::Interrupt,
        WorkflowSignal::Terminate,
    ]);
    let workflow_engine = WorkflowEngine::new(signal_handler);

    // Sample requests to process
    let requests = vec![
        "request_1".to_string(),
        "request_2".to_string(),
        "request_3".to_string(),
        "request_4".to_string(),
        "request_5".to_string(),
    ];

    // Create workflow with exponential backoff retry strategy
    let retry_strategy = RetryStrategy::ExponentialBackoff {
        base_ms: 100,
        factor: 2.0,
        max_ms: Some(3000),
    };

    let retry_workflow = RetryWorkflow::new(
        workflow_engine,
        flaky_service,
        requests,
        retry_strategy,
        5, // max retries
    );

    // Execute workflow
    info!("Starting retry workflow");
    let result = execute_workflow(retry_workflow).await?;

    if result.is_success() {
        info!("Retry workflow completed successfully");
        println!("\nWorkflow Result:\n{}", result.output());

        // Print workflow metrics
        if let Some(duration) = result.duration_ms() {
            println!("Workflow completed in {} ms", duration);
        }
    } else {
        error!(
            "Retry workflow failed: {}",
            result.error().unwrap_or_default()
        );
    }

    Ok(())
}
