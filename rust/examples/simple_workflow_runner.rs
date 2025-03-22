use anyhow::Result;
use colored::Colorize;
use std::collections::HashMap;
use std::time::Duration;
use tracing::info;

use mcp_agent::telemetry::{init_telemetry, TelemetryConfig};
use mcp_agent::workflow::signal::AsyncSignalHandler;
use mcp_agent::workflow::state::WorkflowState;
use mcp_agent::workflow::{execute_workflow, Workflow, WorkflowEngine, WorkflowResult};

// Simple workflow definition
struct SimpleWorkflow {
    state: WorkflowState,
    engine: WorkflowEngine,
    result: Option<String>,
}

impl SimpleWorkflow {
    fn new(engine: WorkflowEngine) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert("type".to_string(), serde_json::json!("simple"));

        Self {
            state: WorkflowState::new(Some("SimpleWorkflow".to_string()), Some(metadata)),
            engine,
            result: None,
        }
    }
}

#[async_trait::async_trait]
impl Workflow for SimpleWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        self.update_status("running").await;

        // Simulate workflow execution
        info!("Executing simple workflow...");
        tokio::time::sleep(Duration::from_secs(1)).await;

        self.result = Some("Workflow completed successfully!".to_string());
        self.update_status("completed").await;

        Ok(WorkflowResult::success(
            self.result.clone().unwrap_or_default(),
        ))
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
    // Initialize telemetry with correct config
    let telemetry_config = TelemetryConfig {
        service_name: "simple-workflow-runner".to_string(),
        otlp_endpoint: None,
        jaeger_endpoint: None,
        enable_console: true,
        enable_json: false,
        enable_tracing: true,
        enable_metrics: true,
        sampling_ratio: 1.0,
        attributes: HashMap::new(),
        alerting_config: None,
    };

    // Handle telemetry initialization errors explicitly
    if let Err(e) = init_telemetry(telemetry_config) {
        eprintln!("Warning: Failed to initialize telemetry: {}", e);
    }

    println!("\n{}", "Simple Workflow Demo".bold().green());
    println!(
        "{}",
        "This demonstrates a basic workflow execution.".yellow()
    );

    // Create the workflow engine with signal handler
    let engine = WorkflowEngine::new(AsyncSignalHandler::new());

    // Create and initialize the workflow
    let workflow = SimpleWorkflow::new(engine);

    println!("\n{}", "Starting workflow execution...".cyan());

    // Execute the workflow
    match execute_workflow(workflow).await {
        Ok(result) => {
            println!("\n{}", "Workflow Result:".bold().green());
            println!("{}", result.output());
            println!(
                "Workflow completed in {} ms",
                result.duration_ms().unwrap_or(0)
            );
        }
        Err(e) => {
            println!("\n{}", "Workflow Error:".bold().red());
            println!("{:?}", e);
        }
    }

    Ok(())
}
