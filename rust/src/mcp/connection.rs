use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, broadcast};
use tokio::time::Duration;
use tokio::io::AsyncWriteExt;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::mcp::error::{McpError, RetryConfig, CircuitBreaker};
use crate::mcp::protocol::McpProtocol;
use crate::mcp::types::{Message, MessageType, Priority, MessageId};

/// Represents the current state of a connection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConnectionState {
    /// The connection is disconnected and not attempting to connect.
    Disconnected,
    /// The connection is in the process of connecting.
    Connecting,
    /// The connection is established and ready for communication.
    Connected,
    /// The connection is in the process of disconnecting.
    Disconnecting,
}

/// Configuration options for a connection.
#[derive(Debug)]
pub struct ConnectionConfig {
    /// The interval between keep-alive messages.
    pub keep_alive_interval: Duration,
    /// The timeout duration for keep-alive messages.
    pub keep_alive_timeout: Duration,
    /// Configuration for retry behavior.
    pub retry_config: RetryConfig,
    /// Circuit breaker for managing connection failures.
    pub circuit_breaker: CircuitBreaker,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            keep_alive_interval: Duration::from_secs(30),
            keep_alive_timeout: Duration::from_secs(90),
            retry_config: RetryConfig::default(),
            circuit_breaker: CircuitBreaker::new(3, Duration::from_secs(60)),
        }
    }
}

/// A connection to a remote endpoint that handles message sending and receiving.
#[derive(Debug)]
pub struct Connection {
    /// The remote address to connect to.
    addr: SocketAddr,
    /// The underlying TCP stream, wrapped in Arc<Mutex> for safe sharing.
    stream: Option<Arc<Mutex<TcpStream>>>,
    /// The current state of the connection.
    state: ConnectionState,
    /// The protocol handler for message serialization/deserialization.
    protocol: McpProtocol,
    /// Configuration options for the connection.
    config: ConnectionConfig,
    /// Task handle for the keep-alive mechanism.
    keep_alive_task: Option<tokio::task::JoinHandle<()>>,
    /// Channel sender for stopping the keep-alive task.
    keep_alive_tx: Option<mpsc::Sender<()>>,
    /// Channel sender for outgoing messages.
    message_tx: Option<broadcast::Sender<Message>>,
    /// Channel receiver for incoming messages.
    message_rx: Option<broadcast::Receiver<Message>>,
    /// Task handle for the message handler.
    message_handler_task: Option<tokio::task::JoinHandle<()>>,
    /// Task handle for the message sender.
    message_sender_task: Option<tokio::task::JoinHandle<()>>,
}

impl Connection {
    /// Creates a new connection with the given address and configuration.
    pub fn new(addr: SocketAddr, config: ConnectionConfig) -> Self {
        Self {
            addr,
            stream: None,
            state: ConnectionState::Disconnected,
            protocol: McpProtocol::new(),
            config,
            keep_alive_task: None,
            keep_alive_tx: None,
            message_tx: None,
            message_rx: None,
            message_handler_task: None,
            message_sender_task: None,
        }
    }

    /// Connects to the remote endpoint.
    pub async fn connect(&mut self) -> Result<(), McpError> {
        if self.state != ConnectionState::Disconnected {
            return Err(McpError::Protocol("Already connected".to_string()));
        }

        self.state = ConnectionState::Connecting;

        let stream = TcpStream::connect(self.addr).await
            .map_err(|e| McpError::Protocol(format!("Failed to connect: {}", e)))?;

        self.stream = Some(Arc::new(Mutex::new(stream)));

        // Set up message channels
        let (tx, rx) = broadcast::channel(32);
        self.message_tx = Some(tx);
        self.message_rx = Some(rx);

        // Start keep-alive, message handler, and message sender
        self.start_keep_alive();
        self.start_message_handler();
        self.start_message_sender();

        self.state = ConnectionState::Connected;
        Ok(())
    }

    /// Sends a message to the remote endpoint.
    pub async fn send_message(&mut self, message: Message) -> Result<(), McpError> {
        if self.state != ConnectionState::Connected {
            return Err(McpError::Protocol("Not connected".to_string()));
        }

        if let Some(stream) = &self.stream {
            let mut stream = stream.lock().await;
            if self.protocol.write_message_async(&mut *stream, &message).await.is_err() {
                return Err(McpError::Protocol("Failed to write message".to_string()));
            }
            if stream.flush().await.is_err() {
                return Err(McpError::Protocol("Failed to flush stream".to_string()));
            }
            Ok(())
        } else {
            Err(McpError::Protocol("Stream not initialized".to_string()))
        }
    }

    /// Receives a message from the remote endpoint.
    pub async fn receive_message(&mut self) -> Result<Message, McpError> {
        if self.state != ConnectionState::Connected {
            return Err(McpError::Protocol("Not connected".to_string()));
        }

        if let Some(stream) = &self.stream {
            let mut stream = stream.lock().await;
            match self.protocol.read_message_async(&mut *stream).await {
                Ok(message) => Ok(message),
                Err(e) => Err(McpError::Protocol(format!("Failed to read message: {}", e))),
            }
        } else {
            Err(McpError::Protocol("Stream not initialized".to_string()))
        }
    }

    /// Closes the connection and cleans up resources.
    pub async fn close(&mut self) -> Result<(), McpError> {
        if self.state == ConnectionState::Disconnected {
            return Ok(());
        }

        self.state = ConnectionState::Disconnecting;
        self.stop_keep_alive();

        // Stop message handler and sender
        if let Some(task) = self.message_handler_task.take() {
            task.abort();
        }
        if let Some(task) = self.message_sender_task.take() {
            task.abort();
        }

        if let Some(stream) = self.stream.take() {
            let mut stream = stream.lock().await;
            stream.shutdown().await.map_err(|e| McpError::Protocol(format!("Failed to shutdown: {}", e)))?;
        }

        self.message_tx = None;
        self.message_rx = None;
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    /// Starts the message handler that processes incoming messages.
    fn start_message_handler(&mut self) {
        let stream = self.stream.as_ref().unwrap().clone();
        let mut protocol = self.protocol.clone();
        let message_tx = self.message_tx.as_ref().unwrap().clone();

        let task = tokio::spawn(async move {
            loop {
                let mut stream = stream.lock().await;
                match protocol.read_message_async(&mut *stream).await {
                    Ok(message) => {
                        if message_tx.send(message).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        self.message_handler_task = Some(task);
    }

    /// Starts the message sender that processes outgoing messages.
    fn start_message_sender(&mut self) {
        let stream = self.stream.as_ref().unwrap().clone();
        let protocol = self.protocol.clone();
        let message_tx = self.message_tx.as_ref().unwrap().clone();

        let task = tokio::spawn(async move {
            let mut message_rx = message_tx.subscribe();
            while let Ok(message) = message_rx.recv().await {
                let mut stream = stream.lock().await;
                if protocol.write_message_async(&mut *stream, &message).await.is_err() {
                    break;
                }
                if stream.flush().await.is_err() {
                    break;
                }
            }
        });

        self.message_sender_task = Some(task);
    }

    /// Starts the keep-alive mechanism that periodically sends keep-alive messages.
    fn start_keep_alive(&mut self) {
        let (tx, mut rx) = mpsc::channel(1);
        self.keep_alive_tx = Some(tx);

        let interval = self.config.keep_alive_interval;
        let protocol = self.protocol.clone();
        let stream = self.stream.as_ref().unwrap().clone();

        let task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);
            interval.tick().await; // Skip the first immediate tick
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let keep_alive = Message::new(
                            MessageId::new("keep-alive".to_string()),
                            MessageType::KeepAlive,
                            Priority::Normal,
                            Vec::new(),
                            None,
                            None,
                        );
                        let mut stream = stream.lock().await;
                        if protocol.write_message_async(&mut *stream, &keep_alive).await.is_ok() {
                            let _ = stream.flush().await;
                        }
                    }
                    _ = rx.recv() => break,
                }
            }
        });

        self.keep_alive_task = Some(task);
    }

    /// Stops the keep-alive mechanism.
    fn stop_keep_alive(&mut self) {
        if let Some(tx) = self.keep_alive_tx.take() {
            let _ = tx.send(());
        }

        if let Some(task) = self.keep_alive_task.take() {
            task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio::time::{sleep, timeout};

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    #[tokio::test]
    async fn test_connection_lifecycle() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let config = ConnectionConfig::default();
        let mut connection = Connection::new(addr, config);

        // Start accepting connections in the background
        let accept_task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let protocol = McpProtocol::new();
            let message = Message::new(
                MessageId::new("test-id".to_string()),
                MessageType::Request,
                Priority::Normal,
                b"test payload".to_vec(),
                None,
                None,
            );
            protocol.write_message_async(&mut stream, &message).await.unwrap();
            stream.flush().await.unwrap();
        });

        // Connect and receive message
        timeout(TEST_TIMEOUT, async {
            connection.connect().await.unwrap();
            let message = connection.receive_message().await.unwrap();
            assert_eq!(message.id.as_str(), "test-id");
            assert_eq!(message.message_type, MessageType::Request);
            assert_eq!(message.priority, Priority::Normal);
            assert_eq!(message.payload, b"test payload");

            // Clean up
            connection.close().await.unwrap();
            accept_task.abort();
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_keep_alive() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let mut config = ConnectionConfig::default();
        config.keep_alive_interval = Duration::from_millis(100);
        let mut connection = Connection::new(addr, config);

        // Start accepting connections in the background
        let accept_task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut protocol = McpProtocol::new();
            
            // Wait for and verify multiple keep-alive messages
            for _ in 0..2 {
                let message = protocol.read_message_async(&mut stream).await.unwrap();
                assert_eq!(message.message_type, MessageType::KeepAlive);
            }
            stream.shutdown().await.unwrap();
        });

        // Connect and wait for keep-alive
        timeout(TEST_TIMEOUT, async {
            connection.connect().await.unwrap();
            // Wait for at least 2 keep-alive intervals
            sleep(Duration::from_millis(250)).await;

            // Clean up
            connection.close().await.unwrap();
            accept_task.abort();
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_message_send_receive() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let config = ConnectionConfig::default();
        let mut connection = Connection::new(addr, config);

        // Start accepting connections in the background
        let accept_task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut protocol = McpProtocol::new();

            // Receive request
            let request = protocol.read_message_async(&mut stream).await.unwrap();
            assert_eq!(request.id.as_str(), "test-id");
            assert_eq!(request.message_type, MessageType::Request);
            assert_eq!(request.priority, Priority::Normal);
            assert_eq!(request.payload, b"test payload");

            // Send response
            let response = Message::new(
                MessageId::new("response-id".to_string()),
                MessageType::Response,
                Priority::Normal,
                b"response payload".to_vec(),
                Some(request.id),
                None,
            );
            protocol.write_message_async(&mut stream, &response).await.unwrap();
            stream.flush().await.unwrap();
            stream.shutdown().await.unwrap();
        });

        // Connect and send message
        timeout(TEST_TIMEOUT, async {
            connection.connect().await.unwrap();

            // Send request
            let request = Message::new(
                MessageId::new("test-id".to_string()),
                MessageType::Request,
                Priority::Normal,
                b"test payload".to_vec(),
                None,
                None,
            );
            connection.send_message(request).await.unwrap();

            // Receive response
            let response = connection.receive_message().await.unwrap();
            assert_eq!(response.id.as_str(), "response-id");
            assert_eq!(response.message_type, MessageType::Response);
            assert_eq!(response.priority, Priority::Normal);
            assert_eq!(response.payload, b"response payload");
            assert_eq!(response.correlation_id.as_ref().unwrap().as_str(), "test-id");

            // Clean up
            connection.close().await.unwrap();
            accept_task.abort();
        })
        .await
        .unwrap();
    }
} 