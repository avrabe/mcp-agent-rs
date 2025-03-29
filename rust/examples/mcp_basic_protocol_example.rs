///! This example demonstrates the basic features of the MCP protocol implementation.
///! It shows initialization, authentication, message validation, and lifecycle management.
use mcp_agent::mcp::auth::{AuthCredentials, AuthService};
use mcp_agent::mcp::jsonrpc::JsonRpcHandler;
use mcp_agent::mcp::lifecycle::{
    ClientInfo, InitializeParams, LifecycleManager, ServerCapabilities, SessionListener,
};
use mcp_agent::mcp::schema::SchemaValidator;
use mcp_agent::mcp::types::JsonRpcRequest;
use mcp_agent::utils::error::McpResult;

use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

// Custom session listener for demonstration
struct ExampleSessionListener {
    session_count: Arc<Mutex<u32>>,
}

impl SessionListener for ExampleSessionListener {
    fn on_initialized(&self, session_id: &str, params: &InitializeParams) -> McpResult<()> {
        println!("Session initialized: {}", session_id);
        println!("Client: {} v{}", params.client.name, params.client.version);

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut count = self.session_count.lock().await;
                *count += 1;
                println!("Active sessions: {}", *count);
            })
        });

        Ok(())
    }

    fn on_terminated(&self, session_id: &str) -> McpResult<()> {
        println!("Session terminated: {}", session_id);

        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let mut count = self.session_count.lock().await;
                *count = count.saturating_sub(1);
                println!("Active sessions: {}", *count);
            })
        });

        Ok(())
    }
}

#[tokio::main]
async fn main() -> McpResult<()> {
    // Set up tracing for debugging
    tracing_subscriber::fmt::init();

    println!("MCP Basic Protocol Example");
    println!("=========================");

    // 1. Set up JSON-RPC handler
    let handler = JsonRpcHandler::new();

    // 2. Set up schema validator
    let validator = SchemaValidator::new();

    // 3. Set up authentication service
    let auth_service = AuthService::new();

    // Add a test API key
    let credentials = AuthCredentials::api_key("test-api-key".to_string());
    let cred_id = credentials.id.clone();
    auth_service.add_credentials(credentials).await?;

    // Get a token for our API key
    let token = auth_service.get_token(&cred_id).await?;
    println!("Authentication token: {}", token.header_value());

    // 4. Set up lifecycle manager with server capabilities
    let capabilities = ServerCapabilities {
        name: "Example MCP Server".to_string(),
        version: "0.1.0".to_string(),
        protocol_version: "0.1".to_string(),
        primitives: vec!["prompt".to_string(), "tools".to_string()],
        batch_processing: true,
        custom: None,
    };

    let lifecycle = LifecycleManager::new(capabilities);

    // Add a session listener
    let listener = ExampleSessionListener {
        session_count: Arc::new(Mutex::new(0)),
    };
    lifecycle.add_listener(listener).await;

    // Register lifecycle methods with the JSON-RPC handler
    lifecycle.register_methods(&handler).await?;

    // 5. Register a custom method for demonstration
    handler
        .register_method("echo", |params| {
            println!("Echo method called with params: {:?}", params);
            Ok(params.unwrap_or(json!(null)))
        })
        .await?;

    // 6. Simulate client-server interaction

    // Create client information
    let client_info = ClientInfo {
        name: "Example Client".to_string(),
        version: "0.1.0".to_string(),
        protocol_version: "0.1".to_string(),
        required_primitives: vec!["prompt".to_string()],
        custom: None,
    };

    // Step 1: Initialize
    println!("\nStep 1: Initialize");
    let init_request = LifecycleManager::create_initialize_request(client_info);

    // Validate request against schema
    let request_json = serde_json::to_value(&init_request)?;
    validator.validate_request(&request_json)?;
    println!("Request validation passed");

    // Process the initialization request
    let init_response = handler.handle_request(init_request).await?;
    let init_result = LifecycleManager::handle_initialize_response(&init_response)?;

    println!("Server capabilities:");
    println!("  Name: {}", init_result.capabilities.name);
    println!("  Version: {}", init_result.capabilities.version);
    println!("  Protocol: {}", init_result.capabilities.protocol_version);
    println!("  Primitives: {:?}", init_result.capabilities.primitives);

    // Extract session ID from response - in a real implementation this would come from the server
    let session_id = init_response.id.as_str().unwrap().to_string();
    println!("Session ID: {}", session_id);

    // Step 2: Call a method
    println!("\nStep 2: Call a method");
    let echo_request = JsonRpcRequest::new(
        "echo",
        Some(json!({ "message": "Hello, MCP!" })),
        json!(session_id.clone()),
    );

    let echo_response = handler.handle_request(echo_request).await?;
    println!("Echo response: {:?}", echo_response.result);

    // Step 3: Call a method with batch processing
    println!("\nStep 3: Batch processing");
    let batch_json = json!([
        {
            "jsonrpc": "2.0",
            "method": "echo",
            "params": { "message": "First message" },
            "id": "batch1"
        },
        {
            "jsonrpc": "2.0",
            "method": "echo",
            "params": { "message": "Second message" },
            "id": "batch2"
        }
    ]);

    // Validate batch
    validator.validate_batch(&batch_json)?;
    println!("Batch validation passed");

    // Convert to bytes and process
    let batch_bytes = serde_json::to_vec(&batch_json)?;
    let batch_response_bytes = handler.process_json_message(&batch_bytes).await?.unwrap();
    let batch_response: serde_json::Value = serde_json::from_slice(&batch_response_bytes)?;

    println!(
        "Batch response received with {} items",
        batch_response.as_array().unwrap().len()
    );

    // Step 4: Shutdown and exit
    println!("\nStep 4: Shutdown and exit");
    let shutdown_request = LifecycleManager::create_shutdown_request(&session_id);
    let shutdown_response = handler.handle_request(shutdown_request).await?;
    println!("Shutdown response: {:?}", shutdown_response.result);

    let exit_notification = LifecycleManager::create_exit_notification(&session_id);
    handler.handle_notification(exit_notification).await?;
    println!("Exit notification processed");

    println!("\nExample completed successfully");

    Ok(())
}
