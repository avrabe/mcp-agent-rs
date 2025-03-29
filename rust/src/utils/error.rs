use serde::{Deserialize, Serialize};
use std::io;
use std::str::FromStr;
use thiserror::Error;

/// Result type for MCP operations
pub type McpResult<T> = Result<T, McpError>;

/// Error type for MCP operations
#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum McpError {
    /// Protocol-related errors
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Connection-related errors
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Invalid message errors
    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    /// Invalid state errors
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// Timeout errors
    #[error("Operation timed out")]
    Timeout,

    /// Not connected errors
    #[error("Not connected")]
    NotConnected,

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Server not found errors
    #[error("Server not found: {0}")]
    ServerNotFound(String),

    /// IO error during read/write operations
    #[error("IO error: {0}")]
    Io(String),

    /// UTF-8 error during string operations
    #[error("UTF-8 error: {0}")]
    Utf8(String),

    /// Custom error with error code and message
    #[error("{message} (code: {code})")]
    Custom {
        /// Error code
        code: u32,
        /// Error message
        message: String,
    },

    /// Feature not implemented
    #[error("Feature not implemented")]
    NotImplemented,

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(String),

    /// Task execution error
    #[error("Execution error: {0}")]
    Execution(String),

    /// Signal error
    #[error("Signal error: {0}")]
    Signal(String),

    /// Task join error
    #[error("Join error: {0}")]
    Join(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Not found error
    #[error("Not found: {0}")]
    NotFound(String),

    /// Validation error
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// IO error
    #[error("IO error: {0}")]
    IoError(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    /// Method not found error for JSON-RPC
    #[error("Method not found: {0}")]
    MethodNotFound(String),

    /// Invalid parameters error for JSON-RPC
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),

    /// Parse error for JSON-RPC
    #[error("Parse error: {0}")]
    ParseError(String),

    /// RPC error with code and message
    #[error("RPC error (code: {0}): {1}")]
    RpcError(i32, String),

    /// Invalid response error
    #[error("Invalid response: {0}")]
    InvalidResponse(String),

    /// Authentication error
    #[error("Authentication error: {0}")]
    AuthenticationError(String),

    /// Authorization error
    #[error("Authorization error: {0}")]
    AuthorizationError(String),

    /// Rate limit exceeded error
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),

    /// Server error
    #[error("Server error: {0}")]
    ServerError(String),

    /// Transport error
    #[error("Transport error: {0}")]
    TransportError(String),

    /// Network error
    #[error("Network error: {0}")]
    NetworkError(String),
}

// Implement FromStr for McpError to replace the custom from_str method
impl FromStr for McpError {
    type Err = McpError;

    /// Converts a string to an error
    ///
    /// # Errors
    ///
    /// This function never returns an error - it always converts the input string
    /// to an InvalidMessage error.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(McpError::InvalidMessage(s.to_string()))
    }
}

impl From<serde_json::Error> for McpError {
    fn from(err: serde_json::Error) -> Self {
        McpError::Json(err.to_string())
    }
}

impl From<tokio::time::error::Elapsed> for McpError {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        McpError::Timeout
    }
}

impl From<io::Error> for McpError {
    fn from(err: io::Error) -> Self {
        McpError::Io(err.to_string())
    }
}

impl From<std::str::Utf8Error> for McpError {
    fn from(err: std::str::Utf8Error) -> Self {
        McpError::Utf8(err.to_string())
    }
}

#[cfg(feature = "transport-http")]
impl From<reqwest::Error> for McpError {
    fn from(err: reqwest::Error) -> Self {
        McpError::NetworkError(format!("HTTP request error: {}", err))
    }
}
