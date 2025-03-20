//! LLM integrations for the MCP agent.
//!
//! This module provides integrations with various LLM providers, starting with Ollama.

/// Common types for LLM integrations
pub mod types;
#[cfg(feature = "ollama")]
pub mod ollama;

// Re-export key components
pub use types::{LlmConfig, Message, MessageRole, Completion, CompletionRequest};
#[cfg(feature = "ollama")]
pub use ollama::OllamaClient; 