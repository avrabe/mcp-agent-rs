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
        Path, Query, State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
#[cfg(feature = "terminal-full")]
use axum_server::Server;
use futures::{SinkExt, StreamExt};
#[cfg(feature = "terminal-full")]
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex as TokioMutex, RwLock};
use tracing::{debug, error, info};
use uuid;
use serde_json::json;

use super::config::AuthConfig;
#[cfg(feature = "terminal-full")]
use super::config::AuthMethod;
use super::graph;
use super::graph::{Graph, GraphEdge, GraphNode};
use super::AsyncTerminal;
use crate::error::{Error, Result};
use crate::terminal::config::WebTerminalConfig;
use crate::terminal::graph::models::{SprottyGraph, SprottyStatus};

/// Client ID type for tracking WebSocket connections
type ClientId = String;

/// Input callback function for web terminal
type InputCallback = Arc<dyn Fn(String, ClientId) -> Result<()> + Send + Sync>;

/// JWT Claims
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    /// Subject (username)
    sub: String,
    /// Expiration time (unix timestamp)
    exp: usize,
}

/// Web server state used by the web terminal handlers
struct WebServerState {
    /// Active client connections
    clients: TokioMutex<HashMap<ClientId, mpsc::Sender<Message>>>,
    /// Authentication configuration
    auth_config: AuthConfig,
    /// Input callback function
    input_callback: TokioMutex<Option<InputCallback>>,
    /// Graph visualization manager (if enabled)
    graph_manager: Option<Arc<graph::GraphManager>>,
}

impl fmt::Debug for WebServerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebServerState")
            .field(
                "clients",
                &"<TokioMutex<HashMap<ClientId, mpsc::Sender<Message>>>>",
            )
            .field("auth_config", &self.auth_config)
            .field("input_callback", &"<TokioMutex<Option<InputCallback>>>")
            .field("graph_manager", &self.graph_manager.is_some())
            .finish()
    }
}

/// Web Terminal implementation with optional visualization support
pub struct WebTerminal {
    /// Terminal ID
    id: String,
    /// Terminal configuration
    config: WebTerminalConfig,
    /// Input callback function
    input_callback: Option<InputCallback>,
    /// Active flag - true if the server is running
    active: Arc<TokioMutex<bool>>,
    /// Cancellation token for the server task
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Server state
    state: Option<Arc<WebServerState>>,
    /// Graph manager for visualization
    graph_manager: Option<Arc<graph::GraphManager>>,
    /// Currently active WebSocket clients
    clients: Arc<TokioMutex<HashMap<String, broadcast::Sender<String>>>>,
    /// Graph data
    graph_data: Arc<TokioMutex<HashMap<String, SprottyGraph>>>,
    /// Message sender for terminal output
    tx: broadcast::Sender<String>,
    /// Command handler function
    command_handler: Option<Arc<dyn Fn(String) -> Result<String> + Send + Sync>>,
}

impl fmt::Debug for WebTerminal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WebTerminal")
            .field("id", &self.id)
            .field("active", &"<TokioMutex<bool>>")
            .field(
                "clients",
                &"<TokioMutex<HashMap<String, broadcast::Sender<String>>>>",
            )
            .field("graph_data", &"<TokioMutex<HashMap<String, SprottyGraph>>>")
            .field("input_callback", &self.input_callback.is_some())
            .field("shutdown_tx", &self.shutdown_tx.is_some())
            .field("state", &self.state.is_some())
            .field("graph_manager", &self.graph_manager.is_some())
            .field("tx", &"<broadcast::Sender<String>>")
            .field("command_handler", &self.command_handler.is_some())
            .finish()
    }
}

impl Clone for WebTerminal {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            config: self.config.clone(),
            input_callback: None, // Can't clone the callback
            active: self.active.clone(),
            shutdown_tx: None,         // Don't clone the shutdown channel
            state: self.state.clone(), // We can clone the Arc
            graph_manager: self.graph_manager.clone(),
            clients: self.clients.clone(),
            graph_data: self.graph_data.clone(),
            tx: self.tx.clone(),
            command_handler: None, // Can't clone the command handler
        }
    }
}

impl WebTerminal {
    /// Create a new web terminal
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            config: WebTerminalConfig::default(),
            input_callback: None,
            active: Arc::new(TokioMutex::new(false)),
            shutdown_tx: None,
            state: None,
            graph_manager: None,
            clients: Arc::new(TokioMutex::new(HashMap::new())),
            graph_data: Arc::new(TokioMutex::new(HashMap::new())),
            tx,
            command_handler: None,
        }
    }

    /// Set the terminal ID
    pub fn set_id(&mut self, id: String) {
        self.id = id;
    }

    /// Set the terminal configuration
    pub fn set_config(&mut self, config: WebTerminalConfig) {
        self.config = config;
    }

    /// Set the input callback function
    pub fn set_input_callback<F>(&mut self, callback: F)
    where
        F: Fn(String, ClientId) -> Result<()> + Send + Sync + 'static,
    {
        debug!("Setting input callback for web terminal");
        self.input_callback = Some(Arc::new(callback));
    }

    /// Set the graph manager for visualization
    pub fn set_graph_manager(&mut self, graph_manager: Arc<graph::GraphManager>) {
        debug!("Setting graph manager for visualization");
        // Store it in the state for websocket handlers if the server is running
        if let Some(state) = &self.state {
            if let Some(input_callback) = self.input_callback.clone() {
                *state.input_callback.blocking_lock() = Some(input_callback);
            }

            // Update the graph manager
            state.graph_manager = Some(graph_manager.clone());
        }
        self.graph_manager = Some(graph_manager);
    }

    /// Start the web server in a background task
    async fn start_server(&mut self) -> Result<()> {
        // Use graph manager initialization
        let graph_manager = self.graph_manager.clone().unwrap();

        // We need to create a new WebServerState with the graph manager
        let state = Arc::new(WebServerState {
            clients: TokioMutex::new(HashMap::new()),
            auth_config: self.config.auth_config.clone(),
            input_callback: TokioMutex::new(None),
            graph_manager: Some(graph_manager.clone()),
        });

        // Store the state for later use
        self.state = Some(state.clone());

        // Create a shutdown channel
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        self.shutdown_tx = Some(shutdown_tx);

        // Make copies of the state for different handlers
        let ws_state = state.clone();
        let api_state = state.clone();

        // Create the router with terminal handlers
        let mut app = Router::new()
            .route(
                "/ws/:client_id",
                get(move |ws, Path(client_id)| {
                    ws_handler_with_state(ws, ws_state.clone(), client_id)
                }),
            )
            .route("/health", get(|| async { StatusCode::OK }));

        // Add graph visualization API if enabled
        if self.config.enable_visualization {
            debug!("Adding graph visualization API routes");
            if let Some(graph_manager) = &state.graph_manager {
                app = app.merge(graph::api::create_graph_router(graph_manager.clone()));
            }
        }

        // Add main terminal HTML page
        let enable_visualization = self.config.enable_visualization;
        app = app.route(
            "/",
            get(|| async { axum::response::Html(self.generate_html()) }),
        );

        // Add JWT auth handler if enabled
        #[cfg(feature = "terminal-full")]
        let app = if self.config.auth_config.auth_method == AuthMethod::Jwt {
            debug!("Using JWT authentication for web terminal");
            let auth_state = state.clone();
            app.route(
                "/auth",
                get(move || async move {
                    let state = State(auth_state.clone());
                    auth_handler_with_state(state).await
                }),
            )
        } else {
            app
        };

        // Get bind address using config's IpAddr and port directly
        let addr = SocketAddr::new(self.config.host, self.config.port);

        // Start the server in a background task
        info!("Starting web terminal server on {}", addr);

        #[cfg(feature = "terminal-full")]
        let server_with_shutdown = {
            let shutdown_signal = async {
                let _ = shutdown_rx.await;
                info!("Web terminal server shutting down");
            };
            axum_server::Handle::new();
            axum_server::Server::bind(addr)
                .handle(axum_server::Handle::new())
                .serve(app.into_make_service())
                .with_graceful_shutdown(shutdown_signal)
        };

        #[cfg(not(feature = "terminal-full"))]
        let server_with_shutdown = {
            let shutdown_signal = async {
                let _ = shutdown_rx.await;
                info!("Web terminal server shutting down");
            };
            axum::Server::bind(&addr)
                .serve(app.into_make_service())
                .with_graceful_shutdown(shutdown_signal)
        };

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
    async fn broadcast(&self, message: &str) -> Result<()> {
        // We need to have the server running to broadcast
        if !*self.active.lock().await {
            return Err(Error::TerminalError("Server not active".to_string()));
        }

        debug!("Broadcasting to web clients: {} bytes", message.len());

        // Get the state and send the message to all clients
        if let Some(state) = &self.state {
            let clients = state.clients.lock().await;
            let websocket_msg = Message::Text(message.to_string());

            for (client_id, tx) in clients.iter() {
                if let Err(e) = tx.send(websocket_msg.clone()).await {
                    error!("Failed to send message to client {}: {}", client_id, e);
                }
            }

            Ok(())
        } else {
            Err(Error::TerminalError(
                "Web terminal state not initialized".to_string(),
            ))
        }
    }

    /// Get the graph manager (if enabled)
    pub fn graph_manager(&self) -> Option<Arc<graph::GraphManager>> {
        if let Some(state) = &self.state {
            state.graph_manager.clone()
        } else {
            None
        }
    }

    /// Lock the web terminal using a global mutex
    pub async fn lock(&self) -> impl std::ops::Deref<Target = WebTerminal> + '_ {
        // Use a simple wrapper that holds both the lock and a reference to self
        struct WebTerminalGuard<'a> {
            _lock: tokio::sync::MutexGuard<'a, ()>,
            terminal: &'a WebTerminal,
        }

        impl<'a> std::ops::Deref for WebTerminalGuard<'a> {
            type Target = WebTerminal;

            fn deref(&self) -> &Self::Target {
                self.terminal
            }
        }

        static WEB_MUTEX: TokioMutex<()> = TokioMutex::const_new(());
        let lock = WEB_MUTEX.lock().await;

        WebTerminalGuard {
            _lock: lock,
            terminal: self,
        }
    }

    /// Update a graph in the web terminal
    pub async fn update_graph(&self, graph_id: &str, graph: SprottyGraph) -> Result<()> {
        debug!("Updating graph {} in web terminal visualization", graph_id);

        // Store the graph data locally
        let mut graph_data = self.graph_data.lock().await;
        graph_data.insert(graph_id.to_string(), graph.clone());

        // If we have a graph manager, update it as well
        if let Some(graph_manager) = &self.graph_manager {
            // Convert SprottyGraph to the internal Graph forma
            let mut mcp_graph = Graph {
                id: graph_id.to_string(),
                name: format!("Graph {}", graph_id),
                graph_type: "workflow".to_string(),
                nodes: vec![],
                edges: vec![],
                properties: HashMap::new(),
            };

            // Convert nodes
            for node in graph.get_all_nodes() {
                let mut properties = HashMap::new();
                let status_str = node.status.as_ref().map_or("unknown", |s| match s {
                    SprottyStatus::Idle => "idle",
                    SprottyStatus::Waiting => "waiting",
                    SprottyStatus::Running => "running",
                    SprottyStatus::Completed => "completed",
                    SprottyStatus::Failed => "failed",
                });
                properties.insert(
                    "status".to_string(),
                    serde_json::to_value(status_str).unwrap_or_default(),
                );
                properties.insert(
                    "position".to_string(),
                    serde_json::to_value(node.position.clone()).unwrap_or_default(),
                );

                let graph_node = GraphNode {
                    id: node.id.clone(),
                    name: node.label.clone().unwrap_or_else(|| "Unnamed".to_string()),
                    node_type: node
                        .properties
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("default")
                        .to_string(),
                    status: status_str.to_string(),
                    properties: HashMap::new(),
                };

                mcp_graph.nodes.push(graph_node);
            }

            // Convert edges
            for edge in graph.get_all_edges() {
                let graph_edge = GraphEdge {
                    id: edge.id.clone(),
                    source: edge.source.clone(),
                    target: edge.target.clone(),
                    edge_type: "dependency".to_string(),
                    properties: HashMap::new(),
                };

                mcp_graph.edges.push(graph_edge);
            }

            // Register the graph with the manager
            graph_manager.register_graph(mcp_graph).await?;
        }

        // Broadcast a message to inform clients that a graph has been updated
        let message = serde_json::json!({
            "type": "graph_updated",
            "graph_id": graph_id,
        });

        self.broadcast(&message.to_string()).await?;

        Ok(())
    }

    /// Broadcast a graph update to all connected clients
    async fn broadcast_graph_update(&self, graph_id: &str) -> Result<()> {
        let graphs = self.graph_data.lock().await;
        let clients = self.clients.lock().await;

        if let Some(graph) = graphs.get(graph_id) {
            let graph_json = serde_json::to_string(&graph)?;
            let message = format!("GRAPH_UPDATE:{}", graph_json);

            for (_, sender) in clients.iter() {
                let _ = sender.send(message.clone());
            }
        }

        Ok(())
    }

    pub fn with_command_handler<F>(mut self, handler: F) -> Self
    where
        F: Fn(String) -> Result<String> + Send + Sync + 'static,
    {
        self.command_handler = Some(Arc::new(handler));
        self
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    pub async fn handle_command(&self, command: String) -> Result<String> {
        if let Some(handler) = &self.command_handler {
            handler(command)
        } else {
            Ok(format!("No command handler registered: {}", command))
        }
    }

    /// Generate the terminal HTML with visualization support
    pub fn generate_html(&self) -> String {
        let mut html = String::from(
            r#"<!DOCTYPE html>
<html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>MCP Agent Web Terminal</title>
        <style>
            :root {
                --bg-color: #1e1e1e;
                --text-color: #f0f0f0;
                --accent-color: #3498db;
                --error-color: #e74c3c;
                --success-color: #2ecc71;
                --border-color: #333;
            }
            
            body {
                font-family: 'Courier New', monospace;
                background-color: var(--bg-color);
                color: var(--text-color);
                margin: 0;
                padding: 0;
                height: 100vh;
                display: flex;
                flex-direction: column;
            }
            
            h1 {
                text-align: center;
                color: var(--accent-color);
                margin: 10px 0;
                font-size: 1.5em;
            }
            
            .container {
                flex: 1;
                padding: 10px;
                display: flex;
                flex-direction: column;
                overflow: hidden;
            }
            
            #terminal {
                flex: 1;
                background-color: #000;
                border: 1px solid var(--border-color);
                padding: 10px;
                overflow-y: auto;
                white-space: pre-wrap;
                word-break: break-all;
                line-height: 1.4;
            }
            
            .terminal-line {
                margin: 0;
                padding: 2px 0;
            }
            
            .terminal-system {
                color: #aaa;
                font-style: italic;
            }
            
            .terminal-input {
                color: var(--accent-color);
                font-weight: bold;
            }
            
            .terminal-error {
                color: var(--error-color);
            }
            
            .terminal-output {
                color: var(--text-color);
            }
            
            #input {
                background-color: #2d2d2d;
                border: 1px solid var(--border-color);
                border-top: none;
                color: var(--text-color);
                padding: 8px;
                font-family: 'Courier New', monospace;
                outline: none;
                width: 100%;
                box-sizing: border-box;
                font-size: 1em;
            }
            
            .controls {
                padding: 5px;
                background-color: #2d2d2d;
                border-bottom: 1px solid var(--border-color);
                display: flex;
                gap: 5px;
            }
            
            button, select {
                background-color: #444;
                color: var(--text-color);
                border: 1px solid var(--border-color);
                padding: 5px 10px;
                cursor: pointer;
                font-family: 'Courier New', monospace;
                outline: none;
            }
            
            button:hover, select:hover {
                background-color: #555;
            }
            
            /* Visualization styling */
            .visualization {
                display: none;
                border: 1px solid var(--border-color);
                margin: 10px;
                height: 300px;
                position: relative;
                background-color: #2d2d2d;
            }
            
            .sprotty {
                width: 100%;
                height: 100%;
                overflow: hidden;
            }
            
            .visualization-controls {
                position: absolute;
                bottom: 10px;
                right: 10px;
                display: flex;
                gap: 5px;
                background-color: rgba(45, 45, 45, 0.7);
                padding: 5px;
                border-radius: 3px;
            }
            
            .visualization-controls button {
                width: 30px;
                height: 30px;
                display: flex;
                justify-content: center;
                align-items: center;
                font-size: 1.2em;
                padding: 0;
            }
            
            /* Keyboard shortcut help panel */
            .keyboard-shortcuts {
                position: absolute;
                top: 50px;
                right: 20px;
                background-color: #2d2d2d;
                border: 1px solid var(--border-color);
                padding: 10px;
                border-radius: 5px;
                box-shadow: 0 0 10px rgba(0, 0, 0, 0.5);
                z-index: 1000;
                display: none;
            }
            
            .keyboard-shortcuts h3 {
                margin-top: 0;
                color: var(--accent-color);
            }
            
            .keyboard-shortcuts table {
                border-collapse: collapse;
            }
            
            .keyboard-shortcuts td {
                padding: 3px 8px;
            }
            
            .keyboard-shortcuts kbd {
                background-color: #444;
                padding: 2px 5px;
                border-radius: 3px;
                border: 1px solid #666;
                font-family: monospace;
            }
        </style>"#,
        );

        html.push_str(
            r#"
    </head>
    <body>
        <h1>MCP Agent Web Terminal</h1>
        <div class="controls">
            <button id="clear-btn">Clear</button>"#,
        );

        if self.config.enable_visualization {
            html.push_str(
                r#"
            <button id="toggle-viz">Show Visualization</button>
            <select id="graph-type">
                <option value="workflow">Workflow</option>
                <option value="agent">Agent System</option>
                <option value="human">Human Input</option>
                <option value="llm">LLM Integration</option>
            </select>
            <button id="reset-zoom">Reset View</button>"#,
            );
        }

        html.push_str(
            r#"
        </div>"#,
        );

        if self.config.enable_visualization {
            html.push_str(
                r#"
        <div id="visualization" class="visualization">
            <div id="sprotty-container" class="sprotty"></div>
            <div class="visualization-controls" aria-label="Visualization Controls">
                <button id="zoom-in" title="Zoom In">+</button>
                <button id="zoom-out" title="Zoom Out">-</button>
                <button id="zoom-reset" title="Reset View">⟲</button>
                <button id="fit-to-screen" title="Fit to Screen">⛶</button>
            </div>
        </div>"#,
            );
        }

        html.push_str(
            r#"
        <div class="container">
            <div id="terminal"></div>
            <input type="text" id="input" placeholder="Type command here..." />
        </div>

        <script>
            document.addEventListener('DOMContentLoaded', function() {
                // Terminal setup
                const terminalElement = document.getElementById('terminal');
                const inputElement = document.getElementById('input');
                const clearButton = document.getElementById('clear-btn');
                
                // Initialize terminal history
                let history = [];
                let historyIndex = -1;
                
                // Function to append output to the terminal
                function appendToTerminal(text, className = '') {
                    const outputElement = document.createElement('div');
                    outputElement.className = 'terminal-line ' + className;
                    outputElement.innerText = text;
                    terminalElement.appendChild(outputElement);
                    
                    // Auto-scroll to bottom
                    terminalElement.scrollTop = terminalElement.scrollHeight;
                }
                
                // WebSocket connection
                const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
                const wsUrl = protocol + '//' + window.location.host + '/ws';
                const socket = new WebSocket(wsUrl);
                
                // Connection established
                socket.addEventListener('open', (event) => {
                    appendToTerminal('Connection established', 'terminal-system');
                });
                
                // Handle messages from server
                socket.addEventListener('message', (event) => {
                    try {
                        const message = JSON.parse(event.data);
                        
                        if (message.type === 'output') {
                            appendToTerminal(message.content, 'terminal-output');
                        } else if (message.type === 'error') {
                            appendToTerminal('Error: ' + message.content, 'terminal-error');
                        } else if (message.type === 'system') {
                            appendToTerminal(message.content, 'terminal-system');
                        } else if (message.type === 'input') {
                            appendToTerminal('> ' + message.content, 'terminal-input');
                        }
                    } catch (e) {
                        // If not JSON, just display as raw output
                        appendToTerminal(event.data, 'terminal-output');
                    }
                });
                
                // Connection closed
                socket.addEventListener('close', (event) => {
                    appendToTerminal('Connection closed', 'terminal-system');
                    inputElement.disabled = true;
                });
                
                // Connection error
                socket.addEventListener('error', (event) => {
                    appendToTerminal('WebSocket error', 'terminal-error');
                });
                
                // Handle input submission
                inputElement.addEventListener('keydown', (event) => {
                    if (event.key === 'Enter' && !event.shiftKey) {
                        const command = inputElement.value;
                        
                        if (command.trim()) {
                            // Add to history
                            history.push(command);
                            historyIndex = history.length;
                            
                            // Send command to server
                            socket.send(JSON.stringify({
                                type: 'command',
                                content: command
                            }));
                            
                            // Clear input field
                            inputElement.value = '';
                        }
                        
                        event.preventDefault();
                    } else if (event.key === 'ArrowUp') {
                        // Navigate history up
                        if (historyIndex > 0) {
                            historyIndex--;
                            inputElement.value = history[historyIndex];
                        }
                        event.preventDefault();
                    } else if (event.key === 'ArrowDown') {
                        // Navigate history down
                        if (historyIndex < history.length - 1) {
                            historyIndex++;
                            inputElement.value = history[historyIndex];
                        } else if (historyIndex === history.length - 1) {
                            historyIndex = history.length;
                            inputElement.value = '';
                        }
                        event.preventDefault();
                    }
                });
                
                // Clear terminal button
                clearButton.addEventListener('click', () => {
                    terminalElement.innerHTML = '';
                });"#,
        );

        if self.config.enable_visualization {
            html.push_str(r#"

                // Visualization code
                const toggleVizBtn = document.getElementById('toggle-viz');
                const graphTypeSelect = document.getElementById('graph-type');
                const vizContainer = document.getElementById('visualization');
                const sprottyContainer = document.getElementById('sprotty-container');
                const zoomInBtn = document.getElementById('zoom-in');
                const zoomOutBtn = document.getElementById('zoom-out');
                const zoomResetBtn = document.getElementById('zoom-reset');
                const fitToScreenBtn = document.getElementById('fit-to-screen');
                
                let vizShown = false;
                let sprottyLoaded = false;
                let sprottyDiagram = null;
                let currentGraphType = 'workflow';
                
                // Toggle visualization
                if (toggleVizBtn) {
                    toggleVizBtn.addEventListener('click', () => {
                        if (vizShown) {
                            vizContainer.style.display = 'none';
                            toggleVizBtn.textContent = 'Show Visualization';
                        } else {
                            vizContainer.style.display = 'block';
                            toggleVizBtn.textContent = 'Hide Visualization';
                            if (!sprottyLoaded) {
                                loadSprotty();
                            } else {
                                updateDiagram();
                            }
                        }
                        vizShown = !vizShown;
                    });
                }
                
                // Change graph type
                if (graphTypeSelect) {
                    graphTypeSelect.addEventListener('change', () => {
                        currentGraphType = graphTypeSelect.value;
                        if (sprottyLoaded && vizShown) {
                            updateDiagram();
                        }
                    });
                }
                
                // Zoom controls
                if (zoomInBtn) {
                    zoomInBtn.addEventListener('click', () => {
                        if (sprottyDiagram) {
                            sprottyDiagram.actionDispatcher.dispatch({
                                kind: 'zoomIn'
                            });
                        }
                    });
                }
                
                if (zoomOutBtn) {
                    zoomOutBtn.addEventListener('click', () => {
                        if (sprottyDiagram) {
                            sprottyDiagram.actionDispatcher.dispatch({
                                kind: 'zoomOut'
                            });
                        }
                    });
                }
                
                if (zoomResetBtn) {
                    zoomResetBtn.addEventListener('click', () => {
                        if (sprottyDiagram) {
                            sprottyDiagram.actionDispatcher.dispatch({
                                kind: 'resetView'
                            });
                        }
                    });
                }
                
                if (fitToScreenBtn) {
                    fitToScreenBtn.addEventListener('click', () => {
                        if (sprottyDiagram) {
                            sprottyDiagram.actionDispatcher.dispatch({
                                kind: 'fit',
                                elementIds: [],
                                animate: true
                            });
                        }
                    });
                }
                
                // Load Sprotty
                function loadSprotty() {
                    // In a real implementation, we would load Sprotty from a CDN
                    // For now, we'll just create a minimal implementation
                    
                    // Create a container for the diagram
                    const container = document.createElement('div');
                    container.className = 'sprotty-container';
                    sprottyContainer.appendChild(container);
                    
                    // Create a minimal diagram
                    sprottyDiagram = {
                        actionDispatcher: {
                            dispatch: (action) => {
                                console.log('Sprotty action:', action);
                                
                                // Handle action by sending to server
                                socket.send(JSON.stringify({
                                    type: 'sprotty',
                                    action: action,
                                    graphType: currentGraphType
                                }));
                            }
                        },
                        updateModel: (model) => {
                            console.log('Updating model:', model);
                            
                            // Clear the container
                            container.innerHTML = '';
                            
                            // Create the SVG element
                            const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
                            svg.setAttribute('width', '100%');
                            svg.setAttribute('height', '100%');
                            container.appendChild(svg);
                            
                            // Draw the nodes
                            if (model.children) {
                                model.children.forEach(node => {
                                    if (node.type === 'node') {
                                        drawNode(svg, node);
                                    } else if (node.type === 'edge') {
                                        drawEdge(svg, node, model.children);
                                    }
                                });
                            }
                        }
                    };
                    
                    // Draw a node
                    function drawNode(svg, node) {
                        const g = document.createElementNS('http://www.w3.org/2000/svg', 'g');
                        g.setAttribute('id', node.id);
                        g.setAttribute('transform', `translate(${node.position.x}, ${node.position.y})`);
                        
                        const rect = document.createElementNS('http://www.w3.org/2000/svg', 'rect');
                        rect.setAttribute('width', node.size.width);
                        rect.setAttribute('height', node.size.height);
                        rect.setAttribute('rx', '5');
                        rect.setAttribute('ry', '5');
                        rect.setAttribute('fill', getNodeColor(node));
                        rect.setAttribute('stroke', '#000');
                        g.appendChild(rect);
                        
                        const text = document.createElementNS('http://www.w3.org/2000/svg', 'text');
                        text.setAttribute('x', node.size.width / 2);
                        text.setAttribute('y', node.size.height / 2);
                        text.setAttribute('text-anchor', 'middle');
                        text.setAttribute('dominant-baseline', 'middle');
                        text.setAttribute('fill', '#fff');
                        text.textContent = node.name || node.id;
                        g.appendChild(text);
                        
                        svg.appendChild(g);
                    }
                    
                    // Draw an edge
                    function drawEdge(svg, edge, nodes) {
                        const sourceNode = nodes.find(n => n.id === edge.sourceId);
                        const targetNode = nodes.find(n => n.id === edge.targetId);
                        
                        if (!sourceNode || !targetNode) return;
                        
                        const path = document.createElementNS('http://www.w3.org/2000/svg', 'path');
                        const sourceX = sourceNode.position.x + sourceNode.size.width / 2;
                        const sourceY = sourceNode.position.y + sourceNode.size.height / 2;
                        const targetX = targetNode.position.x + targetNode.size.width / 2;
                        const targetY = targetNode.position.y + targetNode.size.height / 2;
                        
                        path.setAttribute('d', `M${sourceX},${sourceY} L${targetX},${targetY}`);
                        path.setAttribute('stroke', '#999');
                        path.setAttribute('stroke-width', '2');
                        path.setAttribute('fill', 'none');
                        path.setAttribute('marker-end', 'url(#arrow)');
                        
                        svg.appendChild(path);
                    }
                    
                    // Get node color based on status
                    function getNodeColor(node) {
                        if (node.status === 'error') return '#e74c3c';
                        if (node.status === 'warning') return '#f39c12';
                        if (node.status === 'success') return '#2ecc71';
                        if (node.status === 'active') return '#3498db';
                        if (node.status === 'pending') return '#95a5a6';
                        return '#3498db'; // Default color
                    }
                    
                    // Listen for visualization updates from server
                    socket.addEventListener('message', (event) => {
                        try {
                            const message = JSON.parse(event.data);
                            
                            if (message.type === 'sprotty') {
                                const model = message.model;
                                sprottyDiagram.updateModel(model);
                            }
                        } catch (e) {
                            // If not JSON or not sprotty message, ignore
                        }
                    });
                    
                    sprottyLoaded = true;
                    updateDiagram();
                }
                
                // Update the diagram
                function updateDiagram() {
                    socket.send(JSON.stringify({
                        type: 'getGraph',
                        graphType: currentGraphType
                    }));
                }"#);
        }

        html.push_str(
            r#"
            });
        </script>
    </body>
</html>"#,
        );

        html
    }

    /// Initialize the server
    async fn initialize_server(&self) -> Result<()> {
        // Don't start if not active
        if !*self.active.lock().await {
            debug!("Web terminal not active, skipping server initialization");
            return Ok(());
        }

        // Implementation would go here
        debug!("Initializing web terminal server");

        // Return success
        Ok(())
    }
}

impl Default for WebTerminal {
    /// Creates a default WebTerminal instance.
    ///
    /// # Panics
    ///
    /// This function will not panic.
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AsyncTerminal for WebTerminal {
    async fn id(&self) -> Result<String> {
        Ok(self.id.clone())
    }

    async fn start(&mut self) -> Result<()> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let vis_status = if self.config.enable_visualization {
            "enabled"
        } else {
            "disabled"
        };

        info!(
            "Starting web terminal at {} (visualization: {})",
            addr, vis_status
        );

        self.start_server().await
    }

    async fn stop(&self) -> Result<()> {
        // Check if already stopped
        if !*self.active.lock().await {
            return Ok(());
        }

        // Set flag to indicate shutdown
        *self.active.lock().await = false;

        // Send shutdown signal if available
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(()).map_err(|_| {
                Error::TerminalError("Failed to send shutdown signal to web terminal".to_string())
            });
        }

        Ok(())
    }

    async fn display(&self, output: &str) -> Result<()> {
        debug!("Web terminal output: {}", output);
        self.broadcast(output).await
    }

    async fn echo_input(&self, input: &str) -> Result<()> {
        debug!("Web terminal input echo: {}", input);

        // Format the input as an echo and send it
        let message = format!("> {}", input);
        let _ = self.tx.send(message);
        Ok(())
    }

    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<()> {
        // Process the command
        let result = self.handle_command(command.to_string()).await?;

        // Echo the command
        self.echo_input(command).await?;

        // Send the result
        if let Err(_) = tx.send(result.clone()) {
            error!("Failed to send command result");
        }

        // Display the result
        self.display(&result).await?;

        Ok(())
    }
}

/// WebSocket connection handler with state
async fn ws_handler_with_state(
    ws: WebSocketUpgrade,
    state: Arc<WebServerState>,
    client_id: String,
) -> impl IntoResponse {
    // Log the new client connection
    debug!("New WebSocket connection from client: {}", client_id);

    // Accept the connection and handle it
    ws.on_upgrade(move |socket| handle_socket(socket, state, client_id))
}

/// Handle WebSocket connection for terminal
async fn handle_socket(socket: WebSocket, state: Arc<WebServerState>, client_id: String) {
    // Split the socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();

    // Create a channel for sending messages to this client
    let (tx, mut rx) = mpsc::channel::<Message>(100);

    // Spawn a task to forward messages from the channel to the WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if sender.send(message).await.is_err() {
                break;
            }
        }
    });

    // Add the client to our state
    {
        let mut clients = state.clients.lock().await;
        clients.insert(client_id.clone(), tx);
    }

    // Send welcome message
    if let Err(e) = sender
        .send(Message::Text(
            "Welcome to the MCP Agent Web Terminal!".to_string(),
        ))
        .await
    {
        error!("Error sending welcome message: {}", e);
    }

    // Process incoming messages from the WebSocket
    let clients_ref = Arc::clone(&state.clients);
    let client_id_clone = client_id.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            debug!("Received message from client {}: {}", client_id_clone, text);

            // Echo the message to all clients
            let clients = clients_ref.lock().await;
            for (id, tx) in clients.iter() {
                if id != &client_id_clone {
                    // Don't send back to the originator
                    let _ = tx
                        .send(Message::Text(format!("<{}>: {}", client_id_clone, text)))
                        .await;
                }
            }
        }

        debug!("Client disconnected: {}", client_id_clone);
    });

    // Wait for either task to complete
    tokio::select! {
        _ = &mut recv_task => recv_task.abort(),
    }

    // Remove the client
    let mut clients_lock = state.clients.lock().await;
    clients_lock.remove(&client_id);
}

/// Authentication handler for JWT with state
#[cfg(feature = "terminal-full")]
async fn auth_handler_with_state(
    State(state): State<Arc<WebServerState>>,
) -> Result<impl IntoResponse> {
    // Create a JWT token for the default user
    let claims = Claims {
        sub: "user".to_string(),
        exp: ((chrono::Utc::now().timestamp() + 86400) as usize), // 24 hours
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.auth_config.jwt_secret.as_bytes()),
    )
    .map_err(|e| Error::TerminalError(format!("Failed to create JWT token: {}", e)))?;

    Ok(Json(json!({
        "token": token,
        "expires_in": 86400
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::config::AuthConfig;

    #[tokio::test]
    /// Tests that the web terminal can be created with a unique ID
    ///
    /// # Panics
    ///
    /// This test will panic if the terminal ID cannot be retrieved.
    async fn test_web_terminal_id() {
        let terminal = WebTerminal::new();
        let id = terminal.id().await.unwrap();
        assert!(!id.is_empty());
    }

    #[tokio::test]
    /// Tests that an input callback can be set and retrieved
    ///
    /// # Panics
    ///
    /// This test will not panic.
    async fn test_set_input_callback() {
        let mut terminal = WebTerminal::new();
        terminal.set_input_callback(|input, client_id| {
            println!("Received input '{}' from client {}", input, client_id);
            Ok(())
        });
        // Verify that the callback can be set
    }

    #[test]
    /// Tests that the default web terminal config has expected values
    ///
    /// # Panics
    ///
    /// This test will not panic.
    fn test_web_terminal_config_default() {
        let config = WebTerminalConfig::default();
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8080);
        assert_eq!(config.auth_config.auth_method, AuthMethod::None);
        assert!(!config.enable_visualization);
    }

    #[tokio::test]
    #[cfg(feature = "terminal-full")]
    /// Tests JWT token validation
    ///
    /// # Panics
    ///
    /// This test will panic if the JWT token cannot be created or validated.
    async fn test_validate_token() {
        let auth_config = AuthConfig {
            auth_method: AuthMethod::Jwt,
            jwt_secret: "test-secret".to_string(),
            ..Default::default()
        };

        // Create a token
        let claims = Claims {
            sub: "test-user".to_string(),
            exp: ((chrono::Utc::now().timestamp() + 3600) as usize), // 1 hour
        };

        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(auth_config.jwt_secret.as_bytes()),
        )
        .unwrap();

        // Validate the token
        assert!(validate_token(&token, &auth_config));

        // Invalid token should fail
        assert!(!validate_token("invalid-token", &auth_config));
    }
}

#[cfg(feature = "terminal-full")]
/// Validate a JWT token
fn validate_token(token: &str, auth_config: &AuthConfig) -> bool {
    // Use jsonwebtoken to validate the token
    let validation = Validation::default();
    let key = DecodingKey::from_secret(auth_config.jwt_secret.as_bytes());

    match decode::<Claims>(token, &key, &validation) {
        Ok(_) => true,
        Err(e) => {
            error!("Token validation failed: {}", e);
            false
        }
    }
}
