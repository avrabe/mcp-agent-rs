use mcp_agent::mcp::{ServerManager, ManagerServerSettings as ServerSettings};
use mcp_agent::mcp::types::{Message, MessageType, Priority};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

/// An example of using the ServerManager to start and communicate with MCP servers
///
/// This example shows how to:
/// 1. Create a ServerManager
/// 2. Register server configurations manually or load from config
/// 3. Set up initialization hooks
/// 4. Start servers
/// 5. Send messages to servers
/// 6. Stop servers
///
/// Example usage:
/// ```
/// cargo run --example server_manager_example
/// ```
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    println!("Starting server manager example");
    
    // Example 1: Create a new server manager and register servers manually
    println!("\n=== Example 1: Manual Server Registration ===");
    let manager = ServerManager::new();
    
    // Register a mock server
    let mock_server_settings = ServerSettings {
        transport: "stdio".to_string(),
        command: Some("cargo".to_string()),
        args: Some(vec!["run".to_string(), "--bin".to_string(), "mock_server".to_string()]),
        url: None,
        env: Some(HashMap::from([
            ("MCP_SERVER_PORT".to_string(), "8765".to_string()),
            ("DEBUG".to_string(), "1".to_string()),
        ])),
        auth: None,
        read_timeout_seconds: Some(30),
        auto_reconnect: true,
        max_reconnect_attempts: 3,
    };
    
    manager.register_server("mock-server", mock_server_settings).await?;
    
    // Register an initialization hook
    manager.register_init_hook("mock-server", |name, _connection| {
        println!("Server '{}' has been initialized", name);
        Ok(())
    }).await?;
    
    // Start the server
    println!("Starting mock server...");
    let connection = manager.start_server("mock-server").await?;
    println!("Mock server started with address: {}", connection.addr());
    
    // Send a ping message
    println!("Sending ping message...");
    let ping_message = Message::new(
        MessageType::Request,
        Priority::Normal,
        "ping".as_bytes().to_vec(),
        None,
        None,
    );
    
    let response = manager.send_message("mock-server", ping_message).await?;
    println!("Received response: {:?}", response);
    
    // Stop the server
    println!("Stopping server...");
    manager.stop_server("mock-server").await?;
    println!("Server stopped");
    
    // Example 2: Create a server manager and load from config
    println!("\n=== Example 2: Loading from Configuration ===");
    
    // Create a temporary config file for demonstration
    println!("Creating temporary configuration file...");
    let temp_config = std::env::temp_dir().join("mcp_example_config.yaml");
    let config_content = r#"
# MCP Agent Configuration Example
demo-server:
  transport: stdio
  command: cargo
  args:
    - run
    - --bin
    - mock_server
  env:
    MCP_SERVER_PORT: "8766"
    DEBUG: "1"
  read_timeout_seconds: 30
  auto_reconnect: true
  max_reconnect_attempts: 3
"#;
    
    std::fs::write(&temp_config, config_content)?;
    println!("Created config at: {}", temp_config.display());
    
    // Create a manager and load settings from the config file
    let config_manager = ServerManager::new();
    config_manager.load_from_file(&temp_config).await?;
    
    // Register another initialization hook
    config_manager.register_init_hook("demo-server", |name, connection| {
        println!("Server '{}' initialized with connection to {}", name, connection.addr());
        Ok(())
    }).await?;
    
    // Get the list of registered servers
    let servers = config_manager.get_server_names().await;
    println!("Registered servers: {:?}", servers);
    
    // Start the server from config
    println!("Starting server from config...");
    config_manager.start_server("demo-server").await?;
    println!("Server started from config");
    
    // Send a JSON message
    println!("Sending JSON message...");
    let json_message = Message::new(
        MessageType::Request,
        Priority::High,
        serde_json::to_vec(&json!({
            "command": "get_status",
            "params": {
                "detailed": true
            }
        }))?,
        None,
        None,
    );
    
    let response = config_manager.send_message("demo-server", json_message).await?;
    if let Ok(json_str) = std::str::from_utf8(&response.payload) {
        println!("Received JSON response: {}", json_str);
    } else {
        println!("Received binary response: {:?}", response.payload);
    }
    
    // Wait a bit
    println!("Waiting for 2 seconds...");
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Check server connectivity stats
    let connected_count = config_manager.connected_server_count().await;
    println!("Connected servers: {}", connected_count);
    
    // Stop all servers
    println!("Stopping all servers...");
    config_manager.stop_all_servers().await?;
    println!("All servers stopped");
    
    // Clean up the temporary config file
    std::fs::remove_file(temp_config)?;
    println!("Cleaned up temporary config file");
    
    println!("\nServer manager example completed successfully!");
    Ok(())
} 