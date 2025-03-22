// Example implementation of JSON-RPC in the Model Context Protocol (MCP)
//
// This example demonstrates:
// 1. Creating and configuring a JSON-RPC handler
// 2. Registering method and notification handlers
// 3. Processing various types of JSON-RPC requests and notifications
// 4. Handling success and error responses
// 5. Processing raw JSON messages
//
// The JSON-RPC implementation follows the 2.0 specification with:
// - Request/Response correlation via IDs
// - Support for method calls with parameters
// - Support for notifications (no response expected)
// - Standardized error handling

use std::error::Error;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use mcp_agent::mcp::jsonrpc::JsonRpcHandler;
use mcp_agent::mcp::types::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use mcp_agent::telemetry::{self, TelemetryConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize telemetry for monitoring and metrics collection
    let config = TelemetryConfig {
        service_name: "jsonrpc_example".to_string(),
        ..Default::default()
    };
    let _ = telemetry::init_telemetry(config);

    info!("Starting JSON-RPC example");

    // Create a new JSON-RPC handler
    // This handler manages method registration and message processing
    let handler = JsonRpcHandler::new();

    // Register a method handler for "echo"
    // This simple method returns whatever parameter it receives
    handler
        .register_method("echo", |params| {
            debug!("Handling echo method with params: {:?}", params);
            Ok(params.unwrap_or(serde_json::Value::Null))
        })
        .await?;

    // Register a method handler for "add"
    // This method adds two numbers together and demonstrates parameter validation
    handler
        .register_method("add", |params| {
            debug!("Handling add method with params: {:?}", params);

            // Parse parameters - expects an array with exactly two numbers
            if let Some(serde_json::Value::Array(array)) = params {
                if array.len() != 2 {
                    return Err(mcp_agent::utils::error::McpError::InvalidMessage(
                        "Expected exactly 2 parameters".to_string(),
                    ));
                }

                // Try to convert to numbers - demonstrates error handling
                let parse_number = |val: &serde_json::Value| -> Result<f64, _> {
                    match val {
                        serde_json::Value::Number(n) => n.as_f64().ok_or_else(|| {
                            mcp_agent::utils::error::McpError::InvalidMessage(
                                "Expected a number".to_string(),
                            )
                        }),
                        _ => Err(mcp_agent::utils::error::McpError::InvalidMessage(
                            "Expected a number".to_string(),
                        )),
                    }
                };

                let a = parse_number(&array[0])?;
                let b = parse_number(&array[1])?;

                // Return the sum
                Ok(serde_json::Value::Number(
                    serde_json::Number::from_f64(a + b).unwrap(),
                ))
            } else {
                Err(mcp_agent::utils::error::McpError::InvalidMessage(
                    "Expected an array of parameters".to_string(),
                ))
            }
        })
        .await?;

    // Register a notification handler for "log"
    // Notifications don't require responses and are used for events/logging
    handler
        .register_notification("log", |params| {
            if let Some(serde_json::Value::Object(obj)) = params {
                if let Some(serde_json::Value::String(level)) = obj.get("level") {
                    if let Some(serde_json::Value::String(message)) = obj.get("message") {
                        match level.as_str() {
                            "debug" => debug!("{}", message),
                            "info" => info!("{}", message),
                            "warn" => warn!("{}", message),
                            _ => debug!("[{}] {}", level, message),
                        }
                    }
                }
            }

            Ok(serde_json::Value::Null)
        })
        .await?;

    info!("Registered method handlers");

    // Create example requests to demonstrate different scenarios

    // 1. Basic echo request with a string parameter
    let echo_request = JsonRpcRequest::new(
        "echo",
        Some(serde_json::json!("Hello, world!")),
        serde_json::Value::String("1".to_string()),
    );

    // 2. Add request with two numeric parameters
    let add_request = JsonRpcRequest::new(
        "add",
        Some(serde_json::json!([5, 7])),
        serde_json::Value::String("2".to_string()),
    );

    // 3. Request to a non-existent method (will produce an error)
    let invalid_request = JsonRpcRequest::new(
        "nonexistent",
        None,
        serde_json::Value::String("3".to_string()),
    );

    // 4. Request with invalid parameters (will produce an error)
    let invalid_params_request = JsonRpcRequest::new(
        "add",
        Some(serde_json::json!(["not a number", "also not a number"])),
        serde_json::Value::String("4".to_string()),
    );

    // 5. Notification example (no response expected)
    let log_notification = JsonRpcNotification::new(
        "log",
        Some(serde_json::json!({
            "level": "info",
            "message": "This is a notification message"
        })),
    );

    // Process each request and examine the responses

    info!("Processing echo request");
    let echo_response = handler.handle_request(echo_request).await?;
    print_response(&echo_response);

    info!("Processing add request");
    let add_response = handler.handle_request(add_request).await?;
    print_response(&add_response);

    info!("Processing invalid method request");
    let invalid_response = handler.handle_request(invalid_request).await?;
    print_response(&invalid_response);

    info!("Processing invalid params request");
    let invalid_params_response = handler.handle_request(invalid_params_request).await?;
    print_response(&invalid_params_response);

    info!("Processing notification");
    handler.handle_notification(log_notification).await?;

    // Demonstrate processing raw JSON messages
    // This shows how to handle incoming JSON-RPC messages as raw JSON
    info!("Processing raw JSON message");
    let raw_json =
        r#"{"jsonrpc": "2.0", "method": "echo", "params": "Raw JSON test", "id": "raw1"}"#;
    let result = handler.process_json_message(raw_json.as_bytes()).await?;

    if let Some(response_bytes) = result {
        let response_str = String::from_utf8_lossy(&response_bytes);
        info!("Raw JSON response: {}", response_str);
    }

    // Allow time for logs to flush
    sleep(Duration::from_millis(100)).await;

    info!("JSON-RPC example completed successfully");
    Ok(())
}

/// Helper function to print JSON-RPC responses in a readable format
///
/// This demonstrates how to handle both successful and error responses
/// from JSON-RPC method calls.
fn print_response(response: &JsonRpcResponse) {
    if let Some(error) = &response.error {
        info!(
            "Got error response: code={}, message={}",
            error.code, error.message
        );
    } else {
        info!("Got successful response: result={:?}", response.result);
    }
}
