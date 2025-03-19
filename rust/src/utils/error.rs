use thiserror::Error;

/// Represents errors that can occur during MCP protocol operations.
#[derive(Debug, Error)]
pub enum McpError {
    /// Error related to protocol operations, with a descriptive message.
    #[error("Protocol error: {0}")]
    Protocol(String),

    /// Error related to message validation, with a descriptive message.
    #[error("Validation error: {0}")]
    Validation(String),

    /// Error during serialization or deserialization of messages.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Input/Output error during message transmission.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Error indicating an invalid or malformed message.
    #[error("Invalid message: {0}")]
    InvalidMessage(String),

    /// Error indicating a timeout during an operation.
    #[error("Timeout: {0}")]
    Timeout(String),

    /// Error indicating a connection-related issue.
    #[error("Connection error: {0}")]
    Connection(String),

    /// Error indicating an authentication failure.
    #[error("Authentication error: {0}")]
    Authentication(String),
}

/// A specialized Result type for MCP operations.
pub type McpResult<T> = Result<T, McpError>; 