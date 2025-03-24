//! Dual Terminal Example
//!
//! This example demonstrates using both console and web terminals simultaneously.
//! It shows how terminal input and output are synchronized across both interfaces.

use mcp_agent::{
    error::Result,
    terminal::{
        config::{AuthConfig, AuthMethod, TerminalConfig, WebTerminalConfig},
        TerminalSystem,
    },
};
use std::{net::IpAddr, str::FromStr, time::Duration};
use tokio::signal;
use tracing::{error, info, Level};
use tracing_subscriber::fmt;

#[tokio::main]
async fn main() -> Result<()> {
    // Set up logging
    fmt::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .init();

    info!("Starting dual terminal example");

    // Create a terminal configuration with both console and web enabled
    let mut config = TerminalConfig::default();

    // Enable console terminal
    config.console_terminal_enabled = true;

    // Enable and configure web terminal
    config.web_terminal_enabled = true;
    config.web_terminal_config = WebTerminalConfig {
        host: IpAddr::from_str("127.0.0.1").unwrap(),
        port: 9876,
        auth_config: AuthConfig {
            auth_method: AuthMethod::Jwt,
            jwt_secret: "secret-key-for-jwt-token-generation".to_string(),
            token_expiration_secs: 3600,
            username: "admin".to_string(),
            password: "password".to_string(),
            allow_anonymous: true,
            require_authentication: false,
        },
        enable_visualization: true,
    };

    // Create the terminal system
    let terminal = TerminalSystem::new(config);

    // Print welcome message
    terminal
        .write("\n\n===================================\n")
        .await?;
    terminal
        .write("🌟 MCP-Agent Dual Terminal Demo 🌟\n")
        .await?;
    terminal
        .write("===================================\n\n")
        .await?;

    // Show info about the web terminal
    if let Some(addr) = terminal.web_terminal_address().await {
        terminal
            .write(&format!("Web terminal available at: {}\n\n", addr))
            .await?;
    } else {
        terminal.write("Web terminal is not enabled\n\n").await?;
    }

    // Show help
    terminal.write("Commands:\n").await?;
    terminal.write("  help - Show this help\n").await?;
    terminal.write("  exit - Exit the application\n").await?;
    terminal
        .write("  echo <message> - Echo a message\n")
        .await?;
    terminal.write("  time - Show current time\n\n").await?;

    // Spawn a task to handle Ctrl+C
    let term_clone = terminal.clone();
    tokio::spawn(async move {
        match signal::ctrl_c().await {
            Ok(()) => {
                info!("Shutting down terminal system gracefully");
                if let Err(e) = term_clone.stop().await {
                    error!("Error stopping terminal system: {}", e);
                }
            }
            Err(err) => {
                error!("Error waiting for ctrl-c: {}", err);
            }
        }
    });

    // Main application loop
    let mut running = true;
    while running {
        // Read input from any terminal
        let input = match terminal.read().await {
            Ok(input) => input.trim().to_string(),
            Err(e) => {
                error!("Error reading from terminal: {}", e);
                // Brief delay to avoid tight loop on error
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };

        // Process commands
        match input.as_str() {
            "exit" => {
                terminal.write("Exiting...\n").await?;
                running = false;
            }
            "help" => {
                terminal.write("Commands:\n").await?;
                terminal.write("  help - Show this help\n").await?;
                terminal.write("  exit - Exit the application\n").await?;
                terminal
                    .write("  echo <message> - Echo a message\n")
                    .await?;
                terminal.write("  time - Show current time\n").await?;
            }
            "time" => {
                let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                terminal.write(&format!("Current time: {}\n", time)).await?;
            }
            cmd if cmd.starts_with("echo ") => {
                let message = cmd.strip_prefix("echo ").unwrap_or("");
                terminal.write(&format!("{}\n", message)).await?;
            }
            "" => {
                // Ignore empty input
            }
            _ => {
                terminal
                    .write(&format!("Unknown command: {}\n", input))
                    .await?;
            }
        }
    }

    // Wait a moment before shutdown to ensure everything completes
    tokio::time::sleep(Duration::from_millis(500)).await;

    info!("Example completed successfully");
    Ok(())
}
