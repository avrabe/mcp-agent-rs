//! Terminal helper functions
//!
//! This module provides helper functions for terminal operations.

use once_cell::sync::Lazy;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::sync::Mutex;

use crate::error::Result;
use crate::terminal::Terminal;

/// A global function to send updates to web clients
static WEB_UPDATE_FN: Lazy<Mutex<Option<Arc<dyn Fn(&str, &str) -> Result<()> + Send + Sync>>>> =
    Lazy::new(|| Mutex::new(None));

/// Set the function to be used for web updates
pub async fn set_web_update_function(
    func: impl Fn(&str, &str) -> Result<()> + Send + Sync + 'static,
) {
    let mut update_fn = WEB_UPDATE_FN.lock().await;
    *update_fn = Some(Arc::new(func));
}

/// Get the function to be used for web updates
pub async fn get_web_update_function() -> Option<Arc<dyn Fn(&str, &str) -> Result<()> + Send + Sync>>
{
    let update_fn = WEB_UPDATE_FN.lock().await;
    update_fn.clone()
}

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
