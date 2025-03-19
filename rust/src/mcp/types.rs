use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use validator::Validate;
use chrono::{DateTime, Utc};

/// Represents a unique identifier for MCP messages
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
pub struct MessageId(Cow<'static, str>);

impl MessageId {
    /// Creates a new MessageId with a static string
    pub const fn new_static(id: &'static str) -> Self {
        Self(Cow::Borrowed(id))
    }

    /// Creates a new MessageId with an owned string
    pub fn new(id: String) -> Self {
        Self(Cow::Owned(id))
    }

    /// Returns the string representation of the ID
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Type of message in the MCP protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// A request message that expects a response
    Request,
    /// A response message to a previous request
    Response,
    /// An event message that doesn't expect a response
    Event,
    /// An error message indicating a failure
    Error,
    /// A keep-alive message to maintain connection
    KeepAlive,
}

/// Priority level of a message
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Priority {
    /// Low priority message, can be processed when convenient
    Low,
    /// Normal priority message, should be processed in order
    Normal,
    /// High priority message, should be processed before normal messages
    High,
    /// Critical priority message, should be processed immediately
    Critical,
}

/// Represents the core MCP message structure
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct Message {
    /// Unique identifier for the message
    pub id: MessageId,
    
    /// Type of the message
    pub message_type: MessageType,
    
    /// Priority of the message
    pub priority: Priority,
    
    /// Timestamp of message creation
    pub timestamp: DateTime<Utc>,
    
    /// Optional correlation ID for request-response patterns
    pub correlation_id: Option<MessageId>,
    
    /// The actual payload of the message
    #[serde(with = "serde_bytes")]
    pub payload: Vec<u8>,
    
    /// Optional metadata associated with the message
    pub metadata: Option<serde_json::Value>,
}

impl Message {
    /// Creates a new message with the given parameters
    pub fn new(
        id: MessageId,
        message_type: MessageType,
        priority: Priority,
        payload: Vec<u8>,
        correlation_id: Option<MessageId>,
        metadata: Option<serde_json::Value>,
    ) -> Self {
        Self {
            id,
            message_type,
            priority,
            timestamp: Utc::now(),
            correlation_id,
            payload,
            metadata,
        }
    }

    /// Creates a request message
    pub fn request(id: MessageId, payload: Vec<u8>) -> Self {
        Self::new(id, MessageType::Request, Priority::Normal, payload, None, None)
    }

    /// Creates a response message
    pub fn response(id: MessageId, correlation_id: MessageId, payload: Vec<u8>) -> Self {
        Self::new(
            id,
            MessageType::Response,
            Priority::Normal,
            payload,
            Some(correlation_id),
            None,
        )
    }

    /// Creates an event message
    pub fn event(id: MessageId, payload: Vec<u8>) -> Self {
        Self::new(id, MessageType::Event, Priority::Normal, payload, None, None)
    }

    /// Creates an error message
    pub fn error(id: MessageId, correlation_id: Option<MessageId>, error: String) -> Self {
        let payload = serde_json::to_vec(&error).unwrap_or_default();
        Self::new(
            id,
            MessageType::Error,
            Priority::High,
            payload,
            correlation_id,
            None,
        )
    }
}

/// Represents a message header for zero-copy operations
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C, align(8))]  // Ensure 8-byte alignment
pub struct MessageHeader {
    /// Message type as a u8
    pub message_type: u8,
    /// Priority as a u8
    pub priority: u8,
    /// Padding to ensure proper alignment
    _pad: [u8; 2],
    /// Payload length
    pub payload_len: u32,
    /// Timestamp as Unix timestamp
    pub timestamp: i64,
}

/// Represents a message envelope for zero-copy operations
#[derive(Debug)]
pub struct MessageEnvelope<'a> {
    /// The message header
    pub header: MessageHeader,
    /// The message ID
    pub id: &'a MessageId,
    /// The correlation ID, if any
    pub correlation_id: Option<&'a MessageId>,
    /// The message payload as bytes
    pub payload: &'a [u8],
}

impl<'a> MessageEnvelope<'a> {
    /// Creates a new message envelope from a message
    pub fn from_message(message: &'a Message) -> Self {
        Self {
            header: MessageHeader {
                message_type: message.message_type as u8,
                priority: message.priority as u8,
                _pad: [0; 2],
                payload_len: message.payload.len() as u32,
                timestamp: message.timestamp.timestamp(),
            },
            id: &message.id,
            correlation_id: message.correlation_id.as_ref(),
            payload: &message.payload,
        }
    }

    /// Attempts to create a message from the envelope
    pub fn to_message(&self) -> Result<Message, crate::utils::error::McpError> {
        Ok(Message {
            id: MessageId::new(self.id.as_str().to_string()),
            message_type: match self.header.message_type {
                0 => MessageType::Request,
                1 => MessageType::Response,
                2 => MessageType::Event,
                3 => MessageType::Error,
                4 => MessageType::KeepAlive,
                _ => return Err(crate::utils::error::McpError::InvalidMessage("Invalid message type".into())),
            },
            priority: match self.header.priority {
                0 => Priority::Low,
                1 => Priority::Normal,
                2 => Priority::High,
                3 => Priority::Critical,
                _ => return Err(crate::utils::error::McpError::InvalidMessage("Invalid priority".into())),
            },
            timestamp: DateTime::from_timestamp(self.header.timestamp, 0)
                .ok_or_else(|| crate::utils::error::McpError::InvalidMessage("Invalid timestamp".into()))?,
            correlation_id: self.correlation_id.map(|id| MessageId::new(id.as_str().to_string())),
            payload: self.payload.to_vec(),
            metadata: None,
        })
    }
} 