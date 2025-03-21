//! Dual Terminal Example
//!
//! This example demonstrates using both console and web terminals simultaneously.
//! It shows how terminal input and output are synchronized across both interfaces.

use std::error::Error as StdError;
use std::time::Duration;

// use mcp_agent::error::Error;
use mcp_agent::terminal::config::{AuthConfig, AuthMethod, TerminalConfig};
use mcp_agent::terminal::TerminalSystem;
use tracing::{error, info, Level};
use tracing_subscriber::fmt;

#[tokio::main]
async fn main() -> Result<(), Box<dyn StdError>> {
    // Set up logging
    fmt::fmt()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .init();

    info!("Starting dual terminal example");

    // Create a terminal configuration with both console and web enabled
    let mut config = TerminalConfig::dual_terminal();
    
    // Configure web terminal settings
    config.web_terminal_host = "127.0.0.1".parse().unwrap();
    config.web_terminal_port = 9876;
    config.require_authentication = false;
    
    // Configure authentication
    config.auth_config = AuthConfig {
        auth_method: AuthMethod::None, // No auth for easy testing
        jwt_secret: "test-secret".to_string(),
        token_expiration_secs: 3600,
        username: "user".to_string(),
        password: "password".to_string(),
        allow_anonymous: true,
    };

    // Create the terminal system
    let terminal = TerminalSystem::new(config);

    // Start the terminal system
    info!("Starting terminal system...");
    if let Err(e) = terminal.start().await {
        error!("Failed to start terminal system: {}", e);
        return Err(e.into());
    }

    // Print welcome message
    if let Err(e) = terminal.write("\n\n===================================\n").await {
        error!("Write error: {}", e);
    }
    if let Err(e) = terminal.write("🌟 MCP-Agent Dual Terminal Demo 🌟\n").await {
        error!("Write error: {}", e);
    }
    if let Err(e) = terminal.write("===================================\n\n").await {
        error!("Write error: {}", e);
    }

    // Show info about the web terminal
    if let Some(addr) = terminal.web_terminal_address() {
        if let Err(e) = terminal.write(&format!("Web terminal available at: {}\n\n", addr)).await {
            error!("Write error: {}", e);
        }
    } else {
        if let Err(e) = terminal.write("Web terminal is not enabled\n\n").await {
            error!("Write error: {}", e);
        }
    }

    // Show help
    if let Err(e) = terminal.write("Commands:\n").await {
        error!("Write error: {}", e);
    }
    if let Err(e) = terminal.write("  help - Show this help\n").await {
        error!("Write error: {}", e);
    }
    if let Err(e) = terminal.write("  exit - Exit the application\n").await {
        error!("Write error: {}", e);
    }
    if let Err(e) = terminal.write("  echo <message> - Echo a message\n").await {
        error!("Write error: {}", e);
    }
    if let Err(e) = terminal.write("  time - Show current time\n\n").await {
        error!("Write error: {}", e);
    }

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
                if let Err(e) = terminal.write("Exiting...\n").await {
                    error!("Write error: {}", e);
                }
                running = false;
            }
            "help" => {
                if let Err(e) = terminal.write("Commands:\n").await {
                    error!("Write error: {}", e);
                }
                if let Err(e) = terminal.write("  help - Show this help\n").await {
                    error!("Write error: {}", e);
                }
                if let Err(e) = terminal.write("  exit - Exit the application\n").await {
                    error!("Write error: {}", e);
                }
                if let Err(e) = terminal.write("  echo <message> - Echo a message\n").await {
                    error!("Write error: {}", e);
                }
                if let Err(e) = terminal.write("  time - Show current time\n").await {
                    error!("Write error: {}", e);
                }
            }
            "time" => {
                let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
                if let Err(e) = terminal.write(&format!("Current time: {}\n", time)).await {
                    error!("Write error: {}", e);
                }
            }
            cmd if cmd.starts_with("echo ") => {
                let message = cmd.strip_prefix("echo ").unwrap_or("");
                if let Err(e) = terminal.write(&format!("{}\n", message)).await {
                    error!("Write error: {}", e);
                }
            }
            "" => {
                // Ignore empty input
            }
            _ => {
                if let Err(e) = terminal.write(&format!("Unknown command: {}\n", input)).await {
                    error!("Write error: {}", e);
                }
            }
        }
    }

    // Stop the terminal system
    info!("Stopping terminal system...");
    if let Err(e) = terminal.stop().await {
        error!("Failed to stop terminal system: {}", e);
        return Err(e.into());
    }

    info!("Example completed successfully");
    Ok(())
} 