use mcp_agent::mcp::jsonrpc::JsonRpcMethod;
use mcp_agent::mcp::resources::{
    FileSystemProvider, Resource, ResourceManager, ResourcesHandler, TextContent,
};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("=== MCP Resources Example ===");

    // Create a resource manager
    let mut manager = ResourceManager::new();

    // Set up a file system provider
    let current_dir = std::env::current_dir()?;
    let fs_provider = FileSystemProvider::new(current_dir.clone()).with_extensions(vec![
        "rs".to_string(),
        "toml".to_string(),
        "md".to_string(),
    ]);

    // Register the provider
    manager.register_provider(fs_provider);

    // Get the providers as a shared reference
    let providers = Arc::new(RwLock::new(manager.providers().to_vec()));

    // Create a resources handler
    let handler = ResourcesHandler::new(providers);

    // Set up JSON-RPC message handling
    let (tx, mut rx) = mpsc::channel(32);

    // Spawn a task to handle incoming messages
    tokio::spawn(async move {
        use mcp_agent::mcp::types::JsonRpcRequest;
        use serde_json::json;

        // Example: List resources request
        let list_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!("1"),
            method: "resources/list".to_string(),
            params: Some(json!({})),
            client_id: Some("client1".to_string()),
        };

        tx.send(list_request)
            .await
            .expect("Failed to send list request");

        // Wait a bit
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Example: Read a specific resource
        let uri = format!("file:///{}", "Cargo.toml");
        let read_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: json!("2"),
            method: "resources/read".to_string(),
            params: Some(json!({
                "uri": uri
            })),
            client_id: Some("client1".to_string()),
        };

        tx.send(read_request)
            .await
            .expect("Failed to send read request");
    });

    // Process received requests
    while let Some(request) = rx.recv().await {
        println!("\nReceived request: {}", request.method);

        // Handle the request
        match handler.handle(request.clone()).await {
            Ok(response) => {
                println!(
                    "Response: {}",
                    serde_json::to_string_pretty(&response.result).unwrap()
                );
            }
            Err(error) => {
                println!("Error: {:?}", error);
            }
        }

        // Exit after processing a read request
        if request.method == "resources/read" {
            break;
        }
    }

    // Create a custom resource
    println!("\n=== Custom Resource Example ===");

    let custom_resource = Resource::new("custom:hello-world", "Hello World Resource")
        .with_description("A simple example resource")
        .with_mime_type("text/plain");

    println!("Custom resource: {:#?}", custom_resource);

    let content = TextContent::new(
        "custom:hello-world",
        "Hello, World! This is a sample resource content.",
    )
    .with_mime_type("text/plain");

    println!("Resource content: {:#?}", content);

    Ok(())
}
