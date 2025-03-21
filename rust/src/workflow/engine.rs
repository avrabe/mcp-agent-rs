use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

use super::signal::{SignalHandler, WorkflowSignal};
use super::state::{WorkflowResult, WorkflowState};
use super::task::{RetryConfig, TaskGroup, WorkflowTask};
use crate::telemetry::add_metric;

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
#[derive(Debug, Clone)]
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
    pub fn new(signal_handler: impl SignalHandler + 'static) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            config: Arc::new(WorkflowEngineConfig::default()),
            signal_handler: Arc::new(signal_handler),
        }
    }

    /// Create a new workflow engine with custom configuration
    pub fn new_with_config(
        config: WorkflowEngineConfig,
        signal_handler: impl SignalHandler + 'static,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            config: Arc::new(config),
            signal_handler: Arc::new(signal_handler),
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
        let task_name = task.name.clone();

        // Apply default timeout if not specified
        let task = if task.timeout.is_none() {
            task.with_timeout(Duration::from_secs(self.config.default_timeout_secs))
        } else {
            task
        };

        let result = task.execute().await;

        // Record metrics
        let duration = start.elapsed();
        add_metric(
            "workflow_task_duration_ms",
            duration.as_millis() as f64,
            &[
                ("task_name", task_name),
                ("success", result.is_ok().to_string()),
            ],
        );

        result
    }

    /// Execute multiple tasks concurrently
    #[instrument(skip(self, tasks), fields(task_count = tasks.len()))]
    pub async fn execute_task_group<T: 'static + Send>(
        &self,
        tasks: Vec<WorkflowTask<T>>,
    ) -> Result<Vec<T>> {
        let task_count = tasks.len();
        info!("Executing {} tasks", task_count);

        let start = std::time::Instant::now();
        let task_group = TaskGroup::new_with_tasks(tasks);
        let results = task_group.execute_all().await;
        let duration = start.elapsed();

        // Count successes and failures
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        let failure_count = results.len() - success_count;

        // Record metrics
        add_metric("workflow_tasks_total", results.len() as f64, &[]);
        add_metric("workflow_tasks_success", success_count as f64, &[]);
        add_metric("workflow_tasks_failure", failure_count as f64, &[]);
        add_metric(
            "workflow_tasks_batch_duration_ms",
            duration.as_millis() as f64,
            &[],
        );

        info!(
            "Completed {} tasks ({} succeeded, {} failed) in {:?}",
            results.len(),
            success_count,
            failure_count,
            duration
        );

        // Collect all successful results, or return the first error
        let mut successful_results = Vec::with_capacity(success_count);
        let mut first_error = None;

        for result in results {
            match result {
                Ok(value) => successful_results.push(value),
                Err(err) => {
                    if first_error.is_none() {
                        first_error = Some(err);
                    }
                }
            }
        }

        if let Some(err) = first_error {
            if successful_results.is_empty() {
                // All tasks failed
                Err(err)
            } else {
                // Some tasks succeeded, return successful results and log the error
                warn!("Some tasks failed: {}", err);
                Ok(successful_results)
            }
        } else {
            // All tasks succeeded
            Ok(successful_results)
        }
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
        let result = self
            .signal_handler
            .wait_for_signal(signal_name, workflow_id, timeout_duration)
            .await;

        match &result {
            Ok(signal) => info!("Received signal: {}", signal_name),
            Err(err) => warn!("Error waiting for signal: {}, error: {}", signal_name, err),
        }

        result
    }

    /// Request input from a human
    pub async fn request_human_input(
        &self,
        request: crate::human_input::HumanInputRequest,
    ) -> Result<crate::human_input::HumanInputResponse> {
        use crate::human_input::HUMAN_INPUT_SIGNAL_NAME;

        // Create a value from the request
        let request_value = serde_json::to_value(&request)?;

        // Wait for a signal with the response
        let signal = self
            .wait_for_signal(
                HUMAN_INPUT_SIGNAL_NAME,
                Some(&request.prompt),
                None, // No timeout, wait indefinitely
            )
            .await?;

        // Parse the response from the signal payload
        if let Some(payload) = signal.payload {
            // Try to parse the response directly
            if let Ok(response) =
                serde_json::from_value::<crate::human_input::HumanInputResponse>(payload.clone())
            {
                return Ok(response);
            }

            // If that fails, try to extract the response string and create a response
            if let Some(response_str) = payload.as_str() {
                return Ok(crate::human_input::HumanInputResponse::new(
                    request.request_id,
                    response_str.to_string(),
                ));
            }

            // Check if there's a response field
            if let Some(response_value) = payload.get("response") {
                if let Some(response_str) = response_value.as_str() {
                    return Ok(crate::human_input::HumanInputResponse::new(
                        request.request_id,
                        response_str.to_string(),
                    ));
                }
            }

            // Check if there's a value field
            if let Some(value) = payload.get("value") {
                if let Some(response_str) = value.as_str() {
                    return Ok(crate::human_input::HumanInputResponse::new(
                        request.request_id,
                        response_str.to_string(),
                    ));
                }
            }
        }

        Err(anyhow!("Invalid human input response received"))
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
        use crate::human_input::HumanInputRequest;

        let workflow_name = self
            .state()
            .name
            .clone()
            .unwrap_or_else(|| "unknown".to_string());

        let request = HumanInputRequest::new(format!("Input for workflow: {}", workflow_name))
            .with_description(description.to_string());

        let response = engine.request_human_input(request).await?;
        Ok(response.response)
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
            workflow
                .state_mut()
                .record_error("WorkflowError", &err.to_string());
            workflow.update_status("failed").await;
            Err(err)
        }
    };

    // Record metrics
    let duration = start.elapsed();
    add_metric(
        "workflow_duration_ms",
        duration.as_millis() as f64,
        &[
            ("workflow_type", std::any::type_name::<W>().to_string()),
            ("status", workflow.state().status.clone()),
        ],
    );

    result
}
