use anyhow::Result;
use async_trait::async_trait;
use colored::Colorize;
use std::collections::HashMap;
use std::time::Duration;
use tracing::info;

use mcp_agent::workflow::{execute_workflow, task, Workflow, WorkflowResult, WorkflowState};

// A simple workflow with sequential tasks
struct BasicWorkflow {
    state: WorkflowState,
    input_data: String,
    intermediate_result: Option<String>,
    final_result: Option<String>,
}

impl BasicWorkflow {
    // Create a new workflow with input data
    fn new(input_data: String) -> Self {
        let mut metadata = HashMap::new();
        metadata.insert(
            "input_size".to_string(),
            serde_json::json!(input_data.len()),
        );

        Self {
            state: WorkflowState::new(Some("BasicWorkflow".to_string()), Some(metadata)),
            input_data,
            intermediate_result: None,
            final_result: None,
        }
    }

    // Create a preprocessing task
    fn create_preprocessing_task(&self) -> task::WorkflowTask<String> {
        let input_data = self.input_data.clone();

        task::task("preprocessing", move || async move {
            info!("Preprocessing data");

            // Simple preprocessing: convert to uppercase
            let result = input_data.to_uppercase();

            // Simulate processing time
            tokio::time::sleep(Duration::from_millis(500)).await;

            Ok(result)
        })
    }

    // Create an analysis task
    fn create_analysis_task(&self, preprocessed_data: String) -> task::WorkflowTask<String> {
        task::task("analysis", move || async move {
            info!("Analyzing data");

            // Simple analysis: count words
            let word_count = preprocessed_data.split_whitespace().count();
            let result = format!(
                "Analysis result: {} contains {} words",
                preprocessed_data, word_count
            );

            // Simulate processing time
            tokio::time::sleep(Duration::from_millis(800)).await;

            Ok(result)
        })
    }
}

#[async_trait]
impl Workflow for BasicWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        // Initialize workflow
        self.state.set_status("starting");
        info!("Starting BasicWorkflow");

        // Execute preprocessing task
        self.state.set_status("preprocessing");
        let preprocessing_task = self.create_preprocessing_task();
        let preprocessed_data = preprocessing_task.execute().await?;
        self.intermediate_result = Some(preprocessed_data.clone());

        // Execute analysis task
        self.state.set_status("analyzing");
        let analysis_task = self.create_analysis_task(preprocessed_data);
        let analysis_result = analysis_task.execute().await?;
        self.final_result = Some(analysis_result.clone());

        // Complete workflow
        self.state.set_status("completed");

        // Return the final result
        Ok(WorkflowResult::success(analysis_result))
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
    // Set up console logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("\n{}", "Basic Workflow Example".bold().green());
    println!(
        "{}",
        "This example demonstrates a simple workflow with sequential tasks.".yellow()
    );

    // Create input data
    let input_data = "This is a sample text that will be processed by our workflow.".to_string();

    // Create and execute the workflow
    let workflow = BasicWorkflow::new(input_data);

    println!("\n{}", "Starting workflow execution...".cyan());

    // Execute the workflow
    match execute_workflow(workflow).await {
        Ok(result) => {
            println!("\n{}", "Workflow Result:".bold().green());
            println!("{}", result.output());

            // Print the workflow steps
            println!("\n{}", "Workflow Steps:".bold().cyan());
            println!(
                "1. {} - Converts input text to uppercase",
                "Preprocessing".yellow()
            );
            println!(
                "2. {} - Counts the number of words in the text",
                "Analysis".yellow()
            );
        }
        Err(e) => {
            println!("\n{}", "Workflow Error:".bold().red());
            println!("{:?}", e);
        }
    }

    Ok(())
}
