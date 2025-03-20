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

/// Telemetry and observability with OpenTelemetry integration
pub mod telemetry;

/// Workflow engine for orchestrating tasks and managing workflow state
pub mod workflow;

/// LLM integrations with various providers
pub mod llm;

pub use mcp::{
    protocol::McpProtocol,
    types::{Message, MessageType, Priority},
    executor::Executor,
};

pub use utils::error::{McpError, McpResult};

/// Re-export telemetry types and functions for easier access
pub use telemetry::{
    TelemetryConfig, 
    init_telemetry, 
    shutdown_telemetry, 
    span_duration,
    alerting,
};

/// Re-export alerting system types for easier access
pub use telemetry::alerts::{
    AlertingSystem,
    AlertingConfig,
    AlertDefinition,
    AlertSeverity,
    AlertOperator,
    Alert,
    TerminalAlertOptions,
};

// Re-export LLM types for easier access
pub use llm::{
    LlmConfig, 
    Message as LlmMessage, 
    MessageRole, 
    Completion,
    CompletionRequest,
    OllamaClient,
};
