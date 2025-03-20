use std::fmt;
use thiserror::Error;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

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
}

impl McpError {
    /// Convert a string to an error
    pub fn from_str(s: &str) -> Self {
        McpError::InvalidMessage(s.to_string())
    }
}

impl From<serde_json::Error> for McpError {
    fn from(err: serde_json::Error) -> Self {
        McpError::InvalidMessage(format!("JSON error: {}", err))
    }
}

impl From<tokio::time::error::Elapsed> for McpError {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        McpError::Timeout
    }
}

impl From<std::io::Error> for McpError {
    fn from(err: std::io::Error) -> Self {
        McpError::Io(err.to_string())
    }
}

impl From<std::str::Utf8Error> for McpError {
    fn from(err: std::str::Utf8Error) -> Self {
        McpError::Utf8(err.to_string())
    }
} 