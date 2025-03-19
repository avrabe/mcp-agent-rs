use mcp_agent::{Message, MessageType, Priority};
use mcp_agent::mcp::types::{MessageId, MessageEnvelope};
use validator::Validate;

#[test]
fn test_message_creation() {
    let message = Message::new(
        MessageId::new("test-id".to_string()),
        MessageType::Request,
        Priority::Normal,
        b"test payload".to_vec(),
        None,
        None,
    );

    assert_eq!(message.id.as_str(), "test-id");
    assert_eq!(message.message_type, MessageType::Request);
    assert_eq!(message.priority, Priority::Normal);
    assert_eq!(message.payload, b"test payload");
    assert!(message.correlation_id.is_none());
    assert!(message.metadata.is_none());
}

#[test]
fn test_message_serialization() {
    let id = MessageId::new("test-id".to_string());
    let message = Message::new(
        id.clone(),
        MessageType::Request,
        Priority::Normal,
        b"test payload".to_vec(),
        None,
        None,
    );

    let serialized = serde_json::to_string(&message).unwrap();
    let deserialized: Message = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.id, message.id);
    assert_eq!(deserialized.message_type, message.message_type);
    assert_eq!(deserialized.priority, message.priority);
    assert_eq!(deserialized.payload, message.payload);
}

#[test]
fn test_message_envelope() {
    let message = Message::new(
        MessageId::new("test-id".to_string()),
        MessageType::Request,
        Priority::Normal,
        b"test payload".to_vec(),
        None,
        None,
    );

    let envelope = MessageEnvelope::from_message(&message);
    assert_eq!(envelope.id.as_str(), "test-id");
    assert_eq!(envelope.payload, b"test payload");
    assert_eq!(envelope.header.message_type, MessageType::Request as u8);
    assert_eq!(envelope.header.priority, Priority::Normal as u8);
    assert_eq!(envelope.header.payload_len, message.payload.len() as u32);
}

#[test]
fn test_message_envelope_roundtrip() {
    let message = Message::new(
        MessageId::new("test-id".to_string()),
        MessageType::Request,
        Priority::Normal,
        b"test payload".to_vec(),
        None,
        None,
    );

    let envelope = MessageEnvelope::from_message(&message);
    let reconstructed = envelope.to_message().unwrap();

    assert_eq!(reconstructed.id.as_str(), message.id.as_str());
    assert_eq!(reconstructed.message_type, message.message_type);
    assert_eq!(reconstructed.priority, message.priority);
    assert_eq!(reconstructed.payload, message.payload);
}

#[test]
fn test_message_validation() {
    let id = MessageId::new("test-id".to_string());
    let message = Message::new(
        id.clone(),
        MessageType::Request,
        Priority::Normal,
        b"test payload".to_vec(),
        None,
        None,
    );

    assert!(message.validate().is_ok());
}

#[test]
fn test_message_priority_ordering() {
    assert!(Priority::Critical as u8 > Priority::High as u8);
    assert!(Priority::High as u8 > Priority::Normal as u8);
    assert!(Priority::Normal as u8 > Priority::Low as u8);
}

#[test]
fn test_message_type_conversion() {
    assert_eq!(MessageType::Request as u8, 0);
    assert_eq!(MessageType::Response as u8, 1);
    assert_eq!(MessageType::Event as u8, 2);
    assert_eq!(MessageType::Error as u8, 3);
} 