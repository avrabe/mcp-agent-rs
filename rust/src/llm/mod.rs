//! LLM integrations for the MCP agent.
//!
//! This module provides integrations with various LLM providers, starting with Ollama.

/// Integration with Ollama API for local LLM inference
pub mod ollama;
/// Common types for LLM integrations
pub mod types;

// Re-export key components
#[cfg(feature = "ollama")]
pub use ollama::OllamaClient;
pub use types::{Completion, CompletionRequest, LlmConfig, Message, MessageRole};
