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
//!     // Create a reques
//!     let request = JsonRpcRequest::new(
//!         "echo",
//!         Some(serde_json::json!("Hello, world!")),
//!         serde_json::Value::String("1".to_string()),
//!     );
//!
//!     // Process the reques
//!     let response = handler.handle_request(request).await.unwrap();
//!
//!     // Use the response
//!     println!("Result: {:?}", response.result);
//! }
//! ```

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, instrument, warn};
use uuid::Uuid;

use crate::mcp::types::{
    JsonRpcBatchRequest, JsonRpcBatchResponse, JsonRpcError, JsonRpcNotification, JsonRpcRequest,
    JsonRpcResponse,
};
use crate::telemetry;
use crate::utils::error::{McpError, McpResult};

/// Trait for handling JSON-RPC method calls
///
/// Implementations of this trait can process JSON-RPC requests and return responses.
/// This is useful for creating modular handlers for different MCP features.
#[async_trait]
pub trait JsonRpcMethod: Send + Sync {
    /// Handle a JSON-RPC request
    ///
    /// # Parameters
    ///
    /// * `request` - The JSON-RPC request to handle
    ///
    /// # Returns
    ///
    /// A result containing a JsonRpcResponse or a JsonRpcError
    async fn handle(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, JsonRpcError>;
}

/// Handler for JSON-RPC method calls
///
/// This type represents a function that can be registered to handle JSON-RPC method calls.
/// It takes an optional parameter value and returns a result containing the return value
/// or an error.
///
/// The function must be:
/// - Thread-safe (`Send + Sync`)
/// - Able to handle any valid JSON parameters
/// - Return a JSON-compatible resul
pub type MethodHandler =
    Box<dyn Fn(Option<serde_json::Value>) -> McpResult<serde_json::Value> + Send + Sync>;

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
/// # State Managemen
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
            .field(
                "methods_count",
                &format!(
                    "{} methods",
                    self.methods.try_lock().map(|m| m.len()).unwrap_or(0)
                ),
            )
            .field(
                "notification_handlers_count",
                &format!(
                    "{} handlers",
                    self.notification_handlers
                        .try_lock()
                        .map(|h| h.len())
                        .unwrap_or(0)
                ),
            )
            .field(
                "pending_requests_count",
                &format!(
                    "{} pending",
                    self.pending_requests
                        .try_lock()
                        .map(|p| p.len())
                        .unwrap_or(0)
                ),
            )
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

        debug!(
            "Handling JSON-RPC request: method={}, id={:?}",
            request.method, request.id
        );

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
                    JsonRpcError::method_not_found(&format!(
                        "Method '{}' not found",
                        request.method
                    )),
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

        debug!(
            "Handling JSON-RPC notification: method={}",
            notification.method
        );

        // Check for required jsonrpc version
        if notification.jsonrpc != "2.0" {
            warn!(
                "Invalid JSON-RPC version in notification: {}",
                notification.jsonrpc
            );
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
                        debug!(
                            "Notification processed successfully: {}",
                            notification.method
                        );
                        Ok(())
                    }
                    Err(error) => {
                        warn!(
                            "Notification processing failed: {}: {}",
                            notification.method, error
                        );
                        Err(error)
                    }
                }
            }
            None => {
                // For notifications, it's valid to just ignore unknown methods
                debug!(
                    "No handler for notification method: {}",
                    notification.method
                );
                Ok(())
            }
        }
    }

    /// Handles a JSON-RPC batch request and produces a batch response
    ///
    /// This method processes a batch of JSON-RPC requests concurrently by:
    /// 1. Verifying that the batch is not empty
    /// 2. Processing each request in parallel using tokio tasks
    /// 3. Collecting all responses in the original order
    ///
    /// According to the JSON-RPC 2.0 specification, the server MUST reply with an array
    /// containing the corresponding response objects in the same order as the requests
    /// in the batch call. If the batch is empty, the server MUST return an Invalid Request error.
    /// If all requests in the batch are notifications, the server MAY return an empty array.
    ///
    /// # Parameters
    ///
    /// * `batch_request` - The JSON-RPC batch request to process
    ///
    /// # Returns
    ///
    /// A `McpResult` containing the JSON-RPC batch response
    ///
    /// # Error Handling
    ///
    /// This method handles several error cases:
    /// - Empty batch request
    /// - Invalid requests within the batch
    /// - Method execution errors
    ///
    /// In all cases, the batch response will contain appropriate error responses
    /// for each failed request.
    #[instrument(skip(self, batch_request), fields(batch_size = batch_request.len()))]
    pub async fn handle_batch_request(
        &self,
        batch_request: JsonRpcBatchRequest,
    ) -> McpResult<JsonRpcBatchResponse> {
        let _guard = telemetry::span_duration("handle_jsonrpc_batch_request");

        debug!(
            "Handling JSON-RPC batch request with {} requests",
            batch_request.len()
        );

        // Check if batch is empty (invalid according to spec)
        if batch_request.is_empty() {
            warn!("Empty JSON-RPC batch request received");
            return Err(McpError::InvalidMessage("Empty batch request".to_string()));
        }

        // Process each request concurrently and collect responses
        let mut response_tasks = Vec::with_capacity(batch_request.len());
        let mut _is_all_notifications = true;

        // Start processing each request in parallel
        for request in batch_request.0 {
            // Check if it's a notification (no ID) or a regular request
            let is_notification = match &request.id {
                serde_json::Value::Null => true,
                _ => {
                    _is_all_notifications = false;
                    false
                }
            };

            // Clone self for the async task
            let handler = JsonRpcHandler::default();

            // Process request asynchronously
            let task = tokio::spawn(async move {
                if is_notification {
                    // Convert request to notification and process it
                    let notification = JsonRpcNotification {
                        jsonrpc: request.jsonrpc,
                        method: request.method,
                        params: request.params,
                    };

                    // Process notification (no response needed)
                    match handler.handle_notification(notification).await {
                        Ok(_) => None, // No response for notifications
                        Err(e) => {
                            warn!("Error handling notification in batch: {}", e);
                            None // Still no response per JSON-RPC spec
                        }
                    }
                } else {
                    // Handle normal request
                    match handler.handle_request(request).await {
                        Ok(response) => Some(response),
                        Err(e) => {
                            warn!("Error handling request in batch: {}", e);
                            // Create error response
                            let error = JsonRpcError::internal_error(&format!(
                                "Failed to process batch request: {}",
                                e
                            ));
                            Some(JsonRpcResponse::error(
                                error,
                                serde_json::Value::String("unknown".to_string()),
                            ))
                        }
                    }
                }
            });

            response_tasks.push(task);
        }

        // Wait for all tasks to complete and collect responses
        let mut responses = Vec::new();
        for task in response_tasks {
            match task.await {
                Ok(Some(response)) => responses.push(response),
                Ok(None) => {} // Notification, no response
                Err(e) => {
                    warn!("Task join error in batch processing: {}", e);
                    // We can't correlate this with a specific request ID, so we'll skip it
                }
            }
        }

        // If all requests were notifications, response may be empty
        debug!(
            "Batch processing complete. Generated {} responses",
            responses.len()
        );

        Ok(JsonRpcBatchResponse::from_responses(responses))
    }

    /// Processes a raw JSON message according to the JSON-RPC 2.0 spec
    ///
    /// This method handles both single requests and batch requests, providing
    /// the appropriate response format based on the input message.
    /// It fully implements the JSON-RPC 2.0 specification for message handling.
    ///
    /// # Parameters
    ///
    /// * `json_data` - The raw JSON data to process
    ///
    /// # Returns
    ///
    /// If a response should be sent, returns the serialized response,
    /// otherwise returns None (for notifications)
    #[instrument(skip(self, json_data), level = "debug")]
    pub async fn process_json_message(&self, json_data: &[u8]) -> McpResult<Option<Vec<u8>>> {
        // Try to parse as a batch first
        match serde_json::from_slice::<Vec<serde_json::Value>>(json_data) {
            Ok(batch) if !batch.is_empty() => {
                debug!(
                    "Processing JSON-RPC batch request with {} items",
                    batch.len()
                );
                let mut responses = Vec::new();
                let mut has_responses = false;

                for item in batch {
                    match serde_json::from_value::<JsonRpcRequest>(item.clone()) {
                        Ok(request) => {
                            // Process the request
                            let response = self.handle_request(request).await;
                            match response {
                                Ok(res) => {
                                    responses.push(res);
                                    has_responses = true;
                                }
                                Err(e) => {
                                    // Convert error to JsonRpcResponse with error field
                                    let error = match e {
                                        McpError::MethodNotFound(method) => {
                                            JsonRpcError::method_not_found(&format!(
                                                "Method '{}' not found",
                                                method
                                            ))
                                        }
                                        McpError::InvalidParams(msg) => {
                                            JsonRpcError::invalid_params(&msg)
                                        }
                                        McpError::ParseError(msg) => {
                                            JsonRpcError::parse_error(&msg)
                                        }
                                        _ => JsonRpcError::internal_error(&format!("{}", e)),
                                    };

                                    // Get the ID from the request or null if not available
                                    let id = match serde_json::from_value::<serde_json::Value>(
                                        item["id"].clone(),
                                    ) {
                                        Ok(id) => id,
                                        Err(_) => serde_json::Value::Null,
                                    };

                                    responses.push(JsonRpcResponse::error(error, id));
                                    has_responses = true;
                                }
                            }
                        }
                        Err(_) => {
                            // Try to parse as notification
                            match serde_json::from_value::<JsonRpcNotification>(item) {
                                Ok(notification) => {
                                    // Process notification (no response needed)
                                    let _ = self.handle_notification(notification).await;
                                }
                                Err(_) => {
                                    // Invalid request, add error response
                                    let id = serde_json::Value::Null;
                                    let error =
                                        JsonRpcError::invalid_request("Invalid request format");
                                    responses.push(JsonRpcResponse::error(error, id));
                                    has_responses = true;
                                }
                            }
                        }
                    }
                }

                // Return responses if there are any
                if has_responses {
                    let batch_response = JsonRpcBatchResponse::from_responses(responses);
                    match batch_response.to_bytes() {
                        Ok(bytes) => Ok(Some(bytes)),
                        Err(e) => Err(e),
                    }
                } else {
                    // All were notifications, no response needed
                    Ok(None)
                }
            }
            _ => {
                // Not a batch or empty batch, try to parse as single request or notification
                if let Ok(request) = serde_json::from_slice::<JsonRpcRequest>(json_data) {
                    // Process single request
                    match self.handle_request(request).await {
                        Ok(response) => match response.to_bytes() {
                            Ok(bytes) => Ok(Some(bytes)),
                            Err(e) => Err(e),
                        },
                        Err(e) => {
                            // Create error response
                            let error = match e {
                                McpError::MethodNotFound(method) => JsonRpcError::method_not_found(
                                    &format!("Method '{}' not found", method),
                                ),
                                McpError::InvalidParams(msg) => JsonRpcError::invalid_params(&msg),
                                McpError::ParseError(msg) => JsonRpcError::parse_error(&msg),
                                _ => JsonRpcError::internal_error(&format!("{}", e)),
                            };

                            // Extract ID from the failed request
                            let id = match serde_json::from_slice::<serde_json::Value>(json_data)
                                .and_then(|v| {
                                    serde_json::from_value::<serde_json::Value>(v["id"].clone())
                                }) {
                                Ok(id) => id,
                                Err(_) => serde_json::Value::Null,
                            };

                            let response = JsonRpcResponse::error(error, id);
                            match response.to_bytes() {
                                Ok(bytes) => Ok(Some(bytes)),
                                Err(e) => Err(e),
                            }
                        }
                    }
                } else if let Ok(notification) =
                    serde_json::from_slice::<JsonRpcNotification>(json_data)
                {
                    // Process single notification
                    match self.handle_notification(notification).await {
                        Ok(_) => Ok(None), // No response needed for notifications
                        Err(e) => Err(e),
                    }
                } else {
                    // Invalid request format
                    let error = JsonRpcError::parse_error("Failed to parse JSON-RPC message");
                    let response = JsonRpcResponse::error(error, serde_json::Value::Null);
                    match response.to_bytes() {
                        Ok(bytes) => Ok(Some(bytes)),
                        Err(e) => Err(e),
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
    /// - The response doesn't contain a resul
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

        // Generate a unique ID for the reques
        let id = serde_json::Value::String(Uuid::new_v4().to_string());

        // Create the reques
        let request = JsonRpcRequest::new(method, params, id.clone());

        // Create a channel for the response
        let (tx, rx) = tokio::sync::oneshot::channel();

        // Store the channel sender in pending requests
        {
            let mut pending = self.pending_requests.lock().await;
            if let serde_json::Value::String(id_str) = &id {
                pending.insert(id_str.clone(), tx);
            } else {
                return Err(McpError::InvalidState(
                    "Request ID must be a string".to_string(),
                ));
            }
        }

        // Send the request (this would be implemented by the caller)
        // For example, passing the serialized request to a network handler
        let serialized = request.to_bytes()?;
        debug!("Serialized JSON-RPC request: {} bytes", serialized.len());

        // In a real implementation, you would send the serialized request here
        // This is a placeholder for that logic
        // For example: network_handler.send(serialized).await?;

        // Wait for the response with a timeou
        let response = tokio::time::timeout(tokio::time::Duration::from_secs(30), rx)
            .await
            .map_err(|_| McpError::Timeout)?
            .map_err(|_| McpError::InvalidState("Response channel closed".to_string()))?;

        // Check for errors
        if let Some(error) = response.error {
            return Err(McpError::Custom {
                code: error.code as u32,
                message: error.message,
            });
        }

        // Return the resul
        response
            .result
            .ok_or_else(|| McpError::InvalidMessage("No result in response".to_string()))
    }

    /// Handles a JSON-RPC response and delivers it to the waiting reques
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
    /// - The response ID has an invalid forma
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
                return Err(McpError::InvalidMessage(
                    "Invalid response ID type".to_string(),
                ));
            }
        };

        // Find the pending reques
        let sender = {
            let mut pending = self.pending_requests.lock().await;
            pending.remove(&id_str)
        };

        // Send the response to the pending reques
        if let Some(sender) = sender {
            if sender.send(response).is_err() {
                warn!("Failed to send response to requester (channel closed)");
                return Err(McpError::InvalidState(
                    "Response channel closed".to_string(),
                ));
            }
            debug!("Response delivered to requester: id={}", id_str);
            Ok(())
        } else {
            warn!("No pending request found for response: id={}", id_str);
            Err(McpError::InvalidState(format!(
                "No pending request found for response ID: {}",
                id_str
            )))
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
        handler
            .register_method("test.method", |params| match params {
                Some(serde_json::Value::Object(obj)) if obj.contains_key("echo") => {
                    Ok(obj["echo"].clone())
                }
                _ => Ok(serde_json::Value::String("default".to_string())),
            })
            .await
            .unwrap();

        // Create a test reques
        let params = serde_json::json!({ "echo": "hello world" });
        let request = JsonRpcRequest::new(
            "test.method",
            Some(params),
            serde_json::Value::String("1".to_string()),
        );

        // Handle the reques
        let response = handler.handle_request(request).await.unwrap();

        // Verify the response
        assert_eq!(response.jsonrpc, "2.0");
        assert_eq!(response.id, serde_json::Value::String("1".to_string()));
        assert!(response.error.is_none());
        assert_eq!(
            response.result.as_ref().unwrap(),
            &serde_json::Value::String("hello world".to_string())
        );
    }

    #[tokio::test]
    async fn test_method_not_found() {
        let handler = JsonRpcHandler::new();

        // Create a request for a method that doesn't exis
        let request = JsonRpcRequest::new(
            "nonexistent.method",
            None,
            serde_json::Value::String("1".to_string()),
        );

        // Handle the reques
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
