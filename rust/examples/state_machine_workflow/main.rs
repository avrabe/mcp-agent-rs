use std::time::Duration;
use std::collections::{HashMap, VecDeque};
use anyhow::{anyhow, Result};
use tracing::{info, warn, error, debug};
use chrono::{DateTime, Utc};

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

/// Define states for a document review workflow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ReviewState {
    Draft,              // Initial document creation
    SubmittedForReview, // Document submitted for review
    InReview,           // Document currently being reviewed
    RevisionRequired,   // Changes requested during review
    Approved,           // Document approved
    Published,          // Document published
    Archived,           // Document archived
    Rejected,           // Document rejected
}

impl ReviewState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::SubmittedForReview => "SUBMITTED_FOR_REVIEW",
            Self::InReview => "IN_REVIEW",
            Self::RevisionRequired => "REVISION_REQUIRED",
            Self::Approved => "APPROVED",
            Self::Published => "PUBLISHED",
            Self::Archived => "ARCHIVED",
            Self::Rejected => "REJECTED",
        }
    }
}

/// Define possible transitions between states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ReviewTransition {
    Submit,        // Draft -> SubmittedForReview
    StartReview,   // SubmittedForReview -> InReview
    RequestRevision, // InReview -> RevisionRequired
    Resubmit,      // RevisionRequired -> SubmittedForReview
    Approve,       // InReview -> Approved
    Reject,        // InReview -> Rejected
    Publish,       // Approved -> Published
    Archive,       // Published -> Archived
}

impl ReviewTransition {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Submit => "SUBMIT",
            Self::StartReview => "START_REVIEW",
            Self::RequestRevision => "REQUEST_REVISION",
            Self::Resubmit => "RESUBMIT",
            Self::Approve => "APPROVE",
            Self::Reject => "REJECT",
            Self::Publish => "PUBLISH",
            Self::Archive => "ARCHIVE",
        }
    }
}

/// Validate if a transition is allowed for a given state
fn is_valid_transition(current_state: ReviewState, transition: ReviewTransition) -> bool {
    match (current_state, transition) {
        (ReviewState::Draft, ReviewTransition::Submit) => true,
        (ReviewState::SubmittedForReview, ReviewTransition::StartReview) => true,
        (ReviewState::InReview, ReviewTransition::RequestRevision) => true,
        (ReviewState::InReview, ReviewTransition::Approve) => true,
        (ReviewState::InReview, ReviewTransition::Reject) => true,
        (ReviewState::RevisionRequired, ReviewTransition::Resubmit) => true,
        (ReviewState::Approved, ReviewTransition::Publish) => true,
        (ReviewState::Published, ReviewTransition::Archive) => true,
        _ => false,
    }
}

/// Get the next state after applying a transition
fn next_state_after_transition(current_state: ReviewState, transition: ReviewTransition) -> Option<ReviewState> {
    if !is_valid_transition(current_state, transition) {
        return None;
    }
    
    match (current_state, transition) {
        (ReviewState::Draft, ReviewTransition::Submit) => Some(ReviewState::SubmittedForReview),
        (ReviewState::SubmittedForReview, ReviewTransition::StartReview) => Some(ReviewState::InReview),
        (ReviewState::InReview, ReviewTransition::RequestRevision) => Some(ReviewState::RevisionRequired),
        (ReviewState::InReview, ReviewTransition::Approve) => Some(ReviewState::Approved),
        (ReviewState::InReview, ReviewTransition::Reject) => Some(ReviewState::Rejected),
        (ReviewState::RevisionRequired, ReviewTransition::Resubmit) => Some(ReviewState::SubmittedForReview),
        (ReviewState::Approved, ReviewTransition::Publish) => Some(ReviewState::Published),
        (ReviewState::Published, ReviewTransition::Archive) => Some(ReviewState::Archived),
        _ => None,
    }
}

/// Entry in the state transition audit log
#[derive(Debug, Clone)]
struct StateTransitionEntry {
    timestamp: DateTime<Utc>,
    from_state: ReviewState,
    transition: ReviewTransition,
    to_state: ReviewState,
    comment: String,
    user: String,
}

/// Represents the document under review
#[derive(Debug, Clone)]
struct Document {
    id: String,
    title: String,
    content: String,
    current_state: ReviewState,
    author: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    revision: usize,
}

impl Document {
    fn new(id: String, title: String, content: String, author: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            title,
            content,
            current_state: ReviewState::Draft,
            author,
            created_at: now,
            updated_at: now,
            revision: 1,
        }
    }
    
    fn update_content(&mut self, new_content: String) {
        self.content = new_content;
        self.updated_at = Utc::now();
        self.revision += 1;
    }
}

/// The state machine workflow
struct StateMachineWorkflow {
    state: WorkflowState,
    engine: WorkflowEngine,
    llm_client: OllamaClient,
    document: Document,
    transition_log: Vec<StateTransitionEntry>,
    transition_queue: VecDeque<(ReviewTransition, String, String)>, // (transition, comment, user)
    current_review_comment: Option<String>,
    requires_ai_review: bool,
}

impl StateMachineWorkflow {
    fn new(
        engine: WorkflowEngine,
        llm_client: OllamaClient,
        document: Document,
        requires_ai_review: bool,
    ) -> Self {
        let mut state = WorkflowState::new();
        state.set_metadata("document_id", document.id.clone());
        state.set_metadata("document_title", document.title.clone());
        state.set_metadata("document_state", document.current_state.as_str().to_string());
        
        Self {
            state,
            engine,
            llm_client,
            document,
            transition_log: Vec::new(),
            transition_queue: VecDeque::new(),
            current_review_comment: None,
            requires_ai_review,
        }
    }
    
    /// Queue a transition to be processed
    fn queue_transition(&mut self, transition: ReviewTransition, comment: String, user: String) {
        self.transition_queue.push_back((transition, comment, user));
    }
    
    /// Process a state transition
    fn process_transition(&mut self, transition: ReviewTransition, comment: String, user: String) -> Result<bool> {
        let current_state = self.document.current_state;
        
        // Check if transition is valid
        if !is_valid_transition(current_state, transition) {
            warn!(
                "Invalid transition {:?} from state {:?}", 
                transition, current_state
            );
            return Ok(false);
        }
        
        // Get the next state
        let next_state = next_state_after_transition(current_state, transition)
            .ok_or_else(|| anyhow!("Invalid transition"))?;
        
        // Apply the transition
        info!(
            "Transitioning document {} from {:?} to {:?} via {:?}", 
            self.document.id, current_state, next_state, transition
        );
        
        // Log the transition
        let entry = StateTransitionEntry {
            timestamp: Utc::now(),
            from_state: current_state,
            transition,
            to_state: next_state,
            comment,
            user,
        };
        
        self.transition_log.push(entry);
        
        // Update document state
        self.document.current_state = next_state;
        self.document.updated_at = Utc::now();
        
        // Update workflow state
        self.state.set_metadata("document_state", next_state.as_str().to_string());
        self.state.set_metadata("last_transition", transition.as_str().to_string());
        self.state.set_metadata("transition_count", self.transition_log.len().to_string());
        
        Ok(true)
    }
    
    /// Create a task to perform AI review on the document
    fn create_ai_review_task(&self) -> Task {
        let llm_client = self.llm_client.clone();
        let document_content = self.document.content.clone();
        let document_title = self.document.title.clone();
        
        Task::new("ai_document_review", move |_ctx| {
            info!("Starting AI review for document: {}", document_title);
            
            let prompt = format!(
                "Review the following document for clarity, accuracy, and completeness. \
                Provide specific feedback on areas that need improvement or clarification. \
                If the document is ready for approval, state that explicitly.\n\n\
                DOCUMENT TITLE: {}\n\n\
                DOCUMENT CONTENT:\n{}",
                document_title, document_content
            );
            
            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![Message {
                    role: Role::System,
                    content: "You are a helpful document reviewer with expertise in technical and business documents. Provide constructive feedback.".to_string(),
                }, Message {
                    role: Role::User,
                    content: prompt,
                }],
                stream: false,
                options: None,
            };
            
            match llm_client.create_completion(&request) {
                Ok(completion) => {
                    let review = completion.message.content.clone();
                    info!("AI review completed");
                    Ok(TaskResult::success(review))
                },
                Err(e) => {
                    error!("AI review failed: {}", e);
                    Err(anyhow!("Failed to complete AI review: {}", e))
                }
            }
        })
    }
    
    /// Check if the document needs AI review
    fn needs_ai_review(&self) -> bool {
        self.requires_ai_review && self.document.current_state == ReviewState::InReview
    }
    
    /// Generate a summary of the document's review process
    fn generate_review_summary(&self) -> String {
        let mut summary = String::new();
        
        summary.push_str(&format!("# Review Summary for \"{}\"\n\n", self.document.title));
        summary.push_str(&format!("Document ID: {}\n", self.document.id));
        summary.push_str(&format!("Author: {}\n", self.document.author));
        summary.push_str(&format!("Current State: {:?}\n", self.document.current_state));
        summary.push_str(&format!("Current Revision: {}\n", self.document.revision));
        summary.push_str(&format!("Created: {}\n", self.document.created_at));
        summary.push_str(&format!("Last Updated: {}\n\n", self.document.updated_at));
        
        summary.push_str("## State Transition History\n\n");
        
        for (index, entry) in self.transition_log.iter().enumerate() {
            summary.push_str(&format!(
                "{}. {} - {:?} → {:?} via {:?}\n   User: {}\n   Comment: {}\n\n",
                index + 1,
                entry.timestamp,
                entry.from_state,
                entry.to_state,
                entry.transition,
                entry.user,
                entry.comment
            ));
        }
        
        if let Some(comment) = &self.current_review_comment {
            summary.push_str("## Latest Review Comment\n\n");
            summary.push_str(comment);
            summary.push_str("\n\n");
        }
        
        summary.push_str("## Document Content Preview\n\n");
        let preview_length = 200.min(self.document.content.len());
        summary.push_str(&format!("{}...\n", &self.document.content[..preview_length]));
        
        summary
    }
    
    async fn run(&mut self) -> Result<WorkflowResult> {
        // Initialize workflow
        self.state.set_status("starting");
        info!("Starting state machine workflow for document: {}", self.document.id);
        
        // Check if LLM is available if we need AI review
        if self.requires_ai_review {
            if let Err(e) = self.llm_client.check_availability().await {
                error!("LLM client is not available for AI review: {}", e);
                self.state.set_metadata("ai_review_available", "false");
                self.requires_ai_review = false;
                // We continue without AI review capability
                warn!("Continuing without AI review capability");
            } else {
                self.state.set_metadata("ai_review_available", "true");
            }
        }
        
        self.state.set_status("processing");
        
        // Simulate document workflow
        // In a real application, you would integrate with actual user actions
        // Here we'll set up a sequence of transitions to demonstrate the state machine
        
        // Initial submission
        self.queue_transition(
            ReviewTransition::Submit,
            "Initial document submission".to_string(),
            "John Author".to_string()
        );
        
        // Start review process
        self.queue_transition(
            ReviewTransition::StartReview,
            "Starting formal review process".to_string(),
            "Sarah Reviewer".to_string()
        );
        
        // Process each transition in order
        while let Some((transition, comment, user)) = self.transition_queue.pop_front() {
            info!("Processing transition {:?} for document {}", transition, self.document.id);
            
            match self.process_transition(transition, comment, user) {
                Ok(true) => {
                    info!("Transition successful, new state: {:?}", self.document.current_state);
                    
                    // If we're now in review state and AI review is required, do it
                    if self.needs_ai_review() {
                        info!("Performing AI review for document {}", self.document.id);
                        
                        let review_task = self.create_ai_review_task();
                        let task_group = TaskGroup::new(vec![review_task]);
                        
                        match self.engine.execute_task_group(task_group).await {
                            Ok(results) => {
                                if let Some(result) = results.first() {
                                    if result.status == TaskResultStatus::Success {
                                        let review_comment = result.output.clone();
                                        info!("AI review completed");
                                        
                                        // Store the review comment
                                        self.current_review_comment = Some(review_comment.clone());
                                        
                                        // Based on AI review, decide next transition
                                        if review_comment.to_lowercase().contains("ready for approval") {
                                            self.queue_transition(
                                                ReviewTransition::Approve,
                                                format!("AI Review: {}", review_comment),
                                                "AI Reviewer".to_string()
                                            );
                                        } else {
                                            self.queue_transition(
                                                ReviewTransition::RequestRevision,
                                                format!("AI Review: {}", review_comment),
                                                "AI Reviewer".to_string()
                                            );
                                        }
                                    } else {
                                        warn!("AI review task failed");
                                        // If AI review fails, queue a manual review request
                                        self.queue_transition(
                                            ReviewTransition::RequestRevision,
                                            "AI review failed, manual review required".to_string(),
                                            "System".to_string()
                                        );
                                    }
                                }
                            },
                            Err(e) => {
                                error!("Failed to execute AI review: {}", e);
                                // Handle AI review failure
                                self.queue_transition(
                                    ReviewTransition::RequestRevision,
                                    format!("AI review failed: {}", e),
                                    "System".to_string()
                                );
                            }
                        }
                    }
                    
                    // For demo purposes: if we're in revision required, simulate a resubmission
                    if self.document.current_state == ReviewState::RevisionRequired {
                        // Update document content to simulate revision
                        self.document.update_content(format!(
                            "{}\n\n[REVISED] Addressed reviewer comments in revision {}",
                            self.document.content, 
                            self.document.revision
                        ));
                        
                        // Queue resubmission
                        self.queue_transition(
                            ReviewTransition::Resubmit,
                            "Addressed review comments".to_string(),
                            self.document.author.clone()
                        );
                    }
                    
                    // For demo purposes: if we're approved, proceed to publishing
                    if self.document.current_state == ReviewState::Approved {
                        self.queue_transition(
                            ReviewTransition::Publish,
                            "Document approved and ready for publishing".to_string(),
                            "Maria Publisher".to_string()
                        );
                    }
                    
                    // For demo purposes: if we're published, archive after some time
                    if self.document.current_state == ReviewState::Published {
                        self.queue_transition(
                            ReviewTransition::Archive,
                            "Archiving document after publication period".to_string(),
                            "System".to_string()
                        );
                    }
                },
                Ok(false) => {
                    warn!("Invalid transition {:?} from state {:?}", 
                         transition, self.document.current_state);
                },
                Err(e) => {
                    error!("Error processing transition: {}", e);
                    self.state.set_error(format!("Transition error: {}", e));
                    return Ok(WorkflowResult::failed(&format!("Workflow error: {}", e)));
                }
            }
            
            // Simulate time passing between transitions
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        
        // Completed all transitions
        self.state.set_status("completed");
        
        // Generate summary report
        let summary = self.generate_review_summary();
        
        Ok(WorkflowResult::success(summary))
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
    
    // Create workflow engine with signal handling
    let signal_handler = SignalHandler::new(vec![WorkflowSignal::Interrupt, WorkflowSignal::Terminate]);
    let workflow_engine = WorkflowEngine::new(signal_handler);
    
    // Sample document to process
    let document = Document::new(
        "DOC-2023-05-15-001".to_string(),
        "MCP Protocol Implementation Guide".to_string(),
        "# Model Context Protocol Implementation Guide\n\n\
        This document outlines the implementation steps for integrating the Model Context Protocol (MCP) \
        into existing AI applications. The protocol standardizes context management for large language models.\n\n\
        ## Key Components\n\n\
        1. Context Window Management\n\
        2. Token Compression Techniques\n\
        3. Semantic Routing\n\
        4. Security Considerations\n\n\
        ## Integration Steps\n\n\
        Begin by establishing the connection handlers as described in section 3.2 of the specification...\
        ".to_string(),
        "John Author".to_string()
    );
    
    // Create and run state machine workflow with AI review
    let mut workflow = StateMachineWorkflow::new(
        workflow_engine,
        ollama_client,
        document,
        true // Enable AI review
    );
    
    info!("Starting document review state machine workflow...");
    let result = workflow.run().await?;
    
    if result.is_success() {
        info!("State machine workflow completed successfully!");
        println!("\nWorkflow Result:\n{}", result.output);
        
        // Print workflow metadata
        println!("\nState Machine Workflow Metadata:");
        for (key, value) in workflow.state.metadata() {
            println!("- {}: {}", key, value);
        }
    } else {
        error!("State machine workflow failed: {}", result.error.unwrap_or_default());
    }
    
    Ok(())
} 