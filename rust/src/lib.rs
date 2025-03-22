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
//!
//! ## Basic Usage
//!
//! ```rust,no_run
//! use mcp_agent::mcp::agent::Agent;
//! use mcp_agent::mcp::types::{Message, Priority};
//! use serde_json::json;
//! 
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a new agent
//!     let agent = Agent::new(None);
//!     
//!     // Connect to a server
//!     let server_id = "example-server";
//!     agent.connect_to_test_server(server_id, "127.0.0.1:8080").await?;
//!     
//!     // Create and send a request
//!     let message = Message::request(json!({ "method": "get_status" }).to_string().into_bytes(), Priority::Normal);
//!     agent.send_message(server_id, message).await?;
//!     
//!     // Execute a task with timeout
//!     let result = agent.execute_task(
//!         "example.task", 
//!         json!({ "param": "value" }), 
//!         Some(std::time::Duration::from_secs(5))
//!     ).await?;
//!     
//!     println!("Task result: {}", result);
//!     
//!     // Disconnect from server
//!     agent.disconnect(server_id).await?;
//!     
//!     Ok(())
//! }
//! ```

/// Core MCP protocol implementation including message types, serialization, and transport.
pub mod mcp;

/// Utility modules for error handling and common functionality.
pub mod utils;

/// Configuration managemen
pub mod config;

/// Telemetry and observability with OpenTelemetry integration
pub mod telemetry;

/// Workflow engine for orchestrating tasks and managing workflow state
pub mod workflow;

/// LLM integrations with various providers
pub mod llm;

/// Human input handling for interactive workflows
pub mod human_input;

/// Error types for MCP-Agen
pub mod error;

/// Terminal system with console and web interfaces
#[cfg(feature = "terminal-web")]
pub mod terminal;

/// Re-exported MCP types for convenience
///
/// This includes the core types needed for working with the MCP protocol:
/// - `Executor` - For executing tasks with the MCP protocol
/// - `McpProtocol` - Low-level protocol implementation
/// - `Message` - Core message type
/// - `MessageType` - Enum of message types (Request, Response, etc)
/// - `Priority` - Message priority levels
#[doc(hidden)]
pub use mcp::{
    executor::Executor,
    protocol::McpProtocol,
    types::{Message, MessageType, Priority},
};

// Re-export workflow types
pub use workflow::{
    task, SignalHandler, TaskGroup, WorkflowEngine, WorkflowResult, WorkflowSignal, WorkflowState,
};

// Re-export error types
pub use error::Error;
pub use utils::error::{McpError, McpResult};

/// Re-export telemetry types and functions for easier access
pub use telemetry::{
    add_metric, add_metrics, init_telemetry, shutdown_telemetry, span_duration, TelemetryConfig,
};

/// Re-export LLM types for easier access
pub use llm::{Completion, CompletionRequest, LlmConfig, Message as LlmMessage, MessageRole};

/// Re-export human input types for easier access
pub use human_input::{
    ConsoleInputHandler, HumanInputHandler, HumanInputRequest, HumanInputResponse,
    HUMAN_INPUT_SIGNAL_NAME,
};

/// Re-export terminal system types for easier access
#[cfg(feature = "terminal-web")]
pub use terminal::{config::TerminalConfig, TerminalSystem, TerminalType};

#[cfg(feature = "ollama")]
pub use crate::llm::OllamaClient;
