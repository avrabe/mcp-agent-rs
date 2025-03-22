//! Core MCP protocol implementation including message types, serialization, and transport.
//!
//! # Model Context Protocol (MCP)
//!
//! The Model Context Protocol (MCP) is a standardized protocol for communication between
//! AI models and their environments. It enables structured, efficient, and transparent interactions
//! with AI systems, allowing for better debugging, monitoring, and control.
//!
//! This module provides a complete Rust implementation of the MCP, with the following features:
//!
//! - **Type-safe message handling**: Strong Rust types for all protocol elements
//! - **Asynchronous communication**: Full async/await support using Tokio
//! - **JSON-RPC implementation**: Compatible with JSON-RPC 2.0 specification
//! - **Connection management**: TCP/Stream-based connections with automatic reconnection
//! - **Error handling**: Comprehensive error types with detailed information
//! - **Telemetry integration**: Performance metrics and tracing
//!
//! ## Architecture
//!
//! The MCP implementation is organized into several submodules:
//!
//! - `types`: Core message types and data structures
//! - `protocol`: Low-level protocol implementation for serialization/deserialization
//! - `connection`: Connection management for transport protocols
//! - `agent`: Higher-level agent abstraction for MCP interactions
//! - `executor`: Task execution engine for MCP operations
//! - `jsonrpc`: JSON-RPC 2.0 implementation for the MCP protocol
//! - `transport`: Transport layer implementations (WebSocket, HTTP)
//!
//! ## Usage Example
//!
//! ```rust,no_run
//! use mcp_agent::mcp::agent::Agent;
//! use mcp_agent::mcp::types::{Message, Priority};
//! use mcp_agent::mcp::agent::AgentConfig;
//!
//! async fn example() {
//!     // Create an agent configuration
//!     let config = AgentConfig::default();
//!
//!     // Create a new agent
//!     let agent = Agent::new(config);
//!
//!     // Initialize the agent
//!     agent.initialize().await.expect("Failed to initialize agent");
//!
//!     // Send a message
//!     let message = Message::request(Vec::new(), Priority::Normal);
//!     agent.send_message("server-id", message).await.expect("Failed to send message");
//!
//!     // Disconnect the agent
//!     agent.disconnect().await.expect("Failed to disconnect agent");
//! }
//! ```

/// Types used in the MCP protocol including Message, MessageType, and Priority
pub mod types;

/// Protocol implementation for handling MCP communication
pub mod protocol;

/// Executor for running MCP agents and handling lifecycle
pub mod executor;

/// Connection module for handling MCP protocol connections
pub mod connection;

/// Agent for managing connections and message passing
pub mod agent;

/// JSON-RPC implementation for the MCP protocol
pub mod jsonrpc;

/// Transport layer implementations for WebSocket and HTTP
pub mod transport;
