use std::io::{Read, Write};
use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use crate::utils::error::McpResult;
use crate::mcp::types::{Message, MessageEnvelope, MessageHeader, MessageId};

/// The size of the message header in bytes
const HEADER_SIZE: usize = std::mem::size_of::<MessageHeader>();

/// Protocol implementation for MCP message handling.
#[derive(Debug, Clone)]
pub struct McpProtocol {
    /// Buffer for reading messages
    read_buffer: BytesMut,
    /// Buffer for reading message IDs
    id_buffer: String,
}

impl Default for McpProtocol {
    fn default() -> Self {
        Self::new()
    }
}

impl McpProtocol {
    /// Creates a new MCP protocol handler with a default buffer size.
    pub fn new() -> Self {
        Self {
            read_buffer: BytesMut::with_capacity(4096),
            id_buffer: String::with_capacity(36),
        }
    }

    /// Validates the message header
    fn validate_header(header: &MessageHeader) -> McpResult<()> {
        // Check message type
        if header.message_type > 3 {
            return Err(crate::utils::error::McpError::InvalidMessage("Invalid message type".into()));
        }

        // Check priority
        if header.priority > 3 {
            return Err(crate::utils::error::McpError::InvalidMessage("Invalid priority".into()));
        }

        // Check payload length (max 16MB)
        if header.payload_len == 0 || header.payload_len > 16 * 1024 * 1024 {
            return Err(crate::utils::error::McpError::InvalidMessage("Invalid payload length".into()));
        }

        // Check timestamp (must be within reasonable range)
        let now = chrono::Utc::now().timestamp();
        let max_future = now + 60; // Allow up to 1 minute in the future
        let min_past = now - 60 * 60 * 24 * 365; // Allow up to 1 year in the past
        if header.timestamp < min_past || header.timestamp > max_future {
            return Err(crate::utils::error::McpError::InvalidMessage("Invalid timestamp".into()));
        }

        Ok(())
    }

    /// Writes a message to the given writer
    pub fn write_message<W: Write>(&self, writer: &mut W, message: &Message) -> McpResult<()> {
        let envelope = MessageEnvelope::from_message(message);
        
        // Write header
        let header_bytes = bytemuck::bytes_of(&envelope.header);
        writer.write_all(header_bytes)?;
        
        // Write message ID length and bytes
        let id_bytes = envelope.id.as_str().as_bytes();
        writer.write_all(&(id_bytes.len() as u32).to_le_bytes())?;
        writer.write_all(id_bytes)?;

        // Write correlation ID if present
        if let Some(correlation_id) = envelope.correlation_id {
            writer.write_all(&[1u8])?; // Has correlation ID
            let correlation_id_bytes = correlation_id.as_str().as_bytes();
            writer.write_all(&(correlation_id_bytes.len() as u32).to_le_bytes())?;
            writer.write_all(correlation_id_bytes)?;
        } else {
            writer.write_all(&[0u8])?; // No correlation ID
        }
        
        // Write payload
        writer.write_all(envelope.payload)?;
        
        Ok(())
    }

    /// Reads a message from the given reader
    pub fn read_message<R: Read>(&mut self, reader: &mut R) -> McpResult<Message> {
        // Read header
        let mut header_bytes = [0u8; HEADER_SIZE];
        if let Err(e) = reader.read_exact(&mut header_bytes) {
            return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read header: {}", e)));
        }
        
        let header = bytemuck::pod_read_unaligned::<MessageHeader>(&header_bytes);
        Self::validate_header(&header)?;
        
        // Read message ID length and bytes
        let mut len_bytes = [0u8; 4];
        if let Err(e) = reader.read_exact(&mut len_bytes) {
            return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read ID length: {}", e)));
        }
        let id_len = u32::from_le_bytes(len_bytes) as usize;
        
        // Validate ID length
        if id_len > 1024 {
            return Err(crate::utils::error::McpError::InvalidMessage("Message ID too long".into()));
        }
        
        self.id_buffer.clear();
        self.id_buffer.reserve(id_len);
        let mut id_bytes = vec![0u8; id_len];
        if let Err(e) = reader.read_exact(&mut id_bytes) {
            return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read ID: {}", e)));
        }
        
        if let Err(e) = std::str::from_utf8(&id_bytes) {
            return Err(crate::utils::error::McpError::InvalidMessage(format!("Invalid UTF-8 in ID: {}", e)));
        }
        self.id_buffer.push_str(std::str::from_utf8(&id_bytes)?);
        let message_id = MessageId::new(self.id_buffer.clone());

        // Read correlation ID if present
        let mut has_correlation_id = [0u8];
        if let Err(e) = reader.read_exact(&mut has_correlation_id) {
            return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read correlation ID flag: {}", e)));
        }

        let correlation_id = if has_correlation_id[0] == 1 {
            let mut len_bytes = [0u8; 4];
            if let Err(e) = reader.read_exact(&mut len_bytes) {
                return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read correlation ID length: {}", e)));
            }
            let correlation_id_len = u32::from_le_bytes(len_bytes) as usize;

            // Validate correlation ID length
            if correlation_id_len > 1024 {
                return Err(crate::utils::error::McpError::InvalidMessage("Correlation ID too long".into()));
            }

            let mut correlation_id_bytes = vec![0u8; correlation_id_len];
            if let Err(e) = reader.read_exact(&mut correlation_id_bytes) {
                return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read correlation ID: {}", e)));
            }

            if let Err(e) = std::str::from_utf8(&correlation_id_bytes) {
                return Err(crate::utils::error::McpError::InvalidMessage(format!("Invalid UTF-8 in correlation ID: {}", e)));
            }
            Some(MessageId::new(std::str::from_utf8(&correlation_id_bytes)?.to_string()))
        } else {
            None
        };
        
        // Read payload
        let payload_len = header.payload_len as usize;
        self.read_buffer.resize(payload_len, 0);
        if let Err(e) = reader.read_exact(&mut self.read_buffer) {
            return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read payload: {}", e)));
        }
        
        // Create envelope and convert to message
        let envelope = MessageEnvelope {
            header,
            id: &message_id,
            correlation_id: correlation_id.as_ref(),
            payload: &self.read_buffer,
        };
        
        envelope.to_message()
    }

    /// Writes a message asynchronously to the given writer
    pub async fn write_message_async<W: AsyncWrite + Unpin>(
        &self,
        writer: &mut W,
        message: &Message,
    ) -> McpResult<()> {
        let envelope = MessageEnvelope::from_message(message);
        
        // Write header
        let header_bytes = bytemuck::bytes_of(&envelope.header);
        writer.write_all(header_bytes).await?;
        
        // Write message ID length and bytes
        let id_bytes = envelope.id.as_str().as_bytes();
        writer.write_all(&(id_bytes.len() as u32).to_le_bytes()).await?;
        writer.write_all(id_bytes).await?;

        // Write correlation ID if present
        if let Some(correlation_id) = envelope.correlation_id {
            writer.write_all(&[1u8]).await?; // Has correlation ID
            let correlation_id_bytes = correlation_id.as_str().as_bytes();
            writer.write_all(&(correlation_id_bytes.len() as u32).to_le_bytes()).await?;
            writer.write_all(correlation_id_bytes).await?;
        } else {
            writer.write_all(&[0u8]).await?; // No correlation ID
        }
        
        // Write payload
        writer.write_all(envelope.payload).await?;
        
        Ok(())
    }

    /// Reads a message asynchronously from the given reader
    pub async fn read_message_async<R: AsyncRead + Unpin>(
        &mut self,
        reader: &mut R,
    ) -> McpResult<Message> {
        // Read header
        let mut header_bytes = [0u8; HEADER_SIZE];
        if let Err(e) = reader.read_exact(&mut header_bytes).await {
            return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read header: {}", e)));
        }
        
        let header = bytemuck::pod_read_unaligned::<MessageHeader>(&header_bytes);
        Self::validate_header(&header)?;
        
        // Read message ID length and bytes
        let mut len_bytes = [0u8; 4];
        if let Err(e) = reader.read_exact(&mut len_bytes).await {
            return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read ID length: {}", e)));
        }
        let id_len = u32::from_le_bytes(len_bytes) as usize;
        
        // Validate ID length
        if id_len > 1024 {
            return Err(crate::utils::error::McpError::InvalidMessage("Message ID too long".into()));
        }
        
        self.id_buffer.clear();
        self.id_buffer.reserve(id_len);
        let mut id_bytes = vec![0u8; id_len];
        if let Err(e) = reader.read_exact(&mut id_bytes).await {
            return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read ID: {}", e)));
        }
        
        if let Err(e) = std::str::from_utf8(&id_bytes) {
            return Err(crate::utils::error::McpError::InvalidMessage(format!("Invalid UTF-8 in ID: {}", e)));
        }
        self.id_buffer.push_str(std::str::from_utf8(&id_bytes)?);
        let message_id = MessageId::new(self.id_buffer.clone());

        // Read correlation ID if present
        let mut has_correlation_id = [0u8];
        if let Err(e) = reader.read_exact(&mut has_correlation_id).await {
            return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read correlation ID flag: {}", e)));
        }

        let correlation_id = if has_correlation_id[0] == 1 {
            let mut len_bytes = [0u8; 4];
            if let Err(e) = reader.read_exact(&mut len_bytes).await {
                return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read correlation ID length: {}", e)));
            }
            let correlation_id_len = u32::from_le_bytes(len_bytes) as usize;

            // Validate correlation ID length
            if correlation_id_len > 1024 {
                return Err(crate::utils::error::McpError::InvalidMessage("Correlation ID too long".into()));
            }

            let mut correlation_id_bytes = vec![0u8; correlation_id_len];
            if let Err(e) = reader.read_exact(&mut correlation_id_bytes).await {
                return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read correlation ID: {}", e)));
            }

            if let Err(e) = std::str::from_utf8(&correlation_id_bytes) {
                return Err(crate::utils::error::McpError::InvalidMessage(format!("Invalid UTF-8 in correlation ID: {}", e)));
            }
            Some(MessageId::new(std::str::from_utf8(&correlation_id_bytes)?.to_string()))
        } else {
            None
        };
        
        // Read payload
        let payload_len = header.payload_len as usize;
        self.read_buffer.resize(payload_len, 0);
        if let Err(e) = reader.read_exact(&mut self.read_buffer).await {
            return Err(crate::utils::error::McpError::InvalidMessage(format!("Failed to read payload: {}", e)));
        }
        
        // Create envelope and convert to message
        let envelope = MessageEnvelope {
            header,
            id: &message_id,
            correlation_id: correlation_id.as_ref(),
            payload: &self.read_buffer,
        };
        
        envelope.to_message()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::types::{MessageId, MessageType, Priority};

    #[test]
    fn test_message_roundtrip() {
        let protocol = McpProtocol::new();
        let message = Message::new(
            MessageId::new("test-id".to_string()),
            MessageType::Request,
            Priority::Normal,
            b"test payload".to_vec(),
            None,
            None,
        );

        let mut buffer = Vec::new();
        protocol.write_message(&mut buffer, &message).unwrap();
        
        println!("Written buffer: {:?}", buffer);
        println!("Written buffer length: {}", buffer.len());
        println!("Header size: {}", HEADER_SIZE);
        println!("ID length: {}", message.id.as_str().len());
        println!("Payload length: {}", message.payload.len());
        
        let mut protocol = McpProtocol::new();
        let read_message = protocol.read_message(&mut &buffer[..]).unwrap();
        
        println!("Original payload: {:?}", message.payload);
        println!("Read payload: {:?}", read_message.payload);
        
        assert_eq!(message.id.as_str(), read_message.id.as_str());
        assert_eq!(message.message_type, read_message.message_type);
        assert_eq!(message.priority, read_message.priority);
        assert_eq!(message.payload, read_message.payload);
    }

    #[tokio::test]
    async fn test_async_message_roundtrip() {
        let protocol = McpProtocol::new();
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
        
        println!("Written buffer (async): {:?}", buffer);
        println!("Written buffer length (async): {}", buffer.len());
        println!("Header size: {}", HEADER_SIZE);
        println!("ID length (async): {}", message.id.as_str().len());
        println!("Payload length (async): {}", message.payload.len());
        
        let mut protocol = McpProtocol::new();
        let read_message = protocol.read_message_async(&mut &buffer[..]).await.unwrap();
        
        println!("Original payload (async): {:?}", message.payload);
        println!("Read payload (async): {:?}", read_message.payload);
        
        assert_eq!(message.id.as_str(), read_message.id.as_str());
        assert_eq!(message.message_type, read_message.message_type);
        assert_eq!(message.priority, read_message.priority);
        assert_eq!(message.payload, read_message.payload);
    }
} 