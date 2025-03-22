use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

use mcp_agent::llm::types::LlmClient;
use mcp_agent::telemetry::{init_telemetry, TelemetryConfig};
use mcp_agent::workflow::signal::DefaultSignalHandler;
use mcp_agent::workflow::{
    execute_workflow, task, Workflow, WorkflowEngine, WorkflowResult, WorkflowSignal, WorkflowState,
};
use mcp_agent::{Completion, CompletionRequest, LlmConfig, LlmMessage as Message, MessageRole};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum SubWorkflowType {
    DataPreparation,
    Analysis,
    Summarization,
}

struct SubWorkflow {
    workflow_type: SubWorkflowType,
    state: WorkflowState,
    result: Option<String>,
    dependencies: Vec<SubWorkflowType>,
}

impl SubWorkflow {
    fn new(workflow_type: SubWorkflowType, dependencies: Vec<SubWorkflowType>) -> Self {
        let mut metadata = HashMap::new();
        let type_str = format!("{:?}", workflow_type);
        metadata.insert("workflow_type".to_string(), json!(type_str));

        Self {
            workflow_type,
            state: WorkflowState::new(Some(format!("SubWorkflow_{}", type_str)), Some(metadata)),
            result: None,
            dependencies,
        }
    }
}

pub struct OrchestratorWorkflow {
    state: WorkflowState,
    engine: WorkflowEngine,
    llm_client: Arc<MockLlmClient>,
    input_data: String,
    sub_workflows: HashMap<SubWorkflowType, SubWorkflow>,
    final_result: Option<String>,
}

impl OrchestratorWorkflow {
    pub fn new(engine: WorkflowEngine, llm_client: Arc<MockLlmClient>, input_data: String) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("input_size".to_string(), json!(input_data.len()));

        // Create the sub-workflows with their dependencies
        let mut sub_workflows = HashMap::new();

        sub_workflows.insert(
            SubWorkflowType::DataPreparation,
            SubWorkflow::new(SubWorkflowType::DataPreparation, vec![]),
        );

        sub_workflows.insert(
            SubWorkflowType::Analysis,
            SubWorkflow::new(
                SubWorkflowType::Analysis,
                vec![SubWorkflowType::DataPreparation],
            ),
        );

        sub_workflows.insert(
            SubWorkflowType::Summarization,
            SubWorkflow::new(
                SubWorkflowType::Summarization,
                vec![SubWorkflowType::Analysis],
            ),
        );

        Self {
            state: WorkflowState::new(Some("OrchestratorWorkflow".to_string()), Some(metadata)),
            engine,
            llm_client,
            input_data,
            sub_workflows,
            final_result: None,
        }
    }

    fn check_dependencies_completed(&self, workflow_type: &SubWorkflowType) -> bool {
        if let Some(sub_workflow) = self.sub_workflows.get(workflow_type) {
            for dep in &sub_workflow.dependencies {
                if let Some(dep_workflow) = self.sub_workflows.get(dep) {
                    if dep_workflow.state.status() != "completed" {
                        // Dependency not completed
                        return false;
                    }
                } else {
                    // Dependency not found
                    return false;
                }
            }
            // All dependencies completed
            true
        } else {
            // Workflow not found
            false
        }
    }

    fn create_data_preparation_task(&self) -> task::WorkflowTask<String> {
        let input_data = self.input_data.clone();
        let llm_client = self.llm_client.clone();

        task::task("data_preparation", move || async move {
            info!("Preparing data for processing");

            // Basic preprocessing (in a real scenario, this would be more complex)
            let cleaned_data = input_data.trim().to_string();

            // Use LLM to enhance the data with additional context
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: MessageRole::System,
                    content: "You are a data preparation assistant that structures raw text into a format suitable for analysis.".to_string(),
                    metadata: None,
                }],
                max_tokens: Some(1000),
                temperature: Some(0.3),
                top_p: None,
                parameters: HashMap::new(),
            };

            match llm_client.complete(request).await {
                Ok(completion) => {
                    let prepared_data = format!(
                        "Structured data:\n{}\n\nOriginal raw data:\n{}",
                        completion.content, cleaned_data
                    );
                    info!("Data preparation completed successfully");
                    Ok(prepared_data)
                }
                Err(e) => {
                    error!("Data preparation failed: {}", e);
                    Err(anyhow!("Failed to prepare data: {}", e))
                }
            }
        })
    }

    fn create_analysis_task(&self, prepared_data: String) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();

        task::task("data_analysis", move || async move {
            info!("Analyzing prepared data");

            // Use LLM to perform analysis
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: MessageRole::System,
                    content: format!(
                        "You are a data analysis assistant that identifies key insights and patterns. Analyze the following data:\n{}",
                        prepared_data
                    ),
                    metadata: None,
                }],
                max_tokens: Some(1000),
                temperature: Some(0.5),
                top_p: None,
                parameters: HashMap::new(),
            };

            match llm_client.complete(request).await {
                Ok(completion) => {
                    let analysis = format!("Analysis results:\n{}", completion.content);
                    info!("Data analysis completed successfully");
                    Ok(analysis)
                }
                Err(e) => {
                    error!("Data analysis failed: {}", e);
                    Err(anyhow!("Failed to analyze data: {}", e))
                }
            }
        })
    }

    fn create_summarization_task(&self, analysis: String) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();

        task::task("summarization", move || async move {
            info!("Generating summary from analysis");

            // Use LLM to generate a summary
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: MessageRole::System,
                    content: format!(
                        "You are a summarization assistant that creates concise and informative summaries. Summarize the following analysis:\n{}",
                        analysis
                    ),
                    metadata: None,
                }],
                max_tokens: Some(500),
                temperature: Some(0.7),
                top_p: None,
                parameters: HashMap::new(),
            };

            match llm_client.complete(request).await {
                Ok(completion) => {
                    let summary = format!("Summary:\n{}", completion.content);
                    info!("Summarization completed successfully");
                    Ok(summary)
                }
                Err(e) => {
                    error!("Summarization failed: {}", e);
                    Err(anyhow!("Failed to generate summary: {}", e))
                }
            }
        })
    }
}

#[async_trait]
impl Workflow for OrchestratorWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        // Initialize workflow
        self.state.set_status("starting");

        // Check if LLM is available
        if let Err(e) = self.llm_client.is_available().await {
            error!("LLM client is not available: {}", e);
            self.state.set_status("failed");
            self.state
                .set_error(format!("LLM client is not available: {}", e));
            return Ok(WorkflowResult::failed("LLM client is not available"));
        }

        self.state.set_status("processing");

        // Process the workflows in sequence, respecting dependencies
        // Step 1: Run data preparation workflow (no dependencies)
        self.process_data_preparation().await?;

        // Step 2: Check if we can get to a final result
        if let Some(sub_workflow) = self.sub_workflows.get(&SubWorkflowType::Summarization) {
            if sub_workflow.state.status() == "completed" {
                self.final_result = sub_workflow.result.clone();
                self.state.set_status("completed");
                return Ok(WorkflowResult::success(
                    self.final_result.clone().unwrap_or_default(),
                ));
            }
        }

        // If we got here without completing, something went wrong
        if self.state.status() != "completed" {
            self.state.set_status("failed");
            return Ok(WorkflowResult::failed(
                "Workflow did not complete all stages",
            ));
        }

        Ok(WorkflowResult::success(
            self.final_result.clone().unwrap_or_default(),
        ))
    }

    fn state(&self) -> &WorkflowState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut WorkflowState {
        &mut self.state
    }
}

impl OrchestratorWorkflow {
    // New method to process data preparation
    async fn process_data_preparation(&mut self) -> Result<()> {
        let data_preparation_type = SubWorkflowType::DataPreparation;

        // Create task outside the mutable borrow scope
        let task = self.create_data_preparation_task();

        if let Some(sub_workflow) = self.sub_workflows.get_mut(&data_preparation_type) {
            info!("Starting data preparation sub-workflow");
            sub_workflow.state.set_status("running");

            match self.engine.execute_task(task).await {
                Ok(result) => {
                    info!("Data preparation completed successfully");
                    sub_workflow.result = Some(result.clone());
                    sub_workflow.state.set_status("completed");

                    // Process the next workflow outside this mutable borrow scope
                    let _ = sub_workflow; // Explicitly end the mutable borrow
                    self.process_analysis(result).await?;
                }
                Err(e) => {
                    error!("Data preparation failed: {}", e);
                    sub_workflow.state.set_status("failed");
                    sub_workflow
                        .state
                        .set_error(format!("Data preparation failed: {}", e));
                }
            }
        }

        Ok(())
    }

    // New method to process analysis
    async fn process_analysis(&mut self, prepared_data: String) -> Result<()> {
        let analysis_type = SubWorkflowType::Analysis;

        // Check dependencies before obtaining mutable borrow
        let dependencies_met = self.check_dependencies_completed(&analysis_type);
        if !dependencies_met {
            warn!("Cannot run analysis workflow - dependencies not met");
            return Ok(());
        }

        // Create task outside the mutable borrow scope
        let task = self.create_analysis_task(prepared_data);

        if let Some(sub_workflow) = self.sub_workflows.get_mut(&analysis_type) {
            info!("Starting analysis sub-workflow");
            sub_workflow.state.set_status("running");

            match self.engine.execute_task(task).await {
                Ok(result) => {
                    info!("Analysis completed successfully");
                    sub_workflow.result = Some(result.clone());
                    sub_workflow.state.set_status("completed");

                    // Process the next workflow outside this mutable borrow scope
                    let _ = sub_workflow; // Explicitly end the mutable borrow
                    self.process_summarization(result).await?;
                }
                Err(e) => {
                    error!("Analysis failed: {}", e);
                    sub_workflow.state.set_status("failed");
                    sub_workflow
                        .state
                        .set_error(format!("Analysis failed: {}", e));
                }
            }
        }

        Ok(())
    }

    // New method to process summarization
    async fn process_summarization(&mut self, analysis: String) -> Result<()> {
        let summarization_type = SubWorkflowType::Summarization;

        // Check dependencies before obtaining mutable borrow
        let dependencies_met = self.check_dependencies_completed(&summarization_type);
        if !dependencies_met {
            warn!("Cannot run summarization workflow - dependencies not met");
            return Ok(());
        }

        // Create task outside the mutable borrow scope
        let task = self.create_summarization_task(analysis);

        if let Some(sub_workflow) = self.sub_workflows.get_mut(&summarization_type) {
            info!("Starting summarization sub-workflow");
            sub_workflow.state.set_status("running");

            match self.engine.execute_task(task).await {
                Ok(result) => {
                    info!("Summarization completed successfully");
                    sub_workflow.result = Some(result);
                    sub_workflow.state.set_status("completed");
                }
                Err(e) => {
                    error!("Summarization failed: {}", e);
                    sub_workflow.state.set_status("failed");
                    sub_workflow
                        .state
                        .set_error(format!("Summarization failed: {}", e));
                }
            }
        }

        Ok(())
    }
}

/// A mock LLM client for testing
pub struct MockLlmClient {
    config: LlmConfig,
    responses: HashMap<String, String>,
}

impl Default for MockLlmClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockLlmClient {
    pub fn new() -> Self {
        Self {
            config: LlmConfig {
                model: "mock-llama2".to_string(),
                api_url: "http://localhost:11434".to_string(),
                api_key: None,
                max_tokens: Some(500),
                temperature: Some(0.7),
                top_p: Some(0.9),
                parameters: HashMap::new(),
            },
            responses: HashMap::new(),
        }
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn complete(&self, request: CompletionRequest) -> Result<Completion> {
        // Simulate processing time
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Extract the prompt to generate appropriate mock responses
        let prompt = if let Some(user_message) = request
            .messages
            .iter()
            .find(|m| matches!(m.role, MessageRole::User))
        {
            &user_message.content
        } else {
            "Unknown prompt"
        };

        // Generate a mock response
        let response = "This is a mock response from the LLM service.".to_string();

        let prompt_len = prompt.len();
        let response_len = response.len();

        Ok(Completion {
            content: response,
            model: Some(self.config.model.clone()),
            prompt_tokens: Some(prompt_len as u32),
            completion_tokens: Some(response_len as u32),
            total_tokens: Some((prompt_len + response_len) as u32),
            metadata: None,
        })
    }

    async fn is_available(&self) -> Result<bool> {
        // Always available for the mock
        Ok(true)
    }

    fn config(&self) -> &LlmConfig {
        &self.config
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize telemetry
    init_telemetry(TelemetryConfig {
        service_name: "orchestrator_workflow".to_string(),
        enable_console: true,
        ..TelemetryConfig::default()
    })
    .map_err(|e| anyhow!("Failed to initialize telemetry: {}", e))?;

    // Create LLM client
    let llm_client = Arc::new(MockLlmClient::new());

    // Create signal handler and workflow engine
    let signal_handler = DefaultSignalHandler::new_with_signals(vec![
        WorkflowSignal::INTERRUPT,
        WorkflowSignal::TERMINATE,
    ]);

    let engine = WorkflowEngine::new(signal_handler);

    // Sample input data
    let input_data =
        "This is a sample document that needs to be processed through an orchestrated workflow. 
The workflow will prepare the data, analyze it, and create a summary of the findings. 
This demonstrates how to use sub-workflows in a coordinated manner."
            .to_string();

    // Create and run the orchestrator workflow
    let workflow = OrchestratorWorkflow::new(engine, llm_client, input_data);

    info!("Starting orchestrator workflow");
    let result = execute_workflow(workflow).await?;

    if result.is_success() {
        info!("Orchestrator workflow completed successfully");
        println!("\nWorkflow Result:\n{}", result.output());

        // Print workflow metrics
        if let Some(duration) = result.duration_ms() {
            println!("Workflow completed in {} ms", duration);
        }
    } else {
        error!(
            "Orchestrator workflow failed: {}",
            result.error().unwrap_or_default()
        );
    }

    Ok(())
}
