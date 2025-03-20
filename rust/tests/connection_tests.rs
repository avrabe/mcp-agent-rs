use mcp_agent::mcp::{
    connection::{Connection, ConnectionState, StreamType},
    types::{Message, MessageType, Priority},
};
use mcp_agent::utils::error::{McpError, McpResult};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{self, duplex, AsyncReadExt, AsyncWriteExt};
use tokio::sync::Mutex;
use tokio::time::timeout;

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn test_stream_type_implementation() -> McpResult<()> {
    // Create a mock pair of streams for testing
    let (client, server) = duplex(1024);
    
    // Create a test structure to verify the StreamType implementation
    let tcp_stream = tokio::net::TcpStream::connect("127.0.0.1:80").await
        .or_else(|_| Err(McpError::ConnectionFailed("Test TCP stream failed".to_string())))?;
    
    let tcp_type = StreamType::Tcp(Arc::new(Mutex::new(tcp_stream)));
    assert!(matches!(tcp_type, StreamType::Tcp(_)));
    
    // Test cloning the StreamType
    let tcp_type_clone = tcp_type.clone();
    assert!(matches!(tcp_type_clone, StreamType::Tcp(_)));
    
    // Test debug formatting
    let debug_str = format!("{:?}", tcp_type);
    assert_eq!(debug_str, "StreamType::Tcp");
    
    Ok(())
}

#[tokio::test]
async fn test_message_serialization() -> McpResult<()> {
    // Create a test message
    let test_message = Message::new(
        MessageType::Request,
        Priority::Normal,
        "test payload".as_bytes().to_vec(),
        None,
        None,
    );
    
    // Create a clone to verify equality later
    let test_message_clone = test_message.clone();
    
    // Create a mock protocol instance
    let protocol = mcp_agent::mcp::protocol::McpProtocol::new();
    
    // Create a pair of streams for testing
    let (mut a, mut b) = duplex(1024);
    
    // Write the message to one end
    protocol.write_message_async(&mut a, &test_message).await?;
    
    // Read from the other end
    let received_message = protocol.read_message_async(&mut b).await?;
    
    // Verify the received message matches what we sent
    assert_eq!(received_message.message_type, test_message_clone.message_type);
    assert_eq!(received_message.payload, test_message_clone.payload);
    
    Ok(())
} 