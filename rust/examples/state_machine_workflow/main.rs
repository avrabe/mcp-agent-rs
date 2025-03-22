use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::json;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

use mcp_agent::llm::types::LlmClient;
use mcp_agent::telemetry::{init_telemetry, TelemetryConfig};
use mcp_agent::workflow::{
    execute_workflow, task, AsyncSignalHandler, Workflow, WorkflowEngine, WorkflowResult,
    WorkflowSignal, WorkflowState,
};
use mcp_agent::{Completion, CompletionRequest, LlmConfig, LlmMessage as Message, MessageRole};

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
    Submit,          // Draft -> SubmittedForReview
    StartReview,     // SubmittedForReview -> InReview
    RequestRevision, // InReview -> RevisionRequired
    Resubmit,        // RevisionRequired -> SubmittedForReview
    Approve,         // InReview -> Approved
    Reject,          // InReview -> Rejected
    Publish,         // Approved -> Published
    Archive,         // Published -> Archived
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
fn next_state_after_transition(
    current_state: ReviewState,
    transition: ReviewTransition,
) -> Option<ReviewState> {
    if !is_valid_transition(current_state, transition) {
        return None;
    }

    match (current_state, transition) {
        (ReviewState::Draft, ReviewTransition::Submit) => Some(ReviewState::SubmittedForReview),
        (ReviewState::SubmittedForReview, ReviewTransition::StartReview) => {
            Some(ReviewState::InReview)
        }
        (ReviewState::InReview, ReviewTransition::RequestRevision) => {
            Some(ReviewState::RevisionRequired)
        }
        (ReviewState::InReview, ReviewTransition::Approve) => Some(ReviewState::Approved),
        (ReviewState::InReview, ReviewTransition::Reject) => Some(ReviewState::Rejected),
        (ReviewState::RevisionRequired, ReviewTransition::Resubmit) => {
            Some(ReviewState::SubmittedForReview)
        }
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
    llm_client: Arc<MockLlmClient>,
    document: Document,
    transition_log: Vec<StateTransitionEntry>,
    transition_queue: VecDeque<(ReviewTransition, String, String)>, // (transition, comment, user)
    current_review_comment: Option<String>,
    requires_ai_review: bool,
}

impl StateMachineWorkflow {
    fn new(
        engine: WorkflowEngine,
        llm_client: Arc<MockLlmClient>,
        document: Document,
        requires_ai_review: bool,
    ) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("document_id".to_string(), json!(document.id.clone()));
        metadata.insert("document_title".to_string(), json!(document.title.clone()));
        metadata.insert(
            "document_state".to_string(),
            json!(document.current_state.as_str().to_string()),
        );

        Self {
            state: WorkflowState::new(Some("StateMachineWorkflow".to_string()), Some(metadata)),
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
    fn process_transition(
        &mut self,
        transition: ReviewTransition,
        comment: String,
        user: String,
    ) -> Result<bool> {
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
        self.state
            .set_metadata("document_state", json!(next_state.as_str().to_string()));
        self.state
            .set_metadata("last_transition", json!(transition.as_str().to_string()));
        self.state
            .set_metadata("transition_count", json!(self.transition_log.len()));

        Ok(true)
    }

    /// Create a task to perform AI review on the document
    fn create_ai_review_task(&self) -> task::WorkflowTask<String> {
        let llm_client = self.llm_client.clone();
        let document_content = self.document.content.clone();
        let document_title = self.document.title.clone();

        task::task("ai_document_review", move || async move {
            info!("Starting AI review for document: {}", document_title);

            let prompt = format!(
                "Review the following document for clarity, accuracy, and completeness. \
                Provide constructive feedback and suggestions for improvement. \
                If there are any issues, clearly state what needs to be revised.\n\n\
                DOCUMENT TITLE: {}\n\n\
                DOCUMENT CONTENT:\n{}\n\n\
                Please provide your review:",
                document_title, document_content
            );

            let request = CompletionRequest {
                model: "llama2".to_string(),
                messages: vec![
                    Message {
                        role: MessageRole::System,
                        content:
                            "You are an expert document reviewer providing constructive feedback."
                                .to_string(),
                        metadata: None,
                    },
                    Message {
                        role: MessageRole::User,
                        content: prompt,
                        metadata: None,
                    },
                ],
                max_tokens: Some(1000),
                temperature: Some(0.7),
                top_p: None,
                parameters: HashMap::new(),
            };

            match llm_client.complete(request).await {
                Ok(completion) => {
                    let review = completion.content;
                    info!("AI review completed");
                    Ok(review)
                }
                Err(e) => {
                    error!("AI review failed: {}", e);
                    Err(anyhow!("Failed to generate AI review: {}", e))
                }
            }
        })
    }

    /// Determine if AI review is needed
    fn needs_ai_review(&self) -> bool {
        self.requires_ai_review && self.document.current_state == ReviewState::InReview
    }

    /// Generate a summary of the document's state and transition history
    fn generate_review_summary(&self) -> String {
        let mut summary = format!(
            "Document Review Summary\n\
            =======================\n\
            Title: {}\n\
            ID: {}\n\
            Author: {}\n\
            Current State: {:?}\n\
            Revision: {}\n\
            Created: {}\n\
            Last Updated: {}\n\n\
            Transition History:\n\
            ------------------\n",
            self.document.title,
            self.document.id,
            self.document.author,
            self.document.current_state,
            self.document.revision,
            self.document.created_at.format("%Y-%m-%d %H:%M:%S"),
            self.document.updated_at.format("%Y-%m-%d %H:%M:%S"),
        );

        for (i, entry) in self.transition_log.iter().enumerate() {
            summary.push_str(&format!(
                "{}. {} | {:?} -> {:?} | Transition: {:?} | User: {} | Comment: {}\n",
                i + 1,
                entry.timestamp.format("%Y-%m-%d %H:%M:%S"),
                entry.from_state,
                entry.to_state,
                entry.transition,
                entry.user,
                entry.comment,
            ));
        }

        if let Some(review) = &self.current_review_comment {
            summary.push_str("\nLatest Review Comments:\n");
            summary.push_str("---------------------\n");
            summary.push_str(review);
        }

        summary
    }
}

#[async_trait]
impl Workflow for StateMachineWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        let mut result = WorkflowResult::new();
        result.start();

        // Check if LLM client is available when AI review is required
        if self.requires_ai_review && !self.llm_client.is_available().await? {
            self.state
                .set_error("LLM service is not available for AI review");
            return Ok(WorkflowResult::failed(
                "LLM service is not available for AI review",
            ));
        }

        self.state.set_status("starting");
        info!(
            "Starting state machine workflow for document: {}",
            self.document.title
        );

        // Simulate document workflow with predefined transitions
        if self.document.current_state == ReviewState::Draft {
            // Automatically submit draft for review
            if self.process_transition(
                ReviewTransition::Submit,
                "Initial submission for review".to_string(),
                "system".to_string(),
            )? {
                info!("Document submitted for review");
            }
        }

        // Process the transitions in the queue
        while let Some((transition, comment, user)) = self.transition_queue.pop_front() {
            self.state.set_status("processing_transition");

            // Apply the transition
            if self.process_transition(transition, comment, user)? {
                // If document moves to InReview and AI review is required, perform AI review
                if self.document.current_state == ReviewState::InReview && self.needs_ai_review() {
                    self.state.set_status("ai_review");
                    info!("Performing AI review on document");

                    let review_task = self.create_ai_review_task();
                    let review_result = self.engine.execute_task(review_task).await?;

                    // Save the review comment
                    self.current_review_comment = Some(review_result.clone());
                    self.state.set_metadata("ai_review_complete", json!(true));

                    // For demo purposes, automatically request revision based on AI review
                    if review_result.contains("revise") || review_result.contains("improve") {
                        if self.process_transition(
                            ReviewTransition::RequestRevision,
                            format!("AI Review suggested revisions: {}", review_result),
                            "ai_reviewer".to_string(),
                        )? {
                            info!("AI review requested revisions");
                        }
                    } else if self.process_transition(
                        ReviewTransition::Approve,
                        format!("AI Review approved: {}", review_result),
                        "ai_reviewer".to_string(),
                    )? {
                        info!("AI review approved document");
                    }
                }
            } else {
                warn!("Failed to process transition: {:?}", transition);
            }
        }

        // Update final workflow state
        self.state.set_status("completed");

        // Generate summary of the document's review process
        let summary = self.generate_review_summary();

        // Return the workflow result
        result.value = Some(json!({
            "document_id": self.document.id,
            "title": self.document.title,
            "final_state": self.document.current_state.as_str(),
            "revision": self.document.revision,
            "transition_count": self.transition_log.len(),
            "summary": summary
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

/// Mock LLM client for document review
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
                max_tokens: Some(1000),
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

        // Extract the document content from the prompt
        let prompt = if let Some(user_message) = request
            .messages
            .iter()
            .find(|m| matches!(m.role, MessageRole::User))
        {
            &user_message.content
        } else {
            "Unknown document"
        };

        // Generate different reviews based on content patterns
        let response = if prompt.contains("error")
            || prompt.contains("bug")
            || prompt.contains("issue")
        {
            "This document needs revision. There are several errors and issues that should be addressed:\n\
            1. The technical specifications are unclear in section 2.\n\
            2. There are inconsistencies between the requirements and implementation sections.\n\
            3. Please revise the conclusion to better summarize the key points.\n\
            Overall, the document requires significant improvement before it can be approved."
        } else if prompt.contains("draft") || prompt.contains("initial") {
            "This document is a good starting point, but needs some improvements:\n\
            1. The introduction could be more engaging.\n\
            2. Consider adding more specific examples in the middle section.\n\
            3. The formatting could be more consistent throughout.\n\
            Please revise these points before final approval."
        } else {
            "This document meets all requirements and is well-structured. The content is clear,\
            accurate, and comprehensive. The arguments are well-supported and the conclusions follow\
            logically from the presented information. I recommend approval without further revisions."
        };

        let prompt_len = prompt.len();
        let response_len = response.len();

        Ok(Completion {
            content: response.to_string(),
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
        service_name: "state_machine_workflow".to_string(),
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

    // Create a sample document
    let document = Document::new(
        "DOC-2023-001".to_string(),
        "API Design Specification".to_string(),
        "This document outlines the design specification for the Model Context Protocol API.\n\
        The API provides a standardized interface for model-context interactions and supports\n\
        streaming tokens, tool invocation, and state management.\n\n\
        Key components include:\n\
        1. Context management\n\
        2. Token streaming\n\
        3. Tool usage protocols\n\
        4. State persistence\n\n\
        Implementation details will be provided in subsequent sections."
            .to_string(),
        "Jane Smith".to_string(),
    );

    // Create the workflow
    let mut workflow = StateMachineWorkflow::new(
        engine.clone(),
        llm_client.clone(),
        document,
        true, // Enable AI review
    );

    // Queue up some transitions to simulate workflow
    workflow.queue_transition(
        ReviewTransition::StartReview,
        "Beginning formal review process".to_string(),
        "john.reviewer".to_string(),
    );

    // Run the workflow
    info!("Starting document review workflow...");
    let result = execute_workflow(workflow).await?;

    if result.is_success() {
        println!("\n{}", result.output());
    } else {
        error!("Workflow failed: {}", result.error().unwrap_or_default());
    }

    // Create another document with a different scenario
    let document2 = Document::new(
        "DOC-2023-002".to_string(),
        "System Architecture with Known Issues".to_string(),
        "This document describes the system architecture with several known issues and bugs.\n\
        There are error conditions that need to be addressed in the authentication module.\n\
        The current implementation has performance issues under high load conditions.\n\
        Further testing is required before final approval."
            .to_string(),
        "Bob Johnson".to_string(),
    );

    // Create a second workflow with a different path
    let mut workflow2 = StateMachineWorkflow::new(
        engine, llm_client, document2, true, // Enable AI review
    );

    // Queue up transitions for the second workflow
    workflow2.queue_transition(
        ReviewTransition::Submit,
        "Initial submission for architecture review".to_string(),
        "bob.johnson".to_string(),
    );

    workflow2.queue_transition(
        ReviewTransition::StartReview,
        "Starting architecture review".to_string(),
        "alice.architect".to_string(),
    );

    // Run the second workflow
    info!("Starting second document review workflow...");
    let result2 = execute_workflow(workflow2).await?;

    if result2.is_success() {
        println!("\n{}", result2.output());
    } else {
        error!(
            "Second workflow failed: {}",
            result2.error().unwrap_or_default()
        );
    }

    Ok(())
}
