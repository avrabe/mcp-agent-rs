use anyhow::{anyhow, Result};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

use mcp_agent::llm::types::{
    Completion, CompletionRequest, LlmClient, LlmConfig, Message, MessageRole,
};
use mcp_agent::telemetry::{init_telemetry, TelemetryConfig};
use mcp_agent::workflow::signal::DefaultSignalHandler;
use mcp_agent::workflow::{
    execute_workflow, task, Workflow, WorkflowEngine, WorkflowResult, WorkflowSignal, WorkflowState,
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
            s if s.contains("analyze") || s.contains("analysis") || s.contains("data") => {
                Self::DataAnalysis
            }
            s if s.contains("image") || s.contains("picture") || s.contains("photo") => {
                Self::ImageDescription
            }
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
            Self::CodeGeneration => {
                "You are an expert programming assistant. Your task is to generate clean, efficient, and well-documented code based on the user's requirements."
            }
            Self::TextSummarization => {
                "You are a summarization expert. Your task is to create concise summaries that capture the key points of the provided text."
            }
            Self::DataAnalysis => {
                "You are a data analysis assistant. Your task is to analyze the provided information and extract meaningful insights."
            }
            Self::ImageDescription => {
                "You are an image description assistant. Your task is to provide detailed descriptions of the images presented to you."
            }
            Self::Unknown => {
                "You are a helpful assistant. Please respond to the user's query to the best of your ability."
            }
        }
    }
}

struct RouterWorkflow {
    state: WorkflowState,
    engine: WorkflowEngine,
    llm_client: Arc<MockLlmClient>,
    user_queries: Vec<String>,
    results: HashMap<usize, String>,
}

impl RouterWorkflow {
    fn new(
        engine: WorkflowEngine,
        llm_client: Arc<MockLlmClient>,
        user_queries: Vec<String>,
    ) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("total_queries".to_string(), json!(user_queries.len()));

        Self {
            state: WorkflowState::new(Some("RouterWorkflow".to_string()), Some(metadata)),
            engine,
            llm_client,
            user_queries,
            results: HashMap::new(),
        }
    }

    fn create_classification_task(
        &self,
        query: String,
        index: usize,
    ) -> task::WorkflowTask<String> {
        task::task(&format!("classify_query_{}", index), move || async move {
            info!("Classifying query: {}", query);
            let task_type = TaskType::from_str(&query);
            Ok(format!("{:?}", task_type))
        })
    }

    fn create_execution_task(
        &self,
        query: String,
        task_type: TaskType,
        index: usize,
    ) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();

        task::task(&format!("execute_query_{}", index), move || async move {
            info!("Executing query for task type: {:?}", task_type);

            let model = task_type.get_model();
            let system_prompt = task_type.get_system_prompt();

            let request = CompletionRequest {
                model: model.to_string(),
                messages: vec![
                    Message {
                        role: MessageRole::System,
                        content: system_prompt.to_string(),
                        metadata: None,
                    },
                    Message {
                        role: MessageRole::User,
                        content: query.clone(),
                        metadata: None,
                    },
                ],
                max_tokens: Some(500),
                temperature: Some(0.7),
                top_p: None,
                parameters: HashMap::new(),
            };

            match llm_client.complete(request).await {
                Ok(completion) => {
                    let response = completion.content;
                    info!(
                        "Successfully processed query with task type: {:?}",
                        task_type
                    );
                    Ok(response)
                }
                Err(e) => {
                    error!("Failed to process query: {}", e);
                    // Fallback to a generic response when the model fails
                    let fallback_msg = format!(
                        "Sorry, I couldn't process your request for {:?}. Error: {}",
                        task_type, e
                    );
                    Ok(fallback_msg)
                }
            }
        })
    }
}

#[async_trait]
impl Workflow for RouterWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        let mut result = WorkflowResult::new();
        result.start();

        // Check if LLM is available
        if !self.llm_client.is_available().await? {
            self.state.set_error("LLM service is not available");
            return Ok(WorkflowResult::failed("LLM service is not available"));
        }

        self.state.set_status("processing");

        // Step 1: Classify all queries
        info!("Step 1: Classifying all queries");
        let mut classification_results = Vec::new();

        for (index, query) in self.user_queries.iter().enumerate() {
            let task = self.create_classification_task(query.clone(), index);
            let task_result = self.engine.execute_task(task).await?;
            classification_results.push(task_result);
        }

        // Step 2: Process each query based on its classification
        info!("Step 2: Processing queries based on their classifications");
        let mut task_types = Vec::new();
        let mut execution_results = Vec::new();

        for (index, task_type_str) in classification_results.iter().enumerate() {
            let task_type = match task_type_str.as_str() {
                "CodeGeneration" => TaskType::CodeGeneration,
                "TextSummarization" => TaskType::TextSummarization,
                "DataAnalysis" => TaskType::DataAnalysis,
                "ImageDescription" => TaskType::ImageDescription,
                _ => TaskType::Unknown,
            };

            debug!("Query {} classified as {:?}", index, &task_type);
            task_types.push(task_type.clone());

            let query = self.user_queries[index].clone();
            let task = self.create_execution_task(query, task_type.clone(), index);
            let task_result = self.engine.execute_task(task).await?;
            execution_results.push(task_result);
        }

        self.state
            .set_metadata("task_types", json!(format!("{:?}", task_types)));

        // Process results
        let mut success_count = 0;
        let mut failure_count = 0;

        for (index, result_value) in execution_results.iter().enumerate() {
            if !result_value.is_empty() {
                self.results.insert(index, result_value.clone());
                success_count += 1;
            } else {
                self.results
                    .insert(index, "Failed to process query".to_string());
                failure_count += 1;
            }
        }

        self.state
            .set_metadata("success_count", json!(success_count));
        self.state
            .set_metadata("failure_count", json!(failure_count));

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
            let response = self
                .results
                .get(&i)
                .cloned()
                .unwrap_or_else(|| "No response".to_string());
            let task_type = if i < task_types.len() {
                format!("{:?}", task_types[i])
            } else {
                "Unknown".to_string()
            };

            result_str.push_str(&format!(
                "Query {}: \"{}\"\nType: {}\nResponse: {}\n\n",
                i + 1,
                query,
                task_type,
                response
            ));
        }

        // Return overall workflow result
        if self.state.status() == "failed" {
            return Ok(WorkflowResult::failed(format!(
                "Router workflow failed with {} failures",
                failure_count
            )));
        } else {
            result.value = Some(json!(result_str));
        }

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

        // Get model to determine response style
        let model = &request.model;

        // Generate mock response based on the task type
        let response = if model.contains("codellama") {
            "```python\ndef process_data(data):\n    result = []\n    for item in data:\n        if item.valid:\n            result.append(item.value * 2)\n    return result\n```"
        } else if model.contains("llava") {
            "The image shows a landscape with mountains and a lake. There are trees in the foreground and the sky is clear blue."
        } else if prompt.contains("summarize") || prompt.contains("summary") {
            "This text discusses the importance of workflow automation in modern business processes. It highlights how automation can reduce errors, increase efficiency, and free up human resources for more creative tasks."
        } else if prompt.contains("analyze") || prompt.contains("data") {
            "Analysis shows an upward trend of 15% in the primary metrics. The data indicates seasonal patterns with peaks in Q2 and Q4. Three outliers were identified which correlate with market events."
        } else {
            "I've processed your request and here is a helpful response that addresses your query in a general way."
        }.to_string();

        let prompt_len = prompt.len();
        let response_len = response.len();

        Ok(Completion {
            content: response,
            model: Some(request.model),
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
        service_name: "router_workflow".to_string(),
        enable_console: true,
        ..TelemetryConfig::default()
    })
    .map_err(|e| anyhow!("Failed to initialize telemetry: {}", e))?;

    // Create LLM client
    let client = Arc::new(MockLlmClient::new());

    // Create signal handler and workflow engine
    let signal_handler = DefaultSignalHandler::new_with_signals(vec![
        WorkflowSignal::INTERRUPT,
        WorkflowSignal::TERMINATE,
    ]);

    let engine = WorkflowEngine::new(signal_handler);

    // Example user queries for different task types
    let user_queries = vec![
        "Write a Python function to process a list of data objects and return the valid ones with doubled values.".to_string(),
        "Summarize the benefits of workflow automation for businesses.".to_string(),
        "Analyze this dataset: Monthly sales [150, 200, 175, 300, 250] and provide insights.".to_string(),
        "Describe this image of a mountain landscape with a lake.".to_string(),
        "What's the weather like today?".to_string(),
    ];

    // Create and execute workflow
    println!("Starting Router Workflow...");
    let workflow = RouterWorkflow::new(engine, client, user_queries);
    let result = execute_workflow(workflow).await?;

    // Display results
    if result.is_success() {
        println!("\nWorkflow Result:\n{}", result.output());
    } else {
        error!(
            "Router workflow failed: {}",
            result.error().unwrap_or_default()
        );
        println!("\nWorkflow Failed: {}", result.error().unwrap_or_default());
    }

    Ok(())
}
