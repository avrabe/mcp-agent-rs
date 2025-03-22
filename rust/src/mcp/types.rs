//! # MCP Protocol Types
//!
//! This module defines the core types used throughout the Model Context Protocol (MCP) implementation.
//! It includes message structures, identifiers, enums for message classification, and JSON-RPC related types.
//!
//! ## Core Message Types
//!
//! The module provides several key structures:
//!
//! - `MessageId`: A UUID-based identifier for messages
//! - `Message`: The complete message structure with header information and payload
//! - `MessageEnvelope`: An intermediate representation for serialization
//! - `MessageHeader`: Contains metadata about a message
//!
//! ## Message Classification
//!
//! Messages are classified using these enums:
//!
//! - `MessageType`: Defines whether a message is a Request, Response, Event, or KeepAlive
//! - `Priority`: Specifies the urgency of a message (Low, Normal, High)
//!
//! ## JSON-RPC Types
//!
//! For JSON-RPC compatibility, the module includes:
//!
//! - `JsonRpcRequest`: Represents a method invocation reques
//! - `JsonRpcResponse`: Contains the result of a method call
//! - `JsonRpcNotification`: One-way messages that require no response
//! - `JsonRpcError`: Standard error format for JSON-RPC
//!
//! ## Example
//!
//! ```rus
//! use mcp_agent::mcp::types::{Message, MessageType, Priority};
//!
//! // Create a request message with payload
//! let payload = b"Hello, world!".to_vec();
//! let request = Message::request(payload, Priority::Normal);
//!
//! // Create a response to the reques
//! let response_payload = b"Response data".to_vec();
//! let response = Message::response(response_payload, request.id.clone());
//!
//! // Access message properties
//! assert_eq!(response.message_type, MessageType::Response);
//! assert_eq!(response.correlation_id, Some(request.id));
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use crate::utils::error::{McpError, McpResult};

/// A unique identifier for a message
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub [u8; 16]);

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageId {
    /// Generate a new message ID
    pub fn new() -> Self {
        let bytes = Uuid::new_v4().into_bytes();
        Self(bytes)
    }

    /// Create a MessageId from raw bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes of the MessageId
    pub fn bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Convert to a UUID
    pub fn to_uuid(&self) -> Uuid {
        Uuid::from_bytes(self.0)
    }
}

// Add Display implementation for MessageId
impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", Uuid::from_bytes(self.0))
    }
}

/// The type of message being sen
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    /// A request message
    Request,
    /// A response message
    Response,
    /// An event message
    Event,
    /// A keep-alive message
    KeepAlive,
}

impl MessageType {
    /// Converts a u8 to a MessageType
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Request),
            1 => Some(Self::Response),
            2 => Some(Self::Event),
            3 => Some(Self::KeepAlive),
            _ => None,
        }
    }

    /// Converts a MessageType to a u8
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Request => 0,
            Self::Response => 1,
            Self::Event => 2,
            Self::KeepAlive => 3,
        }
    }
}

/// The priority level of a message
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    /// Low priority messages
    Low,
    /// Normal priority messages
    Normal,
    /// High priority messages
    High,
}

impl Priority {
    /// Converts a u8 to a Priority
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Low),
            1 => Some(Self::Normal),
            2 => Some(Self::High),
            _ => None,
        }
    }

    /// Converts a Priority to a u8
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Normal => 1,
            Self::High => 2,
        }
    }
}

/// The header of a message containing metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHeader {
    /// The unique identifier of the message
    pub id: String,
    /// The type of message
    pub message_type: MessageType,
    /// The priority level of the message
    pub priority: Priority,
    /// The size of the payload in bytes
    pub payload_size: u32,
    /// The ID of the message this is replying to (if any)
    pub correlation_id: Option<String>,
    /// Any error information (if any)
    pub error: Option<McpError>,
}

/// The envelope containing the header and payload of a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope {
    /// The header of the message
    pub header: MessageHeader,
    /// The payload of the message
    pub payload: Vec<u8>,
}

/// A complete message with header information and payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The unique identifier of the message
    pub id: MessageId,
    /// The type of message
    pub message_type: MessageType,
    /// The priority level of the message
    pub priority: Priority,
    /// The payload of the message
    pub payload: Vec<u8>,
    /// The ID of the message this is replying to (if any)
    pub correlation_id: Option<MessageId>,
    /// Any error information (if any)
    pub error: Option<McpError>,
}

impl Message {
    /// Creates a new message
    pub fn new(
        message_type: MessageType,
        priority: Priority,
        payload: Vec<u8>,
        correlation_id: Option<MessageId>,
        error: Option<McpError>,
    ) -> Self {
        Self {
            id: MessageId::new(),
            message_type,
            priority,
            payload,
            correlation_id,
            error,
        }
    }

    /// Creates a new request message
    pub fn request(payload: Vec<u8>, priority: Priority) -> Self {
        Self::new(MessageType::Request, priority, payload, None, None)
    }

    /// Creates a new response message
    pub fn response(payload: Vec<u8>, request_id: MessageId) -> Self {
        Self::new(
            MessageType::Response,
            Priority::Normal,
            payload,
            Some(request_id),
            None,
        )
    }

    /// Creates a new event message
    pub fn event(payload: Vec<u8>) -> Self {
        Self::new(MessageType::Event, Priority::Normal, payload, None, None)
    }

    /// Creates a new keep-alive message
    pub fn keep_alive() -> Self {
        Self::new(MessageType::KeepAlive, Priority::Low, vec![], None, None)
    }

    /// Creates a new error response
    pub fn error(error: McpError, request_id: Option<MessageId>) -> Self {
        Self::new(
            MessageType::Response,
            Priority::High,
            vec![],
            request_id,
            Some(error),
        )
    }

    /// Converts a message to an envelope
    pub fn to_envelope(&self) -> McpResult<MessageEnvelope> {
        Ok(MessageEnvelope {
            header: MessageHeader {
                id: self.id.to_string(),
                message_type: self.message_type,
                priority: self.priority,
                payload_size: self.payload.len() as u32,
                correlation_id: self.correlation_id.as_ref().map(|id| id.to_string()),
                error: self.error.clone(),
            },
            payload: self.payload.clone(),
        })
    }

    /// Converts an envelope to a message
    pub fn from_envelope(envelope: MessageEnvelope) -> McpResult<Self> {
        let id_bytes = Uuid::parse_str(&envelope.header.id)
            .map_err(|_| McpError::InvalidMessage("Invalid UUID format".to_string()))?
            .into_bytes();

        let correlation_id = if let Some(id_str) = envelope.header.correlation_id {
            let id_bytes = Uuid::parse_str(&id_str)
                .map_err(|_| {
                    McpError::InvalidMessage("Invalid correlation ID UUID format".to_string())
                })?
                .into_bytes();
            Some(MessageId(id_bytes))
        } else {
            None
        };

        Ok(Self {
            id: MessageId(id_bytes),
            message_type: envelope.header.message_type,
            priority: envelope.header.priority,
            payload: envelope.payload,
            correlation_id,
            error: envelope.header.error,
        })
    }
}

/// JSON-RPC 2.0 request object for MCP protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    /// JSON-RPC version, always "2.0"
    pub jsonrpc: String,
    /// Method name to invoke
    pub method: String,
    /// Parameters for the method
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
    /// Unique identifier for the reques
    pub id: serde_json::Value,
}

impl JsonRpcRequest {
    /// Create a new JSON-RPC reques
    pub fn new(method: &str, params: Option<serde_json::Value>, id: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id,
        }
    }

    /// Serialize the request to JSON bytes
    pub fn to_bytes(&self) -> McpResult<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|e| McpError::Serialization(format!("Failed to serialize request: {}", e)))
    }

    /// Deserialize from JSON bytes
    pub fn from_bytes(bytes: &[u8]) -> McpResult<Self> {
        serde_json::from_slice(bytes)
            .map_err(|e| McpError::Deserialization(format!("Failed to deserialize request: {}", e)))
    }
}

/// JSON-RPC 2.0 response object for MCP protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    /// JSON-RPC version, always "2.0"
    pub jsonrpc: String,
    /// Result of the method call, must be present if no error
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error information, must be present if no resul
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    /// Request identifier that this response corresponds to
    pub id: serde_json::Value,
}

impl JsonRpcResponse {
    /// Create a new successful JSON-RPC response
    pub fn success(result: serde_json::Value, id: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    /// Create a new error JSON-RPC response
    pub fn error(error: JsonRpcError, id: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(error),
            id,
        }
    }

    /// Serialize the response to JSON bytes
    pub fn to_bytes(&self) -> McpResult<Vec<u8>> {
        serde_json::to_vec(self)
            .map_err(|e| McpError::Serialization(format!("Failed to serialize response: {}", e)))
    }

    /// Deserialize from JSON bytes
    pub fn from_bytes(bytes: &[u8]) -> McpResult<Self> {
        serde_json::from_slice(bytes).map_err(|e| {
            McpError::Deserialization(format!("Failed to deserialize response: {}", e))
        })
    }
}

/// JSON-RPC 2.0 notification object for MCP protocol (has no ID)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    /// JSON-RPC version, always "2.0"
    pub jsonrpc: String,
    /// Method name to invoke
    pub method: String,
    /// Parameters for the method
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcNotification {
    /// Create a new JSON-RPC notification
    pub fn new(method: &str, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        }
    }

    /// Serialize the notification to JSON bytes
    pub fn to_bytes(&self) -> McpResult<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| {
            McpError::Serialization(format!("Failed to serialize notification: {}", e))
        })
    }

    /// Deserialize from JSON bytes
    pub fn from_bytes(bytes: &[u8]) -> McpResult<Self> {
        serde_json::from_slice(bytes).map_err(|e| {
            McpError::Deserialization(format!("Failed to deserialize notification: {}", e))
        })
    }
}

/// JSON-RPC 2.0 batch request for MCP protocol
///
/// According to the JSON-RPC 2.0 specification, a batch request is an array of
/// request objects. Each request in the batch will be processed independently.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcBatchRequest(pub Vec<JsonRpcRequest>);

impl Default for JsonRpcBatchRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonRpcBatchRequest {
    /// Create a new empty batch request
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Create a batch request from a vector of requests
    pub fn from_requests(requests: Vec<JsonRpcRequest>) -> Self {
        Self(requests)
    }

    /// Add a request to the batch
    pub fn add_request(&mut self, request: JsonRpcRequest) {
        self.0.push(request);
    }

    /// Get the number of requests in the batch
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if the batch is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Serialize the batch request to JSON bytes
    pub fn to_bytes(&self) -> McpResult<Vec<u8>> {
        serde_json::to_vec(&self.0).map_err(|e| {
            McpError::Serialization(format!("Failed to serialize batch request: {}", e))
        })
    }

    /// Deserialize from JSON bytes
    pub fn from_bytes(bytes: &[u8]) -> McpResult<Self> {
        let requests = serde_json::from_slice::<Vec<JsonRpcRequest>>(bytes).map_err(|e| {
            McpError::Deserialization(format!("Failed to deserialize batch request: {}", e))
        })?;
        Ok(Self(requests))
    }
}

/// JSON-RPC 2.0 batch response for MCP protocol
///
/// According to the JSON-RPC 2.0 specification, a batch response is an array of
/// response objects that correspond to the batch request, in the same order.
/// The batch response may be empty if all requests in the batch were notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcBatchResponse(pub Vec<JsonRpcResponse>);

impl Default for JsonRpcBatchResponse {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonRpcBatchResponse {
    /// Create a new empty batch response
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Create a batch response from a vector of responses
    pub fn from_responses(responses: Vec<JsonRpcResponse>) -> Self {
        Self(responses)
    }

    /// Add a response to the batch
    pub fn add_response(&mut self, response: JsonRpcResponse) {
        self.0.push(response);
    }

    /// Get the number of responses in the batch
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Check if the batch is empty
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Serialize the batch response to JSON bytes
    pub fn to_bytes(&self) -> McpResult<Vec<u8>> {
        serde_json::to_vec(&self.0).map_err(|e| {
            McpError::Serialization(format!("Failed to serialize batch response: {}", e))
        })
    }

    /// Deserialize from JSON bytes
    pub fn from_bytes(bytes: &[u8]) -> McpResult<Self> {
        let responses = serde_json::from_slice::<Vec<JsonRpcResponse>>(bytes).map_err(|e| {
            McpError::Deserialization(format!("Failed to deserialize batch response: {}", e))
        })?;
        Ok(Self(responses))
    }
}

/// JSON-RPC 2.0 error object for MCP protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    /// Error code
    pub code: i32,
    /// Error message
    pub message: String,
    /// Additional error data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    /// Create a new JSON-RPC error
    pub fn new(code: i32, message: &str, data: Option<serde_json::Value>) -> Self {
        Self {
            code,
            message: message.to_string(),
            data,
        }
    }

    /// Parse error (-32700)
    pub fn parse_error(message: &str) -> Self {
        Self::new(-32700, message, None)
    }

    /// Invalid request error (-32600)
    pub fn invalid_request(message: &str) -> Self {
        Self::new(-32600, message, None)
    }

    /// Method not found error (-32601)
    pub fn method_not_found(message: &str) -> Self {
        Self::new(-32601, message, None)
    }

    /// Invalid params error (-32602)
    pub fn invalid_params(message: &str) -> Self {
        Self::new(-32602, message, None)
    }

    /// Internal error (-32603)
    pub fn internal_error(message: &str) -> Self {
        Self::new(-32603, message, None)
    }

    /// Server error (-32000 to -32099)
    pub fn server_error(code: i32, message: &str, data: Option<serde_json::Value>) -> Self {
        assert!(
            (-32099..=-32000).contains(&code),
            "Server error code must be between -32099 and -32000"
        );
        Self::new(code, message, data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_id_generation() {
        let id = MessageId::new();
        assert!(!id.to_string().is_empty());
        assert_eq!(id.to_string().len(), 36);
    }

    #[test]
    fn test_message_conversion() {
        let message = Message::new(
            MessageType::Request,
            Priority::Normal,
            vec![1, 2, 3],
            None,
            None,
        );

        let envelope = message.to_envelope().unwrap();
        assert_eq!(envelope.header.id, message.id.to_string());
        assert_eq!(envelope.header.message_type, MessageType::Request);
        assert_eq!(envelope.header.priority, Priority::Normal);
        assert_eq!(envelope.header.payload_size, 3);
        assert_eq!(envelope.payload, vec![1, 2, 3]);

        let reconstructed = Message::from_envelope(envelope).unwrap();
        assert_eq!(reconstructed.id.to_string(), message.id.to_string());
        assert_eq!(reconstructed.message_type, MessageType::Request);
        assert_eq!(reconstructed.priority, Priority::Normal);
        assert_eq!(reconstructed.payload, vec![1, 2, 3]);
    }
}
