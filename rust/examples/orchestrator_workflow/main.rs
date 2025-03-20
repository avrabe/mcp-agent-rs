use std::collections::HashMap;
use std::time::Duration;
use anyhow::{anyhow, Result};
use tracing::{info, warn, error};

use mcp_agent::telemetry::{init_telemetry, TelemetryConfig};
use mcp_agent::workflow::{
    WorkflowEngine, WorkflowState, WorkflowResult, 
    TaskGroup, Task, TaskResult, TaskResultStatus,
    SignalHandler, WorkflowSignal,
};
use mcp_agent::llm::{
    ollama::{OllamaClient, OllamaConfig},
    types::{Message, Role, CompletionRequest, Completion},
};

#[derive(Debug, Clone, PartialEq)]
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
        Self {
            workflow_type,
            state: WorkflowState::new(),
            result: None,
            dependencies,
        }
    }
}

struct OrchestratorWorkflow {
    state: WorkflowState,
    engine: WorkflowEngine,
    llm_client: OllamaClient,
    input_data: String,
    sub_workflows: HashMap<SubWorkflowType, SubWorkflow>,
    final_result: Option<String>,
}

impl OrchestratorWorkflow {
    fn new(engine: WorkflowEngine, llm_client: OllamaClient, input_data: String) -> Self {
        let mut sub_workflows = HashMap::new();
        
        // Define workflow dependencies
        sub_workflows.insert(
            SubWorkflowType::DataPreparation, 
            SubWorkflow::new(SubWorkflowType::DataPreparation, vec![])
        );
        
        sub_workflows.insert(
            SubWorkflowType::Analysis, 
            SubWorkflow::new(SubWorkflowType::Analysis, vec![SubWorkflowType::DataPreparation])
        );
        
        sub_workflows.insert(
            SubWorkflowType::Summarization, 
            SubWorkflow::new(SubWorkflowType::Summarization, vec![SubWorkflowType::Analysis])
        );
        
        let mut state = WorkflowState::new();
        state.set_metadata("total_sub_workflows", sub_workflows.len().to_string());
        state.set_metadata("input_data_size", input_data.len().to_string());
        
        Self {
            state,
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
                        return false;
                    }
                } else {
                    return false;
                }
            }
            true
        } else {
            false
        }
    }
    
    fn create_data_preparation_task(&self) -> Task {
        let input_data = self.input_data.clone();
        
        Task::new("data_preparation", move |_ctx| {
            info!("Starting data preparation task");
            
            // Simulate data preparation work
            std::thread::sleep(Duration::from_millis(500));
            
            // Basic preprocessing - extracting key sections
            let lines: Vec<&str> = input_data.lines().collect();
            let processed_data = if lines.len() > 3 {
                let header = lines[0].trim();
                let intro = lines.iter().take(3).map(|s| s.trim()).collect::<Vec<&str>>().join(" ");
                let remaining = lines.iter().skip(3).map(|s| s.trim()).collect::<Vec<&str>>().join(" ");
                
                format!("Header: {}\nIntro: {}\nBody: {}", header, intro, remaining)
            } else {
                input_data.clone()
            };
            
            info!("Data preparation completed successfully");
            Ok(TaskResult::success(processed_data))
        })
    }
    
    fn create_analysis_task(&self, prepared_data: String) -> Task {
        let llm_client = self.llm_client.clone();
        
        Task::new("data_analysis", move |_ctx| {
            info!("Starting data analysis task");
            
            let prompt = format!(
                "Analyze the following text and identify key themes, entities, and sentiments:\n\n{}",
                prepared_data
            );
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: Role::User,
                    content: prompt,
                }],
                stream: false,
                options: None,
            };
            
            match llm_client.create_completion(&request) {
                Ok(completion) => {
                    let analysis = completion.message.content.clone();
                    info!("Analysis completed successfully");
                    Ok(TaskResult::success(analysis))
                },
                Err(e) => {
                    error!("Analysis failed: {}", e);
                    Err(anyhow!("Failed to analyze data: {}", e))
                }
            }
        })
    }
    
    fn create_summarization_task(&self, analysis: String) -> Task {
        let llm_client = self.llm_client.clone();
        
        Task::new("summarization", move |_ctx| {
            info!("Starting summarization task");
            
            let prompt = format!(
                "Based on the following analysis, provide a concise executive summary:\n\n{}",
                analysis
            );
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: Role::User,
                    content: prompt,
                }],
                stream: false,
                options: None,
            };
            
            match llm_client.create_completion(&request) {
                Ok(completion) => {
                    let summary = completion.message.content.clone();
                    info!("Summarization completed successfully");
                    Ok(TaskResult::success(summary))
                },
                Err(e) => {
                    error!("Summarization failed: {}", e);
                    Err(anyhow!("Failed to generate summary: {}", e))
                }
            }
        })
    }
    
    async fn run(&mut self) -> Result<WorkflowResult> {
        // Initialize workflow
        self.state.set_status("starting");
        
        // Check if LLM is available
        if let Err(e) = self.llm_client.check_availability().await {
            error!("LLM client is not available: {}", e);
            self.state.set_status("failed");
            self.state.set_error(format!("LLM client is not available: {}", e));
            return Ok(WorkflowResult::failed("LLM client is not available"));
        }
        
        self.state.set_status("processing");
        
        // Step 1: Run data preparation workflow (no dependencies)
        let data_preparation_type = SubWorkflowType::DataPreparation;
        if let Some(sub_workflow) = self.sub_workflows.get_mut(&data_preparation_type) {
            sub_workflow.state.set_status("running");
            
            let task = self.create_data_preparation_task();
            let task_group = TaskGroup::new(vec![task]);
            
            match self.engine.execute_task_group(task_group).await {
                Ok(results) => {
                    if let Some(result) = results.first() {
                        if result.status == TaskResultStatus::Success {
                            let prepared_data = result.output.clone();
                            sub_workflow.result = Some(prepared_data.clone());
                            sub_workflow.state.set_status("completed");
                            self.state.set_metadata("data_preparation_status", "completed");
                            
                            // Step 2: Run analysis workflow
                            self.run_analysis(prepared_data).await?;
                        } else {
                            sub_workflow.state.set_status("failed");
                            self.state.set_metadata("data_preparation_status", "failed");
                            return Ok(WorkflowResult::failed("Data preparation failed"));
                        }
                    }
                },
                Err(e) => {
                    error!("Data preparation workflow failed: {}", e);
                    sub_workflow.state.set_status("failed");
                    self.state.set_metadata("data_preparation_status", "failed");
                    return Ok(WorkflowResult::failed(&format!("Data preparation failed: {}", e)));
                }
            }
        }
        
        // Check if the entire workflow is complete
        if let Some(sub_workflow) = self.sub_workflows.get(&SubWorkflowType::Summarization) {
            if sub_workflow.state.status() == "completed" {
                self.final_result = sub_workflow.result.clone();
                self.state.set_status("completed");
                return Ok(WorkflowResult::success(self.final_result.clone().unwrap_or_default()));
            }
        }
        
        // If we got here without completing, something went wrong
        if self.state.status() != "completed" {
            self.state.set_status("failed");
            return Ok(WorkflowResult::failed("Workflow did not complete all stages"));
        }
        
        Ok(WorkflowResult::success(self.final_result.clone().unwrap_or_default()))
    }
    
    async fn run_analysis(&mut self, prepared_data: String) -> Result<()> {
        let analysis_type = SubWorkflowType::Analysis;
        
        if let Some(sub_workflow) = self.sub_workflows.get_mut(&analysis_type) {
            if !self.check_dependencies_completed(&analysis_type) {
                warn!("Dependencies for analysis workflow are not completed");
                return Ok(());
            }
            
            sub_workflow.state.set_status("running");
            
            let task = self.create_analysis_task(prepared_data);
            let task_group = TaskGroup::new(vec![task]);
            
            match self.engine.execute_task_group(task_group).await {
                Ok(results) => {
                    if let Some(result) = results.first() {
                        if result.status == TaskResultStatus::Success {
                            let analysis = result.output.clone();
                            sub_workflow.result = Some(analysis.clone());
                            sub_workflow.state.set_status("completed");
                            self.state.set_metadata("analysis_status", "completed");
                            
                            // Run summarization workflow
                            self.run_summarization(analysis).await?;
                        } else {
                            sub_workflow.state.set_status("failed");
                            self.state.set_metadata("analysis_status", "failed");
                        }
                    }
                },
                Err(e) => {
                    error!("Analysis workflow failed: {}", e);
                    sub_workflow.state.set_status("failed");
                    self.state.set_metadata("analysis_status", "failed");
                }
            }
        }
        
        Ok(())
    }
    
    async fn run_summarization(&mut self, analysis: String) -> Result<()> {
        let summarization_type = SubWorkflowType::Summarization;
        
        if let Some(sub_workflow) = self.sub_workflows.get_mut(&summarization_type) {
            if !self.check_dependencies_completed(&summarization_type) {
                warn!("Dependencies for summarization workflow are not completed");
                return Ok(());
            }
            
            sub_workflow.state.set_status("running");
            
            let task = self.create_summarization_task(analysis);
            let task_group = TaskGroup::new(vec![task]);
            
            match self.engine.execute_task_group(task_group).await {
                Ok(results) => {
                    if let Some(result) = results.first() {
                        if result.status == TaskResultStatus::Success {
                            let summary = result.output.clone();
                            sub_workflow.result = Some(summary);
                            sub_workflow.state.set_status("completed");
                            self.state.set_metadata("summarization_status", "completed");
                        } else {
                            sub_workflow.state.set_status("failed");
                            self.state.set_metadata("summarization_status", "failed");
                        }
                    }
                },
                Err(e) => {
                    error!("Summarization workflow failed: {}", e);
                    sub_workflow.state.set_status("failed");
                    self.state.set_metadata("summarization_status", "failed");
                }
            }
        }
        
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize telemetry
    let telemetry_config = TelemetryConfig::default();
    init_telemetry(telemetry_config);
    
    // Create Ollama client
    let ollama_config = OllamaConfig {
        base_url: "http://localhost:11434".to_string(),
        timeout: Duration::from_secs(60),
    };
    let ollama_client = OllamaClient::new(ollama_config);
    
    // Check if Ollama is available before proceeding
    match ollama_client.check_availability().await {
        Ok(_) => info!("Ollama service is available"),
        Err(e) => {
            error!("Ollama service is not available: {}. Using fallback mode.", e);
            // In a real app, you might want to use a fallback or exit
        }
    }
    
    // Create workflow engine with signal handling
    let signal_handler = SignalHandler::new(vec![WorkflowSignal::Interrupt, WorkflowSignal::Terminate]);
    let workflow_engine = WorkflowEngine::new(signal_handler);
    
    // Sample text about a technological concept
    let sample_text = r#"
    The Model Context Protocol (MCP) is a new standard for AI model interaction that enables more efficient and privacy-preserving interactions between applications and AI models. 
    By providing standardized ways to handle context windows and manage complex multi-turn conversations, MCP reduces token usage and allows for more sophisticated AI applications.
    The protocol includes mechanisms for context compression, efficient memory management, and semantic routing between different models specialized for various tasks.
    Key benefits include reduced latency, lower operational costs, and the ability to build more complex AI ecosystems where models can collaborate to solve problems.
    Recent implementations have shown up to 70% reduction in token usage while maintaining or improving output quality.
    "#;
    
    // Create and run orchestrator workflow
    let mut workflow = OrchestratorWorkflow::new(
        workflow_engine, 
        ollama_client,
        sample_text.to_string()
    );
    
    info!("Starting orchestrator workflow...");
    let result = workflow.run().await?;
    
    if result.is_success() {
        info!("Orchestrator workflow completed successfully!");
        println!("\nWorkflow Result:\n{}", result.output);
        
        // Print the state of each sub-workflow
        println!("\nSub-workflow Status:");
        for (workflow_type, sub_workflow) in &workflow.sub_workflows {
            println!("- {:?}: {}", workflow_type, sub_workflow.state.status());
        }
    } else {
        error!("Orchestrator workflow failed: {}", result.error.unwrap_or_default());
    }
    
    Ok(())
} 