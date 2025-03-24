//! Integration tests for the terminal system

use std::time::Duration;
use tokio::time::sleep;

use mcp_agent::error::Error;
use mcp_agent::terminal::{
    config::{TerminalConfig, WebTerminalConfig},
    TerminalSystem, TerminalType,
};

/// Test the full terminal system functionality
#[tokio::test]
async fn test_terminal_system_integration() -> Result<(), Error> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    // Create a console-only configuration for testing
    let config = TerminalConfig::console_only();

    // Create and start the terminal system
    let terminal = TerminalSystem::new(config);
    terminal.start().await?;

    // Verify the terminal types
    assert!(terminal.is_terminal_enabled(TerminalType::Console).await);
    assert!(!terminal.is_terminal_enabled(TerminalType::Web).await);

    // Test writing to the terminal
    terminal.write("Test message\n").await?;

    // Clean up
    terminal.stop().await?;

    Ok(())
}

/// Test the dual terminal configuration
#[tokio::test]
async fn test_dual_terminal_system() -> Result<(), Error> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    // Create a terminal with both console and web enabled
    let config = TerminalConfig {
        console_terminal_enabled: true,
        web_terminal_enabled: true,
        web_terminal_config: WebTerminalConfig::default(),
        ..Default::default()
    };

    // Create and start the terminal system
    let terminal = TerminalSystem::new(config);
    terminal.start().await?;

    // Verify the terminal types
    assert!(terminal.is_terminal_enabled(TerminalType::Console).await);
    assert!(terminal.is_terminal_enabled(TerminalType::Web).await);

    // Verify the web terminal address
    let web_addr_result = terminal.web_terminal_address().await;
    assert!(web_addr_result.is_some());
    let addr_str = web_addr_result.unwrap();
    println!("Web terminal available at: {}", addr_str);

    // Test writing to the terminal
    terminal.write("Test message to dual terminals\n").await?;

    // Clean up
    terminal.stop().await?;

    Ok(())
}

/// Test toggling the web terminal
#[tokio::test]
async fn test_toggle_web_terminal() -> Result<(), Error> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    // Create a console-only config initially
    let config = TerminalConfig::console_only();

    // Create and start the terminal system
    let terminal = TerminalSystem::new(config);
    terminal.start().await?;
    assert!(terminal.is_terminal_enabled(TerminalType::Console).await);
    assert!(!terminal.is_terminal_enabled(TerminalType::Web).await);

    // Enable web terminal
    terminal.toggle_web_terminal(true).await?;
    assert!(terminal.is_terminal_enabled(TerminalType::Web).await);

    // Verify the web terminal address
    let web_addr_result = terminal.web_terminal_address().await;
    assert!(web_addr_result.is_some());
    let addr_str = web_addr_result.unwrap();
    println!("Web terminal available at: {}", addr_str);

    // Test writing to both terminals
    terminal
        .write("Test message after enabling web terminal\n")
        .await?;

    // Disable web terminal
    terminal.toggle_web_terminal(false).await?;
    assert!(!terminal.is_terminal_enabled(TerminalType::Web).await);

    // Clean up
    terminal.stop().await?;

    Ok(())
}

/// Test mock client to simulate input for test data
struct MockClient {
    system: TerminalSystem,
}

impl MockClient {
    async fn new() -> Result<Self, Error> {
        // Create a console-only configuration for testing
        let config = TerminalConfig::console_only();

        // Create and start the terminal system
        let system = TerminalSystem::new(config);
        system.start().await?;

        Ok(Self { system })
    }

    async fn simulate_input(&self, message: &str) -> Result<(), Error> {
        // In a real test, we would inject input into the terminal
        // For now, this is just a placeholder since we can't directly
        // inject into stdin in tests without additional mocking
        println!("Simulating input: {}", message);
        Ok(())
    }

    async fn get_output(&self) -> Result<String, Error> {
        // In a real test, we would capture the output
        // For now, this just returns a fixed string
        Ok("Simulated output".to_string())
    }

    async fn close(self) -> Result<(), Error> {
        self.system.stop().await
    }
}

/// Test sending and receiving data (simulation)
#[tokio::test]
async fn test_simulated_io() -> Result<(), Error> {
    // Create a mock client for simulated I/O
    let client = MockClient::new().await?;

    // Write some data to the terminal
    client.system.write("Hello, terminal!\n").await?;

    // Simulate sending input
    client.simulate_input("Test input").await?;

    // Allow time for processing
    sleep(Duration::from_millis(100)).await;

    // Get the output (simulated)
    let output = client.get_output().await?;
    println!("Received output: {}", output);

    // Clean up
    client.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_error_handling() -> Result<(), Error> {
    // Create a configuration that should fail to start
    let config = TerminalConfig {
        console_terminal_enabled: false,
        web_terminal_enabled: false,
        web_terminal_config: WebTerminalConfig::default(),
        ..Default::default()
    };

    // Create the terminal system
    let terminal = TerminalSystem::new(config);

    // Test error handling: stop before start
    let result = terminal.stop().await;
    // This should not error out in our implementation
    assert!(result.is_ok());

    // Start and then try to start again
    terminal.start().await?;
    let result = terminal.start().await;
    // Starting twice should not cause an error
    assert!(result.is_ok());

    // Clean up
    terminal.stop().await?;

    Ok(())
}
