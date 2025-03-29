//! Terminal Synchronizer
//!
//! This module manages I/O synchronization between multiple terminal interfaces

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::terminal::terminal_helpers;
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{debug, error, info};

use super::Terminal;
use crate::error::{Error, Result};

/// Terminal Synchronizer manages I/O across multiple terminal interfaces
pub struct TerminalSynchronizer {
    /// Connected terminals by ID
    terminals: Mutex<HashMap<String, Arc<dyn Terminal>>>,
    /// Input queue for terminal inputs
    input_queue: Mutex<mpsc::Receiver<String>>,
    /// Sender for input queue
    input_sender: Mutex<mpsc::Sender<String>>,
}

impl fmt::Debug for TerminalSynchronizer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TerminalSynchronizer")
            .field("terminals", &"<Mutex<HashMap<String, Arc<dyn Terminal>>>>")
            .field("input_queue", &"<Mutex<mpsc::Receiver<String>>>")
            .field("input_sender", &"<Mutex<mpsc::Sender<String>>>")
            .finish()
    }
}

impl Default for TerminalSynchronizer {
    fn default() -> Self {
        Self::new()
    }
}

impl TerminalSynchronizer {
    /// Create a new terminal synchronizer
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel(100);
        Self {
            terminals: Mutex::new(HashMap::new()),
            input_queue: Mutex::new(rx),
            input_sender: Mutex::new(tx),
        }
    }

    /// Register a terminal with the synchronizer
    pub async fn register_terminal(&self, terminal: Arc<dyn Terminal>) -> Result<()> {
        let id = terminal.id_sync().await?;
        let mut terminals = self.terminals.lock().await;
        terminals.insert(id, terminal);
        Ok(())
    }

    /// Unregister a terminal from the synchronizer
    pub async fn unregister_terminal(&self, terminal_id: &str) -> Result<()> {
        info!("Unregistering terminal: {}", terminal_id);

        let mut terminals = self.terminals.lock().await;
        terminals.remove(terminal_id);

        Ok(())
    }

    /// Broadcast output to all terminals
    pub async fn broadcast(&self, output: &str) -> Result<()> {
        let terminals = self.terminals.lock().await;
        // Check if we have any terminals
        if terminals.is_empty() {
            return Err(Error::NoTerminals);
        }

        // Send to all terminals
        for terminal in terminals.values() {
            if let Err(e) = terminal_helpers::display(terminal.as_terminal(), output).await {
                error!("Failed to broadcast to terminal: {}", e);
            }
        }

        Ok(())
    }

    /// Echo input to all terminals
    pub async fn echo_input(&self, input: &str) -> Result<()> {
        let terminals = self.terminals.lock().await;
        // Check if we have any terminals
        if terminals.is_empty() {
            return Err(Error::NoTerminals);
        }

        // Send to all terminals
        for terminal in terminals.values() {
            if let Err(e) = terminal_helpers::echo_input(terminal.as_terminal(), input).await {
                error!("Failed to echo input to terminal: {}", e);
            }
        }

        Ok(())
    }

    /// Execute a command on a terminal and return the response
    pub async fn execute_command(&self, command: &str) -> Result<Option<String>> {
        // Check if we have any terminals
        let terminals = self.terminals.lock().await;
        if terminals.is_empty() {
            return Err(Error::NoTerminals);
        }

        // Execute on the first terminal
        if let Some(terminal) = terminals.values().next() {
            let (tx, rx) = oneshot::channel();

            terminal_helpers::execute_command(terminal.as_terminal(), command, tx).await?;

            match rx.await {
                Ok(output) => Ok(Some(output)),
                Err(_) => Ok(None),
            }
        } else {
            Err(Error::NoTerminals)
        }
    }

    /// Broadcast input from one terminal to all other terminals
    pub async fn broadcast_input(&self, input: &str, source_id: &str) -> Result<()> {
        debug!("Broadcasting input from {}", source_id);

        // Send input to the input queue
        let sender = self.input_sender.lock().await;
        sender
            .send(input.to_string())
            .await
            .map_err(|e| Error::TerminalError(format!("Failed to send input to queue: {}", e)))?;

        // Echo input to all terminals except source
        let terminals = self.terminals.lock().await;

        // Clone the terminals we need to avoid lifetime issues
        let terminal_refs: Vec<(String, Arc<dyn Terminal>)> = terminals
            .iter()
            .filter(|(id, _)| id != &source_id)
            .map(|(id, term)| (id.clone(), term.clone()))
            .collect();

        // Drop the lock before spawning tasks
        drop(terminals);

        // Now use the cloned references
        for (id, terminal) in terminal_refs {
            let input_clone = input.to_string();

            let result = terminal.echo_input_sync(&input_clone).await;
            if let Err(e) = result {
                error!("Failed to echo input to terminal {}: {}", id, e);
            }
        }

        Ok(())
    }

    /// Get the next input from any terminal
    pub async fn get_next_input(&self) -> Result<String> {
        debug!("Waiting for next input from any terminal");

        let mut receiver = self.input_queue.lock().await;
        receiver
            .recv()
            .await
            .ok_or_else(|| Error::TerminalError("Input queue closed".to_string()))
    }

    /// Send a command to a specific terminal
    pub async fn send_command(&self, terminal_id: &str, command: &str) -> Result<String> {
        debug!("Sending command to terminal: {}", terminal_id);

        let terminals = self.terminals.lock().await;

        if let Some(terminal) = terminals.get(terminal_id) {
            let terminal_clone = terminal.clone();
            drop(terminals);

            let (tx, rx) = oneshot::channel();

            terminal_helpers::execute_command(&*terminal_clone, command, tx).await?;

            rx.await.map_err(|e| {
                Error::TerminalError(format!("Failed to receive command result: {}", e))
            })
        } else {
            Err(Error::TerminalError(format!(
                "Terminal not found: {}",
                terminal_id
            )))
        }
    }
}

/// Mock terminal implementation for testing
pub struct MockTerminal {
    id: String,
    display_handler: Arc<Mutex<Option<Box<dyn Fn(&str) -> Result<()> + Send + Sync + 'static>>>>,
    echo_handler: Arc<Mutex<Option<Box<dyn Fn(&str) -> Result<()> + Send + Sync + 'static>>>>,
    command_handler: Arc<
        Mutex<
            Option<
                Box<dyn Fn(&str, oneshot::Sender<String>) -> Result<()> + Send + Sync + 'static>,
            >,
        >,
    >,
}

impl std::fmt::Debug for MockTerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MockTerminal")
            .field("id", &self.id)
            .field("display_handler", &"<function>".to_string())
            .field("echo_handler", &"<function>".to_string())
            .field("command_handler", &"<function>".to_string())
            .finish()
    }
}

impl MockTerminal {
    /// Create a new mock terminal with the given ID
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            display_handler: Arc::new(Mutex::new(None)),
            echo_handler: Arc::new(Mutex::new(None)),
            command_handler: Arc::new(Mutex::new(None)),
        }
    }

    /// Register a handler for display events
    pub fn with_display_handler<F>(self, handler: F) -> Self
    where
        F: Fn(&str) -> Result<()> + Send + Sync + 'static,
    {
        // Create a new instance with the handler already set
        Self {
            id: self.id,
            display_handler: Arc::new(Mutex::new(Some(Box::new(handler)))),
            echo_handler: self.echo_handler,
            command_handler: self.command_handler,
        }
    }

    /// Register a handler for echo input events
    pub fn with_echo_handler<F>(self, handler: F) -> Self
    where
        F: Fn(&str) -> Result<()> + Send + Sync + 'static,
    {
        // Create a new instance with the handler already set
        Self {
            id: self.id,
            display_handler: self.display_handler,
            echo_handler: Arc::new(Mutex::new(Some(Box::new(handler)))),
            command_handler: self.command_handler,
        }
    }

    /// Register a handler for command execution
    pub fn with_command_handler<F>(self, handler: F) -> Self
    where
        F: Fn(&str, oneshot::Sender<String>) -> Result<()> + Send + Sync + 'static,
    {
        // Create a new instance with the handler already set
        Self {
            id: self.id,
            display_handler: self.display_handler,
            echo_handler: self.echo_handler,
            command_handler: Arc::new(Mutex::new(Some(Box::new(handler)))),
        }
    }
}

impl Terminal for MockTerminal {
    fn id_sync(&self) -> Box<dyn std::future::Future<Output = Result<String>> + Send + Unpin + '_> {
        Box::new(Box::pin(async move { Ok(self.id.clone()) }))
    }

    fn start_sync(
        &mut self,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        Box::new(Box::pin(async move { Ok(()) }))
    }

    fn stop_sync(&self) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        Box::new(Box::pin(async move { Ok(()) }))
    }

    fn display_sync(
        &self,
        output: &str,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        let handler = self.display_handler.clone();
        let output = output.to_string();

        Box::new(Box::pin(async move {
            if let Some(handler) = handler.lock().await.as_ref() {
                handler(&output)
            } else {
                Ok(())
            }
        }))
    }

    fn echo_input_sync(
        &self,
        input: &str,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        let handler = self.echo_handler.clone();
        let input = input.to_string();

        Box::new(Box::pin(async move {
            if let Some(handler) = handler.lock().await.as_ref() {
                handler(&input)
            } else {
                Ok(())
            }
        }))
    }

    fn execute_command_sync(
        &self,
        command: &str,
        tx: oneshot::Sender<String>,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        let handler = self.command_handler.clone();
        let command = command.to_string();

        Box::new(Box::pin(async move {
            if let Some(handler) = handler.lock().await.as_ref() {
                handler(&command, tx)
            } else {
                tx.send("Mock command result".to_string()).unwrap();
                Ok(())
            }
        }))
    }

    fn as_terminal(&self) -> &dyn Terminal {
        self
    }

    fn write(&mut self, _s: &str) -> Result<()> {
        Ok(())
    }

    fn write_line(&mut self, _s: &str) -> Result<()> {
        Ok(())
    }

    fn read_line(&mut self) -> Result<String> {
        Ok("Mock input".to_string())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn read_password(&mut self, _prompt: &str) -> Result<String> {
        Ok("mock_password".to_string())
    }

    fn read_secret(&mut self, _prompt: &str) -> Result<String> {
        Ok("mock_secret".to_string())
    }

    fn terminal_address_sync(
        &self,
    ) -> Box<dyn std::future::Future<Output = Option<String>> + Send + Unpin + '_> {
        Box::new(Box::pin(async move { None }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::sync::oneshot;

    #[tokio::test]
    async fn test_register_terminal() {
        let sync = TerminalSynchronizer::new();
        let mock = Arc::new(MockTerminal::new("test"));

        let result = sync.register_terminal(mock.clone()).await;
        assert!(result.is_ok());

        let terminals = sync.terminals.lock().await;
        assert_eq!(terminals.len(), 1);
        assert!(terminals.contains_key("test"));
    }

    #[tokio::test]
    async fn test_unregister_terminal() {
        let sync = TerminalSynchronizer::new();
        let mock = Arc::new(MockTerminal::new("test"));

        let result = sync.register_terminal(mock.clone()).await;
        assert!(result.is_ok());

        let result = sync.unregister_terminal("test").await;
        assert!(result.is_ok());

        let terminals = sync.terminals.lock().await;
        assert_eq!(terminals.len(), 0);
    }

    #[tokio::test]
    async fn test_broadcast() {
        // Setup the test data
        let mock1 = Arc::new(MockTerminal::new("term1"));
        let mock2 = Arc::new(MockTerminal::new("term2"));
        let sync = TerminalSynchronizer::new();

        // Register terminals
        sync.register_terminal(mock1.clone()).await.unwrap();
        sync.register_terminal(mock2.clone()).await.unwrap();

        // Test broadcasting output
        let result = sync.broadcast("test output").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_broadcast_input() {
        let sync = TerminalSynchronizer::new();

        // Create two mock terminals
        let mock1 = Arc::new(MockTerminal::new("term1"));
        let mock2 = Arc::new(MockTerminal::new("term2"));

        // Create a channel to verify echo was received on term2
        let (tx, rx) = oneshot::channel();
        let tx = Arc::new(Mutex::new(Some(tx)));

        // Set up echo handler for term2
        let tx_clone = tx.clone();
        let echo_mock = Arc::new(MockTerminal::new("term2").with_echo_handler(move |input| {
            assert_eq!(input, "test input");
            if let Some(sender) = tx_clone.try_lock().unwrap().take() {
                let _ = sender.send(true);
            }
            Ok(())
        }));

        // Register both terminals
        sync.register_terminal(mock1.clone()).await.unwrap();
        sync.register_terminal(echo_mock.clone()).await.unwrap();

        // Broadcast input from term1
        let result = sync.broadcast_input("test input", "term1").await;
        assert!(result.is_ok());

        // Verify echo was received on term2
        let received = rx.await.unwrap();
        assert!(received);
    }

    #[tokio::test]
    async fn test_get_next_input() {
        let sync = TerminalSynchronizer::new();

        // Send input to the queue
        {
            let sender = sync.input_sender.lock().await;
            sender.send("test input".to_string()).await.unwrap();
        }

        // Get next inpu
        let input = sync.get_next_input().await;
        assert!(input.is_ok());
        assert_eq!(input.unwrap(), "test input");
    }

    #[tokio::test]
    async fn test_send_command() {
        let sync = TerminalSynchronizer::new();

        // Create mock terminal with custom command handler
        let mock = Arc::new(MockTerminal::new("test").with_command_handler(|cmd, tx| {
            assert_eq!(cmd, "ls");
            let _ = tx.send("file1 file2".to_string());
            Ok(())
        }));

        let result = sync.register_terminal(mock.clone()).await;
        assert!(result.is_ok());

        // Send command to terminal
        let result = sync.send_command("test", "ls").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "file1 file2");
    }

    #[tokio::test]
    async fn test_echo_input() {
        // Setup the test data
        let mock1 = Arc::new(MockTerminal::new("term1"));
        let sync = TerminalSynchronizer::new();

        // Register terminal
        sync.register_terminal(mock1.clone()).await.unwrap();

        // Test echoing input
        let result = sync.echo_input("test input").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_execute_command() {
        // Setup the test data
        let mock1 = Arc::new(MockTerminal::new("term1"));
        let sync = TerminalSynchronizer::new();

        // Register terminal
        sync.register_terminal(mock1.clone()).await.unwrap();

        // Test executing a command
        let result = sync.execute_command("test command").await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.is_some());
        assert_eq!(output.unwrap(), "Mock command result");
    }
}
