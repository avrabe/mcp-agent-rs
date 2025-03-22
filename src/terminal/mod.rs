use std::collections::HashMap;
use std::sync::Arc;
use std::fmt;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::oneshot;
use tracing::{debug, error, info};

use crate::error::{Error, Result};
use crate::workflow::WorkflowEngine;
// Comment out missing imports for now - these would be implemented based on LLM and human input systems
// use crate::llm::LlmProvider;
// use crate::human_input::HumanInputProvider;

// Module imports
pub mod config;
pub mod router;
pub mod console;
pub mod web;
pub mod sync;
pub mod graph;

// Re-export important items
pub use config::{TerminalConfig, AuthConfig};
pub use console::ConsoleTerminal;
pub use web::WebTerminal;
pub use config::WebTerminalConfig;
// These need to be implemented or adjusted
// pub use sync::{SyncTerminal, TerminalHandle};

// Expose the initialize function
pub use router::initialize_terminal;

// Also expose graph visualization initialization
pub use router::initialize_visualization;

/// Terminal interface trait
#[async_trait]
pub trait Terminal: Send + Sync + fmt::Debug {
    /// Return the terminal's unique identifier
    async fn id(&self) -> Result<String>;
    
    /// Start the terminal
    async fn start(&mut self) -> Result<()>;
    
    /// Stop the terminal
    async fn stop(&mut self) -> Result<()>;
    
    /// Display output to the terminal
    async fn display(&self, output: &str) -> Result<()>;
    
    /// Echo input back to the terminal
    async fn echo_input(&self, input: &str) -> Result<()>;
    
    /// Execute a command in the terminal
    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<()>;
}

/// Default terminal implementation
#[derive(Debug)]
pub struct DefaultTerminal {
    /// Terminal ID
    id: String,
}

impl DefaultTerminal {
    /// Create a new default terminal
    pub fn new(id: String) -> Self {
        Self { id }
    }
}

#[async_trait]
impl Terminal for DefaultTerminal {
    /// Return the terminal's unique identifier
    async fn id(&self) -> Result<String> {
        info!("DefaultTerminal::id");
        Ok(self.id.clone())
    }
    
    /// Start the terminal
    async fn start(&mut self) -> Result<()> {
        info!("DefaultTerminal::start");
        Ok(())
    }
    
    /// Stop the terminal
    async fn stop(&mut self) -> Result<()> {
        info!("DefaultTerminal::stop");
        Ok(())
    }
    
    /// Display output to the terminal
    async fn display(&self, output: &str) -> Result<()> {
        info!("DefaultTerminal::display: {}", output);
        Ok(())
    }
    
    /// Echo input back to the terminal
    async fn echo_input(&self, input: &str) -> Result<()> {
        info!("DefaultTerminal::echo_input: {}", input);
        Ok(())
    }
    
    /// Execute a command in the terminal
    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<()> {
        info!("DefaultTerminal::execute_command: {}", command);
        let _ = tx.send(format!("Executed: {}", command));
        Ok(())
    }
} 