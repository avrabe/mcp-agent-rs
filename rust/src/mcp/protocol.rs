use std::io::{Read, Write};
use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};
use crate::utils::error::McpResult;
use crate::mcp::types::{Message, MessageEnvelope, MessageHeader};

/// The size of the message header in bytes
const HEADER_SIZE: usize = std::mem::size_of::<MessageHeader>();

/// Protocol implementation for MCP message handling.
#[derive(Debug)]
pub struct McpProtocol {
    /// Buffer for reading messages
    read_buffer: BytesMut,
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
        }
    }

    /// Writes a message to the given writer
    pub fn write_message<W: Write>(&self, writer: &mut W, message: &Message) -> McpResult<()> {
        let envelope = MessageEnvelope::from_message(message);
        
        // Write header
        let header_bytes = bytemuck::bytes_of(&envelope.header);
        writer.write_all(header_bytes)?;
        
        // Write payload length
        writer.write_all(&envelope.payload.len().to_le_bytes())?;
        
        // Write payload
        writer.write_all(envelope.payload)?;
        
        Ok(())
    }

    /// Reads a message from the given reader
    pub fn read_message<R: Read>(&mut self, reader: &mut R) -> McpResult<Message> {
        // Read header
        let mut header_bytes = [0u8; HEADER_SIZE];
        reader.read_exact(&mut header_bytes)?;
        
        let header = bytemuck::pod_read_unaligned::<MessageHeader>(&header_bytes);
        
        // Read payload length
        let mut len_bytes = [0u8; 4];
        reader.read_exact(&mut len_bytes)?;
        let payload_len = u32::from_le_bytes(len_bytes) as usize;
        
        // Read payload
        self.read_buffer.resize(payload_len, 0);
        reader.read_exact(&mut self.read_buffer)?;
        
        // Create envelope and convert to message
        let envelope = MessageEnvelope {
            header,
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
        
        // Write payload length
        writer.write_all(&envelope.payload.len().to_le_bytes()).await?;
        
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
        reader.read_exact(&mut header_bytes).await?;
        
        let header = bytemuck::pod_read_unaligned::<MessageHeader>(&header_bytes);
        
        // Read payload length
        let mut len_bytes = [0u8; 4];
        reader.read_exact(&mut len_bytes).await?;
        let payload_len = u32::from_le_bytes(len_bytes) as usize;
        
        // Read payload
        self.read_buffer.resize(payload_len, 0);
        reader.read_exact(&mut self.read_buffer).await?;
        
        // Create envelope and convert to message
        let envelope = MessageEnvelope {
            header,
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
        
        let mut protocol = McpProtocol::new();
        let read_message = protocol.read_message(&mut &buffer[..]).unwrap();
        
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
        
        let mut protocol = McpProtocol::new();
        let read_message = protocol.read_message_async(&mut &buffer[..]).await.unwrap();
        
        assert_eq!(message.id.as_str(), read_message.id.as_str());
        assert_eq!(message.message_type, read_message.message_type);
        assert_eq!(message.priority, read_message.priority);
        assert_eq!(message.payload, read_message.payload);
    }
} 