use anyhow::{anyhow, Result};
use rand::Rng;
use tracing::{error, info};

use mcp_agent::workflow::signal::DefaultSignalHandler;
use mcp_agent::{
    telemetry::{init_telemetry, TelemetryConfig},
    workflow::{task, Workflow, WorkflowEngine, WorkflowResult, WorkflowState},
};

/// Represents a step in a saga transaction
#[derive(Debug, Clone)]
struct SagaStep {
    /// Name of the step
    name: String,

    /// Whether the step has been executed
    executed: bool,

    /// Whether the step has been compensated
    compensated: bool,

    /// Result of execution
    execution_result: Option<String>,

    /// Result of compensation
    compensation_result: Option<String>,
}

impl SagaStep {
    /// Create a new saga step
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            executed: false,
            compensated: false,
            execution_result: None,
            compensation_result: None,
        }
    }

    /// Mark the step as executed with the given result
    fn mark_executed(&mut self, result: &str) {
        self.executed = true;
        self.execution_result = Some(result.to_string());
    }

    /// Mark the step as compensated with the given result
    fn mark_compensated(&mut self, result: &str) {
        self.compensated = true;
        self.compensation_result = Some(result.to_string());
    }
}

/// A workflow that implements the Saga pattern for distributed transactions
struct SagaWorkflow {
    /// Steps in the saga
    steps: Vec<SagaStep>,

    /// The workflow engine
    engine: WorkflowEngine,

    /// The workflow state
    state: WorkflowState,

    /// Indices of steps that have been executed
    executed_steps: Vec<usize>,

    /// Whether compensation should be triggered
    should_compensate: bool,

    /// Reason for compensation
    compensation_reason: Option<String>,
}

impl SagaWorkflow {
    /// Create a new saga workflow with the provided engine
    fn new(engine: WorkflowEngine) -> Self {
        Self {
            steps: Vec::new(),
            engine,
            state: WorkflowState::new(Some("saga_workflow".to_string()), None),
            executed_steps: Vec::new(),
            should_compensate: false,
            compensation_reason: None,
        }
    }

    /// Add a step to the saga
    fn add_step(&mut self, name: &str) {
        self.steps.push(SagaStep::new(name));
    }

    /// Execute a step in the saga
    async fn execute_step(&mut self, step_index: usize) -> Result<()> {
        let step_name = self.steps[step_index].name.clone();
        info!("Executing step {}: {}", step_index, step_name);

        // Create a task for this step's execution function
        let task = task(&format!("step_{}", step_index), {
            let step_name = step_name.clone(); // Clone again to move into the closure
            move || async move {
                info!("Executing step: {}", step_name);

                // Simulate actual operation
                // In a real application, this would perform an actual operation

                // Randomly fail some steps to demonstrate compensation
                let should_fail = rand::thread_rng().gen_bool(0.3); // 30% chance of failure

                if should_fail {
                    error!("Step {} failed during execution", step_name);
                    return Err(anyhow!("Failed to execute step: {}", step_name));
                }

                // Simulate successful operation
                let result = format!("Successfully executed step: {}", step_name);
                info!("{}", result);

                Ok(result)
            }
        });

        // Execute the task
        match self.engine.execute_task(task).await {
            Ok(result) => {
                info!("Step {} completed successfully", step_index);
                self.steps[step_index].mark_executed(&result);
                self.executed_steps.push(step_index);
                Ok(())
            }
            Err(e) => {
                error!("Step {} failed: {}", step_index, e);
                self.should_compensate = true;
                self.compensation_reason =
                    Some(format!("Step {} failed during execution: {}", step_name, e));
                Err(e)
            }
        }
    }

    /// Compensate for a previously executed step
    async fn compensate_step(&mut self, step_index: usize) -> Result<()> {
        // Skip compensation if the step wasn't executed
        if !self.steps[step_index].executed {
            info!(
                "Skipping compensation for step {} as it was not executed",
                self.steps[step_index].name
            );
            return Ok(());
        }

        let step_name = self.steps[step_index].name.clone();
        info!("Compensating step {}: {}", step_index, step_name);

        // Create a task for this step's compensation function
        let task = task(&format!("compensate_step_{}", step_index), {
            let step_name = step_name.clone(); // Clone again to move into the closure
            move || async move {
                info!("Compensating step: {}", step_name);

                // Simulate compensation operation
                // In a real application, this would undo the operation

                // Randomly fail compensation to show error handling
                let should_fail = rand::thread_rng().gen_bool(0.1); // 10% chance of failure

                if should_fail {
                    error!("Step {} failed during compensation", step_name);
                    return Err(anyhow!("Failed to compensate step: {}", step_name));
                }

                // Simulate successful compensation
                let result = format!("Successfully compensated step: {}", step_name);
                info!("{}", result);

                Ok(result)
            }
        });

        // Execute the compensation task
        match self.engine.execute_task(task).await {
            Ok(result) => {
                info!("Step {} compensated successfully", step_index);
                self.steps[step_index].mark_compensated(&result);
                Ok(())
            }
            Err(e) => {
                error!("Failed to compensate step {}: {}", step_index, e);
                Err(e)
            }
        }
    }

    /// Run the saga workflow
    async fn run(&mut self) -> Result<WorkflowResult> {
        info!("Starting saga workflow with {} steps", self.steps.len());
        self.state.update_status("running");

        // Execute all steps
        for i in 0..self.steps.len() {
            if let Err(e) = self.execute_step(i).await {
                error!("Error executing step {}: {}", i, e);
                self.should_compensate = true;
                break;
            }
        }

        // If any step failed, compensate executed steps in reverse order
        if self.should_compensate {
            info!(
                "Initiating compensation for saga workflow: {}",
                self.compensation_reason
                    .as_deref()
                    .unwrap_or("Unknown error")
            );
            self.state.update_status("compensating");

            let executed_steps = self.executed_steps.clone();
            for &step_index in executed_steps.iter().rev() {
                if let Err(e) = self.compensate_step(step_index).await {
                    error!("Error during compensation of step {}: {}", step_index, e);
                    // Continue with other compensations
                }
            }

            // Create the final report with compensation details
            let _final_report = serde_json::json!({
                "status": "rolled_back",
                "steps_executed": self.executed_steps.len(),
                "steps_compensated": self.executed_steps.iter().filter(|&&i| self.steps[i].compensated).count(),
                "reason": self.compensation_reason.clone().unwrap_or_else(|| "Unknown error".to_string()),
                "step_details": self.steps.iter().map(|s| {
                    serde_json::json!({
                        "name": s.name,
                        "executed": s.executed,
                        "compensated": s.compensated,
                        "execution_result": s.execution_result,
                        "compensation_result": s.compensation_result
                    })
                }).collect::<Vec<_>>()
            });

            self.state.update_status("completed_with_errors");
            return Ok(WorkflowResult::failed(
                self.compensation_reason
                    .clone()
                    .unwrap_or_else(|| "Transaction rolled back".to_string()),
            ));
        }

        // All steps completed successfully
        info!("All steps in saga workflow completed successfully");

        // Create the final success report
        let final_report = serde_json::json!({
            "status": "completed",
            "steps_executed": self.steps.len(),
            "step_details": self.steps.iter().map(|s| {
                serde_json::json!({
                    "name": s.name,
                    "executed": s.executed,
                    "execution_result": s.execution_result
                })
            }).collect::<Vec<_>>()
        });

        self.state.update_status("completed");
        Ok(WorkflowResult::success(final_report))
    }
}

/// Create a saga workflow with test steps
fn create_saga_workflow(engine: WorkflowEngine) -> SagaWorkflow {
    let mut saga = SagaWorkflow::new(engine);

    // Add some test steps
    saga.add_step("Reserve Inventory");
    saga.add_step("Process Payment");
    saga.add_step("Update Order Status");
    saga.add_step("Send Notification");

    saga
}

/// Run the saga workflow
async fn run_saga_workflow() -> Result<()> {
    // Initialize telemetry
    let telemetry_config = TelemetryConfig::default();
    let _ = init_telemetry(telemetry_config);

    // Create a signal handler for the workflow engine
    let signal_handler = DefaultSignalHandler::new();

    // Create the workflow engine
    let engine = WorkflowEngine::new(signal_handler);

    // Create the saga workflow
    let mut workflow = create_saga_workflow(engine);

    // Run the workflow
    let result = workflow.run().await?;

    // Print the result
    if result.is_success() {
        println!("\nWorkflow completed successfully!");
        println!("{}", result.output());
    } else {
        println!("\nWorkflow failed!");
        println!("Error: {}", result.error().unwrap_or_default());
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    run_saga_workflow().await
}
