//! MCP (Message Control Protocol) implementation
//! 
//! This module provides a robust implementation of the Message Control Protocol,
//! including connection management, error handling, and message processing.

/// Connection management module
pub mod connection;

/// Protocol implementation and message handling
pub mod protocol;

/// Server registry module
pub mod server_registry;

/// Core types and data structures
pub mod types;

/// Executor module
pub mod executor;

/// Agent API module
pub mod agent;

pub use connection::Connection;
pub use protocol::McpProtocol;
pub use server_registry::{ServerRegistry, ServerSettings, McpSettings};
pub use types::{Message, MessageId, MessageType, Priority, MessageHeader, MessageEnvelope};
pub use executor::{Executor, AsyncioExecutor, ExecutorConfig, Signal, TaskResult};
pub use agent::{Agent, AgentConfig};
pub use crate::utils::error::{McpError, McpResult};

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use uuid::Uuid;

    proptest! {
        #[test]
        fn test_message_serialization_roundtrip(
            message_type in 0..4u8,
            priority in 0..3u8,
            payload in ".*",
        ) {
            // Create a message with the new API
            let message = Message::new(
                match message_type {
                    0 => MessageType::Request,
                    1 => MessageType::Response,
                    2 => MessageType::Event,
                    _ => MessageType::KeepAlive,
                },
                match priority {
                    0 => Priority::Low,
                    1 => Priority::Normal,
                    _ => Priority::High,
                },
                payload.as_bytes().to_vec(),
                None,
                None,
            );

            let serialized = serde_json::to_vec(&message).unwrap();
            let deserialized: Message = serde_json::from_slice(&serialized).unwrap();
            
            // Compare string representations of UUIDs
            assert_eq!(message.id.to_string(), deserialized.id.to_string());
            assert_eq!(message.message_type, deserialized.message_type);
            assert_eq!(message.priority, deserialized.priority);
            assert_eq!(message.payload, deserialized.payload);
        }
    }
} 