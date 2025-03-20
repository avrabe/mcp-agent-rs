use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use serde::{Serialize, Deserialize};
use anyhow::Result;
use tokio::time::timeout;
use tracing::{instrument, debug, warn, error};
use uuid::Uuid;

/// A task that can be executed within a workflow
pub struct WorkflowTask<T> {
    /// Name of the task
    pub(crate) name: String,

    /// Unique ID of the task
    pub(crate) id: String,
    
    /// Function that executes the task
    pub(crate) func: Pin<Box<dyn Future<Output = Result<T>> + Send>>,
    
    /// Retry configuration for the task
    pub(crate) retry_config: RetryConfig,
    
    /// Timeout for the task
    pub(crate) timeout: Option<Duration>,
}

/// Configuration for task retries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    /// Maximum number of retry attempts
    pub max_attempts: usize,
    
    /// Initial interval between retries in milliseconds
    pub initial_interval_ms: u64,
    
    /// Maximum interval between retries in milliseconds
    pub max_interval_ms: u64,
    
    /// Multiplier for backoff
    pub backoff_coefficient: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_interval_ms: 100,
            max_interval_ms: 10000,
            backoff_coefficient: 2.0,
        }
    }
}

impl<T> fmt::Debug for WorkflowTask<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WorkflowTask")
            .field("name", &self.name)
            .field("id", &self.id)
            .field("retry_config", &self.retry_config)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl<T: 'static + Send> WorkflowTask<T> {
    /// Create a new workflow task
    pub fn new<F, Fut>(name: &str, f: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T>> + Send + 'static,
    {
        Self {
            name: name.to_string(),
            id: Uuid::new_v4().to_string(),
            func: Box::pin(f()),
            retry_config: RetryConfig::default(),
            timeout: None,
        }
    }
    
    /// Set the timeout for the task
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
    
    /// Set the retry configuration for the task
    pub fn with_retry(mut self, retry_config: RetryConfig) -> Self {
        self.retry_config = retry_config;
        self
    }
    
    /// Execute the task with retries and timeout
    #[instrument(skip(self), fields(task.name = %self.name, task.id = %self.id))]
    pub async fn execute(self) -> Result<T> {
        let mut attempt = 0;
        let max_attempts = self.retry_config.max_attempts;
        
        loop {
            attempt += 1;
            debug!("Executing task (attempt {}/{}): {}", attempt, max_attempts, self.name);
            
            // Apply timeout if specified
            let result = if let Some(timeout_duration) = self.timeout {
                match timeout(timeout_duration, &mut self.func).await {
                    Ok(result) => result,
                    Err(_) => {
                        error!("Task timed out after {:?}: {}", timeout_duration, self.name);
                        Err(anyhow::anyhow!("Task timed out: {}", self.name))
                    }
                }
            } else {
                self.func.await
            };
            
            match result {
                Ok(value) => {
                    debug!("Task completed successfully: {}", self.name);
                    return Ok(value);
                }
                Err(err) => {
                    // If we've reached the max attempts, return the error
                    if attempt >= max_attempts {
                        error!("Task failed after {} attempts: {}, error: {}", 
                               attempt, self.name, err);
                        return Err(err);
                    }
                    
                    // Otherwise, retry after a delay
                    let delay_ms = self.calculate_retry_delay(attempt);
                    warn!("Task failed, retrying in {}ms: {}, error: {}", 
                          delay_ms, self.name, err);
                    
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }
    
    /// Calculate the delay before the next retry attempt
    fn calculate_retry_delay(&self, attempt: usize) -> u64 {
        let base_delay = self.retry_config.initial_interval_ms as f64 * 
                        self.retry_config.backoff_coefficient.powi(attempt as i32 - 1);
                        
        base_delay.min(self.retry_config.max_interval_ms as f64) as u64
    }
}

/// Create a collection of tasks that can be executed in parallel
pub struct TaskGroup<T> {
    tasks: Vec<WorkflowTask<T>>,
}

impl<T: 'static + Send> TaskGroup<T> {
    /// Create a new task group
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
        }
    }
    
    /// Add a task to the group
    pub fn add_task(&mut self, task: WorkflowTask<T>) {
        self.tasks.push(task);
    }
    
    /// Execute all tasks in parallel
    pub async fn execute_all(self) -> Vec<Result<T>> {
        let mut futures = Vec::with_capacity(self.tasks.len());
        
        for task in self.tasks {
            futures.push(tokio::spawn(async move {
                task.execute().await
            }));
        }
        
        let mut results = Vec::with_capacity(futures.len());
        for future in futures {
            match future.await {
                Ok(result) => results.push(result),
                Err(join_error) => results.push(Err(anyhow::anyhow!("Task join error: {}", join_error))),
            }
        }
        
        results
    }
}

impl<T> Default for TaskGroup<T> {
    fn default() -> Self {
        Self::new()
    }
} 