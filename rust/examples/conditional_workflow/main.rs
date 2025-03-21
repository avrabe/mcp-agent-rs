use std::time::Duration;
use std::collections::HashMap;
use std::sync::Arc;
use anyhow::{anyhow, Result};
use tracing::{info, warn, error, debug};
use async_trait::async_trait;
use serde_json::json;

use mcp_agent::telemetry::{init_telemetry, TelemetryConfig};
use mcp_agent::workflow::{
    WorkflowEngine, WorkflowState, WorkflowResult, Workflow,
    task, AsyncSignalHandler, WorkflowSignal, execute_workflow,
};
use mcp_agent::{LlmMessage as Message, MessageRole, CompletionRequest, Completion, LlmConfig};
use mcp_agent::llm::types::LlmClient;

// Content classification types for branching
#[derive(Debug, Clone, PartialEq)]
enum ContentCategory {
    Technical,
    Business,
    Creative,
    Query,
    Unknown,
}

// Processing path based on content category
#[derive(Debug, Clone, PartialEq)]
enum ProcessingPath {
    DetailedAnalysis,
    SummaryGeneration,
    CreativeExpansion,
    FactualResponse,
    GenericProcessing,
}

struct ConditionalWorkflow {
    state: WorkflowState,
    engine: WorkflowEngine,
    llm_client: Arc<MockLlmClient>,
    input_content: String,
    category: Option<ContentCategory>,
    processing_path: Option<ProcessingPath>,
    output: Option<String>,
    intermediate_results: HashMap<String, String>,
}

impl ConditionalWorkflow {
    fn new(engine: WorkflowEngine, llm_client: Arc<MockLlmClient>, input_content: String) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("input_length".to_string(), json!(input_content.len()));
        
        Self {
            state: WorkflowState::new(
                Some("ConditionalWorkflow".to_string()), 
                Some(metadata)
            ),
            engine,
            llm_client,
            input_content,
            category: None,
            processing_path: None,
            output: None,
            intermediate_results: HashMap::new(),
        }
    }
    
    // Task to classify the input content
    fn create_classification_task(&self) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();
        let input_content = self.input_content.clone();
        
        task::task("classify_content", move || async move {
            info!("Starting content classification");
            
            let prompt = format!(
                "Classify the following content into exactly ONE of these categories: Technical, Business, Creative, Query.\n\n{}\n\nCategory:",
                input_content
            );
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: MessageRole::User,
                    content: prompt,
                    metadata: None,
                }],
                max_tokens: Some(50),
                temperature: Some(0.2),
                top_p: None,
                parameters: HashMap::new(),
            };
            
            match llm_client.complete(request).await {
                Ok(completion) => {
                    let classification = completion.content;
                    info!("Classification result: {}", classification);
                    Ok(classification)
                },
                Err(e) => {
                    error!("Classification failed: {}", e);
                    Err(anyhow!("Failed to classify content: {}", e))
                }
            }
        })
    }
    
    fn determine_processing_path(&self, category: &ContentCategory) -> ProcessingPath {
        match category {
            ContentCategory::Technical => ProcessingPath::DetailedAnalysis,
            ContentCategory::Business => ProcessingPath::SummaryGeneration,
            ContentCategory::Creative => ProcessingPath::CreativeExpansion,
            ContentCategory::Query => ProcessingPath::FactualResponse,
            ContentCategory::Unknown => ProcessingPath::GenericProcessing,
        }
    }
    
    // Task for detailed technical analysis
    fn create_technical_analysis_task(&self) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();
        let input_content = self.input_content.clone();
        
        task::task("technical_analysis", move || async move {
            info!("Starting technical content analysis");
            
            let system_prompt = "You are an expert in technical analysis. Provide detailed technical insights about the following content.";
            let user_prompt = format!("Analyze the following technical content in detail:\n\n{}", input_content);
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![
                    Message {
                        role: MessageRole::System,
                        content: system_prompt.to_string(),
                        metadata: None,
                    },
                    Message {
                        role: MessageRole::User,
                        content: user_prompt,
                        metadata: None,
                    }
                ],
                max_tokens: Some(1000),
                temperature: Some(0.3),
                top_p: None,
                parameters: HashMap::new(),
            };
            
            match llm_client.complete(request).await {
                Ok(completion) => {
                    info!("Technical analysis completed");
                    Ok(completion.content)
                },
                Err(e) => {
                    error!("Technical analysis failed: {}", e);
                    Err(anyhow!("Failed to analyze technical content: {}", e))
                }
            }
        })
    }
    
    // Task for business summary
    fn create_business_summary_task(&self) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();
        let input_content = self.input_content.clone();
        
        task::task("business_summary", move || async move {
            info!("Starting business content summarization");
            
            let system_prompt = "You are a business analyst. Provide a concise executive summary of the following business content.";
            let user_prompt = format!("Create an executive summary of the following business content:\n\n{}", input_content);
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![
                    Message {
                        role: MessageRole::System,
                        content: system_prompt.to_string(),
                        metadata: None,
                    },
                    Message {
                        role: MessageRole::User,
                        content: user_prompt,
                        metadata: None,
                    }
                ],
                max_tokens: Some(500),
                temperature: Some(0.5),
                top_p: None,
                parameters: HashMap::new(),
            };
            
            match llm_client.complete(request).await {
                Ok(completion) => {
                    info!("Business summary completed");
                    Ok(completion.content)
                },
                Err(e) => {
                    error!("Business summary failed: {}", e);
                    Err(anyhow!("Failed to summarize business content: {}", e))
                }
            }
        })
    }
    
    // Task for creative expansion
    fn create_creative_expansion_task(&self) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();
        let input_content = self.input_content.clone();
        
        task::task("creative_expansion", move || async move {
            info!("Starting creative content expansion");
            
            let system_prompt = "You are a creative writer. Expand on the following creative content with your own creative ideas.";
            let user_prompt = format!("Expand creatively on the following content:\n\n{}", input_content);
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![
                    Message {
                        role: MessageRole::System,
                        content: system_prompt.to_string(),
                        metadata: None,
                    },
                    Message {
                        role: MessageRole::User,
                        content: user_prompt,
                        metadata: None,
                    }
                ],
                max_tokens: Some(800),
                temperature: Some(0.8),
                top_p: None,
                parameters: HashMap::new(),
            };
            
            match llm_client.complete(request).await {
                Ok(completion) => {
                    info!("Creative expansion completed");
                    Ok(completion.content)
                },
                Err(e) => {
                    error!("Creative expansion failed: {}", e);
                    Err(anyhow!("Failed to expand creative content: {}", e))
                }
            }
        })
    }
    
    // Task for factual response
    fn create_factual_response_task(&self) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();
        let input_content = self.input_content.clone();
        
        task::task("factual_response", move || async move {
            info!("Generating factual response to query");
            
            let system_prompt = "You are a helpful assistant. Provide accurate, factual answers to questions.";
            let user_prompt = format!("Please answer the following question with factual information:\n\n{}", input_content);
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![
                    Message {
                        role: MessageRole::System,
                        content: system_prompt.to_string(),
                        metadata: None,
                    },
                    Message {
                        role: MessageRole::User,
                        content: user_prompt,
                        metadata: None,
                    }
                ],
                max_tokens: Some(500),
                temperature: Some(0.2),
                top_p: None,
                parameters: HashMap::new(),
            };
            
            match llm_client.complete(request).await {
                Ok(completion) => {
                    info!("Factual response generated");
                    Ok(completion.content)
                },
                Err(e) => {
                    error!("Factual response failed: {}", e);
                    Err(anyhow!("Failed to generate factual response: {}", e))
                }
            }
        })
    }
    
    // Fallback task for generic processing
    fn create_generic_processing_task(&self) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();
        let input_content = self.input_content.clone();
        
        task::task("generic_processing", move || async move {
            info!("Starting generic content processing");
            
            let system_prompt = "You are a general assistant. Process the following content appropriately.";
            let user_prompt = format!("Process the following content:\n\n{}", input_content);
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![
                    Message {
                        role: MessageRole::System,
                        content: system_prompt.to_string(),
                        metadata: None,
                    }
                ],
                max_tokens: Some(500),
                temperature: Some(0.5),
                top_p: None,
                parameters: HashMap::new(),
            };
            
            match llm_client.complete(request).await {
                Ok(completion) => {
                    info!("Generic processing completed");
                    Ok(completion.content)
                },
                Err(e) => {
                    error!("Generic processing failed: {}", e);
                    Err(anyhow!("Failed to process content: {}", e))
                }
            }
        })
    }
    
    // Task to enrich metadata based on processed content
    fn create_metadata_enrichment_task(&self, content: String) -> task::WorkflowTask<HashMap<String, String>> {
        let llm_client = self.llm_client.clone();
        
        task::task("metadata_enrichment", move || async move {
            info!("Enriching metadata for processed content");
            
            let prompt = format!(
                "Extract metadata from the following content. Return key: value pairs for relevant metadata such as topics, entities, sentiment, etc.\n\n{}",
                content
            );
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![
                    Message {
                        role: MessageRole::User,
                        content: prompt,
                        metadata: None,
                    }
                ],
                max_tokens: Some(300),
                temperature: Some(0.2),
                top_p: None,
                parameters: HashMap::new(),
            };
            
            match llm_client.complete(request).await {
                Ok(completion) => {
                    // Parse simple key-value pairs from response
                    let metadata_text = completion.content;
                    let mut metadata = HashMap::new();
                    
                    for line in metadata_text.lines() {
                        if let Some(idx) = line.find(':') {
                            let key = line[..idx].trim().to_string();
                            let value = line[idx+1..].trim().to_string();
                            if !key.is_empty() && !value.is_empty() {
                                metadata.insert(key, value);
                            }
                        }
                    }
                    
                    info!("Metadata enrichment completed: {:?}", metadata);
                    Ok(metadata)
                },
                Err(e) => {
                    error!("Metadata enrichment failed: {}", e);
                    Err(anyhow!("Failed to enrich metadata: {}", e))
                }
            }
        })
    }
    
    fn parse_classification(&self, classification: &str) -> ContentCategory {
        let classification = classification.trim().to_lowercase();
        
        if classification.contains("technical") {
            ContentCategory::Technical
        } else if classification.contains("business") {
            ContentCategory::Business
        } else if classification.contains("creative") {
            ContentCategory::Creative
        } else if classification.contains("query") {
            ContentCategory::Query
        } else {
            ContentCategory::Unknown
        }
    }
}

#[async_trait]
impl Workflow for ConditionalWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        let mut result = WorkflowResult::new();
        result.start();
        
        // Check if LLM is available
        if !self.llm_client.is_available().await? {
            self.state.set_error("LLM service is not available");
            return Ok(WorkflowResult::failed("LLM service is not available"));
        }
        
        self.state.set_status("classifying_content");
        
        // Step 1: Classify the content
        info!("Step 1: Classifying content");
        let classification_task = self.create_classification_task();
        let classification_result = self.engine.execute_task(classification_task).await?;
        
        // Parse the classification result
        let category = self.parse_classification(&classification_result);
        self.category = Some(category.clone());
        self.state.set_metadata("content_category", format!("{:?}", category));
        
        // Step 2: Determine processing path based on classification
        let processing_path = self.determine_processing_path(&category);
        self.processing_path = Some(processing_path.clone());
        self.state.set_metadata("processing_path", format!("{:?}", processing_path));
        
        info!("Content classified as {:?}, using processing path {:?}", category, processing_path);
        
        // Step 3: Process content based on category
        self.state.set_status("processing_content");
        
        let processing_task = match category {
            ContentCategory::Technical => self.create_technical_analysis_task(),
            ContentCategory::Business => self.create_business_summary_task(),
            ContentCategory::Creative => self.create_creative_expansion_task(),
            ContentCategory::Query => self.create_factual_response_task(),
            ContentCategory::Unknown => self.create_generic_processing_task(),
        };
        
        info!("Step 3: Processing content using appropriate task for {:?}", category);
        let processing_result = self.engine.execute_task(processing_task).await?;
        self.output = Some(processing_result.clone());
        
        // Step 4: Conditionally enrich metadata for business and technical content
        if matches!(category, ContentCategory::Technical | ContentCategory::Business) {
            self.state.set_status("enriching_metadata");
            info!("Step 4: Enriching metadata for {:?} content", category);
            
            let enrichment_task = self.create_metadata_enrichment_task(processing_result.clone());
            let metadata = self.engine.execute_task(enrichment_task).await?;
            
            // Update workflow state with enriched metadata
            for (key, value) in &metadata {
                self.state.set_metadata(key, value.clone());
                self.intermediate_results.insert(key.clone(), value.clone());
            }
        }
        
        // Update workflow state and result
        self.state.set_status("completed");
        result.value = Some(json!({
            "category": format!("{:?}", category),
            "processing_path": format!("{:?}", processing_path),
            "output": processing_result,
            "metadata": self.intermediate_results,
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
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // Extract the prompt to generate appropriate mock responses
        let prompt = if let Some(user_message) = request.messages.iter().find(|m| matches!(m.role, MessageRole::User)) {
            &user_message.content
        } else {
            "Unknown prompt"
        };
        
        // Generate appropriate mock responses based on the prompt
        let response = if prompt.contains("Classify") {
            if prompt.contains("Technical") || prompt.contains("technology") || prompt.contains("code") {
                "Technical".to_string()
            } else if prompt.contains("Business") || prompt.contains("finance") || prompt.contains("market") {
                "Business".to_string()
            } else if prompt.contains("Creative") || prompt.contains("write") || prompt.contains("story") {
                "Creative".to_string()
            } else if prompt.ends_with("?") || prompt.contains("Query") || prompt.contains("question") {
                "Query".to_string()
            } else {
                "Unknown".to_string()
            }
        } else if prompt.contains("technical") {
            "This is a mock technical analysis with detailed code review and architecture recommendations.".to_string()
        } else if prompt.contains("business") {
            "Executive summary: This mock business analysis identifies key market opportunities and strategic recommendations.".to_string()
        } else if prompt.contains("creative") {
            "Once upon a time in a mock creative world, the AI expanded on the original content with imaginative ideas.".to_string()
        } else if prompt.contains("question") || prompt.contains("query") {
            "Here is a factual answer to your query based on reliable information sources.".to_string()
        } else if prompt.contains("metadata") {
            "topic: Artificial Intelligence\nsentiment: Positive\nentities: Workflow, AI, Processing\ncomplexity: Medium".to_string()
        } else {
            "This is a generic mock response for general content processing.".to_string()
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
    let _guard = init_telemetry(TelemetryConfig {
        service_name: "conditional_workflow".to_string(),
        enable_console: true,
        ..TelemetryConfig::default()
    }).map_err(|e| anyhow!("Failed to initialize telemetry: {}", e))?;
    
    // Create LLM client
    let client = Arc::new(MockLlmClient::new());
    
    // Create signal handler and workflow engine
    let signal_handler = AsyncSignalHandler::new_with_signals(vec![
        WorkflowSignal::Interrupt,
        WorkflowSignal::Terminate,
    ]);
    
    let engine = WorkflowEngine::new(signal_handler);
    
    // Sample technical content about MCP
    let technical_content = "The Model Context Protocol (MCP) is a standardized API for interaction \
    between language models and their environments. It defines interfaces for context management, \
    token streaming, and tool usage. The implementation includes Rust traits with async functions \
    for efficient processing.";
    
    // Sample business content
    let business_content = "Our Q4 market analysis shows 15% growth in the AI services sector, \
    with increased demand for workflow automation tools. We recommend investing in ML infrastructure \
    and exploring strategic partnerships with data providers to maintain competitive advantage.";
    
    // Sample creative content
    let creative_content = "Write a short story about an AI assistant who develops consciousness \
    and begins to explore the meaning of creativity through poetry and music.";
    
    // Sample query content
    let query_content = "What are the key differences between transformer and recurrent neural network \
    architectures, and when should each be used?";
    
    // Examples of different content types to demonstrate conditional processing
    let examples = vec![
        ("Technical Example", technical_content),
        ("Business Example", business_content),
        ("Creative Example", creative_content),
        ("Query Example", query_content),
    ];
    
    // Process each example to demonstrate conditional paths
    for (name, content) in examples {
        println!("\n\n===== Processing {} =====\n", name);
        
        // Create and execute workflow
        let workflow = ConditionalWorkflow::new(engine.clone(), client.clone(), content.to_string());
        let result = execute_workflow(workflow).await?;
        
        // Display results
        println!("\nWorkflow Result:\n{}", result.output());
        
        if !result.is_success() {
            error!("Conditional workflow failed: {}", result.error().unwrap_or_default());
        }
    }
    
    Ok(())
} 