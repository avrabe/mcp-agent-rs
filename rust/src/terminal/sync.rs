//! Terminal Synchronizer
//!
//! This module manages I/O synchronization between multiple terminal interfaces

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

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
    pub async fn register_terminal<T>(&self, terminal: Arc<T>) -> Result<()>
    where
        T: Terminal + 'static + ?Sized,
    {
        let term_id = terminal.id().await?;
        info!("Registering terminal: {}", term_id);

        let mut terminals = self.terminals.lock().await;
        terminals.insert(term_id, terminal);

        Ok(())
    }

    /// Unregister a terminal from the synchronizer
    pub async fn unregister_terminal(&self, terminal_id: &str) -> Result<()> {
        info!("Unregistering terminal: {}", terminal_id);

        let mut terminals = self.terminals.lock().await;
        terminals.remove(terminal_id);

        Ok(())
    }

    /// Broadcast output to all registered terminals
    pub async fn broadcast_output(&self, output: &str) -> Result<()> {
        debug!("Broadcasting output to all terminals");

        let terminals = self.terminals.lock().await;

        // Clone the terminals we need to avoid lifetime issues
        let terminal_refs: Vec<(String, Arc<dyn Terminal>)> = terminals
            .iter()
            .map(|(id, term)| (id.clone(), term.clone()))
            .collect();

        // Drop the lock before spawning tasks
        drop(terminals);

        // Now use the cloned references
        for (id, terminal) in terminal_refs {
            let output_clone = output.to_string();

            let result = terminal.display(&output_clone).await;
            if let Err(e) = result {
                error!("Failed to send output to terminal {}: {}", id, e);
            }
        }

        Ok(())
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

            let result = terminal.echo_input(&input_clone).await;
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

            terminal_clone.execute_command(command, tx).await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use tokio::sync::oneshot;

    /// Mock terminal for testing
    struct MockTerminal {
        id: String,
        display_handler: Mutex<Box<dyn Fn(String) -> Result<()> + Send + Sync>>,
        echo_handler: Mutex<Box<dyn Fn(String) -> Result<()> + Send + Sync>>,
        command_handler:
            Mutex<Box<dyn Fn(String, oneshot::Sender<String>) -> Result<()> + Send + Sync>>,
    }

    impl MockTerminal {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
                display_handler: Mutex::new(Box::new(|_| Ok(()))),
                echo_handler: Mutex::new(Box::new(|_| Ok(()))),
                command_handler: Mutex::new(Box::new(|_, tx| {
                    let _ = tx.send("OK".to_string());
                    Ok(())
                })),
            }
        }

        fn with_display_handler<F>(mut self, handler: F) -> Self
        where
            F: Fn(String) -> Result<()> + Send + Sync + 'static,
        {
            self.display_handler = Mutex::new(Box::new(handler));
            self
        }

        fn with_echo_handler<F>(mut self, handler: F) -> Self
        where
            F: Fn(String) -> Result<()> + Send + Sync + 'static,
        {
            self.echo_handler = Mutex::new(Box::new(handler));
            self
        }

        fn with_command_handler<F>(mut self, handler: F) -> Self
        where
            F: Fn(String, oneshot::Sender<String>) -> Result<()> + Send + Sync + 'static,
        {
            self.command_handler = Mutex::new(Box::new(handler));
            self
        }
    }

    #[async_trait]
    impl Terminal for MockTerminal {
        async fn id(&self) -> Result<String> {
            Ok(self.id.clone())
        }

        async fn start(&mut self) -> Result<()> {
            Ok(())
        }

        async fn stop(&self) -> Result<()> {
            Ok(())
        }

        async fn display(&self, output: &str) -> Result<()> {
            let handler = self.display_handler.lock().await;
            handler(output.to_string())
        }

        async fn echo_input(&self, input: &str) -> Result<()> {
            let handler = self.echo_handler.lock().await;
            handler(input.to_string())
        }

        async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<()> {
            let handler = self.command_handler.lock().await;
            handler(command.to_string(), tx)
        }
    }

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
    async fn test_broadcast_output() {
        let sync = TerminalSynchronizer::new();

        // Create a channel to verify output was received
        let (tx, rx) = oneshot::channel();
        let tx = Arc::new(Mutex::new(Some(tx)));

        // Create mock terminal with custom display handler
        let tx_clone = tx.clone();
        let mock = Arc::new(
            MockTerminal::new("test").with_display_handler(move |output| {
                assert_eq!(output, "test output");
                if let Some(sender) = tx_clone.try_lock().unwrap().take() {
                    let _ = sender.send(true);
                }
                Ok(())
            }),
        );

        let result = sync.register_terminal(mock.clone()).await;
        assert!(result.is_ok());

        let result = sync.broadcast_output("test output").await;
        assert!(result.is_ok());

        // Verify output was received
        let received = rx.await.unwrap();
        assert!(received);
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
}
