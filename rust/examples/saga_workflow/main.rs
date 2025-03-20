use std::time::Duration;
use std::collections::{HashMap, VecDeque};
use anyhow::{anyhow, Result, Context};
use tracing::{info, warn, error, debug};
use chrono::{DateTime, Utc};
use tokio::sync::mpsc;
use std::sync::{Arc, Mutex};

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

/// An operation step in a saga workflow
#[derive(Debug, Clone)]
struct SagaStep {
    name: String,
    description: String,
    executed: bool,
    compensated: bool,
    execution_result: Option<String>,
    compensation_result: Option<String>,
}

impl SagaStep {
    fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            executed: false,
            compensated: false,
            execution_result: None,
            compensation_result: None,
        }
    }
    
    fn mark_executed(&mut self, result: &str) {
        self.executed = true;
        self.execution_result = Some(result.to_string());
    }
    
    fn mark_compensated(&mut self, result: &str) {
        self.compensated = true;
        self.compensation_result = Some(result.to_string());
    }
    
    fn to_string(&self) -> String {
        let status = if self.executed {
            if self.compensated {
                "COMPENSATED"
            } else {
                "EXECUTED"
            }
        } else {
            "PENDING"
        };
        
        let mut output = format!("Step '{}' ({}): {}\n", self.name, status, self.description);
        
        if let Some(result) = &self.execution_result {
            output.push_str(&format!("  Execution Result: {}\n", result));
        }
        
        if let Some(result) = &self.compensation_result {
            output.push_str(&format!("  Compensation Result: {}\n", result));
        }
        
        output
    }
}

/// Saga workflow for managing a sequence of operations with compensation
struct SagaWorkflow {
    state: WorkflowState,
    engine: WorkflowEngine,
    llm_client: OllamaClient,
    steps: Vec<SagaStep>,
    executed_steps: Vec<usize>,
    current_step: usize,
    should_compensate: bool,
    compensation_reason: Option<String>,
    transaction_id: String,
    saga_name: String,
}

impl SagaWorkflow {
    fn new(
        engine: WorkflowEngine,
        llm_client: OllamaClient,
        saga_name: &str,
    ) -> Self {
        let transaction_id = format!("SAGA-{}-{}", 
            saga_name.to_uppercase().replace(" ", "-"),
            chrono::Utc::now().timestamp()
        );
        
        let mut state = WorkflowState::new();
        state.set_metadata("saga_name", saga_name.to_string());
        state.set_metadata("transaction_id", transaction_id.clone());
        
        Self {
            state,
            engine,
            llm_client,
            steps: Vec::new(),
            executed_steps: Vec::new(),
            current_step: 0,
            should_compensate: false,
            compensation_reason: None,
            transaction_id,
            saga_name: saga_name.to_string(),
        }
    }
    
    /// Add a step to the saga
    fn add_step(&mut self, name: &str, description: &str) {
        self.steps.push(SagaStep::new(name, description));
    }
    
    /// Create a task for executing a step
    fn create_execute_step_task(&self, step_index: usize) -> Task {
        let step = self.steps[step_index].clone();
        
        Task::new(&format!("execute_{}", step.name), move |_ctx| {
            info!("Executing step: {}", step.name);
            
            // Simulate actual operation
            // In a real application, this would perform an actual operation
            
            // Randomly fail some steps to demonstrate compensation
            let should_fail = rand::random::<f32>() < 0.3; // 30% chance of failure
            
            if should_fail {
                error!("Step {} failed during execution", step.name);
                return Err(anyhow!("Failed to execute step: {}", step.name));
            }
            
            // Simulate successful operation
            let result = format!("Successfully executed step: {}", step.name);
            info!("{}", result);
            
            Ok(TaskResult::success(result))
        })
    }
    
    /// Create a task for compensating a step
    fn create_compensate_step_task(&self, step_index: usize) -> Task {
        let step = self.steps[step_index].clone();
        
        Task::new(&format!("compensate_{}", step.name), move |_ctx| {
            info!("Compensating step: {}", step.name);
            
            // Simulate compensation operation
            // In a real application, this would undo the operation
            
            // Randomly fail compensation to show error handling
            let should_fail = rand::random::<f32>() < 0.1; // 10% chance of failure
            
            if should_fail {
                error!("Step {} failed during compensation", step.name);
                return Err(anyhow!("Failed to compensate step: {}", step.name));
            }
            
            // Simulate successful compensation
            let result = format!("Successfully compensated step: {}", step.name);
            info!("{}", result);
            
            Ok(TaskResult::success(result))
        })
    }
    
    /// Execute a step and handle its result
    async fn execute_step(&mut self, step_index: usize) -> Result<()> {
        if step_index >= self.steps.len() {
            return Err(anyhow!("Step index out of bounds"));
        }
        
        let step_name = self.steps[step_index].name.clone();
        info!("Starting execution of step: {}", step_name);
        
        let task = self.create_execute_step_task(step_index);
        let task_group = TaskGroup::new(vec![task]);
        
        match self.engine.execute_task_group(task_group).await {
            Ok(results) => {
                if let Some(result) = results.first() {
                    if result.status == TaskResultStatus::Success {
                        info!("Step {} executed successfully", step_name);
                        self.steps[step_index].mark_executed(&result.output);
                        self.executed_steps.push(step_index);
                        Ok(())
                    } else {
                        error!("Step {} execution failed: {}", 
                             step_name, result.error.as_deref().unwrap_or("Unknown error"));
                        self.should_compensate = true;
                        self.compensation_reason = Some(format!(
                            "Step {} failed during execution: {}", 
                            step_name, 
                            result.error.as_deref().unwrap_or("Unknown error")
                        ));
                        Err(anyhow!("Step execution failed: {}", 
                                  result.error.as_deref().unwrap_or("Unknown error")))
                    }
                } else {
                    error!("No result returned for step {}", step_name);
                    self.should_compensate = true;
                    self.compensation_reason = Some(format!("No result returned for step {}", step_name));
                    Err(anyhow!("No result returned for step execution"))
                }
            },
            Err(e) => {
                error!("Failed to execute step {}: {}", step_name, e);
                self.should_compensate = true;
                self.compensation_reason = Some(format!("Error executing step {}: {}", step_name, e));
                Err(anyhow!("Failed to execute step: {}", e))
            }
        }
    }
    
    /// Compensate a previously executed step
    async fn compensate_step(&mut self, step_index: usize) -> Result<()> {
        if step_index >= self.steps.len() {
            return Err(anyhow!("Step index out of bounds"));
        }
        
        // Skip compensation if the step wasn't executed
        if !self.steps[step_index].executed {
            info!("Skipping compensation for step {} as it was not executed", 
                self.steps[step_index].name);
            return Ok(());
        }
        
        let step_name = self.steps[step_index].name.clone();
        info!("Starting compensation of step: {}", step_name);
        
        let task = self.create_compensate_step_task(step_index);
        let task_group = TaskGroup::new(vec![task]);
        
        match self.engine.execute_task_group(task_group).await {
            Ok(results) => {
                if let Some(result) = results.first() {
                    if result.status == TaskResultStatus::Success {
                        info!("Step {} compensated successfully", step_name);
                        self.steps[step_index].mark_compensated(&result.output);
                        Ok(())
                    } else {
                        error!("Step {} compensation failed: {}", 
                             step_name, result.error.as_deref().unwrap_or("Unknown error"));
                        Err(anyhow!("Step compensation failed: {}", 
                                  result.error.as_deref().unwrap_or("Unknown error")))
                    }
                } else {
                    error!("No result returned for compensating step {}", step_name);
                    Err(anyhow!("No result returned for step compensation"))
                }
            },
            Err(e) => {
                error!("Failed to compensate step {}: {}", step_name, e);
                Err(anyhow!("Failed to compensate step: {}", e))
            }
        }
    }
    
    /// Compensate all executed steps in reverse order
    async fn compensate_all_steps(&mut self) -> Result<()> {
        info!("Starting compensation for all executed steps");
        
        // Update workflow state
        self.state.set_metadata("compensating", "true");
        self.state.set_metadata("compensation_reason", 
            self.compensation_reason.clone().unwrap_or_else(|| "Unknown".to_string()));
        
        // Compensate steps in reverse order (last executed first)
        for &step_index in self.executed_steps.iter().rev() {
            if let Err(e) = self.compensate_step(step_index).await {
                error!("Error during compensation of step {}: {}", 
                     self.steps[step_index].name, e);
                // Continue compensation even if one step fails
            }
        }
        
        info!("Compensation completed for all executed steps");
        Ok(())
    }
    
    /// Generate a report of the saga execution
    fn generate_saga_report(&self) -> String {
        let mut report = String::new();
        
        report.push_str(&format!("# Saga Workflow Report: {}\n\n", self.saga_name));
        report.push_str(&format!("Transaction ID: {}\n", self.transaction_id));
        
        if self.should_compensate {
            report.push_str(&format!("\n## TRANSACTION ROLLED BACK\n"));
            report.push_str(&format!("Reason: {}\n\n", 
                             self.compensation_reason.as_deref().unwrap_or("Unknown")));
        } else {
            report.push_str(&format!("\n## TRANSACTION COMPLETED SUCCESSFULLY\n\n"));
        }
        
        report.push_str("## Step Results\n\n");
        
        for (i, step) in self.steps.iter().enumerate() {
            report.push_str(&format!("### Step {}: {}\n", i + 1, step.to_string()));
        }
        
        report
    }
    
    /// Use LLM to analyze the saga execution and provide insights
    async fn generate_llm_insights(&self) -> Result<String> {
        info!("Generating LLM insights for saga execution");
        
        // Skip if LLM is not available
        match self.llm_client.check_availability().await {
            Ok(_) => {},
            Err(e) => {
                warn!("LLM not available for generating insights: {}", e);
                return Ok("LLM insights not available".to_string());
            }
        }
        
        // Create a detailed history of the saga execution
        let mut history = String::new();
        
        for (i, step) in self.steps.iter().enumerate() {
            history.push_str(&format!("Step {}: {}\n", i + 1, step.to_string()));
        }
        
        if self.should_compensate {
            history.push_str(&format!("\nTransaction rolled back. Reason: {}\n", 
                             self.compensation_reason.as_deref().unwrap_or("Unknown")));
        } else {
            history.push_str("\nTransaction completed successfully.\n");
        }
        
        // Create prompt for LLM
        let prompt = format!(
            "Analyze the following saga transaction execution and provide insights:\n\n\
            Saga Name: {}\n\
            Transaction ID: {}\n\n\
            EXECUTION HISTORY:\n{}\n\n\
            Please provide:\n\
            1. A summary of what happened in this transaction\n\
            2. If there were failures, explain potential causes\n\
            3. Recommendations for improving reliability\n\
            4. Key metrics and observations from this execution",
            self.saga_name, self.transaction_id, history
        );
        
        let request = CompletionRequest {
            model: "llama2".to_string(),
            messages: vec![
                Message {
                    role: Role::System,
                    content: "You are a helpful distributed systems analyst specializing in saga patterns and transaction management. Provide concise, practical insights.".to_string(),
                },
                Message {
                    role: Role::User,
                    content: prompt,
                }
            ],
            stream: false,
            options: None,
        };
        
        match self.llm_client.create_completion(&request) {
            Ok(completion) => {
                let insights = completion.message.content.clone();
                info!("LLM insights generated successfully");
                Ok(insights)
            },
            Err(e) => {
                error!("Failed to generate LLM insights: {}", e);
                Ok("Failed to generate LLM insights".to_string())
            }
        }
    }
    
    /// Run the saga workflow
    async fn run(&mut self) -> Result<WorkflowResult> {
        // Initialize workflow
        self.state.set_status("starting");
        info!("Starting saga workflow: {}", self.saga_name);
        
        // Check if LLM is available
        match self.llm_client.check_availability().await {
            Ok(_) => {
                self.state.set_metadata("llm_available", "true");
            },
            Err(e) => {
                warn!("LLM client is not available: {}", e);
                self.state.set_metadata("llm_available", "false");
                // We continue without LLM capability
            }
        }
        
        self.state.set_status("processing");
        self.state.set_metadata("step_count", self.steps.len().to_string());
        
        // Execute each step in order
        for step_index in 0..self.steps.len() {
            self.current_step = step_index;
            self.state.set_metadata("current_step", (step_index + 1).to_string());
            self.state.set_metadata("current_step_name", self.steps[step_index].name.clone());
            
            if let Err(e) = self.execute_step(step_index).await {
                error!("Error in step {}: {}", self.steps[step_index].name, e);
                // If any step fails, we need to compensate
                self.should_compensate = true;
                self.compensation_reason = Some(format!("Step {} failed: {}", 
                                                      self.steps[step_index].name, e));
                break;
            }
            
            // Check for cancellation signal
            if self.engine.should_cancel() {
                info!("Saga workflow cancelled by signal");
                self.should_compensate = true;
                self.compensation_reason = Some("Workflow cancelled by external signal".to_string());
                break;
            }
        }
        
        // If any steps failed or we received a cancellation signal, compensate
        if self.should_compensate {
            info!("Starting compensation due to: {}", 
                 self.compensation_reason.as_deref().unwrap_or("Unknown reason"));
            self.state.set_metadata("compensation_triggered", "true");
            
            // Compensate all executed steps
            if let Err(e) = self.compensate_all_steps().await {
                error!("Error during compensation: {}", e);
                self.state.set_error(format!("Compensation error: {}", e));
            }
            
            self.state.set_status("compensated");
        } else {
            // All steps succeeded
            self.state.set_metadata("compensation_triggered", "false");
            self.state.set_status("completed");
        }
        
        // Generate execution report
        let report = self.generate_saga_report();
        
        // Add LLM insights if available
        let insights = self.generate_llm_insights().await.unwrap_or_else(|_| 
            "Failed to generate LLM insights".to_string());
        
        let final_report = format!("{}\n\n## LLM Insights\n\n{}", report, insights);
        
        if self.should_compensate {
            Ok(WorkflowResult::partial(
                final_report,
                Some(self.compensation_reason.clone().unwrap_or_else(|| 
                    "Transaction rolled back".to_string()))
            ))
        } else {
            Ok(WorkflowResult::success(final_report))
        }
    }
}

/// Create a mock order processing saga
fn create_order_processing_saga(
    engine: WorkflowEngine,
    llm_client: OllamaClient,
) -> SagaWorkflow {
    let mut saga = SagaWorkflow::new(
        engine,
        llm_client,
        "Order Processing"
    );
    
    saga.add_step(
        "validate_inventory", 
        "Check if all ordered items are available in inventory"
    );
    
    saga.add_step(
        "reserve_inventory", 
        "Reserve inventory items for the order"
    );
    
    saga.add_step(
        "process_payment", 
        "Process customer payment for the order"
    );
    
    saga.add_step(
        "update_customer_record", 
        "Update customer purchase history"
    );
    
    saga.add_step(
        "create_shipping_order", 
        "Create shipping request for the order"
    );
    
    saga.add_step(
        "send_notification", 
        "Send order confirmation to customer"
    );
    
    saga
}

/// Create a mock travel booking saga
fn create_travel_booking_saga(
    engine: WorkflowEngine,
    llm_client: OllamaClient,
) -> SagaWorkflow {
    let mut saga = SagaWorkflow::new(
        engine,
        llm_client,
        "Travel Booking"
    );
    
    saga.add_step(
        "check_availability", 
        "Check availability of flights, hotels, and rental cars"
    );
    
    saga.add_step(
        "reserve_flight", 
        "Reserve flight tickets"
    );
    
    saga.add_step(
        "book_hotel", 
        "Book hotel room"
    );
    
    saga.add_step(
        "reserve_rental_car", 
        "Reserve rental car"
    );
    
    saga.add_step(
        "process_payment", 
        "Process payment for the entire booking"
    );
    
    saga.add_step(
        "generate_itinerary", 
        "Generate travel itinerary documents"
    );
    
    saga.add_step(
        "send_confirmation", 
        "Send booking confirmation to traveler"
    );
    
    saga
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
    
    // Run travel booking saga
    info!("Creating travel booking saga workflow");
    let mut travel_saga = create_travel_booking_saga(
        workflow_engine.clone(),
        ollama_client.clone()
    );
    
    info!("Starting travel booking saga workflow");
    let travel_result = travel_saga.run().await?;
    
    if travel_result.is_success() {
        info!("Travel booking saga completed successfully");
    } else {
        warn!("Travel booking saga required compensation: {}", 
             travel_result.error.as_deref().unwrap_or("Unknown reason"));
    }
    
    println!("\n{}\n", travel_result.output);
    
    // Pause between sagas
    tokio::time::sleep(Duration::from_secs(1)).await;
    
    // Run order processing saga
    info!("Creating order processing saga workflow");
    let mut order_saga = create_order_processing_saga(
        workflow_engine,
        ollama_client
    );
    
    info!("Starting order processing saga workflow");
    let order_result = order_saga.run().await?;
    
    if order_result.is_success() {
        info!("Order processing saga completed successfully");
    } else {
        warn!("Order processing saga required compensation: {}", 
             order_result.error.as_deref().unwrap_or("Unknown reason"));
    }
    
    println!("\n{}\n", order_result.output);
    
    Ok(())
} 