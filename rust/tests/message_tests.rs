use mcp_agent::mcp::{Message, MessageType, Priority};

#[test]
fn test_message_creation() {
    let message = Message::request(b"test payload".to_vec(), Priority::Normal);

    assert_eq!(message.message_type, MessageType::Request);
    assert_eq!(message.priority, Priority::Normal);
    assert_eq!(message.payload, b"test payload");
    assert!(message.correlation_id.is_none());
    assert!(message.error.is_none());
}

#[test]
fn test_message_serialization() {
    let message = Message::request(b"test payload".to_vec(), Priority::Normal);

    let serialized = serde_json::to_string(&message).unwrap();
    let deserialized: Message = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.id.to_string(), message.id.to_string());
    assert_eq!(deserialized.message_type, message.message_type);
    assert_eq!(deserialized.priority, message.priority);
    assert_eq!(deserialized.payload, message.payload);
}

#[test]
fn test_message_envelope() {
    let message = Message::request(b"test payload".to_vec(), Priority::Normal);

    let envelope = message.to_envelope().unwrap();
    assert_eq!(envelope.header.id, message.id.to_string());
    assert_eq!(envelope.payload, b"test payload");
    assert_eq!(envelope.header.message_type, MessageType::Request);
    assert_eq!(envelope.header.priority, Priority::Normal);
    assert_eq!(envelope.header.payload_size, message.payload.len() as u32);
}

#[test]
fn test_message_envelope_roundtrip() {
    let message = Message::request(b"test payload".to_vec(), Priority::Normal);

    let envelope = message.to_envelope().unwrap();
    let reconstructed = Message::from_envelope(envelope).unwrap();

    assert_eq!(reconstructed.id.to_string(), message.id.to_string());
    assert_eq!(reconstructed.message_type, message.message_type);
    assert_eq!(reconstructed.priority, message.priority);
    assert_eq!(reconstructed.payload, message.payload);
}

#[test]
fn test_message_custom_construction() {
    let message = Message::new(
        MessageType::Event, 
        Priority::High,
        b"test payload".to_vec(),
        None,
        None
    );

    assert_eq!(message.message_type, MessageType::Event);
    assert_eq!(message.priority, Priority::High);
    assert_eq!(message.payload, b"test payload");
}

#[test]
fn test_message_priority_ordering() {
    assert!(Priority::High as u8 > Priority::Normal as u8);
    assert!(Priority::Normal as u8 > Priority::Low as u8);
}

#[test]
fn test_message_type_conversion() {
    assert_eq!(MessageType::Request as u8, 0);
    assert_eq!(MessageType::Response as u8, 1);
    assert_eq!(MessageType::Event as u8, 2);
    assert_eq!(MessageType::KeepAlive as u8, 3);
} 