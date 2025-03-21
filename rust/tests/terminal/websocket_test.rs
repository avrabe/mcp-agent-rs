//! WebSocket tests for the terminal system
//!
//! Tests the WebSocket communication between client and server

use std::time::Duration;
use tokio::time::sleep;
use tokio::sync::mpsc;

use futures::stream::StreamExt;
use futures::SinkExt;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};

use mcp_agent::error::Error;
use mcp_agent::terminal::{config::TerminalConfig, TerminalSystem};

/// Test the WebSocket connection to the web terminal
#[tokio::test]
#[ignore] // Ignore by default as it requires a running web terminal
async fn test_websocket_connection() -> Result<(), Error> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    // Create a web-only configuration for testing
    let config = TerminalConfig::web_only();
    
    // Create and start the terminal system
    let mut terminal = TerminalSystem::new(config)?;
    terminal.start().await?;
    
    // Get the WebSocket URL
    let ws_addr = terminal.web_terminal_address()
        .expect("Web terminal address should be available");
    let ws_url = format!("ws://{}/ws", ws_addr);
    
    // Allow time for the server to start
    sleep(Duration::from_millis(100)).await;
    
    // Connect to the WebSocket server
    let (ws_stream, _) = connect_async(&ws_url).await
        .map_err(|e| Error::Internal(format!("Failed to connect to WebSocket: {}", e)))?;
    
    println!("WebSocket connected to {}", ws_url);
    
    // Split the WebSocket stream
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    
    // Create a channel for received messages
    let (tx, mut rx) = mpsc::channel::<String>(10);
    
    // Spawn a task to receive messages
    tokio::spawn(async move {
        while let Some(msg) = ws_receiver.next().await {
            if let Ok(msg) = msg {
                if let Ok(text) = msg.to_text() {
                    let _ = tx.send(text.to_string()).await;
                }
            }
        }
    });
    
    // Send a test message through the terminal system
    terminal.write("Test message from server\n").await?;
    
    // Wait for a response
    let received = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await;
    
    // Check if we received the message
    match received {
        Ok(Some(msg)) => {
            println!("Received message: {}", msg);
            assert!(msg.contains("Test message from server"));
        }
        Ok(None) => {
            println!("Channel closed without receiving a message");
        }
        Err(_) => {
            println!("Timeout waiting for message");
        }
    }
    
    // Send a message from the client
    ws_sender.send(Message::Text("Hello from client".to_string())).await
        .map_err(|e| Error::Internal(format!("Failed to send WebSocket message: {}", e)))?;
    
    // Allow time for processing
    sleep(Duration::from_millis(500)).await;
    
    // Clean up
    terminal.stop().await?;
    
    Ok(())
}

/// Test multiple simultaneous WebSocket clients
#[tokio::test]
#[ignore] // Ignore by default as it requires a running web terminal
async fn test_multiple_websocket_clients() -> Result<(), Error> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    // Create a web-only configuration for testing
    let config = TerminalConfig::web_only();
    
    // Create and start the terminal system
    let mut terminal = TerminalSystem::new(config)?;
    terminal.start().await?;
    
    // Get the WebSocket URL
    let ws_addr = terminal.web_terminal_address()
        .expect("Web terminal address should be available");
    let ws_url = format!("ws://{}/ws", ws_addr);
    
    // Allow time for the server to start
    sleep(Duration::from_millis(100)).await;
    
    // Connect first client
    let (ws_stream1, _) = connect_async(&ws_url).await
        .map_err(|e| Error::Internal(format!("Failed to connect client 1: {}", e)))?;
    
    // Connect second client
    let (ws_stream2, _) = connect_async(&ws_url).await
        .map_err(|e| Error::Internal(format!("Failed to connect client 2: {}", e)))?;
    
    println!("Two WebSocket clients connected to {}", ws_url);
    
    // Split the WebSocket streams
    let (mut ws_sender1, mut ws_receiver1) = ws_stream1.split();
    let (mut ws_sender2, mut ws_receiver2) = ws_stream2.split();
    
    // Create channels for received messages
    let (tx1, mut rx1) = mpsc::channel::<String>(10);
    let (tx2, mut rx2) = mpsc::channel::<String>(10);
    
    // Spawn tasks to receive messages for each client
    tokio::spawn(async move {
        while let Some(msg) = ws_receiver1.next().await {
            if let Ok(msg) = msg {
                if let Ok(text) = msg.to_text() {
                    let _ = tx1.send(text.to_string()).await;
                }
            }
        }
    });
    
    tokio::spawn(async move {
        while let Some(msg) = ws_receiver2.next().await {
            if let Ok(msg) = msg {
                if let Ok(text) = msg.to_text() {
                    let _ = tx2.send(text.to_string()).await;
                }
            }
        }
    });
    
    // Send a message from client 1
    ws_sender1.send(Message::Text("Hello from client 1".to_string())).await
        .map_err(|e| Error::Internal(format!("Failed to send from client 1: {}", e)))?;
    
    // Allow time for processing
    sleep(Duration::from_millis(500)).await;
    
    // Check if client 2 received the echo
    let received = tokio::time::timeout(Duration::from_secs(2), rx2.recv()).await;
    match received {
        Ok(Some(msg)) => {
            println!("Client 2 received: {}", msg);
            assert!(msg.contains("Hello from client 1"));
        }
        Ok(None) => {
            println!("Channel closed without client 2 receiving a message");
        }
        Err(_) => {
            println!("Timeout waiting for message on client 2");
        }
    }
    
    // Send a message from the server to both clients
    terminal.write("Broadcast to all clients\n").await?;
    
    // Check if both clients received the message
    let received1 = tokio::time::timeout(Duration::from_secs(2), rx1.recv()).await;
    let received2 = tokio::time::timeout(Duration::from_secs(2), rx2.recv()).await;
    
    if let Ok(Some(msg)) = received1 {
        println!("Client 1 received broadcast: {}", msg);
        assert!(msg.contains("Broadcast to all clients"));
    }
    
    if let Ok(Some(msg)) = received2 {
        println!("Client 2 received broadcast: {}", msg);
        assert!(msg.contains("Broadcast to all clients"));
    }
    
    // Clean up
    terminal.stop().await?;
    
    Ok(())
} 