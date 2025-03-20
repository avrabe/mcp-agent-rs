//! LLM integrations for the MCP agent.
//!
//! This module provides integrations with various LLM providers, starting with Ollama.

pub mod types;
pub mod ollama;

// Re-export key components
pub use types::{LlmConfig, Message, MessageRole, Completion, CompletionRequest};
pub use ollama::OllamaClient; 