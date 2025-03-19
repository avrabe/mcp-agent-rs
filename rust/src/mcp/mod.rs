mod types;
mod protocol;

pub use types::{Message, MessageId, MessageType, Priority, MessageHeader, MessageEnvelope};
pub use protocol::McpProtocol;

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_message_serialization_roundtrip(
            id in "[a-zA-Z0-9-]+",
            message_type in 0..4u8,
            priority in 0..4u8,
            payload in ".*",
        ) {
            let message = Message::new(
                MessageId::new(id),
                match message_type {
                    0 => MessageType::Request,
                    1 => MessageType::Response,
                    2 => MessageType::Event,
                    _ => MessageType::Error,
                },
                match priority {
                    0 => Priority::Low,
                    1 => Priority::Normal,
                    2 => Priority::High,
                    _ => Priority::Critical,
                },
                payload.as_bytes().to_vec(),
                None,
                None,
            );

            let serialized = serde_json::to_vec(&message).unwrap();
            let deserialized: Message = serde_json::from_slice(&serialized).unwrap();
            
            assert_eq!(message.id.as_str(), deserialized.id.as_str());
            assert_eq!(message.message_type, deserialized.message_type);
            assert_eq!(message.priority, deserialized.priority);
            assert_eq!(message.payload, deserialized.payload);
        }
    }
} 