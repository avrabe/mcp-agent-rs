use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use rand::Rng;
use anyhow::{anyhow, Result};
use tracing::{info, warn, error, debug};
use std::collections::HashMap;
use tokio::sync::Mutex as TokioMutex;
use serde_json::json;
use async_trait::async_trait;

use mcp_agent::telemetry::{init_telemetry, TelemetryConfig};
use mcp_agent::workflow::{
    WorkflowEngine, WorkflowState, WorkflowResult, Workflow,
    TaskGroup, Task, TaskResult, TaskResultStatus,
    SignalHandler, WorkflowSignal, execute_workflow,
};
use mcp_agent::llm::{
    ollama::{OllamaClient, OllamaConfig},
    types::{Message, Role, CompletionRequest, Completion},
};

#[derive(Debug, Clone, Copy, PartialEq)]
enum RetryStrategy {
    /// Fixed interval between retries
    Fixed { interval_ms: u64 },
    
    /// Exponential backoff with configurable base
    ExponentialBackoff { base_ms: u64, factor: f64, max_ms: Option<u64> },
    
    /// Linear backoff with configurable increment
    LinearBackoff { initial_ms: u64, increment_ms: u64, max_ms: Option<u64> },
    
    /// Random jitter within a range
    Jitter { min_ms: u64, max_ms: u64 },
}

impl RetryStrategy {
    fn calculate_delay(&self, attempt: usize) -> Duration {
        match self {
            Self::Fixed { interval_ms } => Duration::from_millis(*interval_ms),
            
            Self::ExponentialBackoff { base_ms, factor, max_ms } => {
                let delay = *base_ms as f64 * factor.powf(attempt as f64);
                let delay = delay as u64;
                
                if let Some(max) = max_ms {
                    Duration::from_millis(delay.min(*max))
                } else {
                    Duration::from_millis(delay)
                }
            },
            
            Self::LinearBackoff { initial_ms, increment_ms, max_ms } => {
                let delay = *initial_ms + (*increment_ms * attempt as u64);
                
                if let Some(max) = max_ms {
                    Duration::from_millis(delay.min(*max))
                } else {
                    Duration::from_millis(delay)
                }
            },
            
            Self::Jitter { min_ms, max_ms } => {
                let delay = rand::thread_rng().gen_range(*min_ms..*max_ms);
                Duration::from_millis(delay)
            },
        }
    }
}

/// Circuit breaker states
#[derive(Debug, Clone, Copy, PartialEq)]
enum CircuitState {
    Closed,     // Normal operation, requests allowed
    Open,       // Failing, requests rejected
    HalfOpen,   // Testing if service recovered
}

/// Circuit breaker for handling service failures
struct CircuitBreaker {
    state: CircuitState,
    failure_threshold: usize,
    failure_count: usize,
    recovery_timeout: Duration,
    last_failure_time: Option<Instant>,
    half_open_max_calls: usize,
    half_open_call_count: usize,
}

impl CircuitBreaker {
    fn new(failure_threshold: usize, recovery_timeout: Duration, half_open_max_calls: usize) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_threshold,
            failure_count: 0,
            recovery_timeout,
            last_failure_time: None,
            half_open_max_calls,
            half_open_call_count: 0,
        }
    }
    
    fn allow_request(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            
            CircuitState::Open => {
                // Check if recovery timeout has elapsed
                if let Some(last_failure) = self.last_failure_time {
                    if last_failure.elapsed() >= self.recovery_timeout {
                        // Move to half-open state to test recovery
                        debug!("Circuit breaker transitioning from Open to HalfOpen state");
                        self.state = CircuitState::HalfOpen;
                        self.half_open_call_count = 0;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            },
            
            CircuitState::HalfOpen => {
                // Allow limited number of test requests
                if self.half_open_call_count < self.half_open_max_calls {
                    self.half_open_call_count += 1;
                    true
                } else {
                    false
                }
            }
        }
    }
    
    fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                // Reset failure count on success
                self.failure_count = 0;
            },
            
            CircuitState::HalfOpen => {
                // On success in half-open state, go back to closed
                debug!("Circuit breaker transitioning from HalfOpen to Closed state");
                self.state = CircuitState::Closed;
                self.failure_count = 0;
            },
            
            CircuitState::Open => {
                // Should not happen, but handle it anyway
                warn!("Unexpected success recorded while circuit breaker is Open");
            }
        }
    }
    
    fn record_failure(&mut self) {
        self.last_failure_time = Some(Instant::now());
        
        match self.state {
            CircuitState::Closed => {
                self.failure_count += 1;
                
                // If we hit the threshold, open the circuit
                if self.failure_count >= self.failure_threshold {
                    debug!("Circuit breaker transitioning from Closed to Open state");
                    self.state = CircuitState::Open;
                }
            },
            
            CircuitState::HalfOpen => {
                // If we fail in half-open state, go back to open
                debug!("Circuit breaker transitioning from HalfOpen to Open state");
                self.state = CircuitState::Open;
            },
            
            CircuitState::Open => {
                // Already open, just update the failure time
            }
        }
    }
    
    fn state(&self) -> CircuitState {
        self.state
    }
}

/// Simulates a flaky external service with configurable reliability
struct FlakyService {
    // Probability of request success (0.0 to 1.0)
    success_rate: f64,
    // Time it takes for service to respond
    response_time: Duration,
    // Circuit breaker
    circuit_breaker: CircuitBreaker,
}

impl FlakyService {
    fn new(success_rate: f64, response_time: Duration) -> Self {
        let circuit_breaker = CircuitBreaker::new(
            3,                         // Fail after 3 consecutive failures
            Duration::from_secs(5),    // Try again after 5 seconds
            2,                         // Allow 2 test requests in half-open state
        );
        
        Self { 
            success_rate, 
            response_time,
            circuit_breaker,
        }
    }
    
    async fn call(&mut self, request: &str) -> Result<String> {
        // Check if request is allowed by circuit breaker
        if !self.circuit_breaker.allow_request() {
            return Err(anyhow!("Circuit breaker is open, request rejected"));
        }
        
        // Simulate processing time
        tokio::time::sleep(self.response_time).await;
        
        // Simulate random failures
        let success = rand::thread_rng().gen_bool(self.success_rate);
        
        if success {
            // Simulate successful response
            self.circuit_breaker.record_success();
            Ok(format!("Processed: {}", request))
        } else {
            // Simulate failure
            self.circuit_breaker.record_failure();
            Err(anyhow!("Service failed to process request"))
        }
    }
    
    fn circuit_state(&self) -> CircuitState {
        self.circuit_breaker.state()
    }
}

struct RetryWorkflow {
    state: WorkflowState,
    engine: WorkflowEngine,
    flaky_service: Arc<TokioMutex<FlakyService>>,
    requests: Vec<String>,
    retry_strategy: RetryStrategy,
    max_retries: usize,
    results: Vec<(String, bool, usize)>, // (request, success, attempts)
}

impl RetryWorkflow {
    fn new(
        engine: WorkflowEngine,
        requests: Vec<String>,
        service_success_rate: f64,
        service_response_time: Duration,
        retry_strategy: RetryStrategy,
        max_retries: usize,
    ) -> Self {
        // Create a flaky service with a circuit breaker
        let flaky_service = Arc::new(TokioMutex::new(FlakyService::new(
            service_success_rate,
            service_response_time
        )));
        
        let mut metadata = HashMap::new();
        metadata.insert("request_count".to_string(), json!(requests.len()));
        metadata.insert("max_retries".to_string(), json!(max_retries));
        
        Self {
            state: WorkflowState::new(
                Some("RetryWorkflow".to_string()),
                Some(metadata)
            ),
            engine,
            flaky_service,
            requests,
            retry_strategy,
            max_retries,
            results: Vec::new(),
        }
    }
    
    fn create_retry_task(&self, request: String, request_index: usize) -> Task {
        let flaky_service = self.flaky_service.clone();
        let retry_strategy = self.retry_strategy;
        let max_retries = self.max_retries;
        
        Task::new(&format!("process_request_{}", request_index), move |ctx| {
            async move {
                info!("Processing request: {}", request);
                
                let mut attempt = 0;
                let mut last_error = None;
                
                // Track whether the task was completed successfully
                let mut success = false;
                
                // Try until max retries reached
                while attempt <= max_retries {
                    debug!("Attempt {} of {} for request: {}", attempt + 1, max_retries + 1, request);
                    
                    // Update task context with current attempt
                    ctx.set_metadata("current_attempt", (attempt + 1).to_string());
                    
                    // Check if task was cancelled
                    if ctx.is_cancelled() {
                        warn!("Task cancelled during retry loop");
                        return Err(anyhow!("Task cancelled during retry processing"));
                    }
                    
                    match flaky_service.lock().await.call(&request).await {
                        Ok(response) => {
                            info!("Request succeeded on attempt {}: {}", attempt + 1, response);
                            success = true;
                            
                            // Return success with attempt count
                            return Ok(TaskResult::success(format!("{{\"success\":true,\"attempts\":{}}}", attempt + 1)));
                        },
                        Err(e) => {
                            warn!("Request failed on attempt {}: {}", attempt + 1, e);
                            last_error = Some(e.to_string());
                            
                            // Check circuit breaker state
                            let circuit_state = flaky_service.lock().await.circuit_state();
                            if circuit_state == CircuitState::Open {
                                warn!("Circuit breaker open, stopping retry attempts");
                                break;
                            }
                            
                            // If we haven't reached max retries, wait before trying again
                            if attempt < max_retries {
                                let delay = retry_strategy.calculate_delay(attempt);
                                debug!("Waiting {:?} before next retry", delay);
                                tokio::time::sleep(delay).await;
                            }
                        }
                    }
                    
                    attempt += 1;
                }
                
                // If we got here, all retries failed
                error!("All retry attempts failed for request: {}", request);
                
                let error_msg = last_error.unwrap_or_else(|| "Unknown error".to_string());
                Ok(TaskResult::success(format!("{{\"success\":false,\"attempts\":{},\"error\":\"{}\"}}", attempt, error_msg)))
            }.await
        })
    }
    
    async fn run(&mut self) -> Result<WorkflowResult> {
        // Initialize workflow
        self.state.set_status("starting");
        info!("Starting retry workflow");
        
        self.state.set_status("processing");
        
        // Create tasks for each request
        let mut tasks = Vec::new();
        
        for (index, request) in self.requests.iter().enumerate() {
            let task = self.create_retry_task(request.clone(), index);
            tasks.push(task);
        }
        
        // Execute all tasks
        let results = self.engine.execute_task_group(tasks).await?;
        
        // Process results
        let mut success_count = 0;
        let mut failure_count = 0;
        let mut total_attempts = 0;
        
        for (index, result) in results.iter().enumerate() {
            if result.status == TaskResultStatus::Success {
                // Parse the JSON response
                let output = result.output.clone();
                
                // In a real implementation, use a proper JSON parser
                // This is a simple parsing for demo purposes
                let success_str = output.split("\"success\":").nth(1).unwrap_or("").split(",").next().unwrap_or("");
                let success = success_str.trim() == "true";
                
                let attempts_str = output.split("\"attempts\":").nth(1).unwrap_or("").split("}").next().unwrap_or("");
                let attempts = attempts_str.trim().parse::<usize>().unwrap_or(0);
                
                // Store the result
                self.results.push((self.requests[index].clone(), success, attempts));
                
                if success {
                    success_count += 1;
                } else {
                    failure_count += 1;
                }
                
                total_attempts += attempts;
            } else {
                // Task execution failed
                failure_count += 1;
                self.results.push((self.requests[index].clone(), false, 0));
            }
        }
        
        // Update workflow metrics
        self.state.set_metadata("success_count", success_count.to_string());
        self.state.set_metadata("failure_count", failure_count.to_string());
        self.state.set_metadata("total_attempts", total_attempts.to_string());
        
        if let Some(avg_attempts) = if success_count > 0 { Some(total_attempts as f64 / success_count as f64) } else { None } {
            self.state.set_metadata("avg_attempts_per_success", format!("{:.2}", avg_attempts));
        }
        
        // Finalize workflow
        if failure_count == 0 {
            self.state.set_status("completed");
        } else if success_count == 0 {
            self.state.set_status("failed");
        } else {
            self.state.set_status("completed_with_errors");
        }
        
        // Generate summary
        let mut summary = String::new();
        summary.push_str(&format!("Processed {} requests with {} successful and {} failed\n", 
                                 self.requests.len(), success_count, failure_count));
        summary.push_str(&format!("Total attempts: {}\n", total_attempts));
        
        if let Some(avg_attempts) = if success_count > 0 { Some(total_attempts as f64 / success_count as f64) } else { None } {
            summary.push_str(&format!("Average attempts per successful request: {:.2}\n\n", avg_attempts));
        }
        
        summary.push_str("Request Results:\n");
        for (index, (request, success, attempts)) in self.results.iter().enumerate() {
            summary.push_str(&format!("{}. \"{}\" - {}, Attempts: {}\n", 
                                     index + 1, request, 
                                     if *success { "SUCCESS" } else { "FAILED" }, 
                                     attempts));
        }
        
        // Return workflow result
        if self.state.status() == "failed" {
            Ok(WorkflowResult::failed(&format!("Retry workflow failed - all {} requests failed", failure_count)))
        } else {
            Ok(WorkflowResult::success(summary))
        }
    }
}

#[async_trait]
impl Workflow for RetryWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        // ... existing implementation ...
        
        // Execute all tasks
        // TaskGroup::new() no longer takes arguments
        // execute_task_group now takes Vec<WorkflowTask<T>> directly
        let results = self.engine.execute_task_group(tasks).await?;
        
        // ... rest of the method ...
    }
    
    fn state(&self) -> &WorkflowState {
        &self.state
    }
    
    fn state_mut(&mut self) -> &mut WorkflowState {
        &mut self.state
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize telemetry
    let telemetry_config = TelemetryConfig::default();
    init_telemetry(telemetry_config);
    
    // Create workflow engine with signal handling
    let signal_handler = SignalHandler::new(vec![WorkflowSignal::Interrupt, WorkflowSignal::Terminate]);
    let workflow_engine = WorkflowEngine::new(signal_handler);
    
    // Sample requests
    let requests = vec![
        "Process user data for user_123".to_string(),
        "Generate monthly report for department_456".to_string(),
        "Send notification to device_789".to_string(),
        "Update database record for order_321".to_string(),
        "Validate payment for transaction_654".to_string(),
    ];
    
    // Configure retry workflow
    let retry_strategy = RetryStrategy::ExponentialBackoff { 
        base_ms: 100, 
        factor: 2.0, 
        max_ms: Some(2000) 
    };
    
    // Create and run retry workflow with a moderately flaky service
    let mut workflow = RetryWorkflow::new(
        workflow_engine,
        requests,
        0.6,                              // 60% success rate
        Duration::from_millis(100),       // 100ms response time
        retry_strategy,
        5,                                // Maximum 5 retries
    );
    
    info!("Starting retry workflow...");
    let result = workflow.run().await?;
    
    if result.is_success() {
        info!("Retry workflow completed!");
        println!("\nWorkflow Result:\n{}", result.output);
        
        // Print workflow metadata
        println!("\nRetry Workflow Metadata:");
        for (key, value) in workflow.state.metadata() {
            println!("- {}: {}", key, value);
        }
    } else {
        error!("Retry workflow failed: {}", result.error.unwrap_or_default());
    }
    
    Ok(())
} 