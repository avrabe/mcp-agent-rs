//! # MCP Prompts System
//!
//! The prompts system provides functionality for defining, managing, and executing
//! pre-defined templates or instructions that guide language model interactions.
//!
//! This module implements the prompts primitive as specified in the Model Context Protocol.
//!
//! ## Features
//!
//! - Prompt template definition and management
//! - Parameter validation and substitution
//! - Prompt versioning and tracking
//! - User-controlled interactive prompts (slash commands, menu options)
//!
//! ## Usage
//!
//! ```rust,no_run
//! use mcp_agent::mcp::prompts::{PromptTemplate, PromptManager};
//!
//! // Create a prompt template
//! let template = PromptTemplate::new(
//!     "analyze_code",
//!     "Analyze the following {{language}} code:\n\n```{{language}}\n{{code}}\n```",
//! );
//!
//! // Register the template with the prompt manager
//! let mut manager = PromptManager::new();
//! manager.register_template(template);
//!
//! // Use the template with parameters
//! let prompt = manager.render_prompt("analyze_code", &[
//!     ("language", "rust"),
//!     ("code", "fn main() { println!(\"Hello, world!\"); }"),
//! ]);
//! ```

mod interactive;
mod manager;
mod template;
pub mod types;

pub use interactive::{MenuOption, SlashCommand};
pub use manager::PromptManager;
pub use template::PromptTemplate;
pub use types::{PromptCategory, PromptId, PromptParameter, PromptParameters, PromptVersion};

use crate::utils::error::McpResult;

/// Initializes the prompts system with default templates
pub async fn initialize() -> McpResult<PromptManager> {
    let manager = PromptManager::new();

    // Register default templates
    manager.register_defaults().await?;

    Ok(manager)
}
