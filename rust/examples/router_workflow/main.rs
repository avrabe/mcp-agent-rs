use std::collections::HashMap;
use std::time::Duration;
use anyhow::{anyhow, Result};
use tracing::{info, warn, error, debug};

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum TaskType {
    CodeGeneration,
    TextSummarization,
    DataAnalysis,
    ImageDescription,
    Unknown,
}

impl TaskType {
    fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            s if s.contains("code") || s.contains("programming") => Self::CodeGeneration,
            s if s.contains("summarize") || s.contains("summary") => Self::TextSummarization,
            s if s.contains("analyze") || s.contains("analysis") || s.contains("data") => Self::DataAnalysis,
            s if s.contains("image") || s.contains("picture") || s.contains("photo") => Self::ImageDescription,
            _ => Self::Unknown,
        }
    }
    
    fn get_model(&self) -> &'static str {
        match self {
            Self::CodeGeneration => "codellama",
            Self::TextSummarization => "llama2",
            Self::DataAnalysis => "llama2",
            Self::ImageDescription => "llava",
            Self::Unknown => "llama2",
        }
    }
    
    fn get_system_prompt(&self) -> &'static str {
        match self {
            Self::CodeGeneration => "You are an expert programming assistant. Your task is to generate clean, efficient, and well-documented code based on the user's requirements.",
            Self::TextSummarization => "You are a summarization expert. Your task is to create concise summaries that capture the key points of the provided text.",
            Self::DataAnalysis => "You are a data analysis assistant. Your task is to analyze the provided information and extract meaningful insights.",
            Self::ImageDescription => "You are an image description assistant. Your task is to provide detailed descriptions of the images presented to you.",
            Self::Unknown => "You are a helpful assistant. Please respond to the user's query to the best of your ability.",
        }
    }
}

struct RouterWorkflow {
    state: WorkflowState,
    engine: WorkflowEngine,
    llm_client: OllamaClient,
    user_queries: Vec<String>,
    results: HashMap<usize, String>,
}

impl RouterWorkflow {
    fn new(engine: WorkflowEngine, llm_client: OllamaClient, user_queries: Vec<String>) -> Self {
        let mut state = WorkflowState::new();
        state.set_metadata("total_queries", user_queries.len().to_string());
        
        Self {
            state,
            engine,
            llm_client,
            user_queries,
            results: HashMap::new(),
        }
    }
    
    fn create_classification_task(&self, query: String, index: usize) -> Task {
        Task::new(&format!("classify_query_{}", index), move |_ctx| {
            info!("Classifying query: {}", query);
            let task_type = TaskType::from_str(&query);
            Ok(TaskResult::success(format!("{:?}", task_type)))
        })
    }
    
    fn create_execution_task(&self, query: String, task_type: TaskType, index: usize) -> Task {
        let llm_client = self.llm_client.clone();
        
        Task::new(&format!("execute_query_{}", index), move |_ctx| {
            info!("Executing query for task type: {:?}", task_type);
            
            let model = task_type.get_model();
            let system_prompt = task_type.get_system_prompt();
            
            let request = CompletionRequest {
                model: model.to_string(),
                messages: vec![
                    Message {
                        role: Role::System,
                        content: system_prompt.to_string(),
                    },
                    Message {
                        role: Role::User,
                        content: query.clone(),
                    },
                ],
                stream: false,
                options: None,
            };
            
            match llm_client.create_completion(&request) {
                Ok(completion) => {
                    let response = completion.message.content.clone();
                    info!("Successfully processed query with task type: {:?}", task_type);
                    Ok(TaskResult::success(response))
                },
                Err(e) => {
                    error!("Failed to process query: {}", e);
                    // Fallback to a generic response when the model fails
                    let fallback_msg = format!("Sorry, I couldn't process your request for {:?}. Error: {}", task_type, e);
                    Ok(TaskResult::success(fallback_msg))
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
        
        // Step 1: Classify all queries
        let mut classification_tasks = Vec::new();
        
        for (index, query) in self.user_queries.iter().enumerate() {
            let task = self.create_classification_task(query.clone(), index);
            classification_tasks.push(task);
        }
        
        let classification_group = TaskGroup::new(classification_tasks);
        let classification_results = self.engine.execute_task_group(classification_group).await?;
        
        // Step 2: Process each query based on its classification
        let mut execution_tasks = Vec::new();
        let mut task_types = Vec::new();
        
        for (index, result) in classification_results.iter().enumerate() {
            if result.status == TaskResultStatus::Success {
                let task_type_str = result.output.clone();
                let task_type = match task_type_str.as_str() {
                    "CodeGeneration" => TaskType::CodeGeneration,
                    "TextSummarization" => TaskType::TextSummarization,
                    "DataAnalysis" => TaskType::DataAnalysis,
                    "ImageDescription" => TaskType::ImageDescription,
                    _ => TaskType::Unknown,
                };
                
                task_types.push(task_type.clone());
                
                let query = self.user_queries[index].clone();
                let task = self.create_execution_task(query, task_type, index);
                execution_tasks.push(task);
                
                debug!("Query {} classified as {:?}", index, task_type);
            } else {
                warn!("Failed to classify query {}: {}", index, result.error.clone().unwrap_or_default());
                task_types.push(TaskType::Unknown);
            }
        }
        
        self.state.set_metadata("task_types", format!("{:?}", task_types));
        
        // Execute all tasks in parallel
        let execution_group = TaskGroup::new(execution_tasks);
        let execution_results = self.engine.execute_task_group(execution_group).await?;
        
        // Process results
        let mut success_count = 0;
        let mut failure_count = 0;
        
        for (index, result) in execution_results.iter().enumerate() {
            if result.status == TaskResultStatus::Success {
                self.results.insert(index, result.output.clone());
                success_count += 1;
            } else {
                self.results.insert(index, format!("Failed: {}", result.error.clone().unwrap_or_default()));
                failure_count += 1;
            }
        }
        
        self.state.set_metadata("success_count", success_count.to_string());
        self.state.set_metadata("failure_count", failure_count.to_string());
        
        // Finalize workflow
        if failure_count == 0 {
            self.state.set_status("completed");
        } else if success_count == 0 {
            self.state.set_status("failed");
        } else {
            self.state.set_status("completed_with_errors");
        }
        
        // Generate combined result
        let mut result_str = String::new();
        
        for i in 0..self.user_queries.len() {
            let query = &self.user_queries[i];
            let response = self.results.get(&i).cloned().unwrap_or_else(|| "No response".to_string());
            let task_type = if i < task_types.len() {
                format!("{:?}", task_types[i])
            } else {
                "Unknown".to_string()
            };
            
            result_str.push_str(&format!("Query {}: \"{}\"\nType: {}\nResponse: {}\n\n", 
                                        i + 1, query, task_type, response));
        }
        
        // Return overall workflow result
        if self.state.status() == "failed" {
            Ok(WorkflowResult::failed(&format!("Router workflow failed with {} failures", failure_count)))
        } else {
            Ok(WorkflowResult::success(result_str))
        }
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
    
    // Sample user queries of different types
    let user_queries = vec![
        "Write a Python function to calculate the Fibonacci sequence".to_string(),
        "Summarize the key points of the Model Context Protocol".to_string(),
        "Analyze the trend data: 10, 15, 13, 17, 20, 22, 25, 23".to_string(),
        "What are the best practices for implementing AI workflows?".to_string(),
    ];
    
    // Create and run router workflow
    let mut workflow = RouterWorkflow::new(
        workflow_engine, 
        ollama_client,
        user_queries
    );
    
    info!("Starting router workflow...");
    let result = workflow.run().await?;
    
    if result.is_success() {
        info!("Router workflow completed successfully!");
        println!("\nWorkflow Results:\n{}", result.output);
        println!("\nWorkflow Metadata:");
        println!("- Status: {}", workflow.state.status());
        println!("- Success Count: {}", workflow.state.metadata().get("success_count").unwrap_or(&"0".to_string()));
        println!("- Failure Count: {}", workflow.state.metadata().get("failure_count").unwrap_or(&"0".to_string()));
    } else {
        error!("Router workflow failed: {}", result.error.unwrap_or_default());
    }
    
    Ok(())
} 