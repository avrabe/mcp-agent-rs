use crate::utils::error::{McpError, McpResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time;
use uuid::Uuid;
use rand::Rng;

/// Configuration for an executor
#[derive(Debug, Clone)]
pub struct ExecutorConfig {
    /// Maximum number of concurrent tasks
    pub max_concurrent_tasks: usize,
    /// Default timeout for tasks in seconds
    pub default_timeout_secs: u64,
    /// Maximum timeout for tasks in seconds
    pub max_timeout_secs: u64,
    /// Number of retries for failed tasks
    pub retry_attempts: u32,
    /// Base delay between retries in milliseconds
    pub retry_base_delay_ms: u64,
    /// Maximum delay between retries in milliseconds
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
#[derive(Clone, Debug)]
pub struct AsyncioExecutor {
    /// Configuration for the executor
    config: ExecutorConfig,
    /// Task registry to track running tasks
    tasks: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

impl AsyncioExecutor {
    /// Create a new executor with the given configuration
    pub fn new(config: Option<ExecutorConfig>) -> Self {
        Self {
            config: config.unwrap_or_default(),
            tasks: Arc::new(Mutex::new(HashMap::new())),
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
    
    /// Cancel a task by its ID
    /// 
    /// # Panics
    /// 
    /// This method will panic if the internal mutex is poisoned.
    pub fn cancel_task(&self, task_id: &str) -> McpResult<()> {
        let mut tasks = self.tasks.lock().unwrap();
        if let Some(handle) = tasks.remove(task_id) {
            handle.abort();
            Ok(())
        } else {
            Err(McpError::Execution(format!("Task {} not found", task_id)))
        }
    }
    
    /// Check if a task is currently running
    /// 
    /// # Panics
    /// 
    /// This method will panic if the internal mutex is poisoned.
    pub fn is_task_running(&self, task_id: &str) -> bool {
        let tasks = self.tasks.lock().unwrap();
        tasks.contains_key(task_id)
    }
    
    /// Get the number of currently running tasks
    /// 
    /// # Panics
    /// 
    /// This method will panic if the internal mutex is poisoned.
    pub fn running_task_count(&self) -> usize {
        let tasks = self.tasks.lock().unwrap();
        tasks.len()
    }
}

#[async_trait]
impl Executor for AsyncioExecutor {
    async fn execute(
        &self,
        task_id: Option<&str>,
        function: &str,
        args: serde_json::Value,
        timeout: Option<Duration>,
    ) -> McpResult<TaskResult> {
        let task_id = task_id.map(|id| id.to_string()).unwrap_or_else(|| Uuid::new_v4().to_string());
        let timeout = timeout.unwrap_or_else(|| Duration::from_secs(self.config.default_timeout_secs));
        
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
            let mut attempt = 0;
            
            while attempt < max_attempts {
                attempt += 1;
                
                // Execute the function
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
                match time::timeout(timeout, result).await {
                    Ok(Ok(result)) => {
                        // Success, send the result and break the loop
                        let _ = tx.send(Ok(result));
                        break;
                    }
                    Ok(Err(e)) => {
                        // Function returned an error
                        // If max attempts reached, return the error
                        if attempt >= max_attempts {
                            let _ = tx.send(Err(e));
                            break;
                        }
                    }
                    Err(_) => {
                        // Timeout
                        // If max attempts reached, return the error
                        if attempt >= max_attempts {
                            let _ = tx.send(Err(McpError::Timeout));
                            break;
                        }
                    }
                }
                
                // Calculate delay before retry
                let delay = this.calculate_retry_delay(attempt - 1);
                time::sleep(delay).await;
            }
            
            // Unregister the task when done
            this.unregister_task(&task_id_clone);
        });
        
        // Register the task
        self.register_task(&task_id, handle);
        
        // Wait for the result
        match rx.await {
            Ok(result) => result,
            Err(e) => Err(McpError::Join(format!("Task join error: {}", e))),
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_execute_success() {
        let executor = AsyncioExecutor::new(None);
        let args = serde_json::json!({ "value": 42 });
        
        let result = executor.execute(None, "test_success", args.clone(), None).await.unwrap();
        assert!(result.is_success());
        assert_eq!(result.success_value(), Some(&args));
    }
    
    #[tokio::test]
    async fn test_execute_failure() {
        let executor = AsyncioExecutor::new(None);
        let args = serde_json::json!({ "value": 42 });
        
        let result = executor.execute(None, "test_failure", args, None).await;
        assert!(result.is_err());
    }
    
    #[tokio::test]
    async fn test_execute_timeout() {
        let executor = AsyncioExecutor::new(None);
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
        let executor = AsyncioExecutor::new(None);
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