//! Console Terminal
//!
//! Provides a terminal interface via the console/stdio

use std::fmt;
use std::io::{self, Read, Write};
use std::sync::Arc;
use std::thread;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error, trace};

use super::Terminal;
use crate::error::Error;

type InputCallback = Box<dyn Fn(String) -> Result<(), Error> + Send + Sync>;

/// Console terminal implementation
pub struct ConsoleTerminal {
    /// Terminal ID
    id: String,
    /// Active flag - true if the terminal is running
    active: bool,
    /// Input callback function
    input_callback: Option<Box<dyn Fn(String) -> Result<(), Error> + Send + Sync>>,
    /// Shutdown channel
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl fmt::Debug for ConsoleTerminal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConsoleTerminal")
            .field("id", &self.id)
            .field("active", &self.active)
            .field("input_callback", &self.input_callback.is_some())
            .field("shutdown_tx", &self.shutdown_tx.is_some())
            .finish()
    }
}

impl Clone for ConsoleTerminal {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            active: self.active,
            input_callback: None, // Can't clone the callback
            shutdown_tx: None,    // Don't clone the shutdown channel
        }
    }
}

impl ConsoleTerminal {
    /// Create a new console terminal
    pub fn new() -> Self {
        Self {
            id: "console".to_string(),
            input_callback: None,
            active: false,
            shutdown_tx: None,
        }
    }
    
    /// Set the input callback function
    pub fn with_input_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(String) -> Result<(), Error> + Send + Sync + 'static,
    {
        self.input_callback = Some(Box::new(callback));
        self
    }
    
    /// Read input from stdin in a separate thread
    fn start_input_thread(&mut self) {
        let (_, mut shutdown_rx) = mpsc::channel::<()>(1);
        let callback = self.input_callback.take();
        
        thread::spawn(move || {
            let mut buffer = [0; 1024];
            let stdin = io::stdin();
            let mut handle = stdin.lock();
            
            debug!("Console input thread started");
            
            'outer: loop {
                // Check if we should shutdown
                if shutdown_rx.try_recv().is_ok() {
                    debug!("Console input thread received shutdown signal");
                    break;
                }
                
                // Try to read from stdin with a timeout
                let mut input = String::new();
                
                match handle.read(&mut buffer) {
                    Ok(0) => {
                        // End of file
                        debug!("Console received EOF");
                        break;
                    }
                    Ok(n) => {
                        // Convert buffer to string and handle the input
                        let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                        trace!(bytes = n, "Console read input");
                        
                        // Process each line (in case we got multiple lines)
                        for line in data.lines() {
                            input.push_str(line);
                            
                            // Process the input if we have a callback
                            if let Some(ref callback) = callback {
                                if let Err(e) = callback(input.clone()) {
                                    error!("Console input callback error: {}", e);
                                    break 'outer;
                                }
                            }
                            
                            input.clear();
                        }
                    }
                    Err(e) => {
                        error!("Error reading from console: {}", e);
                        break;
                    }
                }
                
                // Small sleep to avoid 100% CPU usage
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            
            debug!("Console input thread stopped");
        });
    }
}

#[async_trait]
impl Terminal for ConsoleTerminal {
    async fn id(&self) -> Result<String, Error> {
        Ok(self.id.clone())
    }
    
    async fn start(&mut self) -> Result<(), Error> {
        if self.active {
            return Ok(());
        }
        
        debug!("Starting console terminal");
        
        // Start the input thread and store the shutdown channel
        let (tx, _) = oneshot::channel();
        self.shutdown_tx = Some(tx);
        self.start_input_thread();
        
        self.active = true;
        
        Ok(())
    }
    
    async fn stop(&mut self) -> Result<(), Error> {
        if !self.active {
            return Ok(());
        }
        
        debug!("Stopping console terminal");
        
        // Send shutdown signal to input thread
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        
        self.active = false;
        Ok(())
    }
    
    async fn display(&self, output: &str) -> Result<(), Error> {
        trace!(bytes = output.len(), "Console displaying output");
        
        let mut stdout = io::stdout().lock();
        match stdout.write_all(output.as_bytes()) {
            Ok(_) => {
                let _ = stdout.flush();
                Ok(())
            }
            Err(e) => Err(Error::TerminalError(format!("Failed to write to console: {}", e))),
        }
    }
    
    async fn echo_input(&self, input: &str) -> Result<(), Error> {
        trace!(bytes = input.len(), "Console echoing input");
        
        // Format the input as a user entered command
        let formatted = format!("> {}\n", input);
        self.display(&formatted).await
    }
    
    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<(), Error> {
        debug!("Executing command on console: {}", command);
        
        // Execute the command using the OS shell
        match std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let result = if stderr.is_empty() { stdout } else { format!("{}\n{}", stdout, stderr) };
                
                let _ = tx.send(result);
                Ok(())
            }
            Err(e) => {
                let _ = tx.send(format!("Error executing command: {}", e));
                Err(Error::TerminalError(format!("Failed to execute command: {}", e)))
            }
        }
    }
}

impl Default for ConsoleTerminal {
    fn default() -> Self {
        Self::new()
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
        assert_eq!(id, "console");
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
        
        // Check the command output
        let output = rx.await.unwrap();
        assert!(output.contains("test command"));
    }
    
    #[tokio::test]
    async fn test_console_terminal_with_callback() {
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();
        
        let terminal = ConsoleTerminal::new().with_input_callback(move |input| {
            assert_eq!(input, "test input");
            called_clone.store(true, Ordering::SeqCst);
            Ok(())
        });
        
        // Manually call the callback (can't easily simulate stdin in tests)
        if let Some(ref callback) = terminal.input_callback {
            let result = callback("test input".to_string());
            assert!(result.is_ok());
        }
        
        assert!(called.load(Ordering::SeqCst));
    }
} 