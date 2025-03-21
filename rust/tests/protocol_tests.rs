use mcp_agent::mcp::protocol::McpProtocol;
use mcp_agent::mcp::types::{Message, MessageId, Priority};
use std::io::Cursor;

#[tokio::test]
async fn test_protocol_serialization() {
    let protocol = McpProtocol::new();
    let message = Message::request(b"test payload".to_vec(), Priority::Normal);

    let mut buffer = Vec::new();
    protocol
        .write_message_async(&mut buffer, &message)
        .await
        .unwrap();

    let mut reader = Cursor::new(&buffer);
    let reconstructed = protocol.read_message_async(&mut reader).await.unwrap();

    assert_eq!(reconstructed.message_type, message.message_type);
    assert_eq!(reconstructed.priority, message.priority);
    assert_eq!(reconstructed.payload, message.payload);
}

#[tokio::test]
async fn test_protocol_large_message() {
    let protocol = McpProtocol::new();
    let large_payload = vec![0u8; 1024 * 1024]; // 1MB payload
    let message = Message::request(large_payload, Priority::Normal);

    let mut buffer = Vec::new();
    protocol
        .write_message_async(&mut buffer, &message)
        .await
        .unwrap();

    let mut reader = Cursor::new(&buffer);
    let reconstructed = protocol.read_message_async(&mut reader).await.unwrap();

    assert_eq!(reconstructed.payload.len(), 1024 * 1024);
}

#[tokio::test]
async fn test_protocol_invalid_message() {
    let protocol = McpProtocol::new();
    let invalid_data = vec![0xFF, 0xFF, 0xFF]; // Invalid message type value

    let mut reader = Cursor::new(&invalid_data);
    let result = protocol.read_message_async(&mut reader).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_protocol_multiple_messages() {
    let protocol = McpProtocol::new();
    let messages = vec![
        Message::request(b"first message".to_vec(), Priority::Normal),
        Message::response(b"second message".to_vec(), MessageId::new()),
    ];

    let mut buffer = Vec::new();
    for message in &messages {
        protocol
            .write_message_async(&mut buffer, message)
            .await
            .unwrap();
    }

    let mut reader = Cursor::new(&buffer);
    for expected in messages {
        let reconstructed = protocol.read_message_async(&mut reader).await.unwrap();
        assert_eq!(reconstructed.message_type, expected.message_type);
        assert_eq!(reconstructed.priority, expected.priority);
        assert_eq!(reconstructed.payload, expected.payload);
    }
}
