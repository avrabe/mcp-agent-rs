use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;

use mcp_agent::llm::types::{
    Completion, CompletionRequest, LlmClient, LlmConfig, Message, MessageRole,
};
use mcp_agent::telemetry::{TelemetryConfig, init_telemetry};
use mcp_agent::workflow::{
    AsyncSignalHandler, Workflow, WorkflowEngine, WorkflowResult, WorkflowSignal, WorkflowState,
    execute_workflow, task,
};

/// A parallel workflow that summarizes different aspects of a text
struct ParallelSummarizationWorkflow {
    /// State of the workflow
    state: WorkflowState,

    /// Engine for executing tasks
    engine: WorkflowEngine,

    /// LLM client
    llm_client: Arc<MockLlmClient>,

    /// Text to summarize
    text: String,

    /// Number of concurrent summaries to create
    concurrency: usize,
}

impl ParallelSummarizationWorkflow {
    /// Create a new parallel summarization workflow
    pub fn new(
        engine: WorkflowEngine,
        llm_client: Arc<MockLlmClient>,
        text: String,
        concurrency: usize,
    ) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("text_length".to_string(), json!(text.len()));
        metadata.insert("concurrency".to_string(), json!(concurrency));

        Self {
            state: WorkflowState::new(Some("ParallelSummarization".to_string()), Some(metadata)),
            engine,
            llm_client,
            text,
            concurrency,
        }
    }

    /// Create a task for a specific summary aspect
    fn generate_summary_task(&self, aspect: &str) -> task::WorkflowTask<String> {
        let text = self.text.clone();
        let client = Arc::clone(&self.llm_client);
        let aspect_str = aspect.to_string();

        task::task(&format!("summarize_{}", aspect), move || async move {
            let (prompt, system_prompt) = match aspect_str.as_str() {
                "key_points" => (
                    format!("Extract the key points from this text:\n\n{}", text),
                    "Extract only the most important key points from the text as a bulleted list. Be concise and focus on the main ideas.",
                ),
                "sentiment" => (
                    format!("Analyze the sentiment of this text:\n\n{}", text),
                    "Analyze the sentiment of the text. Is it positive, negative, or neutral? Provide a brief explanation.",
                ),
                "executive_summary" => (
                    format!("Provide an executive summary of this text:\n\n{}", text),
                    "Create a concise executive summary of the text in 2-3 paragraphs. Focus on the most important information and insights.",
                ),
                "audience" => (
                    format!("Who is the intended audience for this text?\n\n{}", text),
                    "Determine the most likely intended audience for this text. Consider the tone, complexity, and subject matter.",
                ),
                _ => (
                    format!("Summarize this text:\n\n{}", text),
                    "Provide a brief summary of the text.",
                ),
            };

            let completion_request = CompletionRequest {
                model: "mistral".to_string(),
                messages: vec![
                    Message {
                        role: MessageRole::System,
                        content: system_prompt.to_string(),
                        metadata: None,
                    },
                    Message {
                        role: MessageRole::User,
                        content: prompt,
                        metadata: None,
                    },
                ],
                max_tokens: Some(500),
                temperature: Some(0.7),
                top_p: None,
                parameters: HashMap::new(),
            };

            let completion = client.complete(completion_request).await?;
            Ok(completion.content)
        })
    }
}

#[async_trait]
impl Workflow for ParallelSummarizationWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        let mut result = WorkflowResult::new();
        result.start();

        // Check if LLM is available
        if !self.llm_client.is_available().await? {
            self.state.set_error("LLM service is not available");
            return Ok(WorkflowResult::failed("LLM service is not available"));
        }

        self.state.set_status("processing");

        // Define aspects to summarize
        let aspects = ["key_points", "sentiment", "executive_summary", "audience"];

        // Create task group for parallel execution
        let task_count = self.concurrency.min(aspects.len());

        // Create tasks for each aspect
        let mut tasks = Vec::with_capacity(task_count);

        for aspect in aspects.iter().take(task_count) {
            tasks.push(self.generate_summary_task(aspect));
        }

        let results = self.engine.execute_task_group(tasks).await?;

        // Process results
        let mut summaries = HashMap::new();

        for (i, summary) in results.iter().enumerate() {
            if i < aspects.len() {
                summaries.insert(aspects[i].to_string(), summary.clone());
            }
        }

        // Update workflow state with results
        self.state
            .set_metadata("successful_summaries", summaries.len());

        let failed_aspects: Vec<&str> = aspects
            .iter()
            .take(task_count)
            .enumerate()
            .filter_map(|(i, &aspect)| {
                if i >= results.len() || !summaries.contains_key(aspect) {
                    Some(aspect)
                } else {
                    None
                }
            })
            .collect();

        if !failed_aspects.is_empty() {
            self.state
                .set_metadata("failed_aspects", failed_aspects.join(", "));
        }

        self.state.set_status("completed");
        result.value = Some(json!(summaries));

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
                model: "mock-model".to_string(),
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
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Extract the prompt from the request to generate a mock response
        let prompt = if let Some(user_message) = request
            .messages
            .iter()
            .find(|m| matches!(m.role, MessageRole::User))
        {
            &user_message.content
        } else {
            "Unknown prompt"
        };

        // Generate a simple mock response based on the prompt
        let response = format!("Mock response for: {}", prompt);
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
        service_name: "parallel_workflow".to_string(),
        enable_console: true,
        ..TelemetryConfig::default()
    })
    .map_err(|e| anyhow::anyhow!("Failed to initialize telemetry: {}", e))?;

    // Create LLM client
    let client = Arc::new(MockLlmClient::new());

    // Create signal handler and workflow engine
    let signal_handler = AsyncSignalHandler::new_with_signals(vec![
        WorkflowSignal::Interrupt,
        WorkflowSignal::Terminate,
    ]);

    let engine = WorkflowEngine::new(signal_handler);

    // Sample text about MCP to summarize
    let text = "The Model Context Protocol (MCP) is a standardized protocol for communication between \
    language models and their environments. It enables more structured, efficient, and transparent \
    interactions with AI systems, allowing for better debugging, monitoring, and control. \
    MCP defines a set of messages and operations that facilitate the exchange of information \
    between a model and the environment, including context windows, tool use, and feedback. \
    By using MCP, developers can create more reliable and explainable AI applications.";

    // Create and execute the workflow
    let workflow = ParallelSummarizationWorkflow::new(
        engine,
        client,
        text.to_string(),
        4, // Concurrency level
    );

    let result = execute_workflow(workflow).await?;

    println!("\nWorkflow completed!");
    if result.is_success() {
        println!("\n=== Parallel Summarization Results ===\n");
        println!("{}", result.output());
        println!(
            "\nSummaries: {}",
            result
                .metadata
                .get("successful_summaries")
                .map_or("0", |v| v.as_str().unwrap_or("0"))
        );
    } else if let Some(error) = result.error() {
        eprintln!("Workflow failed: {}", error);
    }

    Ok(())
}
