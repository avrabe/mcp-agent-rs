//! Human input handling for MCP agent.
//!
//! This module provides utilities for requesting and handling input from humans,
//! particularly useful in workflows that require human intervention.

mod handler;
mod types;

// Re-export key components
pub use handler::ConsoleInputHandler;
pub use types::{
    HumanInputHandler, HumanInputRequest, HumanInputResponse, HUMAN_INPUT_SIGNAL_NAME,
};
