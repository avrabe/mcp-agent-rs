//! Server implementation for the MCP protocol transport layer.
//!
//! This module provides server implementations for different transport mechanisms
//! including WebSocket and HTTP.

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use log::{debug, error, info, warn};

use crate::error::Error;
use crate::mcp::types::Message;

/// Handler for processing incoming messages
pub trait MessageHandler: Send + Sync + 'static {
    /// Process an incoming message
    fn process_message(&self, message: Message) -> tokio::task::JoinHandle<Result<Option<Message>, Error>>;
}

/// State for the transport server
pub struct TransportServerState {
    /// Message handler
    message_handler: Arc<dyn MessageHandler>,
}

impl TransportServerState {
    /// Create a new transport server state
    pub fn new(message_handler: Arc<dyn MessageHandler>) -> Self {
        Self { message_handler }
    }
}

/// Configuration for the transport server
#[derive(Debug, Clone)]
pub struct TransportServerConfig {
    /// WebSocket server address
    pub ws_addr: Option<SocketAddr>,
    /// HTTP server address
    pub http_addr: Option<SocketAddr>,
    /// Enable TLS
    pub use_tls: bool,
    /// TLS certificate path
    pub tls_cert_path: Option<String>,
    /// TLS key path
    pub tls_key_path: Option<String>,
}

impl Default for TransportServerConfig {
    fn default() -> Self {
        Self {
            ws_addr: Some("127.0.0.1:8080".parse().unwrap()),
            http_addr: Some("127.0.0.1:8081".parse().unwrap()),
            use_tls: false,
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

/// Server implementation for MCP protocol transports
pub struct TransportServer {
    /// Server state
    state: Arc<TransportServerState>,
    /// Server configuration
    config: TransportServerConfig,
}

impl TransportServer {
    /// Create a new transport server
    pub fn new(message_handler: Arc<dyn MessageHandler>, config: TransportServerConfig) -> Self {
        let state = Arc::new(TransportServerState::new(message_handler));
        Self { state, config }
    }

    /// Run the transport server with the configured transports
    pub async fn run(&self) -> Result<(), Error> {
        let mut handles = Vec::new();

        // Start WebSocket server if configured
        if let Some(addr) = self.config.ws_addr {
            info!("Starting WebSocket server on {}", addr);
            let state = self.state.clone();
            let config = self.config.clone();
            
            let handle = tokio::spawn(async move {
                if let Err(e) = Self::run_websocket_server(state, addr, config.use_tls, config.tls_cert_path, config.tls_key_path).await {
                    error!("WebSocket server error: {}", e);
                }
            });
            
            handles.push(handle);
        }

        // Start HTTP server if configured
        if let Some(addr) = self.config.http_addr {
            info!("Starting HTTP server on {}", addr);
            let state = self.state.clone();
            let config = self.config.clone();
            
            let handle = tokio::spawn(async move {
                if let Err(e) = Self::run_http_server(state, addr, config.use_tls, config.tls_cert_path, config.tls_key_path).await {
                    error!("HTTP server error: {}", e);
                }
            });
            
            handles.push(handle);
        }

        // Wait for all servers to complete
        for handle in handles {
            handle.await.map_err(|e| Error::Internal(format!("Server task failed: {}", e)))?;
        }

        Ok(())
    }

    /// Run a WebSocket server
    #[cfg(feature = "transport-ws")]
    async fn run_websocket_server(
        state: Arc<TransportServerState>,
        addr: SocketAddr,
        use_tls: bool,
        tls_cert_path: Option<String>,
        tls_key_path: Option<String>,
    ) -> Result<(), Error> {
        use axum::{
            extract::ws::{Message as AxumWsMessage, WebSocket, WebSocketUpgrade},
            extract::State,
            routing::get,
            Router,
        };
        
        // Create router with WebSocket route
        let app = Router::new()
            .route("/ws", get(Self::ws_handler))
            .with_state(state);
        
        // Start server with or without TLS
        if use_tls {
            if let (Some(cert_path), Some(key_path)) = (tls_cert_path, tls_key_path) {
                // Load TLS certificate and key
                let config = Self::create_tls_config(&cert_path, &key_path)?;
                
                // Start TLS server
                axum::Server::bind(&addr)
                    .tls_config(config)
                    .map_err(|e| Error::Internal(format!("Failed to configure TLS: {}", e)))?
                    .serve(app.into_make_service())
                    .await
                    .map_err(|e| Error::Internal(format!("WebSocket server error: {}", e)))?;
            } else {
                return Err(Error::Internal("TLS enabled but certificate or key path not provided".to_string()));
            }
        } else {
            // Start plain HTTP server
            axum::Server::bind(&addr)
                .serve(app.into_make_service())
                .await
                .map_err(|e| Error::Internal(format!("WebSocket server error: {}", e)))?;
        }
        
        Ok(())
    }
    
    /// WebSocket handler for incoming connections
    #[cfg(feature = "transport-ws")]
    async fn ws_handler(
        ws: WebSocketUpgrade,
        State(state): State<Arc<TransportServerState>>,
    ) -> impl axum::response::IntoResponse {
        ws.on_upgrade(move |socket| Self::handle_socket(socket, state))
    }
    
    /// Handle a WebSocket connection
    #[cfg(feature = "transport-ws")]
    async fn handle_socket(mut socket: WebSocket, state: Arc<TransportServerState>) {
        use futures_util::{SinkExt, StreamExt};
        use axum::extract::ws::Message as AxumWsMessage;
        
        while let Some(msg) = socket.recv().await {
            match msg {
                Ok(AxumWsMessage::Text(text)) => {
                    debug!("Received message: {}", text);
                    
                    // Parse message
                    match serde_json::from_str::<Message>(&text) {
                        Ok(message) => {
                            // Process message
                            let handler = state.message_handler.clone();
                            let handle = handler.process_message(message);
                            
                            // Wait for processing to complete and send response if needed
                            match handle.await {
                                Ok(Ok(Some(response))) => {
                                    // Send response
                                    match serde_json::to_string(&response) {
                                        Ok(response_text) => {
                                            if let Err(e) = socket.send(AxumWsMessage::Text(response_text)).await {
                                                error!("Failed to send response: {}", e);
                                            }
                                        },
                                        Err(e) => {
                                            error!("Failed to serialize response: {}", e);
                                        }
                                    }
                                },
                                Ok(Ok(None)) => {
                                    // No response needed
                                },
                                Ok(Err(e)) => {
                                    error!("Error processing message: {}", e);
                                },
                                Err(e) => {
                                    error!("Message handler task failed: {}", e);
                                }
                            }
                        },
                        Err(e) => {
                            error!("Failed to parse message: {}", e);
                        }
                    }
                },
                Ok(AxumWsMessage::Close(_)) => {
                    debug!("WebSocket connection closed");
                    break;
                },
                Ok(_) => {
                    // Ignore other message types
                },
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }
    }
    
    /// Provide a stub implementation when transport-ws is not enabled
    #[cfg(not(feature = "transport-ws"))]
    async fn run_websocket_server(
        _state: Arc<TransportServerState>,
        _addr: SocketAddr,
        _use_tls: bool,
        _tls_cert_path: Option<String>,
        _tls_key_path: Option<String>,
    ) -> Result<(), Error> {
        Err(Error::Internal("WebSocket server is not enabled. Enable the 'transport-ws' feature.".to_string()))
    }

    /// Run an HTTP server
    #[cfg(feature = "transport-http")]
    async fn run_http_server(
        state: Arc<TransportServerState>,
        addr: SocketAddr,
        use_tls: bool,
        tls_cert_path: Option<String>,
        tls_key_path: Option<String>,
    ) -> Result<(), Error> {
        use axum::{
            routing::{post, get},
            Router,
            http::StatusCode,
            response::IntoResponse,
            Json,
        };
        
        // Create router with HTTP routes
        let app = Router::new()
            .route("/message", post(Self::message_handler))
            .route("/request", post(Self::request_handler))
            .route("/health", get(Self::health_handler))
            .with_state(state);
        
        // Start server with or without TLS
        if use_tls {
            if let (Some(cert_path), Some(key_path)) = (tls_cert_path, tls_key_path) {
                // Load TLS certificate and key
                let config = Self::create_tls_config(&cert_path, &key_path)?;
                
                // Start TLS server
                axum::Server::bind(&addr)
                    .tls_config(config)
                    .map_err(|e| Error::Internal(format!("Failed to configure TLS: {}", e)))?
                    .serve(app.into_make_service())
                    .await
                    .map_err(|e| Error::Internal(format!("HTTP server error: {}", e)))?;
            } else {
                return Err(Error::Internal("TLS enabled but certificate or key path not provided".to_string()));
            }
        } else {
            // Start plain HTTP server
            axum::Server::bind(&addr)
                .serve(app.into_make_service())
                .await
                .map_err(|e| Error::Internal(format!("HTTP server error: {}", e)))?;
        }
        
        Ok(())
    }
    
    /// Message handler for HTTP requests
    #[cfg(feature = "transport-http")]
    async fn message_handler(
        State(state): State<Arc<TransportServerState>>,
        Json(message): Json<Message>,
    ) -> impl IntoResponse {
        use axum::http::StatusCode;
        
        debug!("Received message via HTTP");
        
        // Process message
        let handler = state.message_handler.clone();
        let handle = handler.process_message(message);
        
        // Wait for processing to complete
        match handle.await {
            Ok(Ok(_)) => {
                // Success, no response needed
                StatusCode::OK
            },
            Ok(Err(e)) => {
                error!("Error processing message: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            },
            Err(e) => {
                error!("Message handler task failed: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
    
    /// Request handler for HTTP requests
    #[cfg(feature = "transport-http")]
    async fn request_handler(
        State(state): State<Arc<TransportServerState>>,
        Json(message): Json<Message>,
    ) -> impl IntoResponse {
        use axum::http::StatusCode;
        
        debug!("Received request via HTTP");
        
        // Process request
        let handler = state.message_handler.clone();
        let handle = handler.process_message(message);
        
        // Wait for processing to complete and return response
        match handle.await {
            Ok(Ok(Some(response))) => {
                // Return response
                (StatusCode::OK, Json(response))
            },
            Ok(Ok(None)) => {
                // No response (unusual for a request)
                (StatusCode::NO_CONTENT, Json(serde_json::json!({"error": "No response generated"})))
            },
            Ok(Err(e)) => {
                error!("Error processing request: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()})))
            },
            Err(e) => {
                error!("Request handler task failed: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": e.to_string()})))
            }
        }
    }
    
    /// Health check handler
    #[cfg(feature = "transport-http")]
    async fn health_handler() -> impl IntoResponse {
        use axum::http::StatusCode;
        
        (StatusCode::OK, Json(serde_json::json!({"status": "ok"})))
    }
    
    /// Provide a stub implementation when transport-http is not enabled
    #[cfg(not(feature = "transport-http"))]
    async fn run_http_server(
        _state: Arc<TransportServerState>,
        _addr: SocketAddr,
        _use_tls: bool,
        _tls_cert_path: Option<String>,
        _tls_key_path: Option<String>,
    ) -> Result<(), Error> {
        Err(Error::Internal("HTTP server is not enabled. Enable the 'transport-http' feature.".to_string()))
    }
    
    /// Create TLS configuration
    #[cfg(any(feature = "transport-ws", feature = "transport-http"))]
    fn create_tls_config(cert_path: &str, key_path: &str) -> Result<axum_server::tls_rustls::RustlsConfig, Error> {
        use axum_server::tls_rustls::RustlsConfig;
        
        RustlsConfig::from_pem_file(cert_path, key_path)
            .await
            .map_err(|e| Error::Internal(format!("Failed to load TLS configuration: {}", e)))
    }
} 