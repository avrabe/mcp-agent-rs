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

enum PipelineStage {
    Translation,
    Summarization,
    KeypointExtraction,
    Formatting,
}

struct PipelineWorkflow {
    state: WorkflowState,
    engine: WorkflowEngine,
    llm_client: OllamaClient,
    input_text: String,
    target_language: String,
    current_output: Option<String>,
}

impl PipelineWorkflow {
    fn new(
        engine: WorkflowEngine, 
        llm_client: OllamaClient, 
        input_text: String,
        target_language: String,
    ) -> Self {
        let mut state = WorkflowState::new();
        state.set_metadata("input_length", input_text.len().to_string());
        state.set_metadata("target_language", target_language.clone());
        
        Self {
            state,
            engine,
            llm_client,
            input_text,
            target_language,
            current_output: None,
        }
    }
    
    fn create_translation_task(&self) -> Task {
        let llm_client = self.llm_client.clone();
        let input_text = self.input_text.clone();
        let target_language = self.target_language.clone();
        
        Task::new("translate_text", move |_ctx| {
            info!("Starting translation task to {}", target_language);
            
            let prompt = format!(
                "Translate the following text into {}:\n\n{}",
                target_language, input_text
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
                    let translated_text = completion.message.content.clone();
                    info!("Translation completed successfully");
                    Ok(TaskResult::success(translated_text))
                },
                Err(e) => {
                    error!("Translation failed: {}", e);
                    Err(anyhow!("Failed to translate text: {}", e))
                }
            }
        })
    }
    
    fn create_summarization_task(&self, input: String) -> Task {
        let llm_client = self.llm_client.clone();
        
        Task::new("summarize_text", move |_ctx| {
            info!("Starting summarization task");
            
            let prompt = format!(
                "Summarize the following text in a concise paragraph:\n\n{}",
                input
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
                    let summarized_text = completion.message.content.clone();
                    info!("Summarization completed successfully");
                    Ok(TaskResult::success(summarized_text))
                },
                Err(e) => {
                    error!("Summarization failed: {}", e);
                    Err(anyhow!("Failed to summarize text: {}", e))
                }
            }
        })
    }
    
    fn create_keypoint_extraction_task(&self, input: String) -> Task {
        let llm_client = self.llm_client.clone();
        
        Task::new("extract_keypoints", move |_ctx| {
            info!("Starting keypoint extraction task");
            
            let prompt = format!(
                "Extract the 3-5 most important points from this text as a numbered list:\n\n{}",
                input
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
                    let keypoints = completion.message.content.clone();
                    info!("Keypoint extraction completed successfully");
                    Ok(TaskResult::success(keypoints))
                },
                Err(e) => {
                    error!("Keypoint extraction failed: {}", e);
                    Err(anyhow!("Failed to extract keypoints: {}", e))
                }
            }
        })
    }
    
    fn create_formatting_task(&self, input: String) -> Task {
        let llm_client = self.llm_client.clone();
        
        Task::new("format_output", move |_ctx| {
            info!("Starting formatting task");
            
            let prompt = format!(
                "Format the following content in markdown with appropriate headings, \
                bullet points, and emphasis. Make it look professional and well-structured:\n\n{}",
                input
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
                    let formatted_text = completion.message.content.clone();
                    info!("Formatting completed successfully");
                    Ok(TaskResult::success(formatted_text))
                },
                Err(e) => {
                    error!("Formatting failed: {}", e);
                    Err(anyhow!("Failed to format text: {}", e))
                }
            }
        })
    }
    
    async fn process_stage(&mut self, stage: PipelineStage, input: String) -> Result<String> {
        self.state.set_metadata("current_stage", format!("{:?}", stage));
        
        let task = match stage {
            PipelineStage::Translation => self.create_translation_task(),
            PipelineStage::Summarization => self.create_summarization_task(input),
            PipelineStage::KeypointExtraction => self.create_keypoint_extraction_task(input),
            PipelineStage::Formatting => self.create_formatting_task(input),
        };
        
        let task_group = TaskGroup::new(vec![task]);
        let results = self.engine.execute_task_group(task_group).await?;
        
        if let Some(result) = results.first() {
            if result.status == TaskResultStatus::Success {
                let output = result.output.clone();
                self.state.set_metadata(
                    &format!("{:?}_output_length", stage), 
                    output.len().to_string()
                );
                Ok(output)
            } else {
                let error_msg = result.error.clone().unwrap_or_else(|| "Unknown error".to_string());
                self.state.set_error(format!("Stage {:?} failed: {}", stage, error_msg));
                Err(anyhow!("Stage {:?} failed: {}", stage, error_msg))
            }
        } else {
            Err(anyhow!("No result returned from stage {:?}", stage))
        }
    }
    
    async fn run(&mut self) -> Result<WorkflowResult> {
        // Initialize workflow
        self.state.set_status("starting");
        info!("Starting pipeline workflow");
        
        // Check if LLM is available
        if let Err(e) = self.llm_client.check_availability().await {
            error!("LLM client is not available: {}", e);
            self.state.set_status("failed");
            self.state.set_error(format!("LLM client is not available: {}", e));
            return Ok(WorkflowResult::failed("LLM client is not available"));
        }
        
        self.state.set_status("processing");
        
        // Define the pipeline stages
        let stages = vec![
            PipelineStage::Translation,
            PipelineStage::Summarization,
            PipelineStage::KeypointExtraction,
            PipelineStage::Formatting,
        ];
        
        // Start with the input text
        let mut current_output = self.input_text.clone();
        
        // Process each stage in sequence
        for stage in stages {
            info!("Processing stage: {:?}", stage);
            
            match self.process_stage(stage, current_output).await {
                Ok(output) => {
                    current_output = output;
                    self.current_output = Some(current_output.clone());
                    info!("Stage {:?} completed successfully", stage);
                },
                Err(e) => {
                    error!("Stage {:?} failed: {}", stage, e);
                    self.state.set_status("failed");
                    return Ok(WorkflowResult::failed(&format!("Pipeline failed at stage {:?}: {}", stage, e)));
                }
            }
            
            // Update progress
            self.state.set_metadata("current_length", current_output.len().to_string());
        }
        
        // Finalize workflow
        self.state.set_status("completed");
        
        // Return the final result
        Ok(WorkflowResult::success(current_output))
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
    
    // Sample input text
    let sample_text = r#"
    The Model Context Protocol (MCP) is an innovative standard for AI model interactions that addresses several critical challenges in modern AI systems. It focuses on efficient context management, reducing token usage, and enhancing privacy. The protocol standardizes how applications interact with AI models, allowing for more sophisticated conversation handling and context compression. This enables applications to maintain longer conversation histories without exceeding token limits while reducing operational costs.

    MCP includes mechanisms for semantic routing between specialized models, allowing complex AI systems to delegate subtasks to the most appropriate model. This creates more efficient AI ecosystems where models collaborate to solve problems. Implementations have demonstrated up to 70% reduction in token usage while maintaining or improving output quality. Additionally, the protocol's privacy-preserving features ensure sensitive information is properly handled, making it suitable for enterprise applications.
    "#;
    
    // Create and run pipeline workflow
    let mut workflow = PipelineWorkflow::new(
        workflow_engine,
        ollama_client,
        sample_text.to_string(),
        "Spanish".to_string()
    );
    
    info!("Starting pipeline workflow...");
    let result = workflow.run().await?;
    
    if result.is_success() {
        info!("Pipeline workflow completed successfully!");
        println!("\nWorkflow Result:\n{}", result.output);
        
        // Print workflow metadata
        println!("\nPipeline Workflow Metadata:");
        for (key, value) in workflow.state.metadata() {
            println!("- {}: {}", key, value);
        }
    } else {
        error!("Pipeline workflow failed: {}", result.error.unwrap_or_default());
    }
    
    Ok(())
} 