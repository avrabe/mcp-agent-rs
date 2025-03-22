//! Transport layer for the MCP protocol.
//!
//! This module provides abstractions for different transport mechanisms
//! that can be used to send and receive MCP protocol messages.

use crate::{
    error::{Error, Result},
    mcp::types::{Message, Request, Response},
};
use std::sync::Arc;

/// Transport factory trait for creating transport instances
pub trait TransportFactory: Send + Sync + 'static {
    fn create(&self) -> Result<Arc<dyn Transport>>;
}

/// Transport trait for sending and receiving messages
pub trait Transport: Send + Sync + 'static {
    /// Send a message through the transport (non-async wrapper)
    fn send_message_boxed(
        &self,
        message: Message,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin>;

    /// Send a request and wait for a response (non-async wrapper)
    fn send_request_boxed(
        &self,
        request: Request,
    ) -> Box<dyn std::future::Future<Output = Result<Response>> + Send + Unpin>;

    /// Close the transport (non-async wrapper)
    fn close_boxed(&self) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin>;
}

/// Async transport trait with full async methods
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
    fn as_transport(&self) -> &dyn Transport;
}

impl<T: AsyncTransport + 'static> Transport for T {
    fn send_message_boxed(
        &self,
        message: Message,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin> {
        Box::pin(async move { self.send_message(message).await })
    }

    fn send_request_boxed(
        &self,
        request: Request,
    ) -> Box<dyn std::future::Future<Output = Result<Response>> + Send + Unpin> {
        Box::pin(async move { self.send_request(request).await })
    }

    fn close_boxed(&self) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin> {
        Box::pin(async move { self.close().await })
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

    pub async fn send_message(transport: &dyn Transport, message: Message) -> Result<()> {
        transport.send_message_boxed(message).await
    }

    pub async fn send_request(transport: &dyn Transport, request: Request) -> Result<Response> {
        transport.send_request_boxed(request).await
    }

    pub async fn close(transport: &dyn Transport) -> Result<()> {
        transport.close_boxed().await
    }
}

pub mod http;
pub mod websocket;

// Re-export transport implementations
pub use http::HttpTransport;
pub use websocket::WebSocketTransport;
