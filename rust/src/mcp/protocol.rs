//! # MCP Protocol Implementation
//!
//! This module provides the core implementation of the Model Context Protocol (MCP),
//! handling binary serialization/deserialization, message framing, and communication channels.
//!
//! The protocol layer is responsible for:
//! - Binary encoding and decoding of MCP messages
//! - Managing message channels for pub/sub communication
//! - Tracking request/response correlations
//! - Implementing timeout mechanisms
//! - Providing metrics for protocol operations
//!
//! ## Binary Format
//!
//! MCP messages are encoded in a custom binary format:
//! 1. Message type (1 byte)
//! 2. Priority (1 byte)
//! 3. Message ID (16 bytes UUID)
//! 4. Correlation ID flag (1 byte) + Correlation ID (16 bytes, if present)
//! 5. Error flag (1 byte) + Error data (if present)
//! 6. Payload length (4 bytes) + Payload (variable length)
//!
//! ## Example
//!
//! ```rust,no_run
//! use mcp_agent::mcp::protocol::McpProtocol;
//! use mcp_agent::mcp::types::{Message, Priority};
//!
//! async fn example() {
//!     // Create a new protocol instance
//!     let protocol = McpProtocol::new();
//!
//!     // Create a message
//!     let message = Message::request(b"Hello, world!".to_vec(), Priority::Normal);
//!
//!     // Send the message through a stream (example)
//!     // let mut stream = tokio::net::TcpStream::connect("127.0.0.1:8080").await.unwrap();
//!     // protocol.write_message_async(&mut stream, &message).await.unwrap();
//! }
//! ```

use bytes::BytesMut;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::{broadcast, mpsc};
use tokio::time::Duration;
use tracing::{debug, instrument, trace_span, warn};

use crate::mcp::types::{Message, MessageId, MessageType, Priority};
use crate::telemetry;
use crate::utils::error::{McpError, McpResult};

/// Protocol implementation for MCP (Model Context Protocol).
/// Handles message serialization, deserialization, and communication.
#[derive(Debug)]
pub struct McpProtocol {
    /// Buffer for reading incoming data
    read_buffer: BytesMut,
    /// Buffer for storing message IDs
    id_buffer: BytesMut,
    /// Channel sender for outgoing messages
    /// Used to broadcast messages to all subscribers
    message_tx: Option<broadcast::Sender<Message>>,
    /// Channel for receiving Message updates
    _message_rx: Option<broadcast::Receiver<Message>>,
    /// Map of pending requests awaiting responses
    /// The key is the message ID, and the value is a oneshot channel to deliver the response
    pending_requests: Arc<Mutex<HashMap<String, mpsc::Sender<Message>>>>,
    /// Timeout duration for requests
    /// Determines how long to wait for a response before returning an error
    request_timeout: Duration,
}

impl Clone for McpProtocol {
    /// Creates a clone of the protocol instance.
    ///
    /// This allows sharing the protocol between different tasks while maintaining
    /// separate buffers but shared state for pending requests. The shared state ensures
    /// that responses are properly routed to the tasks that sent the corresponding requests.
    fn clone(&self) -> Self {
        Self {
            read_buffer: BytesMut::with_capacity(self.read_buffer.capacity()),
            id_buffer: BytesMut::with_capacity(self.id_buffer.capacity()),
            message_tx: self.message_tx.clone(),
            _message_rx: self.message_tx.as_ref().map(|tx| tx.subscribe()),
            pending_requests: self.pending_requests.clone(),
            request_timeout: self.request_timeout,
        }
    }
}

impl Default for McpProtocol {
    /// Creates a default instance of McpProtocol by calling the new() method.
    fn default() -> Self {
        Self::new()
    }
}

impl McpProtocol {
    /// Creates a new instance of the MCP Protocol handler.
    ///
    /// Initializes buffers and data structures for managing message communication.
    /// The default configuration includes:
    /// - 1KB read buffer
    /// - 16 byte ID buffer
    /// - 30 second request timeou
    /// - Empty pending requests map
    /// - No message channels (these must be initialized separately)
    pub fn new() -> Self {
        let protocol = Self {
            read_buffer: BytesMut::with_capacity(1024),
            id_buffer: BytesMut::with_capacity(16),
            message_tx: None,
            _message_rx: None,
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            request_timeout: Duration::from_secs(30),
        };

        debug!("Created new MCP Protocol instance");
        protocol
    }

    /// Writes a message to a stream asynchronously
    ///
    /// This method serializes an MCP message into its binary representation and writes i
    /// to the provided output stream. The method is instrumented with tracing to capture
    /// performance metrics and debug information.
    ///
    /// ## Message Forma
    /// The binary format consists of:
    /// 1. Message type (1 byte)
    /// 2. Priority (1 byte)
    /// 3. Message ID (16 bytes)
    /// 4. Correlation ID presence flag (1 byte) + Correlation ID (16 bytes, if present)
    /// 5. Error presence flag (1 byte) + Error details (if present)
    /// 6. Payload length (4 bytes) + Payload bytes
    ///
    /// ## Parameters
    /// - `stream`: The async writer to write the serialized message to
    /// - `message`: The message to serialize and send
    ///
    /// ## Returns
    /// - `Ok(())` if the message was written successfully
    /// - `Err(McpError)` if there was an error writing to the stream
    #[instrument(skip(self, stream, message), fields(message_type = ?message.message_type, message_id = %message.id, correlation_id = ?message.correlation_id.as_ref().map(|id| format!("{}", id)), payload_size = message.payload.len()))]
    pub async fn write_message_async<W>(&self, stream: &mut W, message: &Message) -> McpResult<()>
    where
        W: tokio::io::AsyncWrite + Unpin,
    {
        use tokio::io::AsyncWriteExt;

        let start = std::time::Instant::now();
        debug!("Writing message to stream");

        let mut buffer = BytesMut::new();

        // Write message type
        buffer.extend_from_slice(&[message.message_type as u8]);

        // Write priority
        buffer.extend_from_slice(&[message.priority as u8]);

        // Write message ID (16 bytes)
        buffer.extend_from_slice(&message.id.0);

        // Write correlation ID if presen
        let has_correlation_id = message.correlation_id.is_some();
        buffer.extend_from_slice(&[has_correlation_id as u8]);
        if let Some(correlation_id) = &message.correlation_id {
            buffer.extend_from_slice(&correlation_id.0);
        }

        // Write error if presen
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
                }
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

        // Capture serialized message size for metrics
        let message_size = buffer.len();

        // Create a span for the actual write operation
        let write_span = trace_span!("write_to_stream", bytes = message_size);
        let _write_guard = write_span.enter();

        // Write to stream
        stream.write_all(&buffer).await?;
        stream.flush().await?;

        let duration = start.elapsed();

        // Record metrics
        let mut metrics = HashMap::new();
        metrics.insert("message_write_duration_ms", duration.as_millis() as f64);
        metrics.insert("message_size_bytes", message_size as f64);
        telemetry::add_metrics(metrics);

        debug!("Message written successfully in {:?}", duration);
        Ok(())
    }

    /// Reads a message from a stream asynchronously
    ///
    /// This method reads bytes from the provided input stream and deserializes them into
    /// an MCP message. The reading process is split into two phases: header reading and
    /// body reading, each with its own tracing span for performance monitoring.
    ///
    /// ## Binary Format Parsing
    /// The method parses the binary format in this order:
    /// 1. Message type (Request, Response, Event, or KeepAlive)
    /// 2. Priority (Low, Normal, High)
    /// 3. Message ID (UUID)
    /// 4. Optional correlation ID
    /// 5. Optional error information
    /// 6. Payload data
    ///
    /// ## Parameters
    /// - `stream`: The async reader to read the serialized message from
    ///
    /// ## Returns
    /// - `Ok(Message)` containing the deserialized message
    /// - `Err(McpError)` if there was an error reading from the stream or if the message format is invalid
    #[instrument(skip(self, stream), fields(operation = "read_message"))]
    pub async fn read_message_async<R>(&self, stream: &mut R) -> McpResult<Message>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        use tokio::io::AsyncReadExt;

        let start = std::time::Instant::now();
        debug!("Reading message from stream");

        let read_span = trace_span!("read_stream_header");
        let _read_guard = read_span.enter();

        let mut message_type_buf = [0u8; 1];
        stream.read_exact(&mut message_type_buf).await?;
        let message_type = match message_type_buf[0] {
            0 => MessageType::Request,
            1 => MessageType::Response,
            2 => MessageType::Event,
            3 => MessageType::KeepAlive,
            _ => {
                warn!("Invalid message type: {}", message_type_buf[0]);
                return Err(McpError::InvalidMessage("Invalid message type".to_string()));
            }
        };

        let mut priority_buf = [0u8; 1];
        stream.read_exact(&mut priority_buf).await?;
        let priority = match priority_buf[0] {
            0 => Priority::Low,
            1 => Priority::Normal,
            2 => Priority::High,
            _ => {
                warn!("Invalid priority: {}", priority_buf[0]);
                return Err(McpError::InvalidMessage("Invalid priority".to_string()));
            }
        };

        // Read message ID
        let mut id_buf = [0u8; 16];
        stream.read_exact(&mut id_buf).await?;
        let id = MessageId(id_buf);

        // Read correlation ID if presen
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

        // Start a new span for reading the body par
        drop(_read_guard);
        let body_span = trace_span!("read_stream_body", message_id = %id, message_type = ?message_type, priority = ?priority);
        let _body_guard = body_span.enter();

        // Read error if presen
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

            let error_msg = match String::from_utf8(error_msg_buf) {
                Ok(msg) => msg,
                Err(_) => {
                    warn!("Invalid UTF-8 in error message");
                    return Err(McpError::InvalidMessage(
                        "Invalid UTF-8 in error message".to_string(),
                    ));
                }
            };

            Some(McpError::Custom {
                code: error_code,
                message: error_msg,
            })
        } else {
            None
        };

        // Read payload
        let mut payload_len_buf = [0u8; 4];
        stream.read_exact(&mut payload_len_buf).await?;
        let payload_len = u32::from_be_bytes(payload_len_buf) as usize;

        if payload_len > 0 {
            let payload_span = trace_span!("read_payload", size = payload_len);
            let _payload_guard = payload_span.enter();

            let mut payload = vec![0u8; payload_len];
            stream.read_exact(&mut payload).await?;

            let duration = start.elapsed();

            // Record metrics
            let mut metrics = HashMap::new();
            metrics.insert("message_read_duration_ms", duration.as_millis() as f64);
            metrics.insert("message_size_bytes", (payload_len + 28) as f64); // 28 bytes overhead for headers
            telemetry::add_metrics(metrics);

            let result = Message {
                message_type,
                priority,
                id,
                correlation_id,
                error,
                payload,
            };

            debug!(
                "Message read successfully in {:?}: type={:?}, id={}, correlation_id={:?}, payload_size={}",
                duration,
                result.message_type,
                result.id,
                result.correlation_id,
                result.payload.len()
            );

            Ok(result)
        } else {
            let duration = start.elapsed();

            // Record metrics
            let mut metrics = HashMap::new();
            metrics.insert("message_read_duration_ms", duration.as_millis() as f64);
            metrics.insert("message_size_bytes", 28.0); // 28 bytes overhead for headers
            telemetry::add_metrics(metrics);

            let result = Message {
                message_type,
                priority,
                id,
                correlation_id,
                error,
                payload: vec![],
            };

            debug!(
                "Message read successfully in {:?}: type={:?}, id={}, correlation_id={:?}, empty payload",
                duration, result.message_type, result.id, result.correlation_id
            );

            Ok(result)
        }
    }

    /// Process an incoming message with telemetry
    #[instrument(skip(self, message), fields(message_id = %message.id, message_type = ?message.message_type))]
    pub async fn process_message(&self, message: Message) -> McpResult<()> {
        let _span_guard = telemetry::span_duration("process_message");

        // Add processing logic here
        debug!(
            "Processing message: type={:?}, id={}",
            message.message_type, message.id
        );

        // Here you would add actual message processing logic

        Ok(())
    }

    /// Send a message with telemetry
    #[instrument(skip(self, message), fields(message_id = %message.id, message_type = ?message.message_type))]
    pub async fn send_message(&self, message: Message) -> McpResult<()> {
        let _span_guard = telemetry::span_duration("send_message");

        if let Some(tx) = &self.message_tx {
            debug!(
                "Sending message: type={:?}, id={}",
                message.message_type, message.id
            );
            tx.send(message)
                .map_err(|_| McpError::Protocol("Failed to send message".to_string()))?;
            Ok(())
        } else {
            warn!("Cannot send message: no sender channel configured");
            Err(McpError::InvalidState(
                "Message sender not initialized".to_string(),
            ))
        }
    }
}
