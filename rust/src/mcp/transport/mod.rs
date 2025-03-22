//! Transport layer for the MCP protocol.
//!
//! This module provides abstractions for different transport mechanisms
//! that can be used to send and receive MCP protocol messages.

pub mod http;
pub mod server;
pub mod websocket;

use crate::error::Error;
use crate::mcp::types::{JsonRpcRequest as Request, JsonRpcResponse as Response, Message};
use async_trait::async_trait;

/// Transport abstraction for MCP protocol
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// Send a message through the transport
    async fn send_message(&self, message: Message) -> Result<(), Error>;

    /// Send a request and wait for a response
    async fn send_request(&self, request: Request) -> Result<Response, Error>;

    /// Close the transport connection
    async fn close(&self) -> Result<(), Error>;

    /// Check if the transport is connected
    fn is_connected(&self) -> bool;
}

/// Factory for creating transport instances
pub trait TransportFactory: Send + Sync + 'static {
    /// Create a new transport instance
    fn create(&self) -> Result<Box<dyn Transport>, Error>;
}

/// Configuration for transport layer
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// The URL to connect to
    pub url: String,
    /// Timeout for requests in seconds
    pub timeout_seconds: u64,
    /// Maximum number of reconnection attempts
    pub max_reconnect_attempts: usize,
    /// Reconnection backoff delay in milliseconds
    pub reconnect_delay_ms: u64,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            url: "ws://localhost:8080".to_string(),
            timeout_seconds: 30,
            max_reconnect_attempts: 5,
            reconnect_delay_ms: 1000,
        }
    }
}
