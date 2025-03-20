use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::fmt;

use crate::utils::error::{McpError, McpResult};

/// A unique identifier for a message
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub [u8; 16]);

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageId {
    /// Generate a new message ID
    pub fn new() -> Self {
        let bytes = Uuid::new_v4().into_bytes();
        Self(bytes)
    }
    
    /// Create a MessageId from raw bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
    
    /// Get the raw bytes of the MessageId
    pub fn bytes(&self) -> &[u8; 16] {
        &self.0
    }
    
    /// Convert to a UUID
    pub fn to_uuid(&self) -> Uuid {
        Uuid::from_bytes(self.0)
    }
}

// Add Display implementation for MessageId
impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", Uuid::from_bytes(self.0))
    }
}

/// The type of message being sent
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageType {
    /// A request message
    Request,
    /// A response message
    Response,
    /// An event message
    Event,
    /// A keep-alive message
    KeepAlive,
}

impl MessageType {
    /// Converts a u8 to a MessageType
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Request),
            1 => Some(Self::Response),
            2 => Some(Self::Event),
            3 => Some(Self::KeepAlive),
            _ => None,
        }
    }

    /// Converts a MessageType to a u8
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Request => 0,
            Self::Response => 1,
            Self::Event => 2,
            Self::KeepAlive => 3,
        }
    }
}

/// The priority level of a message
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    /// Low priority messages
    Low,
    /// Normal priority messages
    Normal,
    /// High priority messages
    High,
}

impl Priority {
    /// Converts a u8 to a Priority
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Low),
            1 => Some(Self::Normal),
            2 => Some(Self::High),
            _ => None,
        }
    }

    /// Converts a Priority to a u8
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Normal => 1,
            Self::High => 2,
        }
    }
}

/// The header of a message containing metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHeader {
    /// The unique identifier of the message
    pub id: String,
    /// The type of message
    pub message_type: MessageType,
    /// The priority level of the message
    pub priority: Priority,
    /// The size of the payload in bytes
    pub payload_size: u32,
    /// The ID of the message this is replying to (if any)
    pub correlation_id: Option<String>,
    /// Any error information (if any)
    pub error: Option<McpError>,
}

/// The envelope containing the header and payload of a message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEnvelope {
    /// The header of the message
    pub header: MessageHeader,
    /// The payload of the message
    pub payload: Vec<u8>,
}

/// A complete message with header information and payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// The unique identifier of the message
    pub id: MessageId,
    /// The type of message
    pub message_type: MessageType,
    /// The priority level of the message
    pub priority: Priority,
    /// The payload of the message
    pub payload: Vec<u8>,
    /// The ID of the message this is replying to (if any)
    pub correlation_id: Option<MessageId>,
    /// Any error information (if any)
    pub error: Option<McpError>,
}

impl Message {
    /// Creates a new message
    pub fn new(
        message_type: MessageType,
        priority: Priority,
        payload: Vec<u8>,
        correlation_id: Option<MessageId>,
        error: Option<McpError>,
    ) -> Self {
        Self {
            id: MessageId::new(),
            message_type,
            priority,
            payload,
            correlation_id,
            error,
        }
    }

    /// Creates a new request message
    pub fn request(payload: Vec<u8>, priority: Priority) -> Self {
        Self::new(MessageType::Request, priority, payload, None, None)
    }

    /// Creates a new response message
    pub fn response(payload: Vec<u8>, request_id: MessageId) -> Self {
        Self::new(MessageType::Response, Priority::Normal, payload, Some(request_id), None)
    }

    /// Creates a new event message
    pub fn event(payload: Vec<u8>) -> Self {
        Self::new(MessageType::Event, Priority::Normal, payload, None, None)
    }

    /// Creates a new keep-alive message
    pub fn keep_alive() -> Self {
        Self::new(MessageType::KeepAlive, Priority::Low, vec![], None, None)
    }

    /// Creates a new error response
    pub fn error(error: McpError, request_id: Option<MessageId>) -> Self {
        Self::new(MessageType::Response, Priority::High, vec![], request_id, Some(error))
    }

    /// Converts a message to an envelope
    pub fn to_envelope(&self) -> McpResult<MessageEnvelope> {
        Ok(MessageEnvelope {
            header: MessageHeader {
                id: self.id.to_string(),
                message_type: self.message_type,
                priority: self.priority,
                payload_size: self.payload.len() as u32,
                correlation_id: self.correlation_id.as_ref().map(|id| id.to_string()),
                error: self.error.clone(),
            },
            payload: self.payload.clone(),
        })
    }

    /// Converts an envelope to a message
    pub fn from_envelope(envelope: MessageEnvelope) -> McpResult<Self> {
        let id_bytes = Uuid::parse_str(&envelope.header.id)
            .map_err(|_| McpError::InvalidMessage("Invalid UUID format".to_string()))?
            .into_bytes();
            
        let correlation_id = if let Some(id_str) = envelope.header.correlation_id {
            let id_bytes = Uuid::parse_str(&id_str)
                .map_err(|_| McpError::InvalidMessage("Invalid correlation ID UUID format".to_string()))?
                .into_bytes();
            Some(MessageId(id_bytes))
        } else {
            None
        };
        
        Ok(Self {
            id: MessageId(id_bytes),
            message_type: envelope.header.message_type,
            priority: envelope.header.priority,
            payload: envelope.payload,
            correlation_id,
            error: envelope.header.error,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_id_generation() {
        let id = MessageId::new();
        assert!(!id.to_string().is_empty());
        assert_eq!(id.to_string().len(), 36);
    }

    #[test]
    fn test_message_conversion() {
        let message = Message::new(
            MessageType::Request,
            Priority::Normal,
            vec![1, 2, 3],
            None,
            None,
        );

        let envelope = message.to_envelope().unwrap();
        assert_eq!(envelope.header.id, message.id.to_string());
        assert_eq!(envelope.header.message_type, MessageType::Request);
        assert_eq!(envelope.header.priority, Priority::Normal);
        assert_eq!(envelope.header.payload_size, 3);
        assert_eq!(envelope.payload, vec![1, 2, 3]);

        let reconstructed = Message::from_envelope(envelope).unwrap();
        assert_eq!(reconstructed.id.to_string(), message.id.to_string());
        assert_eq!(reconstructed.message_type, MessageType::Request);
        assert_eq!(reconstructed.priority, Priority::Normal);
        assert_eq!(reconstructed.payload, vec![1, 2, 3]);
    }
} 