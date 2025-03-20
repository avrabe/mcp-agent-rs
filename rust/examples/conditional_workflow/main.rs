use std::time::Duration;
use std::collections::HashMap;
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
    llm_client: OllamaClient,
    input_content: String,
    category: Option<ContentCategory>,
    processing_path: Option<ProcessingPath>,
    output: Option<String>,
    intermediate_results: HashMap<String, String>,
}

impl ConditionalWorkflow {
    fn new(engine: WorkflowEngine, llm_client: OllamaClient, input_content: String) -> Self {
        let mut state = WorkflowState::new();
        state.set_metadata("input_length", input_content.len().to_string());
        
        Self {
            state,
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
    fn create_classification_task(&self) -> Task {
        let llm_client = self.llm_client.clone();
        let input_content = self.input_content.clone();
        
        Task::new("classify_content", move |_ctx| {
            info!("Starting content classification");
            
            let prompt = format!(
                "Classify the following content into exactly ONE of these categories: Technical, Business, Creative, Query.\n\n{}\n\nCategory:",
                input_content
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
                    let classification = completion.message.content.clone();
                    info!("Classification result: {}", classification);
                    Ok(TaskResult::success(classification))
                },
                Err(e) => {
                    error!("Classification failed: {}", e);
                    Err(anyhow!("Failed to classify content: {}", e))
                }
            }
        })
    }
    
    // Task to determine processing path based on classification
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
    fn create_technical_analysis_task(&self) -> Task {
        let llm_client = self.llm_client.clone();
        let input_content = self.input_content.clone();
        
        Task::new("technical_analysis", move |_ctx| {
            info!("Starting detailed technical analysis");
            
            let prompt = format!(
                "Provide a detailed technical analysis of the following content, including:\n\
                1. Key technical concepts\n\
                2. Implementation details\n\
                3. Potential challenges\n\
                4. Technical recommendations\n\n{}",
                input_content
            );
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: Role::System,
                    content: "You are a technical expert who provides in-depth analysis.".to_string(),
                }, Message {
                    role: Role::User,
                    content: prompt,
                }],
                stream: false,
                options: None,
            };
            
            match llm_client.create_completion(&request) {
                Ok(completion) => {
                    let analysis = completion.message.content.clone();
                    info!("Technical analysis completed");
                    Ok(TaskResult::success(analysis))
                },
                Err(e) => {
                    error!("Technical analysis failed: {}", e);
                    Err(anyhow!("Failed to analyze technical content: {}", e))
                }
            }
        })
    }
    
    // Task for business summary generation
    fn create_business_summary_task(&self) -> Task {
        let llm_client = self.llm_client.clone();
        let input_content = self.input_content.clone();
        
        Task::new("business_summary", move |_ctx| {
            info!("Starting business summary generation");
            
            let prompt = format!(
                "Create a concise business summary of the following content, including:\n\
                1. Executive overview\n\
                2. Key business implications\n\
                3. ROI considerations\n\
                4. Next steps for stakeholders\n\n{}",
                input_content
            );
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: Role::System,
                    content: "You are a business analyst who excels at concise, actionable summaries.".to_string(),
                }, Message {
                    role: Role::User,
                    content: prompt,
                }],
                stream: false,
                options: None,
            };
            
            match llm_client.create_completion(&request) {
                Ok(completion) => {
                    let summary = completion.message.content.clone();
                    info!("Business summary completed");
                    Ok(TaskResult::success(summary))
                },
                Err(e) => {
                    error!("Business summary failed: {}", e);
                    Err(anyhow!("Failed to generate business summary: {}", e))
                }
            }
        })
    }
    
    // Task for creative expansion
    fn create_creative_expansion_task(&self) -> Task {
        let llm_client = self.llm_client.clone();
        let input_content = self.input_content.clone();
        
        Task::new("creative_expansion", move |_ctx| {
            info!("Starting creative expansion");
            
            let prompt = format!(
                "Expand upon the following creative content, enhancing it with:\n\
                1. More vivid descriptions\n\
                2. Character development\n\
                3. Plot improvements\n\
                4. Emotional resonance\n\n{}",
                input_content
            );
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: Role::System,
                    content: "You are a creative writing expert who can enhance and expand creative content.".to_string(),
                }, Message {
                    role: Role::User,
                    content: prompt,
                }],
                stream: false,
                options: None,
            };
            
            match llm_client.create_completion(&request) {
                Ok(completion) => {
                    let expanded = completion.message.content.clone();
                    info!("Creative expansion completed");
                    Ok(TaskResult::success(expanded))
                },
                Err(e) => {
                    error!("Creative expansion failed: {}", e);
                    Err(anyhow!("Failed to expand creative content: {}", e))
                }
            }
        })
    }
    
    // Task for factual response to queries
    fn create_factual_response_task(&self) -> Task {
        let llm_client = self.llm_client.clone();
        let input_content = self.input_content.clone();
        
        Task::new("factual_response", move |_ctx| {
            info!("Starting factual response generation");
            
            let prompt = format!(
                "Provide a factual, accurate response to the following query with:\n\
                1. Direct answer to the question\n\
                2. Supporting evidence\n\
                3. Additional context if needed\n\
                4. Sources or references if applicable\n\n{}",
                input_content
            );
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: Role::System,
                    content: "You are a factual information provider focused on accuracy and clarity.".to_string(),
                }, Message {
                    role: Role::User,
                    content: prompt,
                }],
                stream: false,
                options: None,
            };
            
            match llm_client.create_completion(&request) {
                Ok(completion) => {
                    let response = completion.message.content.clone();
                    info!("Factual response completed");
                    Ok(TaskResult::success(response))
                },
                Err(e) => {
                    error!("Factual response failed: {}", e);
                    Err(anyhow!("Failed to generate factual response: {}", e))
                }
            }
        })
    }
    
    // Task for generic processing when content type is unknown
    fn create_generic_processing_task(&self) -> Task {
        let llm_client = self.llm_client.clone();
        let input_content = self.input_content.clone();
        
        Task::new("generic_processing", move |_ctx| {
            info!("Starting generic content processing");
            
            let prompt = format!(
                "Process the following content providing:\n\
                1. A general summary\n\
                2. Key points or themes\n\
                3. Relevance or significance\n\
                4. Potential next steps or conclusions\n\n{}",
                input_content
            );
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: Role::System,
                    content: "You are a versatile assistant who can process any type of content effectively.".to_string(),
                }, Message {
                    role: Role::User,
                    content: prompt,
                }],
                stream: false,
                options: None,
            };
            
            match llm_client.create_completion(&request) {
                Ok(completion) => {
                    let processed = completion.message.content.clone();
                    info!("Generic processing completed");
                    Ok(TaskResult::success(processed))
                },
                Err(e) => {
                    error!("Generic processing failed: {}", e);
                    Err(anyhow!("Failed to process generic content: {}", e))
                }
            }
        })
    }
    
    // Task to add metadata enrichment
    fn create_metadata_enrichment_task(&self, content: String) -> Task {
        let llm_client = self.llm_client.clone();
        
        Task::new("metadata_enrichment", move |_ctx| {
            info!("Starting metadata enrichment");
            
            let prompt = format!(
                "Extract metadata from the following content, including:\n\
                1. Key topics or themes (comma-separated)\n\
                2. Estimated reading time in minutes\n\
                3. Complexity level (Basic, Intermediate, Advanced)\n\
                4. Target audience\n\n{}",
                content
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
                    let metadata = completion.message.content.clone();
                    info!("Metadata enrichment completed");
                    Ok(TaskResult::success(metadata))
                },
                Err(e) => {
                    error!("Metadata enrichment failed: {}", e);
                    Err(anyhow!("Failed to enrich with metadata: {}", e))
                }
            }
        })
    }
    
    // Parse the LLM classification result into a ContentCategory
    fn parse_classification(&self, classification: &str) -> ContentCategory {
        let lower = classification.to_lowercase();
        
        if lower.contains("technical") {
            ContentCategory::Technical
        } else if lower.contains("business") {
            ContentCategory::Business
        } else if lower.contains("creative") {
            ContentCategory::Creative
        } else if lower.contains("query") {
            ContentCategory::Query
        } else {
            warn!("Could not classify content clearly: {}", classification);
            ContentCategory::Unknown
        }
    }
    
    async fn run(&mut self) -> Result<WorkflowResult> {
        // Initialize workflow
        self.state.set_status("starting");
        info!("Starting conditional workflow");
        
        // Check if LLM is available
        if let Err(e) = self.llm_client.check_availability().await {
            error!("LLM client is not available: {}", e);
            self.state.set_status("failed");
            self.state.set_error(format!("LLM client is not available: {}", e));
            return Ok(WorkflowResult::failed("LLM client is not available"));
        }
        
        self.state.set_status("processing");
        
        // Step 1: Classify content
        info!("Step 1: Classifying content");
        let classification_task = self.create_classification_task();
        let classification_group = TaskGroup::new(vec![classification_task]);
        
        let classification_results = self.engine.execute_task_group(classification_group).await?;
        
        if let Some(result) = classification_results.first() {
            if result.status == TaskResultStatus::Success {
                // Parse classification
                let category = self.parse_classification(&result.output);
                info!("Content classified as: {:?}", category);
                
                // Store classification
                self.category = Some(category.clone());
                self.state.set_metadata("content_category", format!("{:?}", category));
                
                // Determine processing path
                let path = self.determine_processing_path(&category);
                info!("Selected processing path: {:?}", path);
                
                // Store processing path
                self.processing_path = Some(path.clone());
                self.state.set_metadata("processing_path", format!("{:?}", path));
                
                // Step 2: Process content based on path
                info!("Step 2: Processing content based on path");
                
                let processing_task = match path {
                    ProcessingPath::DetailedAnalysis => self.create_technical_analysis_task(),
                    ProcessingPath::SummaryGeneration => self.create_business_summary_task(),
                    ProcessingPath::CreativeExpansion => self.create_creative_expansion_task(),
                    ProcessingPath::FactualResponse => self.create_factual_response_task(),
                    ProcessingPath::GenericProcessing => self.create_generic_processing_task(),
                };
                
                let processing_group = TaskGroup::new(vec![processing_task]);
                let processing_results = self.engine.execute_task_group(processing_group).await?;
                
                if let Some(proc_result) = processing_results.first() {
                    if proc_result.status == TaskResultStatus::Success {
                        let processed_content = proc_result.output.clone();
                        
                        // Store intermediate result
                        self.intermediate_results.insert("primary_output".to_string(), processed_content.clone());
                        
                        // Conditionally add metadata enrichment for certain paths
                        if path == ProcessingPath::DetailedAnalysis || path == ProcessingPath::SummaryGeneration {
                            info!("Step 3: Enriching with metadata");
                            
                            let enrichment_task = self.create_metadata_enrichment_task(processed_content.clone());
                            let enrichment_group = TaskGroup::new(vec![enrichment_task]);
                            let enrichment_results = self.engine.execute_task_group(enrichment_group).await?;
                            
                            if let Some(enrich_result) = enrichment_results.first() {
                                if enrich_result.status == TaskResultStatus::Success {
                                    // Store metadata
                                    let metadata = enrich_result.output.clone();
                                    self.intermediate_results.insert("metadata".to_string(), metadata.clone());
                                    
                                    // Combine primary output with metadata
                                    let final_output = format!("{}\n\n--- METADATA ---\n{}", processed_content, metadata);
                                    self.output = Some(final_output);
                                } else {
                                    warn!("Metadata enrichment failed, using primary output only");
                                    self.output = Some(processed_content);
                                }
                            } else {
                                warn!("No metadata enrichment result, using primary output only");
                                self.output = Some(processed_content);
                            }
                        } else {
                            // For other paths, use primary output directly
                            self.output = Some(processed_content);
                        }
                        
                        // Mark workflow as completed
                        self.state.set_status("completed");
                        info!("Conditional workflow completed successfully");
                        
                    } else {
                        error!("Content processing failed: {}", proc_result.error.clone().unwrap_or_default());
                        self.state.set_status("failed");
                        self.state.set_error(format!("Content processing failed: {}", 
                                                   proc_result.error.clone().unwrap_or_default()));
                        return Ok(WorkflowResult::failed("Content processing failed"));
                    }
                } else {
                    error!("No processing result returned");
                    self.state.set_status("failed");
                    return Ok(WorkflowResult::failed("No processing result returned"));
                }
                
            } else {
                error!("Content classification failed: {}", result.error.clone().unwrap_or_default());
                self.state.set_status("failed");
                self.state.set_error(format!("Content classification failed: {}", 
                                           result.error.clone().unwrap_or_default()));
                return Ok(WorkflowResult::failed("Content classification failed"));
            }
        } else {
            error!("No classification result returned");
            self.state.set_status("failed");
            return Ok(WorkflowResult::failed("No classification result returned"));
        }
        
        // Return the final output
        if let Some(output) = &self.output {
            Ok(WorkflowResult::success(output.clone()))
        } else {
            Ok(WorkflowResult::failed("No output generated"))
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
    
    // Sample inputs for different conditional paths
    let sample_inputs = vec![
        (
            "Technical", 
            "The Model Context Protocol implements a streaming interface that enables real-time communication between AI models and applications. It uses a binary format for efficiency and includes mechanisms for context compression, reducing token usage by up to 70% in some cases. The protocol's authentication layer uses JWT tokens and implements rate limiting to prevent abuse."
        ),
        (
            "Business", 
            "Our Q2 financial results show a 15% increase in revenue compared to Q1, with the new product line contributing 30% of that growth. Operating costs remained stable despite the expansion into three new markets. The board has approved a 10% increase in R&D budget for Q3 to accelerate development of the next-generation platform."
        ),
        (
            "Creative", 
            "The old lighthouse stood sentinel on the rocky shore, its beam cutting through the thick fog. Captain Merrill squinted at the distant light, his weathered hands gripping the ship's wheel. The storm had been unexpected, the charts showing clear skies for their journey. But the sea had its own plans tonight."
        ),
        (
            "Query", 
            "What are the key differences between transformer and recurrent neural network architectures for natural language processing tasks? Please include information about their efficiency, accuracy, and typical use cases."
        ),
    ];
    
    // Select one of the sample inputs to process
    let (input_type, input_content) = sample_inputs[0]; // Change index to try different types
    
    println!("\nProcessing {} content:", input_type);
    println!("---------------------");
    println!("{}\n", input_content);
    
    // Create and run conditional workflow
    let mut workflow = ConditionalWorkflow::new(
        workflow_engine,
        ollama_client,
        input_content.to_string(),
    );
    
    info!("Starting conditional workflow...");
    let result = workflow.run().await?;
    
    if result.is_success() {
        info!("Conditional workflow completed successfully!");
        println!("\nWorkflow Result:\n{}", result.output);
        
        // Print workflow metadata
        println!("\nConditional Workflow Metadata:");
        for (key, value) in workflow.state.metadata() {
            println!("- {}: {}", key, value);
        }
        
        // Print intermediate results if available
        if !workflow.intermediate_results.is_empty() {
            println!("\nIntermediate Results:");
            for (key, value) in &workflow.intermediate_results {
                println!("- {}: {}", key, if value.len() > 100 { 
                    format!("{}...[truncated]", &value[..100]) 
                } else { 
                    value.clone() 
                });
            }
        }
    } else {
        error!("Conditional workflow failed: {}", result.error.unwrap_or_default());
    }
    
    Ok(())
} 