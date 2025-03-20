use std::error::Error;
use std::net::SocketAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use serde_json::json;

/// Simple mock MCP server for testing the terminal interface
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Configure logging
    env_logger::init();

    // Start TCP server on localhost:7000
    let addr = "127.0.0.1:7000";
    let listener = TcpListener::bind(addr).await?;
    println!("Mock server listening on {}", addr);

    // Handle incoming connections
    loop {
        let (socket, addr) = listener.accept().await?;
        println!("New connection from {}", addr);
        
        // Handle each connection in a separate task
        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, addr).await {
                eprintln!("Error handling connection: {}", e);
            }
        });
    }
}

/// Handle a client connection
async fn handle_connection(mut socket: TcpStream, addr: SocketAddr) -> Result<(), Box<dyn Error + Send + Sync>> {
    println!("Client connected: {}", addr);
    let mut buffer = [0u8; 1024];

    // Simple echo server with some special handling
    loop {
        let n = match socket.read(&mut buffer).await {
            Ok(0) => {
                println!("Client disconnected: {}", addr);
                return Ok(());
            }
            Ok(n) => n,
            Err(e) => {
                println!("Error reading from socket: {}", e);
                return Err(e.into());
            }
        };

        let message = String::from_utf8_lossy(&buffer[..n]);
        println!("Received from {}: {}", addr, message);

        // Echo the message back with some processing
        let response = handle_message(&message);
        println!("Sending to {}: {}", addr, response);
        
        socket.write_all(response.as_bytes()).await?;
    }
}

/// Process a message and generate a response
fn handle_message(message: &str) -> String {
    // For testing purposes, we'll have some predefined responses
    if message.contains("echo") {
        return json!({
            "type": "response",
            "data": message,
            "status": "ok"
        }).to_string();
    } else if message.contains("ping") {
        return json!({
            "type": "response",
            "data": "pong",
            "status": "ok"
        }).to_string();
    } else if message.contains("json") {
        return json!({
            "type": "response",
            "status": "ok",
            "data": {
                "value": 42,
                "name": "mock-server",
                "timestamp": chrono::Local::now().to_rfc3339(),
            }
        }).to_string();
    } else if message.contains("error") {
        return json!({
            "type": "response",
            "status": "error",
            "error": "This is a test error message",
            "code": 400
        }).to_string();
    } else {
        return json!({
            "type": "response",
            "data": format!("Received: {}", message),
            "status": "ok",
            "timestamp": chrono::Local::now().to_rfc3339()
        }).to_string();
    }
} 