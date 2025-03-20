use bytes::BytesMut;
use tokio::sync::{mpsc, broadcast};
use tokio::time::Duration;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;

use crate::mcp::types::{Message, MessageId, MessageType, Priority};
use crate::utils::error::{McpError, McpResult};

/// Protocol implementation for MCP (Management Control Protocol).
/// Handles message serialization, deserialization, and communication.
#[derive(Debug)]
pub struct McpProtocol {
    /// Buffer for reading incoming data
    read_buffer: BytesMut,
    /// Buffer for storing message IDs
    id_buffer: BytesMut,
    /// Channel sender for outgoing messages
    message_tx: Option<broadcast::Sender<Message>>,
    /// Channel receiver for incoming messages
    message_rx: Option<broadcast::Receiver<Message>>,
    /// Map of pending requests awaiting responses
    pending_requests: Arc<Mutex<HashMap<String, mpsc::Sender<Message>>>>,
    /// Timeout duration for requests
    request_timeout: Duration,
}

impl Clone for McpProtocol {
    /// Creates a clone of the protocol instance.
    /// This allows sharing the protocol between different tasks while maintaining
    /// separate buffers but shared state for pending requests.
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

impl Default for McpProtocol {
    fn default() -> Self {
        Self::new()
    }
}

impl McpProtocol {
    /// Creates a new instance of the MCP Protocol handler.
    /// 
    /// Initializes buffers and data structures for managing message communication.
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

    /// Writes a message to a stream asynchronously
    pub async fn write_message_async<W>(&self, stream: &mut W, message: &Message) -> McpResult<()>
    where
        W: tokio::io::AsyncWrite + Unpin,
    {
        use tokio::io::AsyncWriteExt;
        
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
            // Write error code and message
            match error {
                McpError::Custom { code, message } => {
                    // Write error code
                    buffer.extend_from_slice(&{ *code }.to_be_bytes());
                    
                    // Write error message
                    let error_msg = message.as_bytes();
                    buffer.extend_from_slice(&(error_msg.len() as u32).to_be_bytes());
                    buffer.extend_from_slice(error_msg);
                },
                _ => {
                    // For other error types, use code 0 and error's Display implementation
                    buffer.extend_from_slice(&(0u32).to_be_bytes());
                    
                    let error_str = error.to_string();
                    let error_bytes = error_str.as_bytes();
                    buffer.extend_from_slice(&(error_bytes.len() as u32).to_be_bytes());
                    buffer.extend_from_slice(error_bytes);
                }
            }
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
        R: tokio::io::AsyncRead + Unpin,
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
            1 => Priority::Normal,
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
