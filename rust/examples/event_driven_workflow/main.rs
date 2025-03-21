use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use rand::Rng;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};
use uuid::Uuid;

use mcp_agent::telemetry::{TelemetryConfig, init_telemetry};
use mcp_agent::workflow::{
    WorkflowEngine, WorkflowResult, signal::AsyncSignalHandler, state::WorkflowState, task::task,
};

/// An event that can be processed by the workflow
#[derive(Debug, Clone)]
struct Event {
    id: String,
    event_type: EventType,
    payload: String,
    timestamp: DateTime<Utc>,
    source: String,
    priority: EventPriority,
    processed: bool,
}

impl Event {
    /// Creates a new event
    fn new(event_type: EventType, source: &str, payload: &str, priority: EventPriority) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            event_type,
            payload: payload.to_string(),
            timestamp: Utc::now(),
            source: source.to_string(),
            priority,
            processed: false,
        }
    }

    /// Creates a user action event
    fn user_action(action_type: &str, payload: &str) -> Self {
        Self::new(
            EventType::UserAction,
            action_type,
            payload,
            EventPriority::Medium,
        )
    }

    /// Creates a system alert event
    fn system_alert(alert_type: &str, payload: &str) -> Self {
        Self::new(
            EventType::SystemAlert,
            alert_type,
            payload,
            EventPriority::High,
        )
    }
}

/// Types of events the workflow can handle
#[derive(Debug, Clone, PartialEq, Eq)]
enum EventType {
    UserAction,
    SystemAlert,
    DataUpdate,
    ExternalApi,
    TimerTrigger,
    Error,
}

impl EventType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::UserAction => "USER_ACTION",
            Self::SystemAlert => "SYSTEM_ALERT",
            Self::DataUpdate => "DATA_UPDATE",
            Self::ExternalApi => "EXTERNAL_API",
            Self::TimerTrigger => "TIMER_TRIGGER",
            Self::Error => "ERROR",
        }
    }
}

/// Priority levels for events
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EventPriority {
    Low = 0,
    Medium = 1,
    High = 2,
    Critical = 3,
}

/// Result of event processing
#[derive(Debug, Clone)]
struct EventProcessingResult {
    event_id: String,
    success: bool,
    handler_name: String,
    result: Option<String>,
    error: Option<String>,
    processing_time_ms: u64,
}

/// The event-driven workflow
struct EventDrivenWorkflow {
    /// Workflow state
    state: Arc<Mutex<WorkflowState>>,

    /// Workflow engine
    engine: WorkflowEngine,

    /// Queue of events to process
    event_queue: Arc<Mutex<VecDeque<Event>>>,

    /// Results of event processing
    processing_results: Arc<Mutex<Vec<EventProcessingResult>>>,
}

impl EventDrivenWorkflow {
    /// Create a new event-driven workflow
    pub fn new(engine: WorkflowEngine) -> Self {
        let state = WorkflowState::new(Some("event_driven_workflow".to_string()), None);

        Self {
            state: Arc::new(Mutex::new(state)),
            engine,
            event_queue: Arc::new(Mutex::new(VecDeque::new())),
            processing_results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Submit an event to the workflow
    pub async fn submit_event(&self, event: Event) -> Result<()> {
        info!("Submitting event: {}", event.id);
        let mut queue = self.event_queue.lock().await;
        queue.push_back(event);
        Ok(())
    }

    /// Process a specific event
    async fn process_event(&self, event: Event) -> Result<String> {
        info!("Processing event: {}", event.id);

        // Create a task to process the event
        let event_id = event.id.clone();
        let event_type = event.event_type.clone();
        let payload = event.payload.clone();

        let task = task(&format!("process_event_{}", event_id), move || async move {
            info!("Processing event {} of type {:?}", event_id, event_type);

            // Simulate processing based on event type
            match event_type {
                EventType::UserAction => {
                    info!("Handling user action: {}", payload);
                    // Simulate successful processing
                    Ok(format!("Processed user action: {}", payload))
                }
                EventType::SystemAlert => {
                    info!("Handling system alert: {}", payload);
                    // Randomly fail processing for some system alerts
                    if rand::thread_rng().gen_ratio(1, 4) {
                        error!("Failed to process system alert: {}", event_id);
                        return Err(anyhow!("System alert processing failed"));
                    }
                    Ok(format!("Processed system alert: {}", payload))
                }
                EventType::DataUpdate => {
                    info!("Handling data update: {}", payload);
                    Ok(format!("Processed data update: {}", payload))
                }
                EventType::ExternalApi => {
                    info!("Handling external API event: {}", payload);
                    Ok(format!("Processed external API event: {}", payload))
                }
                EventType::TimerTrigger => {
                    info!("Handling timer trigger: {}", payload);
                    Ok(format!("Processed timer trigger: {}", payload))
                }
                EventType::Error => {
                    error!("Received error event: {}", payload);
                    Err(anyhow!("Error event: {}", payload))
                }
            }
        });

        // Execute the task using the workflow engine
        match self.engine.execute_task(task).await {
            Ok(result) => {
                info!("Successfully processed event: {}", event.id);

                // Record the result
                let mut results = self.processing_results.lock().await;
                results.push(EventProcessingResult {
                    event_id: event.id.clone(),
                    success: true,
                    handler_name: format!("handler_{}", event.event_type.as_str()),
                    result: Some(result.clone()),
                    error: None,
                    processing_time_ms: 0, // In a real implementation, we would track this
                });

                Ok(result)
            }
            Err(e) => {
                error!("Failed to process event: {}, error: {}", event.id, e);

                // Record the failure
                let mut results = self.processing_results.lock().await;
                results.push(EventProcessingResult {
                    event_id: event.id.clone(),
                    success: false,
                    handler_name: format!("handler_{}", event.event_type.as_str()),
                    result: None,
                    error: Some(e.to_string()),
                    processing_time_ms: 0,
                });

                Err(e)
            }
        }
    }

    /// Run the workflow by processing all events in the queue
    pub async fn run(&self) -> Result<WorkflowResult> {
        info!("Starting event-driven workflow");
        {
            let mut state = self.state.lock().await;
            state.update_status("running");
        }

        // Process events in the queue
        let mut queue = self.event_queue.lock().await;
        let events: Vec<Event> = queue.drain(..).collect();
        drop(queue);

        let mut successful_events = 0;
        let mut failed_events = 0;

        for event in events {
            match self.process_event(event).await {
                Ok(_) => successful_events += 1,
                Err(_) => failed_events += 1,
            }
        }

        // Generate summary report
        let results = self.processing_results.lock().await;
        let summary = serde_json::json!({
            "total_events": results.len(),
            "succeeded": successful_events,
            "failed": failed_events,
            "details": results.iter().map(|r| {
                serde_json::json!({
                    "event_id": r.event_id,
                    "success": r.success,
                    "handler": r.handler_name,
                    "error": r.error,
                })
            }).collect::<Vec<_>>(),
        });

        // Update status and return result
        if failed_events > 0 {
            {
                let mut state = self.state.lock().await;
                state.update_status("completed_with_errors");
            }
            Ok(WorkflowResult::failed(format!(
                "{} events failed processing",
                failed_events
            )))
        } else {
            {
                let mut state = self.state.lock().await;
                state.update_status("completed");
            }
            Ok(WorkflowResult::success(summary))
        }
    }
}

/// Run an example event-driven workflow
async fn run_event_driven_workflow() -> Result<()> {
    // Initialize telemetry
    let telemetry_config = TelemetryConfig::default();
    let _guard = init_telemetry(telemetry_config);

    // Create signal handler and workflow engine
    let signal_handler = AsyncSignalHandler::new();
    let engine = WorkflowEngine::new(signal_handler);

    // Create workflow
    let workflow = EventDrivenWorkflow::new(engine);

    // Submit events
    workflow
        .submit_event(Event::user_action("login", "user123 logged in"))
        .await?;
    workflow
        .submit_event(Event::system_alert("low_memory", "System memory below 10%"))
        .await?;
    workflow
        .submit_event(Event::user_action("logout", "user456 logged out"))
        .await?;
    workflow
        .submit_event(Event::system_alert("high_cpu", "CPU usage at 90%"))
        .await?;

    // Run the workflow
    let result = workflow.run().await?;

    // Print the result
    if result.is_success() {
        println!("\nWorkflow completed successfully!");
        println!(
            "{}",
            serde_json::to_string_pretty(&result.output()).unwrap()
        );
    } else {
        println!("\nWorkflow completed with errors!");
        println!("Error: {}", result.error().unwrap_or_default());
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    run_event_driven_workflow().await
}
