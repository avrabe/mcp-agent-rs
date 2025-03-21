//! JSON-RPC implementation for MCP protocol, compliant with JSON-RPC 2.0 specification.
//!
//! This module provides a complete implementation of the JSON-RPC 2.0 protocol
//! for the Model Context Protocol (MCP). It handles:
//!
//! - Method registration and invocation
//! - Notification processing
//! - Request/response correlation
//! - Error handling and reporting
//! - Raw message processing
//!
//! The implementation is fully asynchronous and thread-safe, designed to work in
//! a concurrent environment with multiple connections.
//!
//! # Example
//!
//! ```rust,no_run
//! use mcp_agent::mcp::jsonrpc::JsonRpcHandler;
//! use mcp_agent::mcp::types::{JsonRpcRequest, JsonRpcResponse};
//!
//! async fn example() {
//!     // Create a new handler
//!     let handler = JsonRpcHandler::new();
//!
//!     // Register a method
//!     handler.register_method("echo", |params| {
//!         // Simply return the parameters
//!         Ok(params.unwrap_or(serde_json::Value::Null))
//!     }).await.unwrap();
//!
//!     // Create a request
//!     let request = JsonRpcRequest::new(
//!         "echo",
//!         Some(serde_json::json!("Hello, world!")),
//!         serde_json::Value::String("1".to_string()),
//!     );
//!
//!     // Process the request
//!     let response = handler.handle_request(request).await.unwrap();
//!     
//!     // Use the response
//!     println!("Result: {:?}", response.result);
//! }
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, instrument, warn};
use uuid::Uuid;

use crate::mcp::types::{JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::telemetry;
use crate::utils::error::{McpError, McpResult};

/// Handler for JSON-RPC method calls
///
/// This type represents a function that can be registered to handle JSON-RPC method calls.
/// It takes an optional parameter value and returns a result containing the return value
/// or an error.
///
/// The function must be:
/// - Thread-safe (`Send + Sync`)
/// - Able to handle any valid JSON parameters
/// - Return a JSON-compatible result
pub type MethodHandler = Box<dyn Fn(Option<serde_json::Value>) -> McpResult<serde_json::Value> + Send + Sync>;

/// JSON-RPC handler for MCP protocol
///
/// The `JsonRpcHandler` is the central component of the JSON-RPC implementation.
/// It manages method registration, request processing, and response correlation.
///
/// The handler is thread-safe and can be used from multiple tasks concurrently.
/// All internal state is protected by asynchronous mutexes.
///
/// # Thread Safety
///
/// All methods on this struct are thread-safe and can be called from multiple tasks.
/// The handler can be cloned and shared between tasks safely.
///
/// # State Management
///
/// - Method registrations are stored for the lifetime of the handler
/// - Notification handlers are registered separately from methods
/// - Pending requests are tracked for correlation
pub struct JsonRpcHandler {
    /// Registered method handlers mapped by method name
    methods: Arc<Mutex<HashMap<String, MethodHandler>>>,
    /// Notification handlers mapped by notification name
    notification_handlers: Arc<Mutex<HashMap<String, MethodHandler>>>,
    /// Pending requests awaiting responses, mapped by request ID
    pending_requests: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<JsonRpcResponse>>>>,
}

impl std::fmt::Debug for JsonRpcHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsonRpcHandler")
            .field("methods_count", &format!("{} methods", self.methods.try_lock().map(|m| m.len()).unwrap_or(0)))
            .field("notification_handlers_count", &format!("{} handlers", self.notification_handlers.try_lock().map(|h| h.len()).unwrap_or(0)))
            .field("pending_requests_count", &format!("{} pending", self.pending_requests.try_lock().map(|p| p.len()).unwrap_or(0)))
            .finish()
    }
}

impl JsonRpcHandler {
    /// Creates a new JSON-RPC handler with empty registrations
    ///
    /// # Returns
    /// 
    /// A new instance of `JsonRpcHandler` with no registered methods or notification handlers.
    ///
    /// # Example
    ///
    /// ```
    /// use mcp_agent::mcp::jsonrpc::JsonRpcHandler;
    ///
    /// let handler = JsonRpcHandler::new();
    /// ```
    pub fn new() -> Self {
        debug!("Creating new JSON-RPC handler");
        Self {
            methods: Arc::new(Mutex::new(HashMap::new())),
            notification_handlers: Arc::new(Mutex::new(HashMap::new())),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Registers a method handler for a specified method name
    ///
    /// This function associates a handler function with a method name. When a JSON-RPC
    /// request is received for this method, the handler will be invoked with the
    /// parameters from the request.
    ///
    /// # Parameters
    ///
    /// * `name` - The method name to register
    /// * `handler` - The function to handle calls to this method
    ///
    /// # Returns
    ///
    /// A `McpResult` indicating success or failure
    ///
    /// # Example
    ///
    /// ```
    /// use mcp_agent::mcp::jsonrpc::JsonRpcHandler;
    ///
    /// async fn register_method() {
    ///     let handler = JsonRpcHandler::new();
    ///     
    ///     handler.register_method("echo", |params| {
    ///         // Simply return the parameters
    ///         Ok(params.unwrap_or(serde_json::Value::Null))
    ///     }).await.unwrap();
    /// }
    /// ```
    #[instrument(skip(self, handler), fields(method = %name))]
    pub async fn register_method<F>(&self, name: &str, handler: F) -> McpResult<()>
    where
        F: Fn(Option<serde_json::Value>) -> McpResult<serde_json::Value> + Send + Sync + 'static,
    {
        let mut methods = self.methods.lock().await;
        methods.insert(name.to_string(), Box::new(handler));
        debug!("Registered method handler for '{}'", name);
        Ok(())
    }

    /// Registers a notification handler for a specified notification type
    ///
    /// This function associates a handler function with a notification method name.
    /// When a JSON-RPC notification is received with this method, the handler will
    /// be invoked with the parameters from the notification.
    ///
    /// Unlike methods, notifications do not expect or generate responses.
    ///
    /// # Parameters
    ///
    /// * `name` - The notification method name to register
    /// * `handler` - The function to handle calls to this notification method
    ///
    /// # Returns
    ///
    /// A `McpResult` indicating success or failure
    ///
    /// # Example
    ///
    /// ```
    /// use mcp_agent::mcp::jsonrpc::JsonRpcHandler;
    ///
    /// async fn register_notification() {
    ///     let handler = JsonRpcHandler::new();
    ///     
    ///     handler.register_notification("log", |params| {
    ///         if let Some(msg) = params {
    ///             println!("Log: {:?}", msg);
    ///         }
    ///         Ok(serde_json::Value::Null)
    ///     }).await.unwrap();
    /// }
    /// ```
    #[instrument(skip(self, handler), fields(method = %name))]
    pub async fn register_notification<F>(&self, name: &str, handler: F) -> McpResult<()>
    where
        F: Fn(Option<serde_json::Value>) -> McpResult<serde_json::Value> + Send + Sync + 'static,
    {
        let mut handlers = self.notification_handlers.lock().await;
        handlers.insert(name.to_string(), Box::new(handler));
        debug!("Registered notification handler for '{}'", name);
        Ok(())
    }

    /// Handles a JSON-RPC request and produces a response
    ///
    /// This method processes a JSON-RPC request by:
    /// 1. Validating the JSON-RPC version
    /// 2. Looking up the registered method handler
    /// 3. Invoking the handler with the request parameters
    /// 4. Creating a response with the result or error
    ///
    /// # Parameters
    ///
    /// * `request` - The JSON-RPC request to process
    ///
    /// # Returns
    ///
    /// A `McpResult` containing the JSON-RPC response
    ///
    /// # Error Handling
    ///
    /// This method handles several error cases:
    /// - Invalid JSON-RPC version
    /// - Method not found
    /// - Method execution errors
    ///
    /// In all error cases, a valid JSON-RPC error response is returned with
    /// appropriate error codes following the JSON-RPC 2.0 specification.
    #[instrument(skip(self, request), fields(method = %request.method, id = ?request.id))]
    pub async fn handle_request(&self, request: JsonRpcRequest) -> McpResult<JsonRpcResponse> {
        let _guard = telemetry::span_duration("handle_jsonrpc_request");
        
        debug!("Handling JSON-RPC request: method={}, id={:?}", request.method, request.id);
        
        // Check for required jsonrpc version
        if request.jsonrpc != "2.0" {
            warn!("Invalid JSON-RPC version: {}", request.jsonrpc);
            return Ok(JsonRpcResponse::error(
                JsonRpcError::invalid_request("Invalid JSON-RPC version"),
                request.id,
            ));
        }
        
        // Find the method handler
        let methods = self.methods.lock().await;
        let handler = methods.get(&request.method);
        
        match handler {
            Some(handler) => {
                // Call the handler with the parameters
                match handler(request.params) {
                    Ok(result) => {
                        debug!("Method call successful: {}", request.method);
                        Ok(JsonRpcResponse::success(result, request.id))
                    }
                    Err(error) => {
                        warn!("Method call failed: {}: {}", request.method, error);
                        Ok(JsonRpcResponse::error(
                            JsonRpcError::internal_error(&error.to_string()),
                            request.id,
                        ))
                    }
                }
            }
            None => {
                warn!("Method not found: {}", request.method);
                Ok(JsonRpcResponse::error(
                    JsonRpcError::method_not_found(&format!("Method '{}' not found", request.method)),
                    request.id,
                ))
            }
        }
    }

    /// Handles a JSON-RPC notification
    ///
    /// This method processes a JSON-RPC notification by:
    /// 1. Validating the JSON-RPC version
    /// 2. Looking up the registered notification handler
    /// 3. Invoking the handler with the notification parameters
    ///
    /// # Parameters
    ///
    /// * `notification` - The JSON-RPC notification to process
    ///
    /// # Returns
    ///
    /// A `McpResult` indicating success or failure
    ///
    /// # Error Handling
    ///
    /// This method handles several error cases:
    /// - Invalid JSON-RPC version
    /// - Handler execution errors
    ///
    /// Unlike requests, if no handler is found for a notification, 
    /// it is silently ignored per the JSON-RPC 2.0 specification.
    #[instrument(skip(self, notification), fields(method = %notification.method))]
    pub async fn handle_notification(&self, notification: JsonRpcNotification) -> McpResult<()> {
        let _guard = telemetry::span_duration("handle_jsonrpc_notification");
        
        debug!("Handling JSON-RPC notification: method={}", notification.method);
        
        // Check for required jsonrpc version
        if notification.jsonrpc != "2.0" {
            warn!("Invalid JSON-RPC version in notification: {}", notification.jsonrpc);
            return Err(McpError::InvalidMessage(format!(
                "Invalid JSON-RPC version: {}",
                notification.jsonrpc
            )));
        }
        
        // Find the notification handler
        let handlers = self.notification_handlers.lock().await;
        let handler = handlers.get(&notification.method);
        
        match handler {
            Some(handler) => {
                // Call the handler with the parameters
                match handler(notification.params) {
                    Ok(_) => {
                        debug!("Notification processed successfully: {}", notification.method);
                        Ok(())
                    }
                    Err(error) => {
                        warn!("Notification processing failed: {}: {}", notification.method, error);
                        Err(error)
                    }
                }
            }
            None => {
                // For notifications, it's valid to just ignore unknown methods
                debug!("No handler for notification method: {}", notification.method);
                Ok(())
            }
        }
    }

    /// Processes a raw JSON message and determines if it's a request or notification
    ///
    /// This method attempts to parse the raw JSON data as either a request or notification,
    /// and routes it to the appropriate handler. If the message is a request, a response
    /// is generated and returned as bytes.
    ///
    /// # Parameters
    ///
    /// * `json_data` - The raw JSON data to process
    ///
    /// # Returns
    ///
    /// A `McpResult` containing:
    /// - `Some(Vec<u8>)` if the message was a request and a response was generated
    /// - `None` if the message was a notification (no response)
    ///
    /// # Error Handling
    ///
    /// Returns an error if:
    /// - The JSON data is invalid or doesn't match a request or notification format
    /// - The request or notification handler returns an error
    #[instrument(skip(self, json_data))]
    pub async fn process_json_message(&self, json_data: &[u8]) -> McpResult<Option<Vec<u8>>> {
        let _guard = telemetry::span_duration("process_json_message");
        
        // Try to parse as a request
        match serde_json::from_slice::<JsonRpcRequest>(json_data) {
            Ok(request) => {
                let response = self.handle_request(request).await?;
                let response_data = response.to_bytes()?;
                return Ok(Some(response_data));
            }
            Err(_) => {
                // Not a request, try as a notification
                match serde_json::from_slice::<JsonRpcNotification>(json_data) {
                    Ok(notification) => {
                        self.handle_notification(notification).await?;
                        return Ok(None); // Notifications don't have responses
                    }
                    Err(e) => {
                        // Not a notification either
                        warn!("Invalid JSON-RPC message: {}", e);
                        return Err(McpError::InvalidMessage(format!("Invalid JSON-RPC message: {}", e)));
                    }
                }
            }
        }
    }

    /// Sends a JSON-RPC request and waits for the response
    ///
    /// This method creates a JSON-RPC request with the specified method and parameters,
    /// registers it in the pending requests, and awaits the response.
    ///
    /// # Parameters
    ///
    /// * `method` - The method name to call
    /// * `params` - Optional parameters to pass to the method
    ///
    /// # Returns
    ///
    /// A `McpResult` containing the result value from the response
    ///
    /// # Error Handling
    ///
    /// Returns an error if:
    /// - The request times out waiting for a response
    /// - The response channel is closed
    /// - The response contains an error
    /// - The response doesn't contain a result
    ///
    /// # Note
    ///
    /// This implementation assumes the transport layer is provided externally.
    /// The caller must implement the actual sending of the request and delivering
    /// the response to the handler via `handle_response`.
    #[instrument(skip(self, method, params))]
    pub async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> McpResult<serde_json::Value> {
        let _guard = telemetry::span_duration("send_jsonrpc_request");
        
        // Generate a unique ID for the request
        let id = serde_json::Value::String(Uuid::new_v4().to_string());
        
        // Create the request
        let request = JsonRpcRequest::new(method, params, id.clone());
        
        // Create a channel for the response
        let (tx, rx) = tokio::sync::oneshot::channel();
        
        // Store the channel sender in pending requests
        {
            let mut pending = self.pending_requests.lock().await;
            if let serde_json::Value::String(id_str) = &id {
                pending.insert(id_str.clone(), tx);
            } else {
                return Err(McpError::InvalidState("Request ID must be a string".to_string()));
            }
        }
        
        // Send the request (this would be implemented by the caller)
        // For example, passing the serialized request to a network handler
        let serialized = request.to_bytes()?;
        debug!("Serialized JSON-RPC request: {} bytes", serialized.len());
        
        // In a real implementation, you would send the serialized request here
        // This is a placeholder for that logic
        // For example: network_handler.send(serialized).await?;
        
        // Wait for the response with a timeout
        let response = tokio::time::timeout(
            tokio::time::Duration::from_secs(30),
            rx
        ).await.map_err(|_| McpError::Timeout)?
        .map_err(|_| McpError::InvalidState("Response channel closed".to_string()))?;
        
        // Check for errors
        if let Some(error) = response.error {
            return Err(McpError::Custom {
                code: error.code as u32,
                message: error.message,
            });
        }
        
        // Return the result
        response.result.ok_or_else(|| McpError::InvalidMessage("No result in response".to_string()))
    }

    /// Handles a JSON-RPC response and delivers it to the waiting request
    ///
    /// This method processes a JSON-RPC response by:
    /// 1. Validating the JSON-RPC version
    /// 2. Finding the pending request with the matching ID
    /// 3. Delivering the response to the waiting task
    ///
    /// # Parameters
    ///
    /// * `response` - The JSON-RPC response to process
    ///
    /// # Returns
    ///
    /// A `McpResult` indicating success or failure
    ///
    /// # Error Handling
    ///
    /// Returns an error if:
    /// - The JSON-RPC version is invalid
    /// - The response ID has an invalid format
    /// - No pending request is found for the response ID
    /// - The channel for the pending request is closed
    #[instrument(skip(self, response), fields(id = ?response.id))]
    pub async fn handle_response(&self, response: JsonRpcResponse) -> McpResult<()> {
        let _guard = telemetry::span_duration("handle_jsonrpc_response");
        
        debug!("Handling JSON-RPC response: id={:?}", response.id);
        
        // Check for required jsonrpc version
        if response.jsonrpc != "2.0" {
            warn!("Invalid JSON-RPC version: {}", response.jsonrpc);
            return Err(McpError::InvalidMessage(format!(
                "Invalid JSON-RPC version: {}",
                response.jsonrpc
            )));
        }
        
        // Get the ID as string
        let id_str = match &response.id {
            serde_json::Value::String(s) => s.clone(),
            _ => {
                warn!("Invalid response ID type: {:?}", response.id);
                return Err(McpError::InvalidMessage("Invalid response ID type".to_string()));
            }
        };
        
        // Find the pending request
        let sender = {
            let mut pending = self.pending_requests.lock().await;
            pending.remove(&id_str)
        };
        
        // Send the response to the pending request
        if let Some(sender) = sender {
            if sender.send(response).is_err() {
                warn!("Failed to send response to requester (channel closed)");
                return Err(McpError::InvalidState("Response channel closed".to_string()));
            }
            debug!("Response delivered to requester: id={}", id_str);
            Ok(())
        } else {
            warn!("No pending request found for response: id={}", id_str);
            Err(McpError::InvalidState(format!("No pending request found for response ID: {}", id_str)))
        }
    }
}

impl Default for JsonRpcHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_register_and_handle_method() {
        let handler = JsonRpcHandler::new();
        
        // Register a method
        handler.register_method("test.method", |params| {
            match params {
                Some(serde_json::Value::Object(obj)) if obj.contains_key("echo") => {
                    Ok(obj["echo"].clone())
                }
                _ => Ok(serde_json::Value::String("default".to_string())),
            }
        }).await.unwrap();
        
        // Create a test request
        let params = serde_json::json!({ "echo": "hello world" });
        let request = JsonRpcRequest::new("test.method", Some(params), serde_json::Value::String("1".to_string()));
        
        // Handle the request
        let response = handler.handle_request(request).await.unwrap();
        
        // Verify the response
        assert_eq!(response.jsonrpc, "2.0");
        assert_eq!(response.id, serde_json::Value::String("1".to_string()));
        assert!(response.error.is_none());
        assert_eq!(response.result.as_ref().unwrap(), &serde_json::Value::String("hello world".to_string()));
    }
    
    #[tokio::test]
    async fn test_method_not_found() {
        let handler = JsonRpcHandler::new();
        
        // Create a request for a method that doesn't exist
        let request = JsonRpcRequest::new("nonexistent.method", None, serde_json::Value::String("1".to_string()));
        
        // Handle the request
        let response = handler.handle_request(request).await.unwrap();
        
        // Verify the response contains an error
        assert_eq!(response.jsonrpc, "2.0");
        assert_eq!(response.id, serde_json::Value::String("1".to_string()));
        assert_eq!(response.result, None);
        assert!(response.error.is_some());
        
        let error = response.error.unwrap();
        assert_eq!(error.code, -32601); // Method not found error code
    }
} 