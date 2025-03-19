use thiserror::Error;

/// A specialized Result type for MCP operations.
pub type McpResult<T> = Result<T, McpError>;

/// Represents errors that can occur during MCP protocol operations.
#[derive(Debug, Error)]
pub enum McpError {
    /// Invalid message format or content
    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    /// IO error during read/write operations
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// UTF-8 encoding/decoding error
    #[error("UTF-8 error: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    /// Connection failed
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    /// Not connected to the remote endpoint
    #[error("Not connected")]
    NotConnected,

    /// Operation timed out
    #[error("Operation timed out")]
    Timeout,

    /// Feature not implemented
    #[error("Feature not implemented")]
    NotImplemented,
} 