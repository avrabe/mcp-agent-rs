use mcp_agent::mcp::{McpProtocol, Message, MessageId, MessageType, Priority};
use std::io::Cursor;

#[tokio::test]
async fn test_protocol_serialization() {
    let mut protocol = McpProtocol::new();
    let message = Message::new(
        MessageId::new("test-id".to_string()),
        MessageType::Request,
        Priority::Normal,
        b"test payload".to_vec(),
        None,
        None,
    );

    let mut buffer = Vec::new();
    protocol.write_message_async(&mut buffer, &message).await.unwrap();

    let mut reader = Cursor::new(&buffer);
    let reconstructed = protocol.read_message_async(&mut reader).await.unwrap();

    assert_eq!(reconstructed.message_type, message.message_type);
    assert_eq!(reconstructed.priority, message.priority);
    assert_eq!(reconstructed.payload, message.payload);
}

#[tokio::test]
async fn test_protocol_large_message() {
    let mut protocol = McpProtocol::new();
    let large_payload = vec![0u8; 1024 * 1024]; // 1MB payload
    let message = Message::new(
        MessageId::new("large-test-id".to_string()),
        MessageType::Request,
        Priority::Normal,
        large_payload,
        None,
        None,
    );

    let mut buffer = Vec::new();
    protocol.write_message_async(&mut buffer, &message).await.unwrap();

    let mut reader = Cursor::new(&buffer);
    let reconstructed = protocol.read_message_async(&mut reader).await.unwrap();

    assert_eq!(reconstructed.payload.len(), 1024 * 1024);
}

#[tokio::test]
async fn test_protocol_invalid_message() {
    let mut protocol = McpProtocol::new();
    let invalid_data = vec![0u8; 100]; // Invalid message data

    let mut reader = Cursor::new(&invalid_data);
    let result = protocol.read_message_async(&mut reader).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_protocol_multiple_messages() {
    let mut protocol = McpProtocol::new();
    let messages = vec![
        Message::new(
            MessageId::new("msg1".to_string()),
            MessageType::Request,
            Priority::Normal,
            b"first message".to_vec(),
            None,
            None,
        ),
        Message::new(
            MessageId::new("msg2".to_string()),
            MessageType::Response,
            Priority::High,
            b"second message".to_vec(),
            None,
            None,
        ),
    ];

    let mut buffer = Vec::new();
    for message in &messages {
        protocol.write_message_async(&mut buffer, message).await.unwrap();
    }

    let mut reader = Cursor::new(&buffer);
    for expected in messages {
        let reconstructed = protocol.read_message_async(&mut reader).await.unwrap();
        assert_eq!(reconstructed.message_type, expected.message_type);
        assert_eq!(reconstructed.priority, expected.priority);
        assert_eq!(reconstructed.payload, expected.payload);
    }
} 