//! Console Terminal
//!
//! Provides a terminal interface via the console/stdio

use async_trait::async_trait;
use log::{debug, error};
use std::fmt::Debug;
use tokio::sync::oneshot;

use crate::error::Result;
use crate::terminal::AsyncTerminal;
use uuid::Uuid;

/// Console-based terminal implementation
#[derive(Debug)]
pub struct ConsoleTerminal {
    id: String,
    stdin: std::io::Stdin,
    stdout: std::io::Stdout,
    is_running: bool,
}

impl ConsoleTerminal {
    /// Create a new console terminal
    pub fn new(id: String) -> Self {
        Self {
            id,
            stdin: std::io::stdin(),
            stdout: std::io::stdout(),
            is_running: false,
        }
    }

    /// Start the console terminal
    pub async fn start(&mut self) -> Result<()> {
        debug!("ConsoleTerminal started");
        self.is_running = true;
        Ok(())
    }

    /// Stop the console terminal
    pub async fn stop(&mut self) -> Result<()> {
        debug!("ConsoleTerminal marked as stopped");
        self.is_running = false;
        Ok(())
    }
}

impl Default for ConsoleTerminal {
    fn default() -> Self {
        Self::new(Uuid::new_v4().to_string())
    }
}

#[async_trait]
impl AsyncTerminal for ConsoleTerminal {
    /// Return the terminal's unique identifier
    async fn id(&self) -> Result<String> {
        Ok(self.id.clone())
    }

    /// Start the console terminal
    async fn start(&mut self) -> Result<()> {
        ConsoleTerminal::start(self).await
    }

    /// Stop the console terminal
    async fn stop(&self) -> Result<()> {
        // Cannot modify self directly, just return OK for now
        debug!("Stopping console terminal (id: {})", self.id);
        Ok(())
    }

    /// Display output to the terminal
    async fn display(&self, output: &str) -> Result<()> {
        debug!("Console terminal display: {}", output);
        // Just print to stdout for now
        println!("{}", output);
        Ok(())
    }

    /// Echo input to the terminal
    async fn echo_input(&self, input: &str) -> Result<()> {
        debug!("Echo input: {}", input);
        Ok(())
    }

    /// Execute a command on the terminal
    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<()> {
        debug!("Execute command: {}", command);
        // Just echo the command back
        if tx.send(format!("Executed: {}", command)).is_err() {
            error!("Failed to send response");
        }
        Ok(())
    }

    /// Console terminals don't have network addresses
    async fn terminal_address(&self) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::oneshot;

    #[test]
    fn test_console_terminal_new() {
        let terminal = ConsoleTerminal::new("console".to_string());
        assert_eq!(terminal.id, "console");
    }

    #[tokio::test]
    async fn test_console_terminal_id() {
        let terminal = ConsoleTerminal::new("console".to_string());
        let id = terminal.id().await.unwrap();
        assert_eq!(id, "console");
    }

    #[tokio::test]
    async fn test_console_terminal_start_stop() {
        let mut terminal = ConsoleTerminal::new("console".to_string());

        // Start the terminal
        let result = terminal.start().await;
        assert!(result.is_ok());
        assert!(terminal.is_running);

        // Stop the terminal
        let result = terminal.stop().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_console_terminal_display() {
        let terminal = ConsoleTerminal::new("console".to_string());

        // Display to the terminal
        let result = terminal.display("test output\n").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_console_terminal_execute_command() {
        let terminal = ConsoleTerminal::new("console".to_string());

        // Create a oneshot channel for the response
        let (tx, rx) = oneshot::channel();

        // Execute a command
        let result = terminal.execute_command("echo 'test command'", tx).await;
        assert!(result.is_ok());

        // Check the response
        let response = rx.await.unwrap();
        assert_eq!(response, "Executed: echo 'test command'");
    }
}
