//! Terminal helper functions
//!
//! This module provides helper functions for terminal operations.

use tokio::sync::oneshot;

use crate::error::Result;
use crate::terminal::Terminal;

/// Get the terminal ID
pub async fn id(terminal: &dyn Terminal) -> Result<String> {
    terminal.id_sync().await
}

/// Start the terminal
pub async fn start(terminal: &mut dyn Terminal) -> Result<()> {
    terminal.start_sync().await
}

/// Stop the terminal
pub async fn stop(terminal: &dyn Terminal) -> Result<()> {
    terminal.stop_sync().await
}

/// Display output to the terminal
pub async fn display(terminal: &dyn Terminal, output: &str) -> Result<()> {
    terminal.display_sync(output).await
}

/// Echo input to the terminal
pub async fn echo_input(terminal: &dyn Terminal, input: &str) -> Result<()> {
    terminal.echo_input_sync(input).await
}

/// Execute a command on the terminal
pub async fn execute_command(
    terminal: &dyn Terminal,
    command: &str,
    tx: oneshot::Sender<String>,
) -> Result<()> {
    terminal.execute_command_sync(command, tx).await
}
