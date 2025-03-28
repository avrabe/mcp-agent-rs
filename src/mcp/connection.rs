use tokio::net::TcpStream;
use tokio::sync::{mpsc, broadcast};
use tokio::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::HashMap;
use tokio::process::Child;
use tokio::time::sleep;
use std::fmt::Debug;

use crate::utils::error::{McpError, McpResult};
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
    /// Initial state
    Initial,
    /// Connection is closed
    Closed,
}

/// Configuration options for a connection.
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// The interval between keep-alive messages.
    pub keep_alive_interval: Duration,
    /// The timeout duration for keep-alive messages.
    pub keep_alive_timeout: Duration,
    /// Maximum number of retries for failed operations
    pub max_retries: u32,
    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            keep_alive_interval: Duration::from_secs(30),
            keep_alive_timeout: Duration::from_secs(90),
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

/// A connection to a remote endpoint that handles message sending and receiving.
pub struct Connection {
    /// The remote address to connect to.
    addr: String,
    /// The underlying stream for communication
    stream: Arc<Mutex<TcpStream>>,
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
    /// Child process handle (for stdio transport)
    child: Option<Child>,
}

impl Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("addr", &self.addr)
            .field("state", &self.state)
            .field("config", &self.config)
            .field("has_child", &self.child.is_some())
            .finish()
    }
}

impl Connection {
    /// Creates a new connection with the given address and configuration.
    pub fn new(addr: String, stream: TcpStream, config: ConnectionConfig) -> Self {
        Self {
            addr,
            stream: Arc::new(Mutex::new(stream)),
            state: ConnectionState::Initial,
            protocol: McpProtocol::new(),
            config,
            keep_alive_task: None,
            keep_alive_tx: None,
            message_tx: None,
            message_rx: None,
            message_handler_task: None,
            message_sender_task: None,
            child: None,
        }
    }

    /// Connects to the remote endpoint.
    pub async fn connect(&mut self) -> McpResult<()> {
        if self.state != ConnectionState::Disconnected {
            return Err(McpError::InvalidState("Already connected".to_string()));
        }

        self.state = ConnectionState::Connecting;

        let stream = TcpStream::connect(&self.addr).await
            .map_err(|e| McpError::ConnectionFailed(format!("Failed to connect: {}", e)))?;

        self.stream = Arc::new(Mutex::new(stream));

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
    pub async fn send_message(&mut self, message: Message) -> McpResult<()> {
        if self.state != ConnectionState::Connected {
            return Err(McpError::NotConnected);
        }

        let mut retries = 0;
        while retries < self.config.max_retries {
            let mut lock = self.stream.lock().await;
            match self.protocol.write_message_async(&mut *lock, &message).await {
                Ok(_) => return Ok(()),
                Err(e) => {
                    retries += 1;
                    if retries == self.config.max_retries {
                        return Err(e);
                    }
                    sleep(Duration::from_millis(self.config.retry_delay_ms)).await;
                }
            }
        }
        Ok(())
    }

    /// Receives a message from the remote endpoint.
    pub async fn receive_message(&mut self) -> McpResult<Message> {
        if self.state != ConnectionState::Connected {
            return Err(McpError::NotConnected);
        }

        let mut retries = 0;
        while retries < self.config.max_retries {
            let mut lock = self.stream.lock().await;
            match self.protocol.read_message_async(&mut *lock).await {
                Ok(message) => return Ok(message),
                Err(e) => {
                    retries += 1;
                    if retries == self.config.max_retries {
                        return Err(e);
                    }
                    sleep(Duration::from_millis(self.config.retry_delay_ms)).await;
                }
            }
        }
        Err(McpError::Timeout)
    }

    /// Closes the connection and cleans up resources.
    pub async fn close(&mut self) -> McpResult<()> {
        if self.state == ConnectionState::Disconnected {
            return Ok(());
        }

        self.state = ConnectionState::Disconnecting;
        self.stop_keep_alive().await;

        // Stop message handler and sender
        if let Some(task) = self.message_handler_task.take() {
            task.abort();
        }
        if let Some(task) = self.message_sender_task.take() {
            task.abort();
        }

        // Close child process if using stdio transport
        if let Some(mut child) = self.child.take() {
            if let Err(e) = child.kill().await {
                eprintln!("Error killing child process: {}", e);
            }
        }

        self.message_tx = None;
        self.message_rx = None;
        self.state = ConnectionState::Disconnected;
        Ok(())
    }

    /// Starts the message handler that processes incoming messages.
    fn start_message_handler(&mut self) {
        let stream_clone = self.stream.clone();
        let mut protocol = self.protocol.clone();
        let message_tx = self.message_tx.as_ref().unwrap().clone();

        let task = tokio::spawn(async move {
            loop {
                let mut lock = stream_clone.lock().await;
                match protocol.read_message_async(&mut *lock).await {
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
        let stream_clone = self.stream.clone();
        let protocol = self.protocol.clone();
        let message_tx = self.message_tx.as_ref().unwrap().clone();

        let task = tokio::spawn(async move {
            let mut message_rx = message_tx.subscribe();
            while let Ok(message) = message_rx.recv().await {
                let mut lock = stream_clone.lock().await;
                if protocol.write_message_async(&mut *lock, &message).await.is_err() {
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
        let stream_clone = self.stream.clone();
        let protocol = self.protocol.clone();

        self.keep_alive_task = Some(tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = sleep(interval) => {
                        let keep_alive = Message::new(
                            MessageId::new("keep-alive".to_string()),
                            MessageType::Request,
                            Priority::Low,
                            Vec::new(),
                            None,
                            None,
                        );
                        let mut lock = stream_clone.lock().await;
                        if let Err(e) = protocol.write_message_async(&mut *lock, &keep_alive).await {
                            eprintln!("Error sending keep-alive: {}", e);
                        }
                    }
                    _ = rx.recv() => break,
                }
            }
        }));
    }

    /// Stops the keep-alive mechanism.
    async fn stop_keep_alive(&mut self) {
        if let Some(tx) = self.keep_alive_tx.take() {
            let _ = tx.send(()).await;
        }

        if let Some(task) = self.keep_alive_task.take() {
            task.abort();
        }
    }

    /// Connect to a server using stdio transport
    pub async fn connect_stdio(
        _command: &str,
        _args: &[String],
        _env: &HashMap<String, String>,
        _read_timeout: Duration,
    ) -> McpResult<Self> {
        // For stdio we'll need to use a different approach since we can't use TcpStream
        // In a real implementation, we would adapt this to use a proper stream
        // For now, let's just return an error
        Err(McpError::NotImplemented)
    }

    /// Connect to a server using SSE transport
    pub async fn connect_sse(url: &str, _read_timeout: Duration) -> McpResult<Self> {
        let stream = TcpStream::connect(url).await
            .map_err(|e| McpError::ConnectionFailed(format!("Failed to connect: {}", e)))?;
        
        let mut conn = Self::new(
            url.to_string(),
            stream,
            ConnectionConfig::default(),
        );

        conn.initialize().await?;
        Ok(conn)
    }

    /// Initialize the connection
    pub async fn initialize(&mut self) -> McpResult<()> {
        if self.state != ConnectionState::Initial {
            return Err(McpError::InvalidState(format!(
                "Cannot initialize connection in state: {:?}",
                self.state
            )));
        }

        // Set up message channels if not already set up
        if self.message_tx.is_none() {
            let (tx, rx) = broadcast::channel(32);
            self.message_tx = Some(tx);
            self.message_rx = Some(rx);
        }

        // Send initialization message
        let init_msg = Message::new(
            MessageId::new("init".to_string()),
            MessageType::Request,
            Priority::Normal,
            Vec::new(),
            None,
            None,
        );

        self.send_message(init_msg).await?;

        // Wait for response
        let response = self.receive_message().await?;
        if response.message_type != MessageType::Response {
            return Err(McpError::InvalidMessage("Expected response message".to_string()));
        }

        self.state = ConnectionState::Connected;
        self.start_keep_alive();
        Ok(())
    }

    /// Check if the connection is connected
    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if self.state != ConnectionState::Closed {
            let _ = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(self.close());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio::time::timeout;

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    // Create a mock server for testing
    async fn setup_mock_server() -> std::io::Result<String> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        
        // Spawn a task to handle connections
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    // Read the init message
                    let mut buf = [0u8; 1024];
                    if let Ok(_) = socket.read(&mut buf).await {
                        // Send a response
                        let response = Message::new(
                            MessageId::new("response".to_string()),
                            MessageType::Response,
                            Priority::Normal,
                            Vec::new(),
                            None,
                            None,
                        );
                        let mut protocol = McpProtocol::new();
                        let _ = protocol.write_message_async(&mut socket, &response).await;
                    }
                });
            }
        });
        
        Ok(addr.to_string())
    }

    #[tokio::test]
    async fn test_connection_lifecycle() -> McpResult<()> {
        let addr = setup_mock_server().await.unwrap();
        let config = ConnectionConfig::default();
        let stream = TcpStream::connect(&addr).await
            .map_err(|e| McpError::ConnectionFailed(e.to_string()))?;
        let mut conn = Connection::new(
            addr,
            stream,
            config,
        );
        
        timeout(TEST_TIMEOUT, conn.initialize()).await
            .map_err(|_| McpError::Timeout)?;
        assert_eq!(conn.state, ConnectionState::Connected);
        
        timeout(TEST_TIMEOUT, conn.close()).await
            .map_err(|_| McpError::Timeout)?;
        assert_eq!(conn.state, ConnectionState::Disconnected);
        Ok(())
    }

    #[tokio::test]
    async fn test_keep_alive() -> McpResult<()> {
        let addr = setup_mock_server().await.unwrap();
        let config = ConnectionConfig::default();
        let stream = TcpStream::connect(&addr).await
            .map_err(|e| McpError::ConnectionFailed(e.to_string()))?;
        let mut conn = Connection::new(
            addr,
            stream,
            config,
        );
        
        timeout(TEST_TIMEOUT, conn.initialize()).await
            .map_err(|_| McpError::Timeout)?;
        
        let keep_alive = Message::new(
            MessageId::new("keep-alive".to_string()),
            MessageType::Request,
            Priority::Low,
            Vec::new(),
            None,
            None,
        );
        
        timeout(TEST_TIMEOUT, conn.send_message(keep_alive)).await
            .map_err(|_| McpError::Timeout)?;
        Ok(())
    }

    #[tokio::test]
    async fn test_message_send_receive() -> McpResult<()> {
        let addr = setup_mock_server().await.unwrap();
        let config = ConnectionConfig::default();
        let stream = TcpStream::connect(&addr).await
            .map_err(|e| McpError::ConnectionFailed(e.to_string()))?;
        let mut conn = Connection::new(
            addr,
            stream,
            config,
        );
        
        timeout(TEST_TIMEOUT, conn.initialize()).await
            .map_err(|_| McpError::Timeout)?;
        
        let request = Message::new(
            MessageId::new("test-id".to_string()),
            MessageType::Request,
            Priority::Normal,
            Vec::new(),
            None,
            None,
        );
        
        timeout(TEST_TIMEOUT, conn.send_message(request.clone())).await
            .map_err(|_| McpError::Timeout)?;
        
        let response = timeout(TEST_TIMEOUT, conn.receive_message()).await
            .map_err(|_| McpError::Timeout)??;
        assert_eq!(response.id.to_string(), "response");
        assert_eq!(response.message_type, MessageType::Response);
        Ok(())
    }
} 