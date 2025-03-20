#![deny(missing_docs)]
#![deny(missing_debug_implementations)]
#![deny(rustdoc::missing_crate_level_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![deny(rustdoc::private_intra_doc_links)]
#![deny(rustdoc::invalid_codeblock_attributes)]
#![deny(rustdoc::invalid_html_tags)]
#![deny(rustdoc::bare_urls)]
#![deny(clippy::missing_panics_doc)]

//! MCP-Agent is a pure Rust implementation of the Model Context Protocol (MCP) agent framework.
//! It provides a high-performance, type-safe implementation of the MCP protocol with zero-copy
//! operations and comprehensive error handling.

/// Core MCP protocol implementation including message types, serialization, and transport.
pub mod mcp;

/// Utility modules for error handling and common functionality.
pub mod utils;

/// Configuration management
pub mod config;

pub use mcp::{
    connection::{Connection, ConnectionConfig},
    protocol::McpProtocol,
    types::{Message, MessageType, Priority},
    server_manager::{ServerManager, ServerSettings, ServerAuthSettings},
};

pub use utils::error::{McpError, McpResult};
