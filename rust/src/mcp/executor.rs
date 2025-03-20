use crate::utils::error::{McpError, McpResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time;
use uuid::Uuid;
use rand::Rng;
use tracing::{debug, info, warn, error, instrument, trace_span, Span};

use crate::telemetry;

/// Default value for max_concurrent_tasks
fn default_max_concurrent_tasks() -> usize {
    10
}

/// Default value for default_timeout_secs
fn default_timeout_secs() -> u64 {
    60
}

/// Default value for max_timeout_secs
fn default_max_timeout_secs() -> u64 {
    3600
}

/// Default value for retry_attempts
fn default_retry_attempts() -> usize {
    3
}

/// Default value for retry_base_delay_ms
fn default_retry_base_delay_ms() -> u64 {
    100
}

/// Default value for retry_max_delay_ms
fn default_retry_max_delay_ms() -> u64 {
    10000
}

/// Configuration for an executor
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExecutorConfig {
    /// Maximum number of concurrent tasks
    #[serde(default = "default_max_concurrent_tasks")]
    pub max_concurrent_tasks: usize,
    
    /// Default timeout for tasks in seconds
    #[serde(default = "default_timeout_secs")]
    pub default_timeout_secs: u64,
    
    /// Maximum timeout for tasks in seconds
    #[serde(default = "default_max_timeout_secs")]
    pub max_timeout_secs: u64,
    
    /// Number of retries for failed tasks
    #[serde(default = "default_retry_attempts")]
    pub retry_attempts: usize,
    
    /// Base delay between retries in milliseconds
    #[serde(default = "default_retry_base_delay_ms")]
    pub retry_base_delay_ms: u64,
    
    /// Maximum delay between retries in milliseconds
    #[serde(default = "default_retry_max_delay_ms")]
    pub retry_max_delay_ms: u64,
}

impl Default for ExecutorConfig {
    fn default() -> Self {
        Self {
            max_concurrent_tasks: 10,
            default_timeout_secs: 60,
            max_timeout_secs: 3600, // 1 hour
            retry_attempts: 3,
            retry_base_delay_ms: 100,
            retry_max_delay_ms: 10000, // 10 seconds
        }
    }
}

/// A signal that can be sent to a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    /// Unique ID of the signal
    pub id: String,
    /// Name of the signal
    pub name: String,
    /// Description of the signal
    pub description: Option<String>,
    /// Optional payload for the signal
    pub payload: Option<serde_json::Value>,
    /// ID of the workflow this signal belongs to
    pub workflow_id: Option<String>,
    /// Time the signal was created
    pub created_at: DateTime<Utc>,
}

impl Signal {
    /// Create a new signal
    pub fn new(name: &str, payload: Option<serde_json::Value>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: None,
            payload,
            workflow_id: None,
            created_at: Utc::now(),
        }
    }

    /// Create a new signal for a specific workflow
    pub fn for_workflow(name: &str, workflow_id: &str, payload: Option<serde_json::Value>) -> Self {
        let mut signal = Self::new(name, payload);
        signal.workflow_id = Some(workflow_id.to_string());
        signal
    }
}

/// Result of a task execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskResult {
    /// Task succeeded with a result
    Success(serde_json::Value),
    /// Task failed with an error
    Failure(String),
}

impl TaskResult {
    /// Check if the task result is a success
    pub fn is_success(&self) -> bool {
        matches!(self, TaskResult::Success(_))
    }

    /// Get the success value if available
    pub fn success_value(&self) -> Option<&serde_json::Value> {
        match self {
            TaskResult::Success(value) => Some(value),
            TaskResult::Failure(_) => None,
        }
    }

    /// Get the error message if the task failed
    pub fn error_message(&self) -> Option<&str> {
        match self {
            TaskResult::Success(_) => None,
            TaskResult::Failure(msg) => Some(msg),
        }
    }
}

/// A trait that defines the interface for executing tasks
#[async_trait]
pub trait Executor: Send + Sync + 'static {
    /// Execute a task with the given function and arguments
    async fn execute(
        &self,
        task_id: Option<&str>,
        function: &str, 
        args: serde_json::Value,
        timeout: Option<Duration>,
    ) -> McpResult<TaskResult>;
    
    /// Execute a task and stream the results
    async fn execute_stream(
        &self,
        task_id: Option<&str>,
        function: &str,
        args: serde_json::Value,
        timeout: Option<Duration>,
    ) -> McpResult<mpsc::Receiver<TaskResult>>;
    
    /// Send a signal to a workflow
    async fn send_signal(&self, signal: Signal) -> McpResult<()>;
    
    /// Wait for a signal from a workflow
    async fn wait_for_signal(
        &self,
        workflow_id: &str,
        signal_name: &str,
        timeout: Option<Duration>,
    ) -> McpResult<Signal>;
}

/// An executor that uses Tokio for async execution
#[derive(Debug, Clone)]
pub struct AsyncioExecutor {
    /// Configuration
    pub config: ExecutorConfig,
    /// Tasks
    tasks: Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>,
    /// Signals
    signals: Arc<Mutex<HashMap<String, Signal>>>,
    /// Signal receivers
    signal_receivers: Arc<Mutex<HashMap<String, HashSet<String>>>>,
}

impl AsyncioExecutor {
    /// Create a new AsyncIO executor
    #[instrument(skip(config))]
    pub fn new(config: impl Into<Option<ExecutorConfig>>) -> Self {
        let config = config.into().unwrap_or_default();
        debug!("Creating new AsyncioExecutor with config: {:?}", config);
        Self {
            config,
            tasks: Arc::new(Mutex::new(HashMap::new())),
            signals: Arc::new(Mutex::new(HashMap::new())),
            signal_receivers: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Calculate the retry delay using exponential backoff
    fn calculate_retry_delay(&self, attempt: u32) -> Duration {
        let base_delay = self.config.retry_base_delay_ms;
        let max_delay = self.config.retry_max_delay_ms;
        
        // Exponential backoff with jitter
        let mut delay = base_delay * 2u64.pow(attempt);
        if delay > max_delay {
            delay = max_delay;
        }
        
        // Add some randomness to avoid thundering herd
        let jitter = (rand::thread_rng().gen_range(0.0..0.2) - 0.1) * delay as f64;
        let delay = (delay as f64 + jitter) as u64;
        
        Duration::from_millis(delay)
    }
    
    /// Register a task with the executor
    fn register_task(&self, task_id: &str, handle: JoinHandle<()>) {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.insert(task_id.to_string(), handle);
    }
    
    /// Unregister a task from the executor
    fn unregister_task(&self, task_id: &str) {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.remove(task_id);
    }
    
    /// Cancel a running task
    #[instrument(skip(self), fields(task_id = %task_id))]
    pub fn cancel_task(&self, task_id: &str) -> McpResult<()> {
        let _span_guard = telemetry::span_duration("cancel_task");
        
        let start = Instant::now();
        let mut tasks = self.tasks.lock().unwrap();
        
        if let Some(handle) = tasks.remove(task_id) {
            debug!("Cancelling task {}", task_id);
            handle.abort();
            
            let duration = start.elapsed();
            
            // Record metrics
            let mut metrics = HashMap::new();
            metrics.insert("task_cancellation_duration_ms", duration.as_millis() as f64);
            metrics.insert("tasks_cancelled", 1.0);
            metrics.insert("active_tasks", tasks.len() as f64);
            telemetry::add_metrics(metrics);
            
            Ok(())
        } else {
            warn!("Failed to cancel task {}: not found", task_id);
            Err(McpError::Execution(format!("Task {} not found", task_id)))
        }
    }
    
    /// Check if a task is currently running
    #[instrument(skip(self), fields(task_id = %task_id))]
    pub fn is_task_running(&self, task_id: &str) -> bool {
        let tasks = self.tasks.lock().unwrap();
        let is_running = tasks.contains_key(task_id);
        
        debug!("Task {} running status: {}", task_id, is_running);
        is_running
    }
    
    /// Get the number of currently running tasks
    pub fn running_task_count(&self) -> usize {
        let tasks = self.tasks.lock().unwrap();
        let count = tasks.len();
        
        // Record metrics
        let mut metrics = HashMap::new();
        metrics.insert("active_tasks", count as f64);
        telemetry::add_metrics(metrics);
        
        count
    }
}

#[async_trait]
impl Executor for AsyncioExecutor {
    #[instrument(skip(self, args), fields(task_id = ?task_id, function = %function))]
    async fn execute(
        &self,
        task_id: Option<&str>,
        function: &str,
        args: serde_json::Value,
        timeout: Option<Duration>,
    ) -> McpResult<TaskResult> {
        let _span_guard = telemetry::span_duration("execute_task");
        
        let start = Instant::now();
        let task_id = task_id.map(|id| id.to_string()).unwrap_or_else(|| Uuid::new_v4().to_string());
        let timeout = timeout.unwrap_or_else(|| Duration::from_secs(self.config.default_timeout_secs));
        
        debug!("Executing task {} - function: {}, timeout: {:?}", task_id, function, timeout);
        
        // Create a oneshot channel to receive the result
        let (tx, rx) = oneshot::channel();
        
        // Clone values for the task
        let function = function.to_string();
        let args = args.clone();
        let max_attempts = self.config.retry_attempts + 1; // +1 for the initial attempt
        let task_id_clone = task_id.clone();
        let this = self.clone();
        
        // Spawn a task to execute the function
        let handle = tokio::spawn(async move {
            let task_start = Instant::now();
            let mut attempt = 0;
            let mut retry_metrics = HashMap::new();
            
            debug!("Task {} started execution", task_id_clone);
            
            while attempt < max_attempts {
                attempt += 1;
                let attempt_start = Instant::now();
                
                debug!("Task {} - attempt {}/{}", task_id_clone, attempt, max_attempts);
                
                // Execute the function with instrumentation
                let execute_span = trace_span!("execute_function", task_id = %task_id_clone, function = %function, attempt = attempt);
                let _execute_guard = execute_span.enter();
                
                let result = async {
                    // This is where you would actually execute the function
                    // For this example, we'll just simulate execution
                    
                    // In a real implementation, this would dispatch to appropriate function
                    if function == "test_success" {
                        Ok(TaskResult::Success(args.clone()))
                    } else if function == "test_failure" {
                        Err(McpError::Execution("Function failed".to_string()))
                    } else if function == "test_timeout" {
                        time::sleep(timeout + Duration::from_secs(1)).await;
                        Ok(TaskResult::Success(args.clone()))
                    } else {
                        Err(McpError::Execution(format!("Unknown function: {}", function)))
                    }
                };
                
                // Execute with timeout
                let timeout_span = trace_span!("timeout_wrapper", timeout_ms = timeout.as_millis());
                let _timeout_guard = timeout_span.enter();
                
                match time::timeout(timeout, result).await {
                    Ok(Ok(result)) => {
                        // Success, send the result and break the loop
                        debug!("Task {} completed successfully on attempt {}", task_id_clone, attempt);
                        
                        let task_duration = task_start.elapsed();
                        let attempt_duration = attempt_start.elapsed();
                        
                        // Record metrics
                        let mut metrics = HashMap::new();
                        metrics.insert("task_execution_duration_ms", task_duration.as_millis() as f64);
                        metrics.insert("task_attempt_duration_ms", attempt_duration.as_millis() as f64);
                        metrics.insert("task_attempts", attempt as f64);
                        metrics.insert("task_succeeded", 1.0);
                        telemetry::add_metrics(metrics);
                        
                        // Remove the task from the tasks map
                        let mut tasks = this.tasks.lock().unwrap();
                        tasks.remove(&task_id_clone);
                        
                        // Update active tasks count
                        let tasks_count = tasks.len();
                        let mut metrics = HashMap::new();
                        metrics.insert("active_tasks", tasks_count as f64);
                        telemetry::add_metrics(metrics);
                        
                        let _ = tx.send(Ok(result));
                        break;
                    }
                    Ok(Err(e)) => {
                        // Function returned an error
                        warn!("Task {} failed on attempt {}: {:?}", task_id_clone, attempt, e);
                        
                        retry_metrics.insert("task_errors", retry_metrics.get("task_errors").unwrap_or(&0.0) + 1.0);
                        
                        // If max attempts reached, return the error
                        if attempt >= max_attempts {
                            error!("Task {} failed after {} attempts", task_id_clone, attempt);
                            
                            let task_duration = task_start.elapsed();
                            
                            // Record metrics
                            let mut metrics = HashMap::new();
                            metrics.insert("task_execution_duration_ms", task_duration.as_millis() as f64);
                            metrics.insert("task_attempts", attempt as f64);
                            metrics.insert("task_failed", 1.0);
                            
                            // Add retry metrics
                            for (key, value) in retry_metrics.iter() {
                                metrics.insert(key.clone(), *value);
                            }
                            
                            telemetry::add_metrics(metrics);
                            
                            // Remove the task from the tasks map
                            let mut tasks = this.tasks.lock().unwrap();
                            tasks.remove(&task_id_clone);
                            
                            // Update active tasks count
                            let tasks_count = tasks.len();
                            let mut metrics = HashMap::new();
                            metrics.insert("active_tasks", tasks_count as f64);
                            telemetry::add_metrics(metrics);
                            
                            let _ = tx.send(Err(e));
                            break;
                        }
                        
                        // Calculate retry delay with exponential backoff
                        let retry_delay = calculate_retry_delay(
                            attempt,
                            this.config.retry_base_delay_ms,
                            this.config.retry_max_delay_ms
                        );
                        
                        debug!("Task {} will retry in {}ms", task_id_clone, retry_delay);
                        time::sleep(Duration::from_millis(retry_delay)).await;
                    }
                    Err(_) => {
                        // Timeout
                        warn!("Task {} timed out on attempt {}", task_id_clone, attempt);
                        
                        retry_metrics.insert("task_timeouts", retry_metrics.get("task_timeouts").unwrap_or(&0.0) + 1.0);
                        
                        // If max attempts reached, return the error
                        if attempt >= max_attempts {
                            error!("Task {} timed out after {} attempts", task_id_clone, attempt);
                            
                            let task_duration = task_start.elapsed();
                            
                            // Record metrics
                            let mut metrics = HashMap::new();
                            metrics.insert("task_execution_duration_ms", task_duration.as_millis() as f64);
                            metrics.insert("task_attempts", attempt as f64);
                            metrics.insert("task_timed_out", 1.0);
                            
                            // Add retry metrics
                            for (key, value) in retry_metrics.iter() {
                                metrics.insert(key.clone(), *value);
                            }
                            
                            telemetry::add_metrics(metrics);
                            
                            // Remove the task from the tasks map
                            let mut tasks = this.tasks.lock().unwrap();
                            tasks.remove(&task_id_clone);
                            
                            // Update active tasks count
                            let tasks_count = tasks.len();
                            let mut metrics = HashMap::new();
                            metrics.insert("active_tasks", tasks_count as f64);
                            telemetry::add_metrics(metrics);
                            
                            let _ = tx.send(Err(McpError::Timeout));
                            break;
                        }
                        
                        // Calculate retry delay with exponential backoff
                        let retry_delay = calculate_retry_delay(
                            attempt,
                            this.config.retry_base_delay_ms,
                            this.config.retry_max_delay_ms
                        );
                        
                        debug!("Task {} will retry in {}ms", task_id_clone, retry_delay);
                        time::sleep(Duration::from_millis(retry_delay)).await;
                    }
                }
            }
        });
        
        // Store the handle
        {
            let max_concurrent = self.config.max_concurrent_tasks;
            let mut tasks = self.tasks.lock().unwrap();
            
            // Check if we're at the maximum number of concurrent tasks
            if tasks.len() >= max_concurrent {
                handle.abort();
                return Err(McpError::Execution(format!(
                    "Maximum number of concurrent tasks ({}) reached",
                    max_concurrent
                )));
            }
            
            tasks.insert(task_id.clone(), handle);
            
            // Update active tasks count
            let tasks_count = tasks.len();
            let mut metrics = HashMap::new();
            metrics.insert("active_tasks", tasks_count as f64);
            telemetry::add_metrics(metrics);
        }
        
        // Wait for the result
        info!("Awaiting result for task {}", task_id);
        let setup_duration = start.elapsed();
        
        // Record task setup metrics
        let mut metrics = HashMap::new();
        metrics.insert("task_setup_duration_ms", setup_duration.as_millis() as f64);
        telemetry::add_metrics(metrics);
        
        match rx.await {
            Ok(result) => {
                let total_duration = start.elapsed();
                
                // Record overall execution metrics
                let mut metrics = HashMap::new();
                metrics.insert("task_total_duration_ms", total_duration.as_millis() as f64);
                telemetry::add_metrics(metrics);
                
                match &result {
                    Ok(task_result) => {
                        info!("Task {} completed with result: {:?}", task_id, task_result);
                    }
                    Err(e) => {
                        warn!("Task {} failed with error: {:?}", task_id, e);
                    }
                }
                
                result
            }
            Err(_) => {
                error!("Failed to receive result for task {}", task_id);
                Err(McpError::Execution("Task cancelled or panicked".to_string()))
            }
        }
    }
    
    async fn execute_stream(
        &self,
        task_id: Option<&str>,
        function: &str,
        args: serde_json::Value,
        timeout: Option<Duration>,
    ) -> McpResult<mpsc::Receiver<TaskResult>> {
        let task_id = task_id.map(|id| id.to_string()).unwrap_or_else(|| Uuid::new_v4().to_string());
        let _timeout = timeout.unwrap_or_else(|| Duration::from_secs(self.config.default_timeout_secs));
        
        // Create a channel for streaming results
        let (tx, rx) = mpsc::channel(100);
        
        // Clone values for the task
        let function = function.to_string();
        let args = args.clone();
        let task_id_clone = task_id.clone();
        let this = self.clone();
        
        // Spawn a task to execute the function
        let handle = tokio::spawn(async move {
            // This is where you would actually execute the function with streaming
            // For this example, we'll just simulate streaming results
            
            // In a real implementation, this would dispatch to appropriate function
            if function == "test_stream_success" {
                // Simulate streaming results
                for i in 0..5 {
                    let result = serde_json::json!({
                        "step": i,
                        "args": args,
                    });
                    
                    if tx.send(TaskResult::Success(result)).await.is_err() {
                        break;
                    }
                    
                    time::sleep(Duration::from_millis(100)).await;
                }
            } else if function == "test_stream_failure" {
                // Simulate streaming with failure
                for i in 0..3 {
                    let result = serde_json::json!({
                        "step": i,
                        "args": args,
                    });
                    
                    if tx.send(TaskResult::Success(result)).await.is_err() {
                        break;
                    }
                    
                    time::sleep(Duration::from_millis(100)).await;
                }
                
                // Send failure at the end
                let _ = tx.send(TaskResult::Failure("Stream task failed".to_string())).await;
            } else {
                // Unknown function
                let _ = tx.send(TaskResult::Failure(format!("Unknown function: {}", function))).await;
            }
            
            // Unregister the task when done
            this.unregister_task(&task_id_clone);
        });
        
        // Register the task
        self.register_task(&task_id, handle);
        
        Ok(rx)
    }
    
    async fn send_signal(&self, _signal: Signal) -> McpResult<()> {
        // Not implemented for this simplified version
        Ok(())
    }
    
    async fn wait_for_signal(
        &self,
        _workflow_id: &str,
        _signal_name: &str,
        _timeout: Option<Duration>,
    ) -> McpResult<Signal> {
        // Not implemented for this simplified version
        Err(McpError::NotImplemented)
    }
}

/// Calculate retry delay with exponential backoff
#[instrument]
fn calculate_retry_delay(attempt: usize, base_delay: u64, max_delay: u64) -> u64 {
    let backoff = 2u64.saturating_pow(attempt as u32 - 1);
    let delay = base_delay.saturating_mul(backoff);
    delay.min(max_delay)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_execute_success() {
        let executor = AsyncioExecutor::new(ExecutorConfig::default());
        let args = serde_json::json!({ "value": 42 });
        
        let result = executor.execute(None, "test_success", args.clone(), None).await.unwrap();
        assert!(result.is_success());
        assert_eq!(result.success_value(), Some(&args));
    }
    
    #[tokio::test]
    async fn test_execute_failure() {
        let executor = AsyncioExecutor::new(ExecutorConfig::default());
        let args = serde_json::json!({ "value": 42 });
        
        let result = executor.execute(None, "test_failure", args, None).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_execute_timeout() {
        let executor = AsyncioExecutor::new(ExecutorConfig::default());
        let args = serde_json::json!({ "value": 42 });
        
        let result = executor.execute(
            None, 
            "test_timeout", 
            args,
            Some(Duration::from_millis(100)),
        ).await;
        
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(matches!(e, McpError::Timeout));
        }
    }
    
    #[tokio::test]
    async fn test_execute_stream() {
        let executor = AsyncioExecutor::new(ExecutorConfig::default());
        let args = serde_json::json!({ "value": 42 });
        
        let mut rx = executor.execute_stream(None, "test_stream_success", args, None).await.unwrap();
        
        let mut count = 0;
        while let Some(result) = rx.recv().await {
            assert!(result.is_success());
            count += 1;
        }
        
        assert_eq!(count, 5);
    }
} 