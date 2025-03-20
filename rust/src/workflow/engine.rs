use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::{instrument, debug, info, warn, error};
use uuid::Uuid;

use crate::telemetry::{add_metric, record_span_duration};
use super::state::{WorkflowState, WorkflowResult, SharedWorkflowState};
use super::signal::{WorkflowSignal, SignalHandler, AsyncSignalHandler};
use super::task::{WorkflowTask, TaskGroup, RetryConfig};

/// Configuration for the workflow engine
#[derive(Debug, Clone)]
pub struct WorkflowEngineConfig {
    /// Default timeout for tasks
    pub default_timeout_secs: u64,
    
    /// Default retry configuration
    pub default_retry_config: RetryConfig,
    
    /// Maximum number of concurrent tasks
    pub max_concurrent_tasks: Option<usize>,
}

impl Default for WorkflowEngineConfig {
    fn default() -> Self {
        Self {
            default_timeout_secs: 60,
            default_retry_config: RetryConfig::default(),
            max_concurrent_tasks: None,
        }
    }
}

/// Engine for executing workflows
#[derive(Clone)]
pub struct WorkflowEngine {
    /// Unique ID of the workflow engine
    id: String,
    
    /// Configuration for the workflow engine
    config: Arc<WorkflowEngineConfig>,
    
    /// Signal handler for the workflow engine
    signal_handler: Arc<dyn SignalHandler>,
}

impl WorkflowEngine {
    /// Create a new workflow engine
    pub fn new(config: Option<WorkflowEngineConfig>) -> Self {
        let config = Arc::new(config.unwrap_or_default());
        
        Self {
            id: Uuid::new_v4().to_string(),
            config,
            signal_handler: Arc::new(AsyncSignalHandler::new()),
        }
    }
    
    /// Get the ID of the workflow engine
    pub fn id(&self) -> &str {
        &self.id
    }
    
    /// Get the signal handler
    pub fn signal_handler(&self) -> Arc<dyn SignalHandler> {
        Arc::clone(&self.signal_handler)
    }
    
    /// Execute a task
    #[instrument(skip(self, task), fields(task.name = %task.name, task.id = %task.id))]
    pub async fn execute_task<T: 'static + Send>(&self, task: WorkflowTask<T>) -> Result<T> {
        let start = std::time::Instant::now();
        
        // Apply default timeout if not specified
        let task = if task.timeout.is_none() {
            task.with_timeout(Duration::from_secs(self.config.default_timeout_secs))
        } else {
            task
        };
        
        let result = task.execute().await;
        
        // Record metrics
        let duration = start.elapsed();
        add_metric("workflow_task_duration_ms", duration.as_millis() as f64, &[
            ("task_name", task.name.clone()),
            ("success", result.is_ok().to_string()),
        ]);
        
        result
    }
    
    /// Execute multiple tasks concurrently
    #[instrument(skip(self, tasks), fields(task_count = tasks.len()))]
    pub async fn execute_tasks<T: 'static + Send>(
        &self,
        tasks: TaskGroup<T>
    ) -> Vec<Result<T>> {
        info!("Executing {} tasks", tasks.len());
        
        let start = std::time::Instant::now();
        let results = tasks.execute_all().await;
        let duration = start.elapsed();
        
        // Count successes and failures
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let failure_count = results.len() - success_count;
        
        // Record metrics
        add_metric("workflow_tasks_total", results.len() as f64, &[]);
        add_metric("workflow_tasks_success", success_count as f64, &[]);
        add_metric("workflow_tasks_failure", failure_count as f64, &[]);
        add_metric("workflow_tasks_batch_duration_ms", duration.as_millis() as f64, &[]);
        
        info!("Completed {} tasks ({} succeeded, {} failed) in {:?}", 
             results.len(), success_count, failure_count, duration);
        
        results
    }
    
    /// Send a signal
    #[instrument(skip(self, payload), fields(signal.name = %signal_name))]
    pub async fn signal(
        &self,
        signal_name: &str,
        payload: Option<Value>,
        description: Option<&str>,
    ) -> Result<()> {
        let mut signal = WorkflowSignal::new(signal_name, payload);
        if let Some(desc) = description {
            signal.description = Some(desc.to_string());
        }
        
        self.signal_handler.signal(signal).await
    }
    
    /// Wait for a signal
    #[instrument(skip(self), fields(signal.name = %signal_name))]
    pub async fn wait_for_signal(
        &self,
        signal_name: &str,
        description: Option<&str>,
        timeout_duration: Option<Duration>,
    ) -> Result<WorkflowSignal> {
        let workflow_id = Some(self.id.as_str());
        
        info!("Waiting for signal: {}", signal_name);
        let result = self.signal_handler.wait_for_signal(signal_name, workflow_id, timeout_duration).await;
        
        match &result {
            Ok(signal) => info!("Received signal: {}", signal_name),
            Err(err) => warn!("Error waiting for signal: {}, error: {}", signal_name, err),
        }
        
        result
    }
}

/// Trait for workflow implementations
#[async_trait]
pub trait Workflow: Send + Sync + 'static {
    /// Run the workflow
    async fn run(&mut self) -> Result<WorkflowResult>;
    
    /// Get the workflow state
    fn state(&self) -> &WorkflowState;
    
    /// Get mutable reference to the workflow state
    fn state_mut(&mut self) -> &mut WorkflowState;
    
    /// Update the workflow state
    async fn update_state(&mut self, updates: HashMap<String, Value>) {
        self.state_mut().update(updates);
    }
    
    /// Update the workflow status
    async fn update_status(&mut self, status: &str) {
        self.state_mut().update_status(status);
    }
    
    /// Wait for input from a human
    async fn wait_for_input(&self, engine: &WorkflowEngine, description: &str) -> Result<String> {
        let signal = engine.wait_for_signal("human_input", Some(description), None).await?;
        
        if let Some(payload) = signal.payload {
            if let Some(input) = payload.as_str() {
                return Ok(input.to_string());
            }
            
            if let Some(value) = payload.get("value") {
                if let Some(input) = value.as_str() {
                    return Ok(input.to_string());
                }
            }
        }
        
        Err(anyhow!("Invalid input received"))
    }
}

/// Execute a workflow with telemetry and proper error handling
#[instrument(skip(workflow), fields(workflow.type = std::any::type_name::<W>()))]
pub async fn execute_workflow<W: Workflow>(mut workflow: W) -> Result<WorkflowResult> {
    let start = std::time::Instant::now();
    info!("Starting workflow");
    
    // Mark workflow as running
    workflow.update_status("running").await;
    
    // Execute the workflow
    let result = match workflow.run().await {
        Ok(result) => {
            info!("Workflow completed successfully");
            workflow.update_status("completed").await;
            Ok(result)
        }
        Err(err) => {
            error!("Workflow failed: {}", err);
            workflow.state_mut().record_error("WorkflowError", &err.to_string());
            workflow.update_status("failed").await;
            Err(err)
        }
    };
    
    // Record metrics
    let duration = start.elapsed();
    add_metric("workflow_duration_ms", duration.as_millis() as f64, &[
        ("workflow_type", std::any::type_name::<W>().to_string()),
        ("status", workflow.state().status.clone()),
    ]);
    
    result
} 