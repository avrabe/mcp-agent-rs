use anyhow::Result;
use colored::Colorize;
use tokio::time::Duration;
use tracing::info;

use mcp_agent::human_input::{ConsoleInputHandler, HumanInputHandler, HumanInputRequest};
use mcp_agent::telemetry::{init_telemetry, TelemetryConfig};
use mcp_agent::workflow::signal::AsyncSignalHandler;
use mcp_agent::workflow::{Workflow, WorkflowEngine, WorkflowResult, WorkflowState};
use serde_json::Value;
use std::collections::HashMap;

// Example workflow that includes human input
struct HumanInputWorkflow {
    state: WorkflowState,
    handler: ConsoleInputHandler,
}

impl HumanInputWorkflow {
    fn new(name: &str) -> Self {
        let state = WorkflowState::new(Some(name.to_string()), None);

        Self {
            state,
            handler: ConsoleInputHandler::new(),
        }
    }

    async fn process_step(
        &self,
        engine: &WorkflowEngine,
        step_name: &str,
        prompt: &str,
    ) -> Result<String> {
        println!(
            "\n{} {}\n",
            "Workflow Step:".bold().blue(),
            step_name.bold()
        );

        let request = HumanInputRequest::new(prompt)
            .with_description(format!("Workflow step: {}", step_name))
            .with_workflow_id(engine.id().to_string());

        match self.handler.handle_request(request).await {
            Ok(response) => {
                println!(
                    "\n{} {}\n",
                    "Response received:".bold().green(),
                    response.response
                );
                Ok(response.response)
            }
            Err(e) => {
                println!("\n{} {}\n", "Error:".bold().red(), e);
                Err(e)
            }
        }
    }
}

#[async_trait::async_trait]
impl Workflow for HumanInputWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        info!("Starting human input workflow");

        // Create signal handler for the workflow engine
        let signal_handler = AsyncSignalHandler::new();
        let engine = WorkflowEngine::new(signal_handler);

        // Record starting workflow in state
        self.state.set_metadata("event", "workflow_started");
        self.state
            .set_metadata("description", "Starting human input workflow");

        // First step: Get user name
        let name = self
            .process_step(&engine, "User Information", "What is your name?")
            .await?;
        self.state.update(HashMap::from([(
            "user_name".to_string(),
            Value::String(name.clone()),
        )]));

        // Second step: Get user preference
        let favorite_thing = self
            .process_step(
                &engine,
                "User Preferences",
                &format!("Hi {}! What's your favorite thing to do?", name),
            )
            .await?;

        self.state.update(HashMap::from([(
            "favorite_thing".to_string(),
            Value::String(favorite_thing.clone()),
        )]));

        // Final step: Get confirmation
        let confirmation = self
            .process_step(
                &engine,
                "Confirmation",
                &format!(
                    "You said your name is {} and you like {}. Is this correct? (yes/no)",
                    name, favorite_thing
                ),
            )
            .await?;

        self.state.update(HashMap::from([(
            "confirmation".to_string(),
            Value::String(confirmation.clone()),
        )]));

        if confirmation.to_lowercase().contains("yes") {
            self.state.set_metadata("event", "workflow_completed");
            self.state
                .set_metadata("description", "User confirmed information");
            println!("\n{}\n", "Workflow completed successfully!".bold().green());
        } else {
            self.state.set_metadata("event", "workflow_incomplete");
            self.state
                .set_metadata("description", "User did not confirm information");
            println!(
                "\n{}\n",
                "Workflow incomplete - information not confirmed"
                    .bold()
                    .yellow()
            );
        }

        Ok(WorkflowResult::success("Workflow completed"))
    }

    fn state(&self) -> &WorkflowState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut WorkflowState {
        &mut self.state
    }
}

async fn run_workflow_example() -> Result<()> {
    println!(
        "\n\n{}\n{}\n",
        "Workflow Example".bold().green(),
        "=".repeat(50)
    );
    println!("This demonstrates human input in a workflow context\n");

    // Create and run the workflow
    let workflow = HumanInputWorkflow::new("HumanInputExample");

    match mcp_agent::workflow::execute_workflow(workflow).await {
        Ok(_) => {
            println!("\n{}\n", "Workflow executed successfully".bold().green());
        }
        Err(e) => {
            println!("\n{} {}\n", "Workflow execution failed:".bold().red(), e);
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize telemetry
    let config = TelemetryConfig::default();
    let _guard = init_telemetry(config);

    println!("{}", "Human Input Example".bold().green());
    println!("{}", "=".repeat(50));
    println!("This example demonstrates the human input functionality.\n");

    // Create a handler
    let handler = ConsoleInputHandler::new();

    // Simple input request
    let simple_request = HumanInputRequest::new("What is your name?");
    let name_response = handler.handle_request(simple_request).await?;

    println!("\n{} {}\n", "You entered:".bold(), name_response.response);

    // Request with description and timeout
    let timeout_request = HumanInputRequest::new("What is your favorite color?")
        .with_description("Please answer in 10 seconds")
        .with_timeout(10);

    match handler.handle_request(timeout_request).await {
        Ok(response) => {
            println!(
                "\n{} {}\n",
                "Your favorite color is:".bold(),
                response.response.bold().color(response.response.as_str())
            );
        }
        Err(e) => {
            println!("\n{} {}\n", "Error:".bold().red(), e);
        }
    }

    // Multiple-choice question
    let options = ["Option 1", "Option 2", "Option 3"];
    let mut prompt = "Select an option:\n".to_string();

    for (i, option) in options.iter().enumerate() {
        prompt.push_str(&format!("{}) {}\n", i + 1, option));
    }

    let choice_request =
        HumanInputRequest::new(prompt).with_description("Multiple choice question");

    let choice_response = handler.handle_request(choice_request).await?;

    // Parse the response
    if let Ok(choice) = choice_response.response.parse::<usize>() {
        if choice > 0 && choice <= options.len() {
            println!(
                "\n{} {}\n",
                "You selected:".bold(),
                options[choice - 1].bold()
            );
        } else {
            println!("\n{}\n", "Invalid selection".bold().red());
        }
    } else {
        println!(
            "\n{} {}\n",
            "Invalid input:".bold().red(),
            choice_response.response
        );
    }

    // Final question with a 5-second delay to see how it works
    println!("{}", "Processing something...".italic());
    tokio::time::sleep(Duration::from_secs(5)).await;

    let final_request = HumanInputRequest::new("Did you enjoy this demo? (yes/no)")
        .with_description("Final question");

    let final_response = handler.handle_request(final_request).await?;

    if final_response.response.to_lowercase().contains("yes") {
        println!("\n{}\n", "Great! Thanks for trying it out.".bold().green());
    } else {
        println!("\n{}\n", "I'll try to improve next time.".bold().yellow());
    }

    // Run the workflow example if user wants to continue
    let continue_request =
        HumanInputRequest::new("Would you like to see the workflow example? (yes/no)")
            .with_description("Continue to workflow example");

    let continue_response = handler.handle_request(continue_request).await?;

    if continue_response.response.to_lowercase().contains("yes") {
        run_workflow_example().await?;
    } else {
        println!(
            "\n{}\n",
            "Skipping workflow example. Goodbye!".bold().blue()
        );
    }

    Ok(())
}
