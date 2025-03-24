#![allow(missing_docs)]
#![allow(dead_code)]

//! Error types for MCP-Agen
//!
//! This module defines the error types used throughout the MCP-Agent codebase.

use std::fmt;
use std::io;

/// MCP-Agent error types
#[derive(Debug)]
pub enum Error {
    /// I/O errors
    Io(io::Error),

    /// JSON serialization/deserialization errors
    Json(serde_json::Error),

    /// Internal errors with custom messages
    Internal(String),

    /// Validation errors
    Validation(String),

    /// Configuration errors
    Config(String),

    /// Authentication errors
    Auth(String),

    /// Protocol errors
    Protocol(String),

    /// Server-side errors
    Server(String),

    /// Client-side errors
    Client(String),

    /// Feature not implemented
    NotImplemented(String),

    /// Request timeou
    Timeout,

    /// Request cancelled
    Cancelled,

    /// Operation not permitted
    NotPermitted(String),

    /// Resource not found
    NotFound(String),

    /// Resource already exists
    AlreadyExists(String),

    /// Unexpected error
    Unexpected(String),

    /// Terminal error
    TerminalError(String),

    /// Generic error
    GenericError(String),

    /// Terminal not found error
    TerminalNotFound(String),

    /// No terminals available error
    NoTerminals,

    /// Command execution error
    CommandError(String),
}

/// Result type for MCP-Agent operations
pub type Result<T> = std::result::Result<T, Error>;

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(err) => write!(f, "IO error: {}", err),
            Error::Json(err) => write!(f, "JSON error: {}", err),
            Error::Internal(msg) => write!(f, "Internal error: {}", msg),
            Error::Validation(msg) => write!(f, "Validation error: {}", msg),
            Error::Config(msg) => write!(f, "Config error: {}", msg),
            Error::Auth(msg) => write!(f, "Authentication error: {}", msg),
            Error::Protocol(msg) => write!(f, "Protocol error: {}", msg),
            Error::Server(msg) => write!(f, "Server error: {}", msg),
            Error::Client(msg) => write!(f, "Client error: {}", msg),
            Error::NotImplemented(msg) => write!(f, "Not implemented: {}", msg),
            Error::Timeout => write!(f, "Request timeout"),
            Error::Cancelled => write!(f, "Request cancelled"),
            Error::NotPermitted(msg) => write!(f, "Operation not permitted: {}", msg),
            Error::NotFound(msg) => write!(f, "Resource not found: {}", msg),
            Error::AlreadyExists(msg) => write!(f, "Resource already exists: {}", msg),
            Error::Unexpected(msg) => write!(f, "Unexpected error: {}", msg),
            Error::TerminalError(msg) => write!(f, "Terminal error: {}", msg),
            Error::GenericError(msg) => write!(f, "Generic error: {}", msg),
            Error::TerminalNotFound(msg) => write!(f, "Terminal not found error: {}", msg),
            Error::NoTerminals => write!(f, "No terminals available error"),
            Error::CommandError(msg) => write!(f, "Command execution error: {}", msg),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err)
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Json(err)
    }
}

#[cfg(feature = "terminal-web")]
impl From<axum::Error> for Error {
    fn from(err: axum::Error) -> Self {
        Error::TerminalError(format!("Axum error: {}", err))
    }
}

#[cfg(feature = "terminal-full")]
impl From<jsonwebtoken::errors::Error> for Error {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        Error::TerminalError(format!("JWT error: {}", err))
    }
}

#[cfg(feature = "terminal-web")]
impl<T> From<tokio::sync::mpsc::error::SendError<T>> for Error {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Error::TerminalError("Channel send error".to_string())
    }
}

#[cfg(feature = "terminal-web")]
impl From<tokio::sync::oneshot::error::RecvError> for Error {
    fn from(_: tokio::sync::oneshot::error::RecvError) -> Self {
        Error::TerminalError("Oneshot receive error".to_string())
    }
}

#[cfg(feature = "terminal-web")]
impl From<tokio::task::JoinError> for Error {
    fn from(err: tokio::task::JoinError) -> Self {
        Error::TerminalError(format!("Task join error: {}", err))
    }
}

// Add From implementations for &str and String
impl From<&str> for Error {
    fn from(msg: &str) -> Self {
        Error::GenericError(msg.to_string())
    }
}

impl From<String> for Error {
    fn from(msg: String) -> Self {
        Error::GenericError(msg)
    }
}
