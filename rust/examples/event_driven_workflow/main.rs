use std::time::Duration;
use std::collections::{HashMap, VecDeque};
use anyhow::{anyhow, Result, Context};
use tracing::{info, warn, error, debug, Instrument};
use chrono::{DateTime, Utc};
use tokio::sync::{mpsc, Mutex, RwLock};
use std::sync::Arc;
use futures::future::join_all;
use rand::Rng;

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

impl EventPriority {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Medium => "MEDIUM",
            Self::High => "HIGH",
            Self::Critical => "CRITICAL",
        }
    }
}

/// A handler for processing specific event types
trait EventHandler: Send + Sync {
    fn can_handle(&self, event_type: &EventType) -> bool;
    fn handle_event(&self, event: Event) -> Result<TaskResult>;
    fn name(&self) -> &str;
}

/// Handler for user action events
struct UserActionHandler {
    name: String,
}

impl UserActionHandler {
    fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

impl EventHandler for UserActionHandler {
    fn can_handle(&self, event_type: &EventType) -> bool {
        event_type == &EventType::UserAction
    }
    
    fn handle_event(&self, event: Event) -> Result<TaskResult> {
        info!("UserActionHandler processing event: {}", event.id);
        // Simulate processing a user action
        let response = format!("Processed user action: {}", event.payload);
        Ok(TaskResult::success(response))
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Handler for system alert events
struct SystemAlertHandler {
    name: String,
}

impl SystemAlertHandler {
    fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

impl EventHandler for SystemAlertHandler {
    fn can_handle(&self, event_type: &EventType) -> bool {
        event_type == &EventType::SystemAlert
    }
    
    fn handle_event(&self, event: Event) -> Result<TaskResult> {
        info!("SystemAlertHandler processing event: {}", event.id);
        // Simulate processing a system alert
        
        // For demo purposes, randomly fail some critical alerts
        if event.priority == EventPriority::Critical && rand::thread_rng().gen_bool(0.3) {
            error!("Failed to process critical system alert: {}", event.id);
            return Err(anyhow!("Critical alert processing failed"));
        }
        
        let response = format!("Processed system alert: {}", event.payload);
        Ok(TaskResult::success(response))
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Handler for data update events
struct DataUpdateHandler {
    name: String,
}

impl DataUpdateHandler {
    fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

impl EventHandler for DataUpdateHandler {
    fn can_handle(&self, event_type: &EventType) -> bool {
        event_type == &EventType::DataUpdate
    }
    
    fn handle_event(&self, event: Event) -> Result<TaskResult> {
        info!("DataUpdateHandler processing event: {}", event.id);
        // Simulate processing a data update
        let response = format!("Processed data update: {}", event.payload);
        Ok(TaskResult::success(response))
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Handler for external API events
struct ExternalApiHandler {
    name: String,
}

impl ExternalApiHandler {
    fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

impl EventHandler for ExternalApiHandler {
    fn can_handle(&self, event_type: &EventType) -> bool {
        event_type == &EventType::ExternalApi
    }
    
    fn handle_event(&self, event: Event) -> Result<TaskResult> {
        info!("ExternalApiHandler processing event: {}", event.id);
        // Simulate processing an external API event
        let response = format!("Processed external API event: {}", event.payload);
        Ok(TaskResult::success(response))
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Handler for timer trigger events
struct TimerTriggerHandler {
    name: String,
}

impl TimerTriggerHandler {
    fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

impl EventHandler for TimerTriggerHandler {
    fn can_handle(&self, event_type: &EventType) -> bool {
        event_type == &EventType::TimerTrigger
    }
    
    fn handle_event(&self, event: Event) -> Result<TaskResult> {
        info!("TimerTriggerHandler processing event: {}", event.id);
        // Simulate processing a timer trigger
        let response = format!("Processed timer trigger: {}", event.payload);
        Ok(TaskResult::success(response))
    }
    
    fn name(&self) -> &str {
        &self.name
    }
}

/// Handler for error events
struct ErrorHandler {
    name: String,
}

impl ErrorHandler {
    fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

impl EventHandler for ErrorHandler {
    fn can_handle(&self, event_type: &EventType) -> bool {
        event_type == &EventType::Error
    }
    
    fn handle_event(&self, event: Event) -> Result<TaskResult> {
        info!("ErrorHandler processing event: {}", event.id);
        // Simulate processing an error event
        let response = format!("Processed error event: {}", event.payload);
        Ok(TaskResult::success(response))
    }
    
    fn name(&self) -> &str {
        &self.name
    }
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
    state: WorkflowState,
    engine: WorkflowEngine,
    llm_client: OllamaClient,
    event_queue: Arc<Mutex<VecDeque<Event>>>,
    event_handlers: Vec<Box<dyn EventHandler>>,
    processing_results: Arc<Mutex<Vec<EventProcessingResult>>>,
    event_sender: mpsc::Sender<Event>,
    event_receiver: mpsc::Receiver<Event>,
    max_concurrent_events: usize,
    running: Arc<RwLock<bool>>,
}

impl EventDrivenWorkflow {
    fn new(
        engine: WorkflowEngine,
        llm_client: OllamaClient,
        max_concurrent_events: usize,
    ) -> Self {
        let (tx, rx) = mpsc::channel(100); // Channel capacity of 100 events
        
        let mut state = WorkflowState::new();
        state.set_metadata("workflow_type", "event_driven".to_string());
        state.set_metadata("max_concurrent_events", max_concurrent_events.to_string());
        
        Self {
            state,
            engine,
            llm_client,
            event_queue: Arc::new(Mutex::new(VecDeque::new())),
            event_handlers: Vec::new(),
            processing_results: Arc::new(Mutex::new(Vec::new())),
            event_sender: tx,
            event_receiver: rx,
            max_concurrent_events,
            running: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Register an event handler
    fn register_handler(&mut self, handler: Box<dyn EventHandler>) {
        info!("Registering handler: {}", handler.name());
        self.event_handlers.push(handler);
    }
    
    /// Get an event sender that can be used to submit events to the workflow
    fn get_event_sender(&self) -> mpsc::Sender<Event> {
        self.event_sender.clone()
    }
    
    /// Creates a new event with the given parameters
    fn create_event(
        event_type: EventType,
        payload: &str,
        source: &str,
        priority: EventPriority,
    ) -> Event {
        Event {
            id: format!("evt-{}", uuid::Uuid::new_v4()),
            event_type,
            payload: payload.to_string(),
            timestamp: Utc::now(),
            source: source.to_string(),
            priority,
            processed: false,
        }
    }
    
    /// Find appropriate handler for an event
    fn find_handler_for_event(&self, event: &Event) -> Option<&Box<dyn EventHandler>> {
        self.event_handlers
            .iter()
            .find(|handler| handler.can_handle(&event.event_type))
    }
    
    /// Create a task to process an event
    fn create_event_processing_task(&self, event: Event) -> Task {
        let event_clone = event.clone();
        let handlers = self.event_handlers.clone();
        
        Task::new(&format!("process_event_{}", event.id), move |_ctx| {
            // Find a handler for this event type
            let handler = handlers
                .iter()
                .find(|h| h.can_handle(&event_clone.event_type))
                .ok_or_else(|| anyhow!("No handler found for event type: {:?}", event_clone.event_type))?;
            
            // Process the event with the appropriate handler
            let start_time = std::time::Instant::now();
            let result = handler.handle_event(event_clone.clone());
            let processing_time = start_time.elapsed().as_millis() as u64;
            
            // Return the result
            match result {
                Ok(task_result) => {
                    // Include processing metadata
                    let mut result_with_metadata = task_result;
                    result_with_metadata.metadata.insert(
                        "processing_time_ms".to_string(), 
                        processing_time.to_string()
                    );
                    result_with_metadata.metadata.insert(
                        "handler".to_string(), 
                        handler.name().to_string()
                    );
                    result_with_metadata.metadata.insert(
                        "event_id".to_string(), 
                        event_clone.id
                    );
                    result_with_metadata.metadata.insert(
                        "event_type".to_string(), 
                        event_clone.event_type.as_str().to_string()
                    );
                    result_with_metadata.metadata.insert(
                        "priority".to_string(), 
                        event_clone.priority.as_str().to_string()
                    );
                    
                    Ok(result_with_metadata)
                },
                Err(e) => Err(e),
            }
        })
    }
    
    /// Process events from the queue
    async fn process_events(&self) {
        let running = self.running.clone();
        
        while *running.read().await {
            // Process events in batches up to max_concurrent_events
            let mut events_to_process = Vec::new();
            
            // Take events from the queue based on priority
            {
                let mut queue = self.event_queue.lock().await;
                if queue.is_empty() {
                    // No events, wait a bit before checking again
                    drop(queue); // Release the lock before sleeping
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
                
                // Sort the queue by priority (highest first)
                let mut queue_vec: Vec<Event> = queue.drain(..).collect();
                queue_vec.sort_by(|a, b| b.priority.cmp(&a.priority));
                
                // Take up to max_concurrent_events
                let to_process_count = std::cmp::min(queue_vec.len(), self.max_concurrent_events);
                events_to_process = queue_vec.drain(0..to_process_count).collect();
                
                // Put remaining events back in the queue
                queue.extend(queue_vec);
            }
            
            if events_to_process.is_empty() {
                continue;
            }
            
            info!("Processing batch of {} events", events_to_process.len());
            
            // Create tasks for each event
            let mut tasks = Vec::new();
            for event in &events_to_process {
                if let Some(_handler) = self.find_handler_for_event(event) {
                    let task = self.create_event_processing_task(event.clone());
                    tasks.push(task);
                } else {
                    warn!("No handler found for event type: {:?}", event.event_type);
                    // Record as failed processing
                    let mut results = self.processing_results.lock().await;
                    results.push(EventProcessingResult {
                        event_id: event.id.clone(),
                        success: false,
                        handler_name: "none".to_string(),
                        result: None,
                        error: Some(format!("No handler for event type: {:?}", event.event_type)),
                        processing_time_ms: 0,
                    });
                }
            }
            
            if tasks.is_empty() {
                continue;
            }
            
            // Execute tasks in parallel
            let task_group = TaskGroup::new(tasks);
            
            match self.engine.execute_task_group(task_group).await {
                Ok(results) => {
                    // Record processing results
                    let mut processing_results = self.processing_results.lock().await;
                    
                    for (i, result) in results.iter().enumerate() {
                        if i < events_to_process.len() {
                            let event = &events_to_process[i];
                            
                            let handler_name = result.metadata.get("handler")
                                .cloned()
                                .unwrap_or_else(|| "unknown".to_string());
                            
                            let processing_time = result.metadata.get("processing_time_ms")
                                .and_then(|s| s.parse::<u64>().ok())
                                .unwrap_or(0);
                            
                            processing_results.push(EventProcessingResult {
                                event_id: event.id.clone(),
                                success: result.status == TaskResultStatus::Success,
                                handler_name,
                                result: Some(result.output.clone()),
                                error: result.error.clone(),
                                processing_time_ms: processing_time,
                            });
                            
                            if result.status == TaskResultStatus::Success {
                                info!("Successfully processed event: {}", event.id);
                            } else {
                                error!("Failed to process event: {}, error: {}", 
                                     event.id, result.error.as_deref().unwrap_or("Unknown error"));
                            }
                        }
                    }
                },
                Err(e) => {
                    error!("Failed to execute task group: {}", e);
                }
            }
            
            // Brief pause to allow other tasks to run
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
    
    /// Listen for new events
    async fn listen_for_events(&self) {
        let running = self.running.clone();
        let event_queue = self.event_queue.clone();
        let mut receiver = self.event_receiver.clone();
        
        while *running.read().await {
            match tokio::time::timeout(Duration::from_secs(1), receiver.recv()).await {
                Ok(Some(event)) => {
                    info!("Received new event: {} of type {:?}", event.id, event.event_type);
                    let mut queue = event_queue.lock().await;
                    queue.push_back(event);
                },
                Ok(None) => {
                    // Channel closed, exit the loop
                    warn!("Event channel closed");
                    break;
                },
                Err(_) => {
                    // Timeout, just continue
                    continue;
                }
            }
        }
    }
    
    /// Generate events at regular intervals for demonstration
    async fn generate_demo_events(&self, interval_ms: u64, count: usize) {
        let running = self.running.clone();
        let sender = self.event_sender.clone();
        
        let event_types = [
            EventType::UserAction,
            EventType::SystemAlert,
            EventType::DataUpdate,
            EventType::ExternalApi,
            EventType::TimerTrigger,
            EventType::Error,
        ];
        
        let priorities = [
            EventPriority::Low,
            EventPriority::Medium,
            EventPriority::High,
            EventPriority::Critical,
        ];
        
        let sources = ["web", "mobile", "api", "system", "scheduler"];
        
        let mut rng = rand::thread_rng();
        
        for i in 0..count {
            if !*running.read().await {
                break;
            }
            
            // Randomly select event parameters
            let event_type = event_types[rng.gen_range(0..event_types.len())].clone();
            let priority = priorities[rng.gen_range(0..priorities.len())];
            let source = sources[rng.gen_range(0..sources.len())];
            
            // Generate a payload based on the event type
            let payload = match event_type {
                EventType::UserAction => format!("User clicked button {}", rng.gen_range(1..10)),
                EventType::SystemAlert => format!("CPU usage at {}%", rng.gen_range(70..100)),
                EventType::DataUpdate => format!("Record {} updated", rng.gen_range(1000..9999)),
                EventType::ExternalApi => format!("API response received with status {}", rng.gen_range(200..500)),
                EventType::TimerTrigger => format!("Scheduled job {} triggered", rng.gen_range(1..20)),
                EventType::Error => format!("Error code {} in module {}", rng.gen_range(1000..9999), rng.gen_range(1..5)),
            };
            
            // Create and send the event
            let event = Self::create_event(event_type, &payload, source, priority);
            
            if let Err(e) = sender.send(event).await {
                error!("Failed to send event: {}", e);
                break;
            }
            
            info!("Generated demo event {} of {}", i + 1, count);
            
            // Wait for the next interval
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        }
    }
    
    /// Generate a summary report of event processing
    fn generate_event_summary(&self, results: &[EventProcessingResult]) -> String {
        let mut summary = String::new();
        
        summary.push_str("# Event-Driven Workflow Processing Summary\n\n");
        
        // Overall statistics
        let total_events = results.len();
        let successful_events = results.iter().filter(|r| r.success).count();
        let failed_events = total_events - successful_events;
        
        summary.push_str(&format!("## Overall Statistics\n\n"));
        summary.push_str(&format!("- Total Events Processed: {}\n", total_events));
        summary.push_str(&format!("- Successful Events: {} ({:.1}%)\n", 
                        successful_events, 
                        if total_events > 0 { (successful_events as f64 / total_events as f64) * 100.0 } else { 0.0 }));
        summary.push_str(&format!("- Failed Events: {} ({:.1}%)\n", 
                        failed_events, 
                        if total_events > 0 { (failed_events as f64 / total_events as f64) * 100.0 } else { 0.0 }));
        
        // Average processing time
        let total_processing_time: u64 = results.iter().map(|r| r.processing_time_ms).sum();
        let avg_processing_time = if total_events > 0 { 
            total_processing_time as f64 / total_events as f64 
        } else { 
            0.0 
        };
        
        summary.push_str(&format!("- Average Processing Time: {:.2} ms\n\n", avg_processing_time));
        
        // Group by handler
        let mut handler_stats: HashMap<String, (usize, usize, u64)> = HashMap::new(); // (total, success, time)
        
        for result in results {
            let entry = handler_stats.entry(result.handler_name.clone()).or_insert((0, 0, 0));
            entry.0 += 1;
            if result.success {
                entry.1 += 1;
            }
            entry.2 += result.processing_time_ms;
        }
        
        summary.push_str("## Handler Statistics\n\n");
        for (handler, (total, success, time)) in &handler_stats {
            summary.push_str(&format!("### Handler: {}\n", handler));
            summary.push_str(&format!("- Total Events: {}\n", total));
            summary.push_str(&format!("- Successful Events: {} ({:.1}%)\n", 
                            success, 
                            if *total > 0 { (*success as f64 / *total as f64) * 100.0 } else { 0.0 }));
            summary.push_str(&format!("- Average Processing Time: {:.2} ms\n\n", 
                            if *total > 0 { *time as f64 / *total as f64 } else { 0.0 }));
        }
        
        // List all events
        summary.push_str("## Event Processing Details\n\n");
        for result in results {
            let status = if result.success { "✅ SUCCESS" } else { "❌ FAILED" };
            summary.push_str(&format!("### Event: {} ({})\n", result.event_id, status));
            summary.push_str(&format!("- Handler: {}\n", result.handler_name));
            summary.push_str(&format!("- Processing Time: {} ms\n", result.processing_time_ms));
            
            if let Some(result_output) = &result.result {
                summary.push_str(&format!("- Result: {}\n", result_output));
            }
            
            if let Some(error) = &result.error {
                summary.push_str(&format!("- Error: {}\n", error));
            }
            
            summary.push_str("\n");
        }
        
        summary
    }
    
    /// Run the event-driven workflow
    async fn run(&mut self, duration_secs: u64) -> Result<WorkflowResult> {
        // Initialize workflow
        self.state.set_status("starting");
        info!("Starting event-driven workflow");
        
        // Set workflow as running
        {
            let mut running = self.running.write().await;
            *running = true;
        }
        
        // Register default handlers if none are registered
        if self.event_handlers.is_empty() {
            info!("No handlers registered, using default handlers");
            self.register_handler(Box::new(UserActionHandler::new("UserActionHandler")));
            self.register_handler(Box::new(SystemAlertHandler::new("SystemAlertHandler")));
            self.register_handler(Box::new(DataUpdateHandler::new("DataUpdateHandler")));
            self.register_handler(Box::new(ExternalApiHandler::new("ExternalApiHandler")));
            self.register_handler(Box::new(TimerTriggerHandler::new("TimerTriggerHandler")));
            self.register_handler(Box::new(ErrorHandler::new("ErrorHandler")));
        }
        
        self.state.set_metadata("registered_handlers", 
                              self.event_handlers.iter()
                                  .map(|h| h.name())
                                  .collect::<Vec<_>>()
                                  .join(", "));
        
        self.state.set_status("processing");
        
        // Start event listener and processor tasks
        let event_listener = tokio::spawn({
            let workflow = self;
            async move {
                workflow.listen_for_events().await;
            }
        }.instrument(tracing::info_span!("event_listener")));
        
        let event_processor = tokio::spawn({
            let workflow = self;
            async move {
                workflow.process_events().await;
            }
        }.instrument(tracing::info_span!("event_processor")));
        
        // Generate some demo events
        let demo_events = tokio::spawn({
            let workflow = self;
            async move {
                // Generate 30 events at 500ms intervals
                workflow.generate_demo_events(500, 30).await;
            }
        }.instrument(tracing::info_span!("demo_event_generator")));
        
        // Run for the specified duration
        info!("Workflow running for {} seconds", duration_secs);
        tokio::time::sleep(Duration::from_secs(duration_secs)).await;
        
        // Stop the workflow
        {
            let mut running = self.running.write().await;
            *running = false;
        }
        
        // Wait for tasks to complete
        let _ = join_all(vec![event_listener, event_processor, demo_events]).await;
        
        // Generate summary report
        let processing_results = self.processing_results.lock().await;
        let summary = self.generate_event_summary(&processing_results);
        
        self.state.set_status("completed");
        self.state.set_metadata("total_events_processed", processing_results.len().to_string());
        self.state.set_metadata("successful_events", 
                              processing_results.iter().filter(|r| r.success).count().to_string());
        
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
    
    // Create and run event-driven workflow
    let mut workflow = EventDrivenWorkflow::new(
        workflow_engine,
        ollama_client,
        5, // Process up to 5 events concurrently
    );
    
    // Run the workflow for 20 seconds
    info!("Starting event-driven workflow...");
    let result = workflow.run(20).await?;
    
    if result.is_success() {
        info!("Event-driven workflow completed successfully!");
        println!("\n{}\n", result.output);
    } else {
        error!("Event-driven workflow failed: {}", result.error.unwrap_or_default());
    }
    
    Ok(())
} 