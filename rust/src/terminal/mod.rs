//! Terminal System
//!
//! This module provides a unified terminal interface layer that supports both
//! console and web-based terminals. It manages I/O synchronization between
//! multiple terminal interfaces and provides a clean API for the agent to
//! interact with terminals.
//!
//! Key components:
//!
//! - Terminal Router: Connects terminal interfaces to the agent
//! - Terminal Synchronizer: Manages I/O between terminals
//! - Console Terminal: Console-based implementation
//! - Web Terminal Server: Web-based implementation

pub mod config;
pub mod console;
pub mod router;
pub mod sync;
pub mod web;

use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::{oneshot, Mutex};
use crate::error::Error;

/// The Terminal trait defines the interface for all terminal implementations
#[async_trait]
pub trait Terminal: Send + Sync {
    /// Get the terminal identifier
    async fn id(&self) -> Result<String, Error>;
    
    /// Start the terminal
    async fn start(&mut self) -> Result<(), Error>;
    
    /// Stop the terminal
    async fn stop(&mut self) -> Result<(), Error>;
    
    /// Display output on the terminal
    async fn display(&self, output: &str) -> Result<(), Error>;
    
    /// Echo input from another terminal
    async fn echo_input(&self, input: &str) -> Result<(), Error>;
    
    /// Execute a command on the terminal and return the result
    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<(), Error>;
}

/// Implement Terminal for Mutex<T> where T: Terminal
#[async_trait]
impl<T> Terminal for Mutex<T>
where
    T: Terminal + Send,
{
    async fn id(&self) -> Result<String, Error> {
        let guard = self.lock().await;
        guard.id().await
    }
    
    async fn start(&mut self) -> Result<(), Error> {
        let mut guard = self.lock().await;
        guard.start().await
    }
    
    async fn stop(&mut self) -> Result<(), Error> {
        let mut guard = self.lock().await;
        guard.stop().await
    }
    
    async fn display(&self, output: &str) -> Result<(), Error> {
        let guard = self.lock().await;
        guard.display(output).await
    }
    
    async fn echo_input(&self, input: &str) -> Result<(), Error> {
        let guard = self.lock().await;
        guard.echo_input(input).await
    }
    
    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<(), Error> {
        let guard = self.lock().await;
        guard.execute_command(command, tx).await
    }
}

/// Terminal types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalType {
    /// Console terminal
    Console,
    /// Web terminal
    Web,
}

impl ToString for TerminalType {
    fn to_string(&self) -> String {
        match self {
            TerminalType::Console => "console".to_string(),
            TerminalType::Web => "web".to_string(),
        }
    }
}

/// Terminal system that manages all terminal interfaces
#[derive(Clone, Debug)]
pub struct TerminalSystem {
    /// Terminal router
    router: Arc<router::TerminalRouter>,
}

impl TerminalSystem {
    /// Create a new terminal system with the provided configuration
    pub fn new(config: config::TerminalConfig) -> Self {
        Self {
            router: Arc::new(router::TerminalRouter::new(config)),
        }
    }
    
    /// Start the terminal system
    pub async fn start(&self) -> Result<(), Error> {
        self.router.start().await
    }
    
    /// Stop the terminal system
    pub async fn stop(&self) -> Result<(), Error> {
        self.router.stop().await
    }
    
    /// Toggle the web terminal on or off
    pub async fn toggle_web_terminal(&self, enabled: bool) -> Result<(), Error> {
        self.router.toggle_web_terminal(enabled).await
    }
    
    /// Write data to all connected terminals
    pub async fn write(&self, data: &str) -> Result<(), Error> {
        self.router.write(data).await
    }
    
    /// Read the next input from any terminal (blocks until input is available)
    pub async fn read(&self) -> Result<String, Error> {
        self.router.read().await
    }
    
    /// Check if a terminal is enabled
    pub fn is_terminal_enabled(&self, terminal_type: TerminalType) -> bool {
        self.router.is_terminal_enabled(&terminal_type.to_string())
    }
    
    /// Get the web terminal address if enabled
    pub fn web_terminal_address(&self) -> Option<String> {
        self.router.web_terminal_address()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_terminal_system_default() {
        // Create a default terminal system (console only)
        let config = config::TerminalConfig::console_only();
        let system = TerminalSystem::new(config);
        
        // Check initial state (no terminals are active until started)
        assert!(!system.is_terminal_enabled(TerminalType::Console));
        assert!(!system.is_terminal_enabled(TerminalType::Web));
        
        // Start the system
        let result = system.start().await;
        assert!(result.is_ok());
        
        // Check that only console is enabled
        assert!(system.is_terminal_enabled(TerminalType::Console));
        assert!(!system.is_terminal_enabled(TerminalType::Web));
        
        // Stop the system
        let result = system.stop().await;
        assert!(result.is_ok());
    }
} 