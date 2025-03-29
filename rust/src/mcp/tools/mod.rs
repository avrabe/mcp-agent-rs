//! # MCP Tools System
//!
//! The tools system provides functionality for defining and executing functions
//! that allow language models to perform actions or retrieve information.
//!
//! This module implements the tools primitive as specified in the Model Context Protocol.
//!
//! ## Features
//!
//! - Tool definition and registration
//! - Parameter validation and type checking
//! - Execution with proper error handling
//! - Safe serialization/deserialization of inputs and outputs
//!
//! ## Usage
//!
//! ```rust,no_run
//! use mcp_agent::mcp::tools::{Tool, ToolResult, BasicToolProvider, ToolsHandler};
//! use serde_json::json;
//! use std::sync::Arc;
//!
//! // Create a tool provider
//! let provider = BasicToolProvider::new();
//!
//! // Define a tool
//! let tool = Tool::new(
//!     "search_code",
//!     "Search for code patterns in the codebase",
//!     json!({
//!         "type": "object",
//!         "properties": {
//!             "query": {
//!                 "type": "string",
//!                 "description": "Search query"
//!             }
//!         },
//!         "required": ["query"]
//!     }),
//! );
//!
//! // Register the tool with the provider
//! provider.register_tool(tool, |params| {
//!     // Extract query from params
//!     let query = params["query"].as_str().unwrap_or_default();
//!     
//!     // Perform the search
//!     let results = vec!["file1.rs:10", "file2.rs:25"];
//!     
//!     // Return the result
//!     Ok(ToolResult::text(&format!("Found {} results for {}: {:?}", results.len(), query, results)))
//! }).unwrap();
//!
//! // Create a handler for the provider
//! let handler = ToolsHandler::new(Arc::new(provider));
//! ```

mod handler;
mod models;
mod provider;
mod tests;

// Re-export the public API
pub use handler::{
    CallToolParams, ListToolsParams, ListToolsResponse, ToolsCapabilities, ToolsHandler,
    ToolsProvider,
};
pub use models::{ResourceContent, Tool, ToolResult, ToolResultContent};
pub use provider::BasicToolProvider;

/// Initializes the tools system
pub fn initialize() -> std::result::Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
