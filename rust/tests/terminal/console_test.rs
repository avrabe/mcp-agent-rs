//! Console terminal tests
//!
//! Tests the console terminal functionality

use std::time::Duration;
use tokio::time::sleep;
use tokio::sync::{mpsc, oneshot};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};

use mcp_agent::error::Error;
use mcp_agent::terminal::{config::TerminalConfig, TerminalSystem};

/// Test the console terminal internal functionality
#[tokio::test]
async fn test_console_terminal_internal() -> Result<(), Error> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    // Create a console-only configuration for testing
    let config = TerminalConfig::console_only();
    
    // Create the terminal system
    let mut terminal = TerminalSystem::new(config)?;
    
    // Start the terminal system
    terminal.start().await?;
    
    // Test writing to the terminal
    terminal.write("Test message to console\n").await?;
    
    // Clean up
    terminal.stop().await?;
    
    Ok(())
}

/// Mock IO process for testing console terminal
struct MockProcess {
    child: Child,
    stdin_tx: mpsc::Sender<String>,
    stdout_rx: mpsc::Receiver<String>,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl MockProcess {
    /// Create a new mock process
    async fn new() -> Result<Self, Error> {
        // Create channels
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(100);
        let (stdout_tx, stdout_rx) = mpsc::channel::<String>(100);
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        
        // Start a dummy process for stdin/stdout redirection
        // This is a simple echo process that just reads stdin and writes to stdout
        let mut child = Command::new("bash")
            .args(["-c", "cat"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| Error::Internal(format!("Failed to spawn process: {}", e)))?;
        
        // Get stdin/stdout handles
        let mut stdin = child.stdin.take()
            .ok_or_else(|| Error::Internal("Failed to get stdin handle".to_string()))?;
        let mut stdout = child.stdout.take()
            .ok_or_else(|| Error::Internal("Failed to get stdout handle".to_string()))?;
        
        // Spawn task to handle stdin
        tokio::spawn(async move {
            let mut buffer = Vec::new();
            while let Some(data) = stdin_rx.recv().await {
                buffer.clear();
                buffer.extend_from_slice(data.as_bytes());
                if let Err(e) = stdin.write_all(&buffer).await {
                    eprintln!("Error writing to stdin: {}", e);
                    break;
                }
                if let Err(e) = stdin.flush().await {
                    eprintln!("Error flushing stdin: {}", e);
                    break;
                }
            }
        });
        
        // Spawn task to handle stdout
        tokio::spawn(async move {
            let mut buffer = [0u8; 1024];
            loop {
                tokio::select! {
                    n = stdout.read(&mut buffer) => {
                        match n {
                            Ok(0) => break, // EOF
                            Ok(n) => {
                                if let Ok(s) = String::from_utf8(buffer[..n].to_vec()) {
                                    if let Err(e) = stdout_tx.send(s).await {
                                        eprintln!("Error sending stdout: {}", e);
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Error reading stdout: {}", e);
                                break;
                            }
                        }
                    }
                    _ = &mut shutdown_rx => {
                        break;
                    }
                }
            }
        });
        
        Ok(Self {
            child,
            stdin_tx,
            stdout_rx,
            shutdown_tx: Some(shutdown_tx),
        })
    }
    
    /// Send input to the process
    async fn send_input(&self, input: &str) -> Result<(), Error> {
        self.stdin_tx.send(input.to_string()).await
            .map_err(|e| Error::Internal(format!("Failed to send input: {}", e)))
    }
    
    /// Receive output from the process with timeout
    async fn receive_output(&mut self, timeout: Duration) -> Result<Option<String>, Error> {
        match tokio::time::timeout(timeout, self.stdout_rx.recv()).await {
            Ok(Some(output)) => Ok(Some(output)),
            Ok(None) => Ok(None),
            Err(_) => Ok(None), // Timeout
        }
    }
    
    /// Shutdown the process
    async fn shutdown(mut self) -> Result<(), Error> {
        // Send shutdown signal
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        
        // Kill the process
        let _ = self.child.kill().await;
        
        Ok(())
    }
}

/// Test external process interaction (simulated)
#[tokio::test]
#[ignore] // Ignore by default as it involves process creation
async fn test_external_process() -> Result<(), Error> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();
    
    // Create a mock process
    let mut process = MockProcess::new().await?;
    
    // Send some input
    process.send_input("Hello, world!\n").await?;
    
    // Wait for output
    let output = process.receive_output(Duration::from_secs(1)).await?;
    
    // Check if we received the echoed input
    if let Some(output) = output {
        println!("Received output: {}", output);
        assert_eq!(output, "Hello, world!\n");
    } else {
        println!("No output received");
    }
    
    // Clean up
    process.shutdown().await?;
    
    Ok(())
} 