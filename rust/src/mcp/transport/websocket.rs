//! WebSocket transport implementation for the MCP protocol.
//!
//! This module provides a WebSocket-based transport for the MCP protocol using
//! the tokio-tungstenite library for WebSocket support.

#[cfg(feature = "transport-ws")]
use futures_util::{SinkExt, StreamExt};
#[cfg(feature = "transport-ws")]
use tracing::{debug, error, info, warn};
#[cfg(feature = "transport-ws")]
use std::collections::HashMap;
#[cfg(feature = "transport-ws")]
use std::sync::Arc;
#[cfg(not(feature = "transport-ws"))]
use std::sync::Arc;
#[cfg(feature = "transport-ws")]
use std::time::Duration;
#[cfg(feature = "transport-ws")]
use tokio::net::TcpStream;
#[cfg(feature = "transport-ws")]
use tokio::sync::{mpsc, Mutex, RwLock};
#[cfg(feature = "transport-ws")]
use tokio::time;
#[cfg(feature = "transport-ws")]
use tokio_tungstenite::{
    connect_async, tungstenite::protocol::Message as WsMessage, MaybeTlsStream, WebSocketStream,
};
#[cfg(feature = "transport-ws")]
use url::Url;
#[cfg(feature = "transport-ws")]
use uuid::Uuid;

use super::{Transport, TransportConfig, TransportFactory};
use crate::error::{Error, Result};

#[cfg(feature = "transport-ws")]
type WebSocketClientType = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// WebSocket transport for MCP protocol
#[cfg(feature = "transport-ws")]
pub struct WebSocketTransport {
    /// Channel for sending messages to the outgoing WebSocket stream
    tx: mpsc::Sender<Message>,
    /// Connection ID
    connection_id: String,
    /// Pending requests map
    pending_requests: Arc<RwLock<HashMap<String, mpsc::Sender<Response>>>>,
    /// Connection status flag
    connected: Arc<std::sync::atomic::AtomicBool>,
    /// Configuration
    config: TransportConfig,
}

#[cfg(feature = "transport-ws")]
impl WebSocketTransport {
    /// Create a new WebSocket transport
    pub async fn new(config: TransportConfig) -> Result<Self> {
        let (tx, mut rx) = mpsc::channel::<Message>(100);
        let pending_requests = Arc::new(RwLock::new(HashMap::new()));
        let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let connection_id = Uuid::new_v4().to_string();

        let ws_url = Url::parse(&config.url)
            .map_err(|_| Error::Internal(format!("Invalid WebSocket URL: {}", config.url)))?;

        let pending_requests_clone = pending_requests.clone();
        let connected_clone = connected.clone();
        let config_clone = config.clone();

        // Spawn background task for managing the WebSocket connection
        tokio::spawn(async move {
            let mut reconnect_attempts = 0;
            // Default reconnection parameters if not in config
            let max_reconnect_attempts = 5;
            let reconnect_delay_ms = 1000;

            'connection_loop: loop {
                info!("Connecting to WebSocket at {}", ws_url);

                match connect_async(&ws_url).await {
                    Ok((ws_stream, _)) => {
                        info!("WebSocket connected to {}", ws_url);
                        connected_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                        reconnect_attempts = 0;

                        // Handle the WebSocket connection
                        if let Err(e) = WebSocketTransport::handle_connection(
                            ws_stream,
                            &mut rx,
                            pending_requests_clone.clone(),
                            connected_clone.clone(),
                        )
                        .await
                        {
                            error!("WebSocket connection error: {}", e);
                        }

                        connected_clone.store(false, std::sync::atomic::Ordering::SeqCst);
                    }
                    Err(e) => {
                        error!("Failed to connect to WebSocket: {}", e);
                        connected_clone.store(false, std::sync::atomic::Ordering::SeqCst);
                    }
                }

                // Attempt to reconnect with backoff
                reconnect_attempts += 1;
                if reconnect_attempts > max_reconnect_attempts {
                    error!("Maximum reconnection attempts reached, giving up.");
                    break 'connection_loop;
                }

                let delay = reconnect_delay_ms * reconnect_attempts as u64;
                warn!(
                    "Reconnecting in {} ms (attempt {}/{})",
                    delay, reconnect_attempts, max_reconnect_attempts
                );
                time::sleep(Duration::from_millis(delay)).await;
            }
        });

        Ok(Self {
            tx,
            connection_id,
            pending_requests,
            connected,
            config,
        })
    }

    /// Handle the WebSocket connection
    async fn handle_connection(
        ws_stream: WebSocketClientType,
        rx: &mut mpsc::Receiver<Message>,
        pending_requests: Arc<RwLock<HashMap<String, mpsc::Sender<Response>>>>,
        connected: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<(), Error> {
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Task for sending messages to the WebSocket
        let send_task = tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let msg_json = serde_json::to_string(&message)
                    .map_err(|e| Error::Internal(format!("Failed to serialize message: {}", e)))?;

                debug!("Sending message: {}", msg_json);

                if let Err(e) = ws_sender.send(WsMessage::Text(msg_json)).await {
                    error!("Error sending WebSocket message: {}", e);
                    connected.store(false, std::sync::atomic::Ordering::SeqCst);
                    return Err(Error::Internal(format!("WebSocket send error: {}", e)));
                }
            }

            Ok::<_, Error>(())
        });

        // Task for receiving messages from the WebSocket
        let receive_task = tokio::spawn(async move {
            while let Some(msg_result) = ws_receiver.next().await {
                match msg_result {
                    Ok(msg) => {
                        if let WsMessage::Text(text) = msg {
                            debug!("Received message: {}", text);

                            match serde_json::from_str::<Message>(&text) {
                                Ok(message) => {
                                    // Process the message based on type
                                    // For now, we'll just handle responses
                                    if let Some(correlation_id) = &message.correlation_id {
                                        let id = correlation_id.to_string();

                                        // Check if we have a pending request for this response
                                        let mut pending = pending_requests.write().await;
                                        if let Some(tx) = pending.remove(&id) {
                                            // Convert Message to Response and send it
                                            if let Ok(response) = Response::from_message(&message) {
                                                if let Err(e) = tx.send(response).await {
                                                    error!(
                                                        "Failed to send response to waiting request: {}",
                                                        e
                                                    );
                                                }
                                            } else {
                                                error!("Failed to convert message to response");
                                            }
                                        } else {
                                            warn!("Received response for unknown request: {}", id);
                                        }
                                    } else {
                                        debug!("Received non-response message");
                                        // Handle other message types if needed
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to deserialize message: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error receiving WebSocket message: {}", e);
                        connected.store(false, std::sync::atomic::Ordering::SeqCst);
                        return Err(Error::Internal(format!("WebSocket receive error: {}", e)));
                    }
                }
            }

            Ok::<_, Error>(())
        });

        // Wait for either task to complete
        tokio::select! {
            res = send_task => {
                if let Err(e) = res {
                    error!("WebSocket send task panicked: {}", e);
                } else if let Ok(Err(e)) = res {
                    error!("WebSocket send task error: {}", e);
                }
            },
            res = receive_task => {
                if let Err(e) = res {
                    error!("WebSocket receive task panicked: {}", e);
                } else if let Ok(Err(e)) = res {
                    error!("WebSocket receive task error: {}", e);
                }
            }
        }

        Ok(())
    }
}

#[cfg(feature = "transport-ws")]
#[async_trait]
impl AsyncTransport for WebSocketTransport {
    async fn send_message(&self, message: Message) -> Result<()> {
        if !self.connected.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Error::Internal("WebSocket is not connected".to_string()));
        }

        // Send message through the channel to the WebSocket handler
        self.tx
            .send(message)
            .await
            .map_err(|_| Error::Internal("Failed to send message to WebSocket handler".to_string()))?;

        Ok(())
    }

    async fn send_request(&self, request: Request) -> Result<Response> {
        if !self.connected.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Error::Internal("WebSocket is not connected".to_string()));
        }

        // Convert Request to Message
        let message = request.clone().into_message();

        // Create oneshot channel for the response
        let (resp_tx, resp_rx) = mpsc::channel::<Response>(1);

        // Register the request in the pending requests map
        let req_id = request.id().to_string();
        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(req_id.clone(), resp_tx);
        }

        // Send the request message
        self.tx
            .send(message)
            .await
            .map_err(|_| Error::Internal("Failed to send request to WebSocket handler".to_string()))?;

        // Wait for the response with timeout
        let timeout = Duration::from_secs(self.config.timeout_seconds);
        match tokio::time::timeout(timeout, resp_rx.recv()).await {
            Ok(Some(response)) => {
                // Response received
                Ok(response)
            }
            Ok(None) => {
                // Channel closed without response
                Err(Error::Internal("Response channel closed unexpectedly".to_string()))
            }
            Err(_) => {
                // Timeout
                // Remove the pending request
                let mut pending = self.pending_requests.write().await;
                pending.remove(&req_id);

                Err(Error::Timeout)
            }
        }
    }

    async fn close(&self) -> Result<()> {
        // There's no need to explicitly close, the WebSocket will be closed
        // when the struct is dropped and the tx channel is closed
        Ok(())
    }

    async fn send_batch_request(&self, batch_request: BatchRequest) -> Result<BatchResponse> {
        if !self.connected.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Error::Internal("WebSocket is not connected".to_string()));
        }

        // For each request in the batch, create a oneshot channel
        let mut response_channels = HashMap::new();
        let mut batch_ids = Vec::new();

        // Register all the requests in the pending requests map
        {
            let mut pending = self.pending_requests.write().await;
            for request in batch_request.requests() {
                let req_id = request.id().to_string();
                batch_ids.push(req_id.clone());

                let (resp_tx, resp_rx) = mpsc::channel::<Response>(1);
                pending.insert(req_id.clone(), resp_tx);
                response_channels.insert(req_id, resp_rx);
            }
        }

        // Serialize the batch request
        let message_payload = serde_json::to_vec(&batch_request)
            .map_err(|e| Error::Internal(format!("Failed to serialize batch request: {}", e)))?;

        // Create and send a message with the batch payload
        let message = Message::new(
            MessageType::BatchRequest,
            Priority::Normal,
            message_payload,
            None,
            None,
        );

        self.tx
            .send(message)
            .await
            .map_err(|_| Error::Internal("Failed to send batch request".to_string()))?;

        // Wait for all responses with timeout
        let timeout = Duration::from_secs(self.config.timeout_seconds);
        let mut responses = Vec::new();

        for req_id in batch_ids {
            if let Some(rx) = response_channels.get(&req_id) {
                match tokio::time::timeout(timeout, rx.recv()).await {
                    Ok(Some(response)) => {
                        responses.push(response);
                    }
                    Ok(None) | Err(_) => {
                        // Channel closed or timeout
                        let mut pending = self.pending_requests.write().await;
                        pending.remove(&req_id);
                    }
                }
            }
        }

        // Create batch response from the responses
        Ok(BatchResponse::new(responses))
    }
}

#[cfg(feature = "transport-ws")]
impl Transport for WebSocketTransport {
    fn send_message_boxed(
        &self,
        message: Message,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        Box::pin(self.send_message(message))
    }

    fn send_request_boxed(
        &self,
        request: Request,
    ) -> Box<dyn std::future::Future<Output = Result<Response>> + Send + Unpin + '_> {
        Box::pin(self.send_request(request))
    }

    fn send_batch_request_boxed(
        &self,
        batch_request: BatchRequest,
    ) -> Box<dyn std::future::Future<Output = Result<BatchResponse>> + Send + Unpin + '_> {
        Box::pin(self.send_batch_request(batch_request))
    }

    fn close_boxed(&self) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        Box::pin(self.close())
    }
}

/// Factory for creating WebSocket transport instances
#[cfg(feature = "transport-ws")]
#[derive(Debug)]
pub struct WebSocketTransportFactory {
    config: TransportConfig,
}

#[cfg(feature = "transport-ws")]
impl WebSocketTransportFactory {
    /// Create a new WebSocket transport factory
    ///
    /// # Arguments
    /// * `config` - The transport configuration to use for created clients.
    pub fn new(config: TransportConfig) -> Self {
        Self { config }
    }
}

#[cfg(feature = "transport-ws")]
impl TransportFactory for WebSocketTransportFactory {
    fn create(&self) -> Result<Arc<dyn Transport>> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| Error::Internal(format!("Failed to create Tokio runtime: {}", e)))?;
        
        let transport = rt.block_on(async {
            WebSocketTransport::new(self.config.clone()).await
        })?;
        
        Ok(Arc::new(transport))
    }
}

/// Empty implementation when the transport-ws feature is not enabled
#[cfg(not(feature = "transport-ws"))]
#[derive(Debug)]
pub struct WebSocketTransportFactory {
    _config: TransportConfig,
}

#[cfg(not(feature = "transport-ws"))]
impl WebSocketTransportFactory {
    /// Creates a placeholder WebSocket transport factory when the feature is disabled
    ///
    /// # Arguments
    /// * `config` - The transport configuration (not used when feature is disabled)
    pub fn new(config: TransportConfig) -> Self {
        Self { _config: config }
    }
}

#[cfg(not(feature = "transport-ws"))]
impl TransportFactory for WebSocketTransportFactory {
    fn create(&self) -> Result<Arc<dyn Transport>> {
        Err(Error::Internal(
            "WebSocket transport is not enabled. Enable the 'transport-ws' feature.".to_string(),
        ))
    }
}
