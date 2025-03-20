use bytes::BytesMut;
use tokio::sync::{mpsc, broadcast};
use tokio::time::Duration;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::io::{Read, Write};
use tokio::io::{AsyncRead, AsyncWrite, AsyncReadExt, AsyncWriteExt};

use crate::mcp::types::{Message, MessageId, MessageType, Priority};
use crate::utils::error::{McpError, McpResult};

/// Protocol handler for MCP messages
#[derive(Debug)]
pub struct McpProtocol {
    /// Buffer for reading messages
    read_buffer: BytesMut,
    /// Buffer for reading message IDs
    id_buffer: BytesMut,
    /// Channel for sending messages
    message_tx: Option<broadcast::Sender<Message>>,
    /// Channel for receiving messages
    message_rx: Option<broadcast::Receiver<Message>>,
    /// Map of pending requests
    pending_requests: Arc<Mutex<HashMap<String, mpsc::Sender<Message>>>>,
    /// Timeout for requests
    request_timeout: Duration,
}

impl Clone for McpProtocol {
    fn clone(&self) -> Self {
        Self {
            read_buffer: BytesMut::with_capacity(self.read_buffer.capacity()),
            id_buffer: BytesMut::with_capacity(self.id_buffer.capacity()),
            message_tx: self.message_tx.clone(),
            message_rx: self.message_tx.as_ref().map(|tx| tx.subscribe()),
            pending_requests: self.pending_requests.clone(),
            request_timeout: self.request_timeout,
        }
    }
}

impl McpProtocol {
    /// Creates a new protocol handler
    pub fn new() -> Self {
        Self {
            read_buffer: BytesMut::with_capacity(1024),
            id_buffer: BytesMut::with_capacity(16),
            message_tx: None,
            message_rx: None,
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            request_timeout: Duration::from_secs(30),
        }
    }

    /// Writes a message to a stream synchronously
    pub fn write_message<W: Write>(&self, writer: &mut W, message: &Message) -> McpResult<()> {
        // Write message type
        writer.write_all(&[message.message_type.as_u8()])?;
        
        // Write priority
        writer.write_all(&[message.priority.as_u8()])?;
        
        // Write ID
        let id_str = message.id.to_string();
        writer.write_all(&(id_str.len() as u32).to_le_bytes())?;
        writer.write_all(id_str.as_bytes())?;
        
        // Write in_reply_to if present
        if let Some(ref reply_id) = message.in_reply_to {
            writer.write_all(&[1u8])?; // Has in_reply_to
            let reply_str = reply_id.to_string();
            writer.write_all(&(reply_str.len() as u32).to_le_bytes())?;
            writer.write_all(reply_str.as_bytes())?;
        } else {
            writer.write_all(&[0u8])?; // No in_reply_to
        }
        
        // Write error if present
        if let Some(ref error) = message.error {
            writer.write_all(&[1u8])?; // Has error
            writer.write_all(&(error.len() as u32).to_le_bytes())?;
            writer.write_all(error.as_bytes())?;
        } else {
            writer.write_all(&[0u8])?; // No error
        }
        
        // Write payload
        writer.write_all(&(message.payload.len() as u32).to_le_bytes())?;
        writer.write_all(&message.payload)?;
        
        Ok(())
    }
    
    /// Reads a message from a stream synchronously
    pub fn read_message<R: Read>(&mut self, reader: &mut R) -> McpResult<Message> {
        // Read message type
        let mut type_buf = [0u8; 1];
        reader.read_exact(&mut type_buf)?;
        let message_type = MessageType::from_u8(type_buf[0])
            .ok_or_else(|| McpError::InvalidMessage("Invalid message type".to_string()))?;
        
        // Read priority
        let mut priority_buf = [0u8; 1];
        reader.read_exact(&mut priority_buf)?;
        let priority = Priority::from_u8(priority_buf[0])
            .ok_or_else(|| McpError::InvalidMessage("Invalid priority".to_string()))?;
        
        // Read ID
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf)?;
        let id_len = u32::from_le_bytes(len_buf) as usize;
        let mut id_buf = vec![0u8; id_len];
        reader.read_exact(&mut id_buf)?;
        let id_str = String::from_utf8(id_buf)
            .map_err(|_| McpError::InvalidMessage("Invalid UTF-8 in ID".to_string()))?;
        let id = MessageId::new(id_str);
        
        // Read in_reply_to if present
        let mut has_reply_buf = [0u8; 1];
        reader.read_exact(&mut has_reply_buf)?;
        let in_reply_to = if has_reply_buf[0] == 1 {
            reader.read_exact(&mut len_buf)?;
            let reply_len = u32::from_le_bytes(len_buf) as usize;
            let mut reply_buf = vec![0u8; reply_len];
            reader.read_exact(&mut reply_buf)?;
            let reply_str = String::from_utf8(reply_buf)
                .map_err(|_| McpError::InvalidMessage("Invalid UTF-8 in reply ID".to_string()))?;
            Some(MessageId::new(reply_str))
        } else {
            None
        };
        
        // Read error if present
        let mut has_error_buf = [0u8; 1];
        reader.read_exact(&mut has_error_buf)?;
        let error = if has_error_buf[0] == 1 {
            reader.read_exact(&mut len_buf)?;
            let error_len = u32::from_le_bytes(len_buf) as usize;
            let mut error_buf = vec![0u8; error_len];
            reader.read_exact(&mut error_buf)?;
            let error_str = String::from_utf8(error_buf)
                .map_err(|_| McpError::InvalidMessage("Invalid UTF-8 in error".to_string()))?;
            Some(error_str)
        } else {
            None
        };
        
        // Read payload
        reader.read_exact(&mut len_buf)?;
        let payload_len = u32::from_le_bytes(len_buf) as usize;
        let mut payload = vec![0u8; payload_len];
        reader.read_exact(&mut payload)?;
        
        Ok(Message::new(
            id,
            message_type,
            priority,
            payload,
            in_reply_to,
            error,
        ))
    }
    
    /// Writes a message to a stream asynchronously
    pub async fn write_message_async<W>(&self, stream: &mut W, message: &Message) -> McpResult<()>
    where
        W: AsyncWrite + Unpin,
    {
        let mut buffer = BytesMut::new();
        
        // Write message type
        buffer.extend_from_slice(&[message.message_type as u8]);
        
        // Write priority
        buffer.extend_from_slice(&[message.priority as u8]);
        
        // Write message ID (16 bytes)
        buffer.extend_from_slice(&message.id.0);
        
        // Write correlation ID if present
        let has_correlation_id = message.correlation_id.is_some();
        buffer.extend_from_slice(&[has_correlation_id as u8]);
        if let Some(correlation_id) = &message.correlation_id {
            buffer.extend_from_slice(&correlation_id.0);
        }
        
        // Write error if present
        let has_error = message.error.is_some();
        buffer.extend_from_slice(&[has_error as u8]);
        if let Some(error) = &message.error {
            // Write error code
            buffer.extend_from_slice(&(error.code as u32).to_be_bytes());
            
            // Write error message
            let error_msg = error.message.as_bytes();
            buffer.extend_from_slice(&(error_msg.len() as u32).to_be_bytes());
            buffer.extend_from_slice(error_msg);
        }
        
        // Write payload
        buffer.extend_from_slice(&(message.payload.len() as u32).to_be_bytes());
        buffer.extend_from_slice(&message.payload);
        
        // Write to stream
        stream.write_all(&buffer).await?;
        stream.flush().await?;
        
        Ok(())
    }
    
    /// Reads a message from a stream asynchronously
    pub async fn read_message_async<R>(&self, stream: &mut R) -> McpResult<Message>
    where
        R: AsyncRead + Unpin,
    {
        use tokio::io::AsyncReadExt;
        
        let mut message_type_buf = [0u8; 1];
        stream.read_exact(&mut message_type_buf).await?;
        let message_type = match message_type_buf[0] {
            0 => MessageType::Request,
            1 => MessageType::Response,
            2 => MessageType::Event,
            3 => MessageType::KeepAlive,
            _ => return Err(McpError::InvalidMessage("Invalid message type".to_string())),
        };
        
        let mut priority_buf = [0u8; 1];
        stream.read_exact(&mut priority_buf).await?;
        let priority = match priority_buf[0] {
            0 => Priority::Low,
            1 => Priority::Medium,
            2 => Priority::High,
            _ => return Err(McpError::InvalidMessage("Invalid priority".to_string())),
        };
        
        // Read message ID
        let mut id_buf = [0u8; 16];
        stream.read_exact(&mut id_buf).await?;
        let id = MessageId(id_buf);
        
        // Read correlation ID if present
        let mut has_correlation_id_buf = [0u8; 1];
        stream.read_exact(&mut has_correlation_id_buf).await?;
        let has_correlation_id = has_correlation_id_buf[0] == 1;
        
        let correlation_id = if has_correlation_id {
            let mut correlation_id_buf = [0u8; 16];
            stream.read_exact(&mut correlation_id_buf).await?;
            Some(MessageId(correlation_id_buf))
        } else {
            None
        };
        
        // Read error if present
        let mut has_error_buf = [0u8; 1];
        stream.read_exact(&mut has_error_buf).await?;
        let has_error = has_error_buf[0] == 1;
        
        let error = if has_error {
            // Read error code
            let mut error_code_buf = [0u8; 4];
            stream.read_exact(&mut error_code_buf).await?;
            let error_code = u32::from_be_bytes(error_code_buf);
            
            // Read error message
            let mut error_msg_len_buf = [0u8; 4];
            stream.read_exact(&mut error_msg_len_buf).await?;
            let error_msg_len = u32::from_be_bytes(error_msg_len_buf) as usize;
            
            let mut error_msg_buf = vec![0u8; error_msg_len];
            stream.read_exact(&mut error_msg_buf).await?;
            
            let error_msg = String::from_utf8(error_msg_buf)
                .map_err(|_| McpError::InvalidMessage("Invalid UTF-8 in error message".to_string()))?;
                
            Some(McpError::Custom { 
                code: error_code,
                message: error_msg 
            })
        } else {
            None
        };
        
        // Read payload
        let mut payload_len_buf = [0u8; 4];
        stream.read_exact(&mut payload_len_buf).await?;
        let payload_len = u32::from_be_bytes(payload_len_buf) as usize;
        
        let mut payload = vec![0u8; payload_len];
        stream.read_exact(&mut payload).await?;
        
        Ok(Message {
            message_type,
            priority,
            id,
            correlation_id,
            error,
            payload,
        })
    }
} 