use mcp_agent::mcp::agent::Agent;
use mcp_agent::mcp::types::{Message, MessageType, Priority};
use serde_json::json;
use std::time::Duration;

/// A simple example of using the MCP agent to connect to a server
/// and send messages programmatically.
///
/// Run a mock server first with:
/// cargo run --bin mock_server
///
/// Then run this example:
/// cargo run --example simple_client
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();

    println!("Starting simple MCP client example");

    // Create a new agent
    let agent = Agent::new(None);

    // Connect to the mock server
    let server_id = "test-server";
    let server_address = "127.0.0.1:7000";

    println!("Connecting to MCP server at {}", server_address);
    agent
        .connect_to_test_server(server_id, server_address)
        .await?;
    println!("Connected to server: {}", server_id);

    // List active connections
    let count = agent.connection_count().await;
    let connections = agent.list_connections().await;
    println!("Connected to {} servers: {}", count, connections.join(", "));

    // Send a ping message
    println!("\nSending ping message...");
    let message = Message::new(
        MessageType::Request,
        Priority::Normal,
        "ping".as_bytes().to_vec(),
        None,
        None,
    );

    agent.send_message(server_id, message).await?;
    println!("Ping message sent");

    // Small delay to ensure message processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Send a JSON request
    println!("\nSending JSON request...");
    let json_request = json!({
        "command": "get_status",
        "params": {
            "detailed": true
        }
    });

    let json_bytes = serde_json::to_vec(&json_request)?;
    let message = Message::new(MessageType::Request, Priority::High, json_bytes, None, None);

    agent.send_message(server_id, message).await?;
    println!("JSON request sent");

    // Small delay to ensure message processing
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Execute a function with arguments
    println!("\nExecuting function...");
    let args = json!({
        "verbose": true,
        "timeout": 5000
    });

    let result = agent
        .execute_task("get_status", args, Some(Duration::from_secs(30)))
        .await?;

    println!("Function result: {:?}", result);

    // Disconnect from server
    println!("\nDisconnecting from server...");
    agent.disconnect(server_id).await?;
    println!("Disconnected from server: {}", server_id);

    // Final connection count
    let count = agent.connection_count().await;
    println!("Connected servers count: {}", count);

    println!("Simple MCP client example completed successfully");
    Ok(())
}
