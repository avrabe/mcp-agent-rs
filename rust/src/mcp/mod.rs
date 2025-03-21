//! Core MCP protocol implementation including message types, serialization, and transport.

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
