//! WebSocket transport implementation for MCP
//!
//! This module provides a WebSocket-based transport for the MCP protocol.

#[cfg(feature = "transport-ws")]
use crate::error::{Error, Result};
#[cfg(feature = "transport-ws")]
use crate::mcp::transport::{AsyncTransport, TransportConfig};
#[cfg(feature = "transport-ws")]
use crate::mcp::types::{
    JsonRpcBatchRequest as BatchRequest, JsonRpcBatchResponse as BatchResponse,
    JsonRpcRequest as Request, JsonRpcResponse as Response, Message, MessageType, Priority,
};
#[cfg(feature = "transport-ws")]
use async_trait::async_trait;
#[cfg(feature = "transport-ws")]
use futures::{stream::StreamExt, SinkExt};
#[cfg(feature = "transport-ws")]
use log::{debug, error, info, warn};
#[cfg(feature = "transport-ws")]
use std::collections::HashMap;
#[cfg(feature = "transport-ws")]
use std::fmt;
#[cfg(feature = "transport-ws")]
use std::sync::Arc;
#[cfg(feature = "transport-ws")]
use std::time::Duration;
#[cfg(feature = "transport-ws")]
use tokio::sync::{mpsc, oneshot, RwLock};
#[cfg(feature = "transport-ws")]
use tokio::time;
#[cfg(feature = "transport-ws")]
use tokio_tungstenite::{
    connect_async, tungstenite::Message as WsMessage, MaybeTlsStream, WebSocketStream,
};
#[cfg(feature = "transport-ws")]
use url::Url;
#[cfg(feature = "transport-ws")]
use uuid::Uuid;

/// WebSocket-based transport for the MCP protocol
///
/// This transport uses WebSockets to send and receive MCP messages
#[cfg(feature = "transport-ws")]
pub struct WebSocketTransport {
    /// Channel for sending messages to the WebSocket
    tx: mpsc::Sender<Message>,
    /// Unique ID for this transport connection
    connection_id: String,
    /// Map of pending requests waiting for responses
    pending_requests: Arc<RwLock<HashMap<String, oneshot::Sender<Response>>>>,
    /// Flag indicating if the transport is connected
    connected: Arc<std::sync::atomic::AtomicBool>,
    /// Configuration for this transport
    config: TransportConfig,
}

// Implement Debug manually since some fields don't implement Debug
#[cfg(feature = "transport-ws")]
impl fmt::Debug for WebSocketTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebSocketTransport")
            .field("connection_id", &self.connection_id)
            .field("connected", &self.connected)
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "transport-ws")]
impl WebSocketTransport {
    /// Create a new WebSocket transport
    pub async fn new(config: TransportConfig) -> Result<Self> {
        // Create a channel for outgoing messages
        let (tx, mut rx) = mpsc::channel::<Message>(100);
        let pending_requests = Arc::new(RwLock::new(HashMap::new()));
        let connected = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let connection_id = Uuid::new_v4().to_string();

        // Create a broadcast channel for distributing messages to all connections
        let (broadcast_tx, _) = tokio::sync::broadcast::channel::<Message>(100);

        let ws_url = Url::parse(&config.url)
            .map_err(|_| Error::Internal(format!("Invalid WebSocket URL: {}", config.url)))?;

        let pending_requests_clone = pending_requests.clone();
        let connected_clone = connected.clone();
        let broadcast_tx_clone = broadcast_tx.clone();

        // Forward messages from tx's rx to broadcast channel
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if broadcast_tx_clone.send(msg).is_err() {
                    // If broadcast fails, likely all receivers are gone
                    break;
                }
            }
        });

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

                        // Subscribe to the broadcast channel for this connection
                        let broadcast_rx = broadcast_tx.subscribe();

                        // Handle the WebSocket connection
                        if let Err(e) = Self::handle_connection(
                            ws_stream,
                            broadcast_rx,
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

    /// Handle a WebSocket connection
    async fn handle_connection(
        ws_stream: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
        mut broadcast_rx: tokio::sync::broadcast::Receiver<Message>,
        pending_requests: Arc<RwLock<HashMap<String, oneshot::Sender<Response>>>>,
        connected: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<()> {
        // Split the WebSocket into a sender and receiver
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Clone the connected flag for use in tasks
        let connected_send = connected.clone();

        // Task for sending messages to WebSocket
        let send_task = tokio::spawn(async move {
            while let Ok(msg) = broadcast_rx.recv().await {
                let msg_json = serde_json::to_string(&msg)
                    .map_err(|e| Error::Internal(format!("Failed to serialize message: {}", e)))
                    .unwrap();

                let ws_msg = WsMessage::Text(msg_json);
                if ws_sender.send(ws_msg).await.is_err() {
                    connected_send.store(false, std::sync::atomic::Ordering::SeqCst);
                    break;
                }
            }
        });

        // Task for receiving messages from WebSocket
        let recv_task = tokio::spawn(async move {
            while let Some(msg_result) = ws_receiver.next().await {
                match msg_result {
                    Ok(WsMessage::Text(text)) => {
                        match serde_json::from_str::<Response>(&text) {
                            Ok(response) => {
                                debug!("Received response: {:?}", response);
                                // Look up the pending request and fulfill it with the response
                                if let Some(sender) = {
                                    let mut requests = pending_requests.write().await;
                                    requests.remove(&response.id.to_string())
                                } {
                                    let _ = sender.send(response);
                                } else {
                                    warn!(
                                        "Received response for unknown request: {:?}",
                                        response.id
                                    );
                                }
                            }
                            Err(e) => {
                                error!("Failed to parse WebSocket message: {}", e);
                            }
                        }
                    }
                    _ => {}
                }
            }
            connected.store(false, std::sync::atomic::Ordering::SeqCst);
        });

        // Wait for either task to complete, then exit
        tokio::select! {
            _ = send_task => {},
            _ = recv_task => {},
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

        self.tx.send(message).await.map_err(|_| {
            Error::Internal("Failed to send message to WebSocket channel".to_string())
        })?;
        Ok(())
    }

    async fn send_request(&self, request: Request) -> Result<Response> {
        if !self.connected.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Error::Internal("WebSocket is not connected".to_string()));
        }

        // Create a channel for receiving the response
        let (resp_tx, resp_rx) = oneshot::channel::<Response>();

        // Add the request ID to the pending requests map
        let request_id = request.id.to_string();
        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(request_id.clone(), resp_tx);
        }

        // Convert the request to a Message and send it
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| Error::Internal(format!("Failed to serialize request: {}", e)))?;

        let message = Message::new(
            MessageType::Request,
            Priority::Normal,
            request_bytes,
            None,
            None,
        );

        self.tx.send(message).await.map_err(|_| {
            Error::Internal("Failed to send request to WebSocket channel".to_string())
        })?;

        // Wait for the response or timeout
        let timeout_duration = Duration::from_secs(self.config.timeout_seconds);
        match time::timeout(timeout_duration, resp_rx).await {
            Ok(result) => match result {
                Ok(response) => Ok(response),
                Err(_) => Err(Error::Internal("Response channel closed".to_string())),
            },
            Err(_) => {
                // Remove the pending request if we timed out
                let mut pending = self.pending_requests.write().await;
                pending.remove(&request_id);
                Err(Error::Timeout)
            }
        }
    }

    async fn send_batch_request(&self, batch_request: BatchRequest) -> Result<BatchResponse> {
        if !self.connected.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Error::Internal("WebSocket is not connected".to_string()));
        }

        let mut response_channels = Vec::with_capacity(batch_request.0.len());
        let mut responses = Vec::with_capacity(batch_request.0.len());

        // Create response channels for each request
        for request in &batch_request.0 {
            let (resp_tx, resp_rx) = oneshot::channel::<Response>();
            let request_id = request.id.to_string();
            {
                let mut pending = self.pending_requests.write().await;
                pending.insert(request_id, resp_tx);
            }
            response_channels.push(resp_rx);
        }

        // Convert the batch request to a Message and send it
        let batch_bytes = serde_json::to_vec(&batch_request)
            .map_err(|e| Error::Internal(format!("Failed to serialize batch request: {}", e)))?;

        let message = Message::new(
            MessageType::Request,
            Priority::Normal,
            batch_bytes,
            None,
            None,
        );

        self.tx.send(message).await.map_err(|_| {
            Error::Internal("Failed to send batch request to WebSocket channel".to_string())
        })?;

        // Wait for all responses or timeout
        let timeout_duration = Duration::from_secs(self.config.timeout_seconds);
        for rx in response_channels {
            match time::timeout(timeout_duration, rx).await {
                Ok(result) => match result {
                    Ok(response) => responses.push(response),
                    Err(_) => return Err(Error::Internal("Response channel closed".to_string())),
                },
                Err(_) => return Err(Error::Timeout),
            }
        }

        Ok(BatchResponse(responses))
    }

    async fn close(&self) -> Result<()> {
        // No explicit close needed for the channel-based implementation
        Ok(())
    }
}

// Add a placeholder when the feature isn't enabled
/// WebSocket transport implementation placeholder
///
/// This is a placeholder struct that exists when the `transport-ws` feature
/// is not enabled. For actual functionality, enable the `transport-ws` feature.
#[cfg(not(feature = "transport-ws"))]
#[derive(Debug)]
pub struct WebSocketTransport {
    _private: (),
}
