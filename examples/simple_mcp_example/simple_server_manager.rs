use mcp_agent::mcp::{ServerManager, ManagerServerSettings as ServerSettings};
use mcp_agent::mcp::types::{Message, MessageType, Priority};
use serde_json::json;
use std::collections::HashMap;
use std::time::Duration;

/// A simplified example of using the ServerManager to manage MCP servers
///
/// This example shows how to:
/// 1. Create a ServerManager
/// 2. Register server configurations
/// 3. Set up initialization hooks
/// 4. Start servers and send messages
///
/// Example usage:
/// ```
/// cargo run --example simple_server_manager
/// ```
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    println!("Starting simple server manager example");
    
    // Create a new server manager
    let manager = ServerManager::new();
    
    // Register a mock server
    let mock_server_settings = ServerSettings {
        transport: "stdio".to_string(),
        command: Some("echo".to_string()),
        args: Some(vec!["Hello MCP".to_string()]),
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
    
    println!("Registering mock server...");
    manager.register_server("mock-server", mock_server_settings).await?;
    
    // Register an initialization hook
    println!("Registering initialization hook...");
    manager.register_init_hook("mock-server", |name, connection| {
        println!("Server '{}' has been initialized with address: {}", name, connection.addr());
        Ok(())
    }).await?;
    
    // Get the list of registered servers
    let servers = manager.get_server_names().await;
    println!("Registered servers: {:?}", servers);
    
    // Create a JSON message
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
    
    println!("Server manager example completed successfully");
    Ok(())
} 