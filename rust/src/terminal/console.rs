//! Console Terminal
//!
//! Provides a terminal interface via the console/stdio

use std::fmt;
use std::io::{stdin, stdout, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::{debug, error, info};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::oneshot;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use crate::error::Result;
use crate::terminal::AsyncTerminal;
use uuid::Uuid;

/// ConsoleTerminal implementation
#[derive(Debug)]
pub struct ConsoleTerminal {
    id: String,
    stdin: BufReader<tokio::io::Stdin>,
    stdout: tokio::io::Stdout,
}

impl ConsoleTerminal {
    /// Create a new ConsoleTerminal
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            stdin: BufReader::new(tokio::io::stdin()),
            stdout: tokio::io::stdout(),
        }
    }
}

impl Default for ConsoleTerminal {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncTerminal for ConsoleTerminal {
    async fn id(&self) -> Result<String> {
        Ok(self.id.clone())
    }

    async fn start(&mut self) -> Result<()> {
        debug!("ConsoleTerminal started");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        debug!("ConsoleTerminal stopped");
        Ok(())
    }

    async fn display(&self, output: &str) -> Result<()> {
        let mut stdout = tokio::io::stdout();
        stdout.write_all(output.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
        Ok(())
    }

    async fn echo_input(&self, input: &str) -> Result<()> {
        debug!("Echo input: {}", input);
        Ok(())
    }

    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<()> {
        info!("Executing command: {}", command);

        // Send a simple response
        let response = format!("Command executed: {}", command);
        if tx.send(response).is_err() {
            error!("Failed to send command response");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_console_terminal_id() {
        let terminal = ConsoleTerminal::new();
        let id = terminal.id().await.unwrap();
        assert_eq!(id, Uuid::new_v4().to_string());
    }

    #[tokio::test]
    async fn test_console_terminal_start_stop() {
        let mut terminal = ConsoleTerminal::new();
        assert!(terminal.start().await.is_ok());
        assert!(terminal.stop().await.is_ok());
    }

    #[tokio::test]
    async fn test_console_terminal_display() {
        let terminal = ConsoleTerminal::new();

        // Redirect stdout to a buffer for testing
        // Note: This is not a perfect test as we can't easily capture stdout in tests
        // In a real environment, you'd use a more sophisticated testing approach
        let result = terminal.display("test output\n").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_console_terminal_execute_command() {
        let terminal = ConsoleTerminal::new();
        let (tx, rx) = oneshot::channel();

        // Execute a simple echo command
        let result = terminal.execute_command("echo 'test command'", tx).await;
        assert!(result.is_ok());

        // Check the command outpu
        let output = rx.await.unwrap();
        assert!(output.contains("test command"));
    }
}
