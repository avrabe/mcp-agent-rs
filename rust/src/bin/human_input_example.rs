use anyhow::Result;
use colored::Colorize;
use tokio::time::Duration;

use mcp_agent::human_input::{ConsoleInputHandler, HumanInputHandler, HumanInputRequest};
use mcp_agent::telemetry::{TelemetryConfig, init_telemetry};

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

    Ok(())
}
