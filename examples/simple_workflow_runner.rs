use std::collections::HashMap;
use std::time::Duration;
use std::sync::Arc;

use anyhow::Result;
use tokio::time;
use tracing::{info, debug};

use mcp_agent::workflow::{
    Workflow, WorkflowEngine, WorkflowResult, WorkflowState,
    execute_workflow, task,
};
use mcp_agent::telemetry::{init_telemetry, TelemetryConfig};
use mcp_agent::terminal::{TerminalConfig, initialize_terminal, initialize_visualization};
use mcp_agent::terminal::config::AuthConfig;

#[derive(Debug)]
struct SimpleWorkflow {
    engine: WorkflowEngine,
    name: String,
}

impl SimpleWorkflow {
    fn new(engine: WorkflowEngine, name: String) -> Self {
        Self { engine, name }
    }
    
    #[task]
    async fn first_task(&self) -> WorkflowResult<String> {
        debug!("Executing first task");
        // Simulate some work
        time::sleep(Duration::from_secs(1)).await;
        
        Ok("First task completed".to_string())
    }
    
    #[task]
    async fn second_task(&self, input: &str) -> WorkflowResult<String> {
        debug!("Executing second task with input: {}", input);
        // Simulate some more work
        time::sleep(Duration::from_secs(2)).await;
        
        Ok(format!("{} and second task completed", input))
    }
    
    #[task]
    async fn final_task(&self, input: &str) -> WorkflowResult<String> {
        debug!("Executing final task with input: {}", input);
        // Simulate final work
        time::sleep(Duration::from_secs(1)).await;
        
        Ok(format!("Workflow result: {} and final task completed", input))
    }
}

#[async_trait::async_trait]
impl Workflow<String> for SimpleWorkflow {
    // Define the workflow's execution path
    async fn run(&self) -> WorkflowResult<String> {
        debug!("Starting workflow: {}", self.name);
        
        // Execute the first task
        let first_result = self.first_task().await?;
        
        // Execute the second task, passing the first result
        let second_result = self.second_task(&first_result).await?;
        
        // Execute the final task, passing the second result
        let final_result = self.final_task(&second_result).await?;
        
        Ok(final_result)
    }
    
    fn engine(&self) -> &WorkflowEngine {
        &self.engine
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize telemetry
    let telemetry_config = TelemetryConfig {
        service_name: "simple-workflow-runner".to_string(),
        enable_console: true,
        ..Default::default()
    };
    init_telemetry(telemetry_config)?;
    
    // Create the workflow engine
    let engine = WorkflowEngine::new();
    
    // Setup terminal with visualization enabled
    let terminal_config = TerminalConfig {
        console_enabled: true,
        web_terminal_enabled: true,
        web_terminal_host: "127.0.0.1".to_string(),
        web_terminal_port: 8080,
        auth_config: AuthConfig::default(),
    };
    
    // Initialize the terminal system
    let terminal_system = initialize_terminal(terminal_config).await?;
    
    // Initialize visualization
    let graph_manager = initialize_visualization(
        &terminal_system,
        Some(engine.clone()),
        vec![],
        vec![],
        None,
    ).await;
    
    if let Some(graph_manager) = graph_manager {
        println!("Visualization system started at http://127.0.0.1:8080");
        println!("Open the URL in your browser and click 'Show Visualization'");
    }
    
    // Create a workflow
    let workflow = SimpleWorkflow::new(engine, "Simple Demo Workflow".to_string());
    
    // Execute the workflow
    info!("Starting workflow execution");
    let result = execute_workflow(workflow).await?;
    info!("Workflow completed with result: {}", result);
    
    // Keep the process running for a few seconds to view the visualization
    println!("Keeping server alive for 30 seconds so you can view the visualization...");
    time::sleep(Duration::from_secs(30)).await;
    
    Ok(())
} 