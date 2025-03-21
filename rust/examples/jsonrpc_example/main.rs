use std::error::Error;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};

use mcp_agent::mcp::jsonrpc::JsonRpcHandler;
use mcp_agent::mcp::types::{JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use mcp_agent::telemetry::{self, TelemetryConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Initialize telemetry
    let config = TelemetryConfig {
        service_name: "jsonrpc_example".to_string(),
        ..Default::default()
    };
    telemetry::init_telemetry(config);
    
    info!("Starting JSON-RPC example");
    
    // Create a new JSON-RPC handler
    let handler = JsonRpcHandler::new();
    
    // Register a method handler for "echo"
    handler.register_method("echo", |params| {
        debug!("Handling echo method with params: {:?}", params);
        Ok(params.unwrap_or(serde_json::Value::Null))
    }).await?;
    
    // Register a method handler for "add"
    handler.register_method("add", |params| {
        debug!("Handling add method with params: {:?}", params);
        
        // Parse parameters
        if let Some(serde_json::Value::Array(array)) = params {
            if array.len() != 2 {
                return Err(mcp_agent::utils::error::McpError::InvalidMessage(
                    "Expected exactly 2 parameters".to_string(),
                ));
            }
            
            // Try to convert to numbers
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
            Ok(serde_json::Value::Number(serde_json::Number::from_f64(a + b).unwrap()))
        } else {
            Err(mcp_agent::utils::error::McpError::InvalidMessage(
                "Expected an array of parameters".to_string(),
            ))
        }
    }).await?;
    
    // Register a notification handler for "log"
    handler.register_notification("log", |params| {
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
    }).await?;
    
    info!("Registered method handlers");
    
    // Create some example requests and process them
    let echo_request = JsonRpcRequest::new(
        "echo",
        Some(serde_json::json!("Hello, world!")),
        serde_json::Value::String("1".to_string()),
    );
    
    let add_request = JsonRpcRequest::new(
        "add",
        Some(serde_json::json!([5, 7])),
        serde_json::Value::String("2".to_string()),
    );
    
    let invalid_request = JsonRpcRequest::new(
        "nonexistent",
        None,
        serde_json::Value::String("3".to_string()),
    );
    
    let invalid_params_request = JsonRpcRequest::new(
        "add",
        Some(serde_json::json!(["not a number", "also not a number"])),
        serde_json::Value::String("4".to_string()),
    );
    
    // Create a notification
    let log_notification = JsonRpcNotification::new(
        "log",
        Some(serde_json::json!({
            "level": "info",
            "message": "This is a notification message"
        })),
    );
    
    // Process the requests
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
    
    // Show how to process raw JSON messages
    info!("Processing raw JSON message");
    let raw_json = r#"{"jsonrpc": "2.0", "method": "echo", "params": "Raw JSON test", "id": "raw1"}"#;
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

fn print_response(response: &JsonRpcResponse) {
    if let Some(error) = &response.error {
        info!(
            "Got error response: code={}, message={}",
            error.code, error.message
        );
    } else {
        info!(
            "Got successful response: result={:?}",
            response.result
        );
    }
} 