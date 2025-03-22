//! Transport layer for the MCP protocol.
//!
//! This module provides abstractions for different transport mechanisms
//! that can be used to send and receive MCP protocol messages.

use crate::mcp::types::{JsonRpcRequest as Request, JsonRpcResponse as Response};
use crate::{
    error::{Error, Result},
    mcp::types::Message,
};
use async_trait::async_trait;
use serde_json;
use std::sync::Arc;

/// Configuration for transport implementations
#[derive(Debug, Clone)]
pub struct TransportConfig {
    /// The transport endpoint URL
    pub url: String,
    /// Additional configuration options
    pub options: serde_json::Value,
}

impl TransportConfig {
    /// Create a new transport config with the given URL
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            options: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// Create a new transport config with the given URL and options
    pub fn new_with_options(url: impl Into<String>, options: serde_json::Value) -> Self {
        Self {
            url: url.into(),
            options,
        }
    }
}

/// Transport factory trait for creating transport instances
pub trait TransportFactory: Send + Sync + 'static {
    /// Create a new transport instance
    fn create(&self) -> Result<Arc<dyn Transport>>;
}

/// Transport trait for sending and receiving messages
pub trait Transport: Send + Sync + 'static {
    /// Send a message through the transport (non-async wrapper)
    fn send_message_boxed(
        &self,
        message: Message,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_>;

    /// Send a request and wait for a response (non-async wrapper)
    fn send_request_boxed(
        &self,
        request: Request,
    ) -> Box<dyn std::future::Future<Output = Result<Response>> + Send + Unpin + '_>;

    /// Close the transport (non-async wrapper)
    fn close_boxed(&self) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_>;
}

/// Async transport trait with full async methods
#[async_trait]
pub trait AsyncTransport: Send + Sync + 'static {
    /// Send a message through the transport
    async fn send_message(&self, message: Message) -> Result<()>;

    /// Send a request and wait for a response
    async fn send_request(&self, request: Request) -> Result<Response>;

    /// Close the transport
    async fn close(&self) -> Result<()>;
}

/// Helper extension trait for AsyncTransport implementations
pub trait TransportExt: AsyncTransport {
    /// Get a reference to the underlying Transport trait object
    fn as_transport(&self) -> &dyn Transport;
}

impl<T: AsyncTransport + 'static> Transport for T {
    fn send_message_boxed(
        &self,
        message: Message,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        Box::new(Box::pin(async move { self.send_message(message).await }))
    }

    fn send_request_boxed(
        &self,
        request: Request,
    ) -> Box<dyn std::future::Future<Output = Result<Response>> + Send + Unpin + '_> {
        Box::new(Box::pin(async move { self.send_request(request).await }))
    }

    fn close_boxed(&self) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        Box::new(Box::pin(async move { self.close().await }))
    }
}

impl<T: AsyncTransport + 'static> TransportExt for T {
    fn as_transport(&self) -> &dyn Transport {
        self
    }
}

/// Async helper functions to make working with dyn Transport easier
pub mod transport_helpers {
    use super::*;

    /// Send a message through the transport
    pub async fn send_message(transport: &dyn Transport, message: Message) -> Result<()> {
        transport.send_message_boxed(message).await
    }

    /// Send a request and wait for a response
    pub async fn send_request(transport: &dyn Transport, request: Request) -> Result<Response> {
        transport.send_request_boxed(request).await
    }

    /// Close the transport
    pub async fn close(transport: &dyn Transport) -> Result<()> {
        transport.close_boxed().await
    }
}

pub mod http;
pub mod websocket;

// Re-export transport implementations
#[cfg(feature = "transport-http")]
pub use http::HttpTransport;
#[cfg(feature = "transport-ws")]
pub use websocket::WebSocketTransport;
