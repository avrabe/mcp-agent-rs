use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

use mcp_agent::llm::types::{
    Completion, CompletionRequest, LlmClient, LlmConfig, Message, MessageRole,
};
use mcp_agent::telemetry::{TelemetryConfig, init_telemetry};
use mcp_agent::workflow::{
    AsyncSignalHandler, Workflow, WorkflowEngine, WorkflowResult, WorkflowSignal, WorkflowState,
    execute_workflow, task,
};

#[derive(Debug, Clone, Copy)]
enum PipelineStage {
    Translation,
    Summarization,
    KeypointExtraction,
    Formatting,
}

struct PipelineWorkflow {
    state: WorkflowState,
    engine: WorkflowEngine,
    llm_client: Arc<MockLlmClient>,
    input_text: String,
    target_language: String,
    current_output: Option<String>,
}

impl PipelineWorkflow {
    fn new(
        engine: WorkflowEngine,
        llm_client: Arc<MockLlmClient>,
        input_text: String,
        target_language: String,
    ) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("input_length".to_string(), json!(input_text.len()));
        metadata.insert(
            "target_language".to_string(),
            json!(target_language.clone()),
        );

        Self {
            state: WorkflowState::new(Some("PipelineWorkflow".to_string()), Some(metadata)),
            engine,
            llm_client,
            input_text,
            target_language,
            current_output: None,
        }
    }

    fn create_translation_task(&self) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();
        let input_text = self.input_text.clone();
        let target_language = self.target_language.clone();

        task::task("translate_text", move || async move {
            info!("Starting translation task to {}", target_language);

            let prompt = format!(
                "Translate the following text into {}:\n\n{}",
                target_language, input_text
            );

            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: MessageRole::User,
                    content: prompt,
                    metadata: None,
                }],
                max_tokens: Some(1000),
                temperature: Some(0.3),
                top_p: None,
                parameters: HashMap::new(),
            };

            match llm_client.complete(request).await {
                Ok(completion) => {
                    let translated_text = completion.content;
                    info!("Translation completed successfully");
                    Ok(translated_text)
                }
                Err(e) => {
                    error!("Translation failed: {}", e);
                    Err(anyhow!("Failed to translate text: {}", e))
                }
            }
        })
    }

    fn create_summarization_task(&self, input: String) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();

        task::task("summarize_text", move || async move {
            info!("Starting summarization task");

            let prompt = format!(
                "Summarize the following text in a concise paragraph:\n\n{}",
                input
            );

            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: MessageRole::User,
                    content: prompt,
                    metadata: None,
                }],
                max_tokens: Some(500),
                temperature: Some(0.5),
                top_p: None,
                parameters: HashMap::new(),
            };

            match llm_client.complete(request).await {
                Ok(completion) => {
                    let summarized_text = completion.content;
                    info!("Summarization completed successfully");
                    Ok(summarized_text)
                }
                Err(e) => {
                    error!("Summarization failed: {}", e);
                    Err(anyhow!("Failed to summarize text: {}", e))
                }
            }
        })
    }

    fn create_keypoint_extraction_task(&self, input: String) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();

        task::task("extract_keypoints", move || async move {
            info!("Starting keypoint extraction task");

            let prompt = format!(
                "Extract the 3-5 most important points from this text as a numbered list:\n\n{}",
                input
            );

            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: MessageRole::User,
                    content: prompt,
                    metadata: None,
                }],
                max_tokens: Some(500),
                temperature: Some(0.4),
                top_p: None,
                parameters: HashMap::new(),
            };

            match llm_client.complete(request).await {
                Ok(completion) => {
                    let keypoints = completion.content;
                    info!("Keypoint extraction completed successfully");
                    Ok(keypoints)
                }
                Err(e) => {
                    error!("Keypoint extraction failed: {}", e);
                    Err(anyhow!("Failed to extract keypoints: {}", e))
                }
            }
        })
    }

    fn create_formatting_task(&self, input: String) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();

        task::task("format_output", move || async move {
            info!("Starting formatting task");

            let prompt = format!(
                "Format the following content in markdown with appropriate headings, \
                bullet points, and emphasis. Make it look professional and well-structured:\n\n{}",
                input
            );

            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: MessageRole::User,
                    content: prompt,
                    metadata: None,
                }],
                max_tokens: Some(800),
                temperature: Some(0.4),
                top_p: None,
                parameters: HashMap::new(),
            };

            match llm_client.complete(request).await {
                Ok(completion) => {
                    let formatted_text = completion.content;
                    info!("Formatting completed successfully");
                    Ok(formatted_text)
                }
                Err(e) => {
                    error!("Formatting failed: {}", e);
                    Err(anyhow!("Failed to format text: {}", e))
                }
            }
        })
    }

    async fn process_stage(&mut self, stage: PipelineStage, input: String) -> Result<String> {
        self.state
            .set_metadata("current_stage", json!(format!("{:?}", stage)));

        let task = match stage {
            PipelineStage::Translation => self.create_translation_task(),
            PipelineStage::Summarization => self.create_summarization_task(input),
            PipelineStage::KeypointExtraction => self.create_keypoint_extraction_task(input),
            PipelineStage::Formatting => self.create_formatting_task(input),
        };

        let output = self.engine.execute_task(task).await?;
        self.state
            .set_metadata(&format!("{:?}_output_length", stage), json!(output.len()));
        Ok(output)
    }
}

#[async_trait]
impl Workflow for PipelineWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        // Initialize workflow
        let mut result = WorkflowResult::new();
        result.start();

        // Check if LLM is available
        if !self.llm_client.is_available().await? {
            self.state.set_error("LLM service is not available");
            return Ok(WorkflowResult::failed("LLM service is not available"));
        }

        self.state.set_status("processing");

        // Define the pipeline stages
        let stages = [
            PipelineStage::Translation,
            PipelineStage::Summarization,
            PipelineStage::KeypointExtraction,
            PipelineStage::Formatting,
        ];

        // Start with the input text
        let mut current_output = self.input_text.clone();

        // Process each stage sequentially
        for (i, stage) in stages.iter().enumerate() {
            info!("Pipeline Stage {}/{}: {:?}", i + 1, stages.len(), stage);
            self.state
                .set_status(&format!("stage_{:?}", stage).to_lowercase());

            match self.process_stage(*stage, current_output.clone()).await {
                Ok(output) => {
                    current_output = output;
                    info!("Stage {:?} completed successfully", stage);
                    self.state
                        .set_metadata("last_successful_stage", json!(format!("{:?}", stage)));
                }
                Err(e) => {
                    error!("Pipeline failed at stage {:?}: {}", stage, e);
                    self.state
                        .set_error(format!("Pipeline failed at stage {:?}: {}", stage, e));
                    result.value = Some(json!({
                        "error": format!("Failed at stage {:?}", stage),
                        "completed_stages": i,
                        "partial_output": current_output
                    }));
                    result.complete();
                    return Ok(result);
                }
            }
        }

        // Store the final output
        self.current_output = Some(current_output.clone());

        // Update workflow state and return result
        self.state.set_status("completed");
        result.value = Some(json!({
            "target_language": self.target_language,
            "output": current_output,
            "stages_completed": stages.len()
        }));

        result.complete();
        Ok(result)
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
    fn new() -> Self {
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

        // Generate appropriate mock responses based on the pipeline stage
        let response = if prompt.contains("Translate") {
            if prompt.contains("Spanish") {
                "Este es un texto traducido al español. Contiene información importante sobre el Protocolo de Contexto de Modelo, que es una interfaz para la comunicación entre modelos de lenguaje y sus entornos.".to_string()
            } else if prompt.contains("French") {
                "Voici un texte traduit en français. Il contient des informations importantes sur le Protocole de Contexte de Modèle, qui est une interface pour la communication entre les modèles de langage et leurs environnements.".to_string()
            } else {
                "This is a translated text in the target language. It contains important information about the Model Context Protocol, which is an interface for communication between language models and their environments.".to_string()
            }
        } else if prompt.contains("Summarize") {
            "The Model Context Protocol (MCP) provides a standardized interface for interaction between language models and their contexts, enabling efficient communication, state management, and tool usage. It supports streaming token handling and defines clear boundaries for security and data isolation.".to_string()
        } else if prompt.contains("Extract") && prompt.contains("points") {
            "1. The Model Context Protocol (MCP) standardizes the interaction between language models and applications.\n2. MCP includes mechanisms for context management, token streaming, and tool usage.\n3. The protocol improves efficiency through streamlined communication channels.\n4. Implementation is available through a Rust-based library with async support.".to_string()
        } else if prompt.contains("Format") {
            "# Model Context Protocol\n\n## Key Features\n\n* **Standardized Interface**: Provides consistent APIs for model-context interaction\n* **Efficient Communication**: Optimized for low-latency token streaming\n* **Tool Support**: Built-in mechanisms for tool definition and usage\n\n## Implementation\n\nThe protocol is implemented in Rust with async support, allowing for efficient concurrency handling.\n\n> The MCP is designed to simplify the integration of language models into various applications.".to_string()
        } else {
            "This is a generic response for the pipeline workflow example. The specific stage couldn't be determined from the prompt.".to_string()
        };

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
        service_name: "pipeline_workflow".to_string(),
        enable_console: true,
        ..TelemetryConfig::default()
    })
    .map_err(|e| anyhow!("Failed to initialize telemetry: {}", e))?;

    // Create LLM client
    let llm_client = Arc::new(MockLlmClient::new());

    // Create signal handler and workflow engine
    let signal_handler = AsyncSignalHandler::new_with_signals(vec![
        WorkflowSignal::Interrupt,
        WorkflowSignal::Terminate,
    ]);

    let engine = WorkflowEngine::new(signal_handler);

    // Sample text about MCP
    let input_text = "The Model Context Protocol (MCP) defines a standardized interface for \
    interaction between language models and their context. It specifies methods for managing \
    context, streaming tokens, and utilizing tools. The implementation provides efficient \
    communication channels and supports state persistence.";

    // Create and run the pipeline workflow
    let workflow = PipelineWorkflow::new(
        engine,
        llm_client,
        input_text.to_string(),
        "Spanish".to_string(),
    );

    info!("Starting pipeline workflow...");
    let result = execute_workflow(workflow).await?;

    if result.is_success() {
        println!("\nWorkflow Result:\n{}", result.output());
    } else {
        error!(
            "Pipeline workflow failed: {}",
            result.error().unwrap_or_default()
        );
    }

    Ok(())
}
