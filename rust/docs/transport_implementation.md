# WebSocket/HTTP Transport Implementation Plan

This document outlines the plan for implementing WebSocket and HTTP transport layers for the MCP protocol (REQ-023).

## Overview

Based on the codebase exploration, we have identified that:

1. There is already WebSocket implementation for the web terminal interface
2. There is currently no specific transport layer for the MCP protocol
3. We need to implement both WebSocket and HTTP transport options for the MCP protocol

## Implementation Strategy

### 1. Create Transport Abstraction

Create a transport module that defines a common interface for different transport mechanisms:

```rust
// src/mcp/transport/mod.rs
pub mod http;
pub mod websocket;

use async_trait::async_trait;
use crate::error::Error;
use crate::mcp::protocol::{Message, Request, Response};
use std::sync::Arc;

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
```

### 2. Implement WebSocket Transport

Use `tokio-tungstenite` for WebSocket implementation:

```rust
// src/mcp/transport/websocket.rs
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as WsMessage, WebSocketStream, MaybeTlsStream};
use tokio::sync::{mpsc, Mutex};
use url::Url;
use std::sync::Arc;

use crate::error::Error;
use crate::mcp::protocol::{Message, Request, Response};
use super::Transport;

pub struct WebSocketTransport {
    sender: mpsc::Sender<Message>,
    receiver: Arc<Mutex<mpsc::Receiver<Message>>>,
    connected: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait]
impl Transport for WebSocketTransport {
    async fn send_message(&self, message: Message) -> Result<(), Error> {
        // Implementation
    }

    async fn send_request(&self, request: Request) -> Result<Response, Error> {
        // Implementation with request-response matching
    }

    async fn close(&self) -> Result<(), Error> {
        // Implementation
    }

    fn is_connected(&self) -> bool {
        self.connected.load(std::sync::atomic::Ordering::Relaxed)
    }
}

pub struct WebSocketTransportFactory {
    url: Url,
}

impl WebSocketTransportFactory {
    pub fn new(url: Url) -> Self {
        Self { url }
    }
}

impl super::TransportFactory for WebSocketTransportFactory {
    fn create(&self) -> Result<Box<dyn Transport>, Error> {
        // Implementation
    }
}
```

### 3. Implement HTTP Transport

Use `reqwest` for HTTP implementation:

```rust
// src/mcp/transport/http.rs
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use std::sync::Arc;
use url::Url;

use crate::error::Error;
use crate::mcp::protocol::{Message, Request, Response};
use super::Transport;

pub struct HttpTransport {
    client: Client,
    url: Url,
}

#[async_trait]
impl Transport for HttpTransport {
    async fn send_message(&self, message: Message) -> Result<(), Error> {
        // Implementation
    }

    async fn send_request(&self, request: Request) -> Result<Response, Error> {
        // Implementation
    }

    async fn close(&self) -> Result<(), Error> {
        // No-op for HTTP as it's stateless
        Ok(())
    }

    fn is_connected(&self) -> bool {
        true // HTTP is stateless
    }
}

pub struct HttpTransportFactory {
    url: Url,
}

impl HttpTransportFactory {
    pub fn new(url: Url) -> Self {
        Self { url }
    }
}

impl super::TransportFactory for HttpTransportFactory {
    fn create(&self) -> Result<Box<dyn Transport>, Error> {
        // Implementation
    }
}
```

### 4. Update MCP Connection to Use the Transport Layer

Modify the `Connection` struct to use the transport abstraction:

```rust
// src/mcp/connection.rs
use crate::mcp::transport::{Transport, TransportFactory};

pub struct Connection {
    transport: Box<dyn Transport>,
    // Other fields
}

impl Connection {
    pub fn new(transport_factory: &dyn TransportFactory) -> Result<Self, Error> {
        let transport = transport_factory.create()?;
        // Other initialization
        Ok(Self { transport, /* other fields */ })
    }

    pub async fn send_request(&self, request: Request) -> Result<Response, Error> {
        self.transport.send_request(request).await
    }

    // Other methods
}
```

### 5. Implement Server-side Transport Handlers

Create server implementations that can accept connections:

```rust
// src/mcp/transport/server.rs
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::net::TcpListener;
use axum::{
    Router,
    routing::{get, post},
    extract::{Path, State, WebSocketUpgrade},
};

use crate::error::Error;
use crate::mcp::protocol::{Message, Request, Response};

pub struct TransportServer {
    // Fields
}

impl TransportServer {
    pub async fn run_http_server(&self, addr: &str) -> Result<(), Error> {
        // Implementation
    }

    pub async fn run_websocket_server(&self, addr: &str) -> Result<(), Error> {
        // Implementation
    }

    // Handler methods
}
```

## Implementation Tasks

1. **Create Transport Module Structure**:

   - Create `src/mcp/transport/mod.rs` with trait definitions
   - Add module to `src/mcp/mod.rs`

2. **Implement WebSocket Transport**:

   - Add dependencies (`tokio-tungstenite`) to `Cargo.toml`
   - Create `src/mcp/transport/websocket.rs`
   - Implement client-side WebSocket transport
   - Implement server-side WebSocket handler

3. **Implement HTTP Transport**:

   - Add dependencies (`reqwest`) to `Cargo.toml` if not already included
   - Create `src/mcp/transport/http.rs`
   - Implement client-side HTTP transport
   - Implement server-side HTTP endpoints

4. **Integration with MCP Connection**:

   - Update `src/mcp/connection.rs` to use the transport layer
   - Add configuration options for selecting transport type

5. **Testing**:

   - Create unit tests for each transport implementation
   - Create integration tests for the full transport stack
   - Test both client and server sides

6. **Documentation**:
   - Document the transport API
   - Add examples for different transport configurations
   - Update architecture documentation

## Next Steps

The first step will be to add the transport module structure and implement the WebSocket transport, as WebSocket support is more critical for real-time communication in the MCP protocol.

Dependencies to add:

- tokio-tungstenite: WebSocket implementation for Tokio
- reqwest: HTTP client for Rust
- url: URL parsing and manipulation library
