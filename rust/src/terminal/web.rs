//! Web Terminal Server
//!
//! Provides a WebSocket-based terminal interface for browser access

use std::collections::HashMap;
use std::fmt;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
#[cfg(feature = "terminal-full")]
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot, Mutex, RwLock};
use tracing::{debug, error, info};

#[cfg(feature = "terminal-full")]
use super::config::AuthMethod;
use super::config::AuthConfig;
use super::Terminal;
use crate::error::Error;

/// Client ID type for tracking WebSocket connections
type ClientId = String;

/// Input callback type for handling input from web clients
type InputCallback = Box<dyn Fn(String, ClientId) -> Result<(), Error> + Send + Sync>;

/// JWT claims for authentication
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    /// Subject (username)
    sub: String,
    /// Expiration time (unix timestamp)
    exp: usize,
}

/// Server state shared across all connections
struct WebServerState {
    /// Active client connections
    clients: RwLock<HashMap<ClientId, mpsc::Sender<Message>>>,
    /// Authentication configuration
    auth_config: AuthConfig,
    /// Input callback function
    input_callback: Mutex<Option<InputCallback>>,
}

impl fmt::Debug for WebServerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebServerState")
            .field("clients", &"<RwLock<HashMap<ClientId, mpsc::Sender<Message>>>>")
            .field("auth_config", &self.auth_config)
            .field("input_callback", &"<Mutex<Option<InputCallback>>>")
            .finish()
    }
}

/// Web Terminal Server implementation
pub struct WebTerminalServer {
    /// Server ID
    id: String,
    /// Host to bind to
    host: String,
    /// Port to bind to
    port: u16,
    /// Authentication configuration
    auth_config: AuthConfig,
    /// Input callback function
    input_callback: Option<InputCallback>,
    /// Active flag - true if the server is running
    active: bool,
    /// Cancellation token for the server task
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Server state
    state: Option<Arc<WebServerState>>,
}

impl fmt::Debug for WebTerminalServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebTerminalServer")
            .field("id", &self.id)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("auth_config", &self.auth_config)
            .field("input_callback", &self.input_callback.is_some())
            .field("active", &self.active)
            .field("shutdown_tx", &self.shutdown_tx.is_some())
            .field("state", &self.state)
            .finish()
    }
}

impl Clone for WebTerminalServer {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            host: self.host.clone(),
            port: self.port,
            auth_config: self.auth_config.clone(),
            input_callback: None, // Can't clone the callback
            active: self.active,
            shutdown_tx: None, // Don't clone the shutdown channel
            state: self.state.clone(), // We can clone the Arc
        }
    }
}

impl WebTerminalServer {
    /// Create a new web terminal server
    pub fn new(host: String, port: u16, auth_config: AuthConfig) -> Self {
        Self {
            id: "web".to_string(),
            host,
            port,
            auth_config,
            input_callback: None,
            active: false,
            shutdown_tx: None,
            state: None,
        }
    }
    
    /// Set the input callback function
    pub fn set_input_callback<F>(&mut self, callback: F)
    where
        F: Fn(String, ClientId) -> Result<(), Error> + Send + Sync + 'static,
    {
        self.input_callback = Some(Box::new(callback));
    }
    
    /// Start the web server in a background task
    async fn start_server(&mut self) -> Result<(), Error> {
        // Create shared state
        let state = Arc::new(WebServerState {
            clients: RwLock::new(HashMap::new()),
            auth_config: self.auth_config.clone(),
            input_callback: Mutex::new(self.input_callback.take()),
        });
        
        // Store the state for later use
        self.state = Some(state.clone());
        
        // Create a shutdown channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(shutdown_tx);
        
        // Make copies of the state for different handlers
        let ws_state = state.clone();
        
        // Create a simple router
        let app = Router::new()
            .route("/ws/:client_id", get(move |ws, Path(client_id)| {
                ws_handler_with_state(ws, ws_state.clone(), client_id)
            }))
            .route("/health", get(|| async { StatusCode::OK }))
            .route("/", get(|| async { 
                axum::response::Html(
                    r#"<!DOCTYPE html>
                    <html>
                    <head>
                        <title>MCP Agent Web Terminal</title>
                        <style>
                            body { font-family: monospace; background: #222; color: #0f0; margin: 0; padding: 1em; }
                            #terminal { width: 100%; height: 400px; background: #000; border: 1px solid #0f0; padding: 0.5em; overflow-y: scroll; }
                            #input { width: 100%; background: #000; color: #0f0; border: 1px solid #0f0; padding: 0.5em; margin-top: 0.5em; }
                            h1 { color: #0f0; }
                        </style>
                    </head>
                    <body>
                        <h1>MCP Agent Web Terminal</h1>
                        <div id="terminal"></div>
                        <input type="text" id="input" placeholder="Type command here..." />
                        
                        <script>
                            const terminal = document.getElementById('terminal');
                            const input = document.getElementById('input');
                            const clientId = 'web-' + Math.random().toString(36).substring(2, 10);
                            let socket;
                            
                            function connect() {
                                const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
                                socket = new WebSocket(`${protocol}//${location.host}/ws/${clientId}`);
                                
                                socket.onopen = () => {
                                    addMessage('System', 'Connected to terminal server');
                                };
                                
                                socket.onmessage = (event) => {
                                    addMessage('Server', event.data);
                                };
                                
                                socket.onclose = () => {
                                    addMessage('System', 'Disconnected from terminal server');
                                    setTimeout(connect, 3000);
                                };
                                
                                socket.onerror = (error) => {
                                    addMessage('Error', 'WebSocket error');
                                    console.error('WebSocket error:', error);
                                };
                            }
                            
                            function addMessage(source, message) {
                                const lines = message.split('\n');
                                for (const line of lines) {
                                    if (line.trim() === '') continue;
                                    const msg = document.createElement('div');
                                    msg.textContent = line;
                                    terminal.appendChild(msg);
                                }
                                terminal.scrollTop = terminal.scrollHeight;
                            }
                            
                            input.addEventListener('keydown', (event) => {
                                if (event.key === 'Enter') {
                                    const command = input.value;
                                    if (command) {
                                        if (socket && socket.readyState === WebSocket.OPEN) {
                                            socket.send(command);
                                        }
                                        input.value = '';
                                    }
                                }
                            });
                            
                            // Connect on page load
                            connect();
                        </script>
                    </body>
                    </html>"#
                )
            }));
            
        // Add JWT auth handler if enabled
        #[cfg(feature = "terminal-full")]
        let app = if self.auth_config.auth_method == AuthMethod::Jwt {
            debug!("Using JWT authentication for web terminal");
            let auth_state = state.clone();
            app.route("/auth", get(move || auth_handler_with_state(auth_state.clone())))
        } else {
            app
        };
        
        // Get bind address
        let addr = format!("{}:{}", self.host, self.port)
            .parse::<SocketAddr>()
            .map_err(|e| Error::TerminalError(format!("Invalid address: {}", e)))?;
        
        // Start the server in a background task
        info!("Starting web terminal server on {}", addr);
        
        // Use axum::Server with axum 0.6
        let server = axum::Server::bind(&addr)
            .serve(app.into_make_service());
        
        // Handle graceful shutdown
        let server_with_shutdown = server.with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
            info!("Web terminal server shutting down");
        });
        
        // Spawn the server task
        tokio::spawn(async move {
            if let Err(e) = server_with_shutdown.await {
                error!("Web terminal server error: {}", e);
            }
            info!("Web terminal server stopped");
        });
        
        Ok(())
    }
    
    /// Broadcast a message to all connected clients
    async fn broadcast(&self, message: &str) -> Result<(), Error> {
        // We need to have the server running to broadcast
        if !self.active {
            return Err(Error::TerminalError("Web terminal server is not running".to_string()));
        }
        
        debug!("Broadcasting to web clients: {} bytes", message.len());
        
        // Get the state and send the message to all clients
        if let Some(state) = &self.state {
            let clients = state.clients.read().await;
            let websocket_msg = Message::Text(message.to_string());
            
            for (client_id, tx) in clients.iter() {
                if let Err(e) = tx.send(websocket_msg.clone()).await {
                    error!("Failed to send message to client {}: {}", client_id, e);
                }
            }
            
            Ok(())
        } else {
            Err(Error::TerminalError("Web terminal state not initialized".to_string()))
        }
    }
    
    /// Lock the web terminal using a global mutex
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, WebTerminalServer> {
        static WEB_MUTEX: Mutex<()> = Mutex::const_new(());
        let _ = WEB_MUTEX.lock().await;
        // This is a hack to satisfy the lifetime checker
        // We're using a global mutex to ensure only one thread can access the web terminal
        // But we return a guard to the callers mutex as a convenience
        unsafe {
            std::mem::transmute(WEB_MUTEX.lock().await)
        }
    }
}

/// WebSocket connection handler with state
async fn ws_handler_with_state(
    ws: WebSocketUpgrade,
    state: Arc<WebServerState>,
    client_id: String,
) -> impl IntoResponse {
    debug!("New WebSocket connection request from client: {}", client_id);
    
    // Upgrade the WebSocket connection
    ws.on_upgrade(move |socket| handle_socket(socket, state, client_id))
}

/// Process the WebSocket connection
async fn handle_socket(socket: WebSocket, state: Arc<WebServerState>, client_id: String) {
    debug!("WebSocket connection established for client: {}", client_id);
    
    // Split the socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();
    
    // Create a channel for sending messages to this client
    let (tx, mut rx) = mpsc::channel::<Message>(100);
    
    // Register the client
    {
        let mut clients = state.clients.write().await;
        clients.insert(client_id.clone(), tx);
        info!("Client registered: {}", client_id);
    }
    
    // Spawn a task to forward messages from the channel to the WebSocket
    let client_id_for_sender = client_id.clone();
    let mut ws_sender = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Err(e) = sender.send(msg).await {
                error!("Error sending WebSocket message to client {}: {}", client_id_for_sender, e);
                break;
            }
        }
        
        debug!("WebSocket sender task for client {} terminated", client_id_for_sender);
    });
    
    // Process incoming WebSocket messages
    let state_clone = state.clone();
    let client_id_for_receiver = client_id.clone();
    let mut ws_receiver = tokio::spawn(async move {
        while let Some(result) = receiver.next().await {
            match result {
                Ok(Message::Text(text)) => {
                    debug!("Received text message from client {}: {}", client_id_for_receiver, text);
                    
                    // Process input with the callback
                    let input_callback = state_clone.input_callback.lock().await;
                    if let Some(callback) = &*input_callback {
                        if let Err(e) = callback(text, client_id_for_receiver.clone()) {
                            error!("Error processing input from client {}: {}", client_id_for_receiver, e);
                        }
                    }
                }
                Ok(Message::Close(_)) => {
                    debug!("WebSocket close frame received from client {}", client_id_for_receiver);
                    break;
                }
                Err(e) => {
                    error!("WebSocket error from client {}: {}", client_id_for_receiver, e);
                    break;
                }
                _ => {} // Ignore other message types
            }
        }
        
        debug!("WebSocket receiver task for client {} terminated", client_id_for_receiver);
    });
    
    // Wait for either task to complete
    tokio::select! {
        _ = &mut ws_sender => {
            debug!("WebSocket sender task completed first for client {}", client_id);
            ws_receiver.abort();
        }
        _ = &mut ws_receiver => {
            debug!("WebSocket receiver task completed first for client {}", client_id);
            ws_sender.abort();
        }
    }
    
    // Unregister client
    {
        let mut clients = state.clients.write().await;
        clients.remove(&client_id);
        info!("Client unregistered: {}", client_id);
    }
}

/// Authentication handler for JWT with state
#[cfg(feature = "terminal-full")]
async fn auth_handler_with_state(
    state: Arc<WebServerState>,
) -> Result<impl IntoResponse, StatusCode> {
    debug!("Auth token requested");
    
    // Make sure we have JWT auth enabled
    if state.auth_config.auth_method != AuthMethod::Jwt {
        error!("JWT authentication not enabled");
        return Err(StatusCode::BAD_REQUEST);
    }
    
    // Create token expiration time
    let expiration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize
        + state.auth_config.token_expiration_secs as usize;
    
    // Create claims
    let claims = Claims {
        sub: state.auth_config.username.clone(),
        exp: expiration,
    };
    
    // Generate token
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.auth_config.jwt_secret.as_bytes()),
    )
    .map_err(|e| {
        error!("Failed to create JWT token: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    
    Ok(token)
}

#[async_trait]
impl Terminal for WebTerminalServer {
    async fn id(&self) -> Result<String, Error> {
        Ok(self.id.clone())
    }
    
    async fn start(&mut self) -> Result<(), Error> {
        if self.active {
            return Ok(());
        }
        
        debug!("Starting web terminal server");
        self.start_server().await?;
        self.active = true;
        
        Ok(())
    }
    
    async fn stop(&mut self) -> Result<(), Error> {
        if !self.active {
            return Ok(());
        }
        
        debug!("Stopping web terminal server");
        
        // Send shutdown signal to server task
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        
        self.active = false;
        Ok(())
    }
    
    async fn display(&self, output: &str) -> Result<(), Error> {
        debug!("Web terminal displaying output: {} bytes", output.len());
        self.broadcast(output).await
    }
    
    async fn echo_input(&self, input: &str) -> Result<(), Error> {
        debug!("Web terminal echoing input: {} bytes", input.len());
        
        // Format the input as a user command
        let formatted = format!("> {}\n", input);
        self.broadcast(&formatted).await
    }
    
    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<(), Error> {
        debug!("Executing command on web terminal: {}", command);
        
        // Web terminal doesn't directly execute commands
        // Instead, send the command to be displayed and simulate the result
        let result = format!("Command execution simulated: {}\n", command);
        let _ = tx.send(result);
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::config::AuthConfig;
    
    #[tokio::test]
    async fn test_web_terminal_id() {
        let terminal = WebTerminalServer::new(
            "127.0.0.1".to_string(),
            8080,
            AuthConfig::default(),
        );
        
        let id = terminal.id().await.unwrap();
        assert_eq!(id, "web");
    }
    
    #[tokio::test]
    async fn test_set_input_callback() {
        let mut terminal = WebTerminalServer::new(
            "127.0.0.1".to_string(),
            8080,
            AuthConfig::default(),
        );
        
        // Initially no callback
        assert!(terminal.input_callback.is_none());
        
        // Set a callback
        terminal.set_input_callback(|_, _| Ok(()));
        
        // Now there should be a callback
        assert!(terminal.input_callback.is_some());
    }
    
    #[cfg(feature = "terminal-full")]
    #[tokio::test]
    async fn test_validate_token() {
        use crate::terminal::config::AuthMethod;
        
        let auth_config = AuthConfig {
            auth_method: AuthMethod::Jwt,
            jwt_secret: "test-secret".to_string(),
            token_expiration_secs: 3600,
            require_authentication: true,
            username: "user".to_string(),
            password: "pass".to_string(),
            allow_anonymous: false,
        };
        
        // Create claims for the token
        let expiration = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize
            + 3600;
        
        let claims = Claims {
            sub: "user".to_string(),
            exp: expiration,
        };
        
        // Create a valid token
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(auth_config.jwt_secret.as_bytes()),
        )
        .unwrap();
        
        // Validate the token
        let valid = validate_token(&token, &auth_config);
        assert!(valid);
        
        // Test an invalid token
        let invalid = validate_token("invalid-token", &auth_config);
        assert!(!invalid);
    }
} 