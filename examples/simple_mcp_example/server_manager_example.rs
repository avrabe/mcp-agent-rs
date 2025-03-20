use mcp_agent::mcp::server_manager::{ServerManager, ServerSettings};
use mcp_agent::mcp::types::{Message, MessageType, Priority};
use serde_json::json;
use std::time::Duration;

/// An example of using the ServerManager to start and communicate with MCP servers
///
/// This example shows how to:
/// 1. Create a ServerManager
/// 2. Register server configurations
/// 3. Start servers
/// 4. Send messages to servers
/// 5. Stop servers
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
    
    // Create a new server manager
    let manager = ServerManager::new();
    
    // Register a mock server
    let mock_server_settings = ServerSettings {
        transport: "stdio".to_string(),
        command: Some("cargo".to_string()),
        args: Some(vec!["run".to_string(), "--bin".to_string(), "mock_server".to_string()]),
        url: None,
        env: None,
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
    manager.start_server("mock-server").await?;
    println!("Mock server started");
    
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
    
    let response = manager.send_message("mock-server", json_message).await?;
    println!("Received response: {:?}", response);
    
    // Wait a bit
    println!("Waiting for 2 seconds...");
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Check connection status
    let is_connected = manager.is_server_connected("mock-server").await;
    println!("Server is connected: {}", is_connected);
    
    // Stop the server
    println!("Stopping server...");
    manager.stop_server("mock-server").await?;
    println!("Server stopped");
    
    // Wait a bit and check connection status again
    tokio::time::sleep(Duration::from_secs(1)).await;
    let is_connected = manager.is_server_connected("mock-server").await;
    println!("Server is connected: {}", is_connected);
    
    println!("Server manager example completed");
    Ok(())
} 