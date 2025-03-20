use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::json;
use tokio::sync::Mutex;

use mcp_agent::llm::{CompletionRequest, LlmClient, LlmMessage, MessageRole, OllamaClient};
use mcp_agent::telemetry::init_telemetry;
use mcp_agent::workflow::{
    WorkflowEngine, WorkflowState, WorkflowResult, Workflow, WorkflowTask, TaskGroup,
    execute_workflow,
};

/// A simple parallel workflow that summarizes different aspects of a text using LLM
struct ParallelSummarizationWorkflow {
    /// State of the workflow
    state: WorkflowState,
    
    /// Engine for executing tasks
    engine: WorkflowEngine,
    
    /// LLM client
    llm_client: Arc<OllamaClient>,
    
    /// Text to summarize
    text: String,
    
    /// Number of concurrent summaries to create
    concurrency: usize,
}

impl ParallelSummarizationWorkflow {
    /// Create a new parallel summarization workflow
    pub fn new(engine: WorkflowEngine, llm_client: Arc<OllamaClient>, text: String, concurrency: usize) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("text_length".to_string(), json!(text.len()));
        metadata.insert("concurrency".to_string(), json!(concurrency));
        
        Self {
            state: WorkflowState::new(
                Some("ParallelSummarization".to_string()), 
                Some(metadata)
            ),
            engine,
            llm_client,
            text,
            concurrency,
        }
    }
    
    /// Generate a task for a specific aspect of summarization
    fn generate_summary_task(&self, aspect: &str) -> WorkflowTask<String> {
        let text = self.text.clone();
        let aspect = aspect.to_string();
        let llm_client = Arc::clone(&self.llm_client);
        
        WorkflowTask::new(&format!("summarize_{}", aspect), move || {
            let text_clone = text.clone();
            let aspect_clone = aspect.clone();
            let llm_client_clone = Arc::clone(&llm_client);
            
            async move {
                let prompt = match aspect_clone.as_str() {
                    "key_points" => format!(
                        "Extract the 3-5 most important key points from this text: {}", 
                        text_clone
                    ),
                    "sentiment" => format!(
                        "Analyze the sentiment of this text. Is it positive, negative, or neutral? Why? Text: {}", 
                        text_clone
                    ),
                    "executive" => format!(
                        "Create a 2-3 sentence executive summary of this text: {}", 
                        text_clone
                    ),
                    "audience" => format!(
                        "Who is the intended audience for this text? Explain why: {}", 
                        text_clone
                    ),
                    _ => format!(
                        "Summarize this text with focus on {}: {}", 
                        aspect_clone, text_clone
                    ),
                };
                
                let request = CompletionRequest {
                    model: llm_client_clone.config().model.clone(),
                    messages: vec![
                        LlmMessage::system("You are a helpful assistant that specializes in summarization."),
                        LlmMessage::user(prompt),
                    ],
                    max_tokens: Some(300),
                    temperature: Some(0.7),
                    top_p: None,
                    parameters: HashMap::new(),
                };
                
                let completion = llm_client_clone.complete(request).await?;
                Ok(completion.content)
            }
        })
        .with_timeout(Duration::from_secs(30))
    }
}

#[async_trait]
impl Workflow for ParallelSummarizationWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        // Initialize the result
        let mut result = WorkflowResult::new();
        result.start();
        
        // Check if LLM is available
        if !self.llm_client.is_available().await? {
            return Err(anyhow!("LLM is not available"));
        }
        
        // Update workflow state
        self.update_status("processing").await;
        
        // Define the different aspects to summarize
        let aspects = vec![
            "key_points",
            "sentiment", 
            "executive", 
            "audience",
        ];
        
        // Create tasks
        let mut task_group = TaskGroup::new();
        for aspect in &aspects {
            task_group.add_task(self.generate_summary_task(aspect));
        }
        
        // Execute tasks in parallel
        let summaries = self.engine.execute_tasks(task_group).await;
        
        // Process results
        let mut summary_map = HashMap::new();
        let mut failed_aspects = Vec::new();
        
        for (i, summary_result) in summaries.into_iter().enumerate() {
            let aspect = aspects[i];
            match summary_result {
                Ok(summary) => {
                    summary_map.insert(aspect.to_string(), json!(summary));
                }
                Err(e) => {
                    failed_aspects.push(aspect);
                    self.state.metadata.insert(
                        format!("error_{}", aspect), 
                        json!(e.to_string())
                    );
                }
            }
        }
        
        // Update state with successful summary count
        self.state.metadata.insert(
            "successful_summaries".to_string(), 
            json!(aspects.len() - failed_aspects.len())
        );
        
        if !failed_aspects.is_empty() {
            self.state.metadata.insert(
                "failed_aspects".to_string(), 
                json!(failed_aspects)
            );
        }
        
        // Set result and complete
        result.value = Some(json!({
            "summaries": summary_map,
            "success_rate": (aspects.len() - failed_aspects.len()) as f32 / aspects.len() as f32,
        }));
        
        result.complete();
        
        // Update workflow state
        self.update_status("completed").await;
        
        Ok(result)
    }
    
    fn state(&self) -> &WorkflowState {
        &self.state
    }
    
    fn state_mut(&mut self) -> &mut WorkflowState {
        &mut self.state
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize telemetry
    let _telemetry_guard = init_telemetry(None);
    
    // Create Ollama client
    let ollama_client = Arc::new(OllamaClient::new_with_defaults());
    
    // Check if Ollama is available
    if !ollama_client.is_available().await? {
        eprintln!("Ollama is not available. Please make sure it's running.");
        return Ok(());
    }
    
    // Create workflow engine
    let engine = WorkflowEngine::new(None);
    
    // Sample text to summarize
    let text = "
    The Model Context Protocol (MCP) is an open standard that enables seamless integration between different AI models, components, and applications. It provides a structured way for these systems to communicate, share context, and interoperate effectively. 
    
    MCP defines a consistent interface for sending messages, maintaining state, and handling errors across diverse AI systems. This standardization allows developers to build complex AI applications that can leverage multiple specialized components without tight coupling or vendor lock-in.
    
    Key benefits of MCP include improved modularity, easier testing and deployment, and the ability to swap out components as technology evolves. By establishing clear communication patterns, MCP helps orchestrate the interactions between models in multi-step reasoning processes, retrieval-augmented generation, and other advanced AI workflows.
    
    The protocol is designed to be language-agnostic, with implementations available in Python, JavaScript, and Rust. It supports both synchronous and asynchronous communication patterns, making it suitable for a wide range of application architectures.
    ";
    
    // Create the workflow
    let workflow = ParallelSummarizationWorkflow::new(
        engine,
        ollama_client,
        text.to_string(),
        4 // Concurrent tasks
    );
    
    // Execute the workflow
    println!("Starting parallel summarization workflow...");
    let result = execute_workflow(workflow).await?;
    
    // Print the results
    if let Some(value) = result.value {
        if let Some(summaries) = value.get("summaries") {
            println!("\nSummaries:");
            println!("==========");
            
            if let Some(summary_map) = summaries.as_object() {
                for (aspect, summary) in summary_map {
                    println!("\n{}: {}", aspect, summary.as_str().unwrap_or(""));
                }
            }
        }
        
        if let Some(success_rate) = value.get("success_rate") {
            println!("\nSuccess rate: {:.1}%", success_rate.as_f64().unwrap_or(0.0) * 100.0);
        }
    }
    
    println!("\nWorkflow completed in: {:?} ms", result.duration_ms().unwrap_or(0));
    
    Ok(())
} 