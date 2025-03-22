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
    Router, Server,
};
use futures::{SinkExt, StreamExt};
#[cfg(feature = "terminal-full")]
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex as TokioMutex, RwLock};
use tracing::{debug, error, info};
use uuid;

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
    clients: RwLock<HashMap<ClientId, mpsc::Sender<Message>>>,
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
                &"<RwLock<HashMap<ClientId, mpsc::Sender<Message>>>>",
            )
            .field("auth_config", &self.auth_config)
            .field("input_callback", &"<TokioMutex<Option<InputCallback>>>")
            .field("graph_manager", &self.graph_manager.is_some())
            .finish()
    }
}

/// Web Terminal implementation with optional visualization suppor
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
    /// Message sender for terminal outpu
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
        // Create shared state
        let state = Arc::new(WebServerState {
            clients: RwLock::new(HashMap::new()),
            auth_config: self.config.auth_config.clone(),
            input_callback: TokioMutex::new(self.input_callback.take()),
            graph_manager: if self.config.enable_visualization {
                Some(Arc::new(graph::GraphManager::new()))
            } else {
                None
            },
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
            get(|| async { axum::response::Html(get_terminal_html(enable_visualization)) }),
        );

        // Add JWT auth handler if enabled
        #[cfg(feature = "terminal-full")]
        let app = if self.config.auth_config.auth_method == AuthMethod::Jwt {
            debug!("Using JWT authentication for web terminal");
            let auth_state = state.clone();
            app.route(
                "/auth",
                get(move || auth_handler_with_state(auth_state.clone())),
            )
        } else {
            app
        };

        // Get bind address using config's IpAddr and port directly
        let addr = SocketAddr::new(self.config.host, self.config.port);

        // Start the server in a background task
        info!("Starting web terminal server on {}", addr);

        // Use axum::Server with axum 0.6
        let server = axum::Server::bind(&addr).serve(app.into_make_service());

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
    async fn broadcast(&self, message: &str) -> Result<()> {
        // We need to have the server running to broadcas
        if !self.active.lock().await.unwrap() {
            return Err(Error::TerminalError("Server not active".to_string()));
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
                let status_str = node.status.as_ref().map_or("unknown", |s| s.as_str());
                properties.insert(
                    "status".to_string(),
                    serde_json::to_value(status_str).unwrap_or_default(),
                );
                properties.insert(
                    "position".to_string(),
                    serde_json::to_value(node.position).unwrap_or_default(),
                );

                let graph_node = GraphNode {
                    id: node.id.clone(),
                    name: node.label.clone().unwrap_or_else(|| "Unnamed".to_string()),
                    node_type: node.node_type.clone(),
                    status: node
                        .status
                        .clone()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
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
}

impl Default for WebTerminal {
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

        self.start_server().awai
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
                Error::Runtime("Failed to send shutdown signal to web terminal".to_string())
            });
        }

        Ok(())
    }

    async fn display(&self, output: &str) -> Result<()> {
        debug!("Web terminal output: {}", output);
        self.broadcast(output).awai
    }

    async fn echo_input(&self, input: &str) -> Result<()> {
        debug!("Web terminal input echo: {}", input);

        // Format the input as an echo and send i
        let message = format!("> {}", input);
        let _ = self.tx.send(message);
        Ok(())
    }

    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<()> {
        // Process the command
        let result = self.handle_command(command.to_string()).await?;

        // Echo the command
        self.echo_input(command).await?;

        // Send the resul
        if let Err(_) = tx.send(result.clone()) {
            error!("Failed to send command result");
        }

        // Display the resul
        self.display(&result).await?;

        Ok(())
    }
}

/// Generate the terminal HTML with visualization suppor
fn get_terminal_html(enable_visualization: bool) -> String {
    let mut html = r#"<!DOCTYPE html>
<html>
    <head>
        <title>MCP Agent Web Terminal</title>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <style>
            body { font-family: monospace; background: #222; color: #0f0; margin: 0; padding: 1em; }
            .container { display: flex; flex-direction: column; height: 98vh; }
            #terminal { flex: 1; background: #000; border: 1px solid #0f0; padding: 0.5em; overflow-y: scroll; }
            #input { width: 100%; background: #000; color: #0f0; border: 1px solid #0f0; padding: 0.5em; margin-top: 0.5em; }
            h1 { color: #0f0; margin-bottom: 0.5em; }
            .controls { display: flex; margin-bottom: 0.5em; align-items: center; }
            .controls button { margin-right: 0.5em; padding: 0.5em; background: #333; color: #0f0; border: 1px solid #0f0; cursor: pointer; }
            .controls button:hover { background: #444; }
            .controls select { margin-right: 0.5em; padding: 0.5em; background: #333; color: #0f0; border: 1px solid #0f0; }
            .visualization { display: none; height: 50%; border: 1px solid #0f0; margin-bottom: 0.5em; background: #111; position: relative; }
            .sprotty { width: 100%; height: 100%; }

            /* Sprotty styling */
            .sprotty-node {
                fill: #2a2a2a;
                stroke: #0f0;
                stroke-width: 1;
                rx: 5;
                ry: 5;
                transition: fill 0.3s, stroke 0.3s, stroke-width 0.3s;
            }
            .sprotty-node:hover {
                stroke: #afa;
                stroke-width: 2;
            }
            .sprotty-node.selected {
                stroke: #ff0;
                stroke-width: 2;
                filter: drop-shadow(0 0 5px rgba(255, 255, 0, 0.5));
            }
            .sprotty-edge {
                fill: none;
                stroke: #0f0;
                stroke-width: 1;
                transition: stroke 0.3s, stroke-width 0.3s;
            }
            .sprotty-edge:hover {
                stroke: #afa;
                stroke-width: 2;
            }
            .sprotty-edge.selected {
                stroke: #ff0;
                stroke-width: 2;
            }
            .sprotty-label {
                fill: #0f0;
                font-size: 12px;
                pointer-events: none;
            }
            .status-active {
                fill: #2a4a2a;
            }
            .status-completed {
                fill: #2a2a4a;
            }
            .status-error {
                fill: #4a2a2a;
            }
            .sprotty-edge.type-dependency {
                stroke-dasharray: 5,5;
            }
            .sprotty-edge.type-dataflow {
                marker-end: url(#arrow);
            }
            .sprotty-missing {
                stroke-dasharray: 5,5;
                stroke: #f00;
            }
            .control-point {
                fill: #ff0;
            }

            /* Status indicators and badges */
            .status-indicator {
                font-size: 24px;
            }
            .badge {
                fill: #333;
                stroke: #0f0;
                stroke-width: 1px;
                font-size: 10px;
                padding: 2px;
                border-radius: 10px;
            }
            .importance-badge {
                fill: #432;
                stroke: #fa0;
            }
            .progress-badge {
                fill: #243;
                stroke: #0fa;
            }
            .type-label, .status-label {
                font-size: 10px;
                fill: #afa;
            }

            /* Focus outline for accessibility */
            *:focus {
                outline: 2px solid #ff0 !important;
            }

            /* High contrast mode support */
            @media (forced-colors: active) {
                .sprotty-node {
                    stroke: CanvasText;
                    fill: Canvas;
                }
                .sprotty-edge {
                    stroke: CanvasText;
                }
                .sprotty-label {
                    fill: CanvasText;
                }
            }

            /* Keyboard shortcuts helper */
            .keyboard-shortcuts {
                position: absolute;
                bottom: 10px;
                right: 10px;
                background: rgba(0,0,0,0.7);
                border: 1px solid #0f0;
                padding: 10px;
                color: #0f0;
                font-family: monospace;
                font-size: 12px;
                z-index: 100;
                display: none;
            }
            .keyboard-shortcuts table {
                border-collapse: collapse;
            }
            .keyboard-shortcuts th, .keyboard-shortcuts td {
                padding: 3px 5px;
                text-align: left;
            }
            .keyboard-shortcuts kbd {
                background: #333;
                border: 1px solid #0a0;
                border-radius: 3px;
                padding: 2px 5px;
                margin: 0 3px;
            }

            /* Graph info panel */
            .graph-info-panel {
                position: absolute;
                top: 10px;
                right: 10px;
                background: rgba(0,0,0,0.7);
                border: 1px solid #0f0;
                padding: 10px;
                color: #0f0;
                max-width: 300px;
                max-height: 80%;
                overflow: auto;
                z-index: 100;
                font-family: monospace;
                font-size: 12px;
            }
            .graph-info-panel h3 {
                margin-top: 0;
                margin-bottom: 5px;
                border-bottom: 1px solid #0f0;
                padding-bottom: 5px;
            }
            .graph-info-panel table {
                width: 100%;
                border-collapse: collapse;
            }
            .graph-info-panel th, .graph-info-panel td {
                text-align: left;
                padding: 3px;
                border-bottom: 1px solid #0a0;
            }
            .graph-info-panel th {
                color: #afa;
            }
        </style>"#.to_string();

    html += r#"
    </head>
    <body>
        <h1>MCP Agent Web Terminal</h1>
        <div class="controls">
            <button id="clear-btn">Clear</button>"#
        .to_string();

    // Add visualization controls if enabled
    if enable_visualization {
        html += r#"
            <button id="toggle-viz">Show Visualization</button>
            <select id="graph-type">
                <option value="workflow">Workflow</option>
                <option value="agent">Agent System</option>
                <option value="human_input">Human Input</option>
                <option value="llm_integration">LLM Integration</option>
            </select>
            <button id="reset-zoom">Reset View</button>"#
            .to_string();
    }

    html += r#"
        </div>"#
        .to_string();

    // Add visualization container if enabled
    if enable_visualization {
        html += r#"
        <div id="visualization" class="visualization">
            <div id="sprotty-container" class="sprotty"></div>
            <div class="visualization-controls" aria-label="Visualization Controls">
                <button id="zoom-in" aria-label="Zoom In">+</button>
                <button id="zoom-out" aria-label="Zoom Out">-</button>
                <button id="reset-zoom" aria-label="Reset Zoom">Reset</button>
                <button id="toggle-help" aria-label="Show Keyboard Shortcuts">?</button>
            </div>
        </div>"#
            .to_string();
    }

    html += r#"
        <div class="container">
            <div id="terminal"></div>
            <input type="text" id="input" placeholder="Type command here..." />
        </div>

        <script>
            const terminal = document.getElementById('terminal');
            const input = document.getElementById('input');
            const clearBtn = document.getElementById('clear-btn');
            const clientId = 'web-' + Math.random().toString(36).substring(2, 10);
            let socket;

            // Clear terminal
            clearBtn.addEventListener('click', () => {
                terminal.innerHTML = '';
            });

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
            });"#
        .to_string();

    // Add visualization code if enabled
    if enable_visualization {
        html += r#"

            // Visualization code
            const toggleVizBtn = document.getElementById('toggle-viz');
            const vizContainer = document.getElementById('visualization');
            const graphTypeSelect = document.getElementById('graph-type');
            const resetZoomBtn = document.getElementById('reset-zoom');
            const zoomInBtn = document.getElementById('zoom-in');
            const zoomOutBtn = document.getElementById('zoom-out');
            const toggleHelpBtn = document.getElementById('toggle-help');
            let vizSocket;
            let currentGraph = null;
            let sprottyInstance = null;

            // Toggle visualization panel
            toggleVizBtn.addEventListener('click', () => {
                if (vizContainer.style.display === 'none' || !vizContainer.style.display) {
                    vizContainer.style.display = 'block';
                    toggleVizBtn.textContent = 'Hide Visualization';

                    // Initialize Sprotty if not already done
                    if (!sprottyInstance) {
                        sprottyInstance = Sprotty.init('sprotty-container');
                    }

                    connectToGraphWs();
                    loadGraph();

                    // Register keyboard shortcuts when visualization is active
                    document.addEventListener('keydown', handleKeyboardShortcuts);
                } else {
                    vizContainer.style.display = 'none';
                    toggleVizBtn.textContent = 'Show Visualization';
                    if (vizSocket) {
                        vizSocket.close();
                    }

                    // Remove keyboard shortcut handler when visualization is hidden
                    document.removeEventListener('keydown', handleKeyboardShortcuts);
                }
            });

            // Change graph type
            graphTypeSelect.addEventListener('change', () => {
                loadGraph();
            });

            // Reset zoom button
            resetZoomBtn.addEventListener('click', () => {
                if (sprottyInstance) {
                    sprottyInstance.svg.call(sprottyInstance.zoom.transform, d3.zoomIdentity);
                }
            });

            // Connect to graph WebSocke
            function connectToGraphWs() {
                if (vizSocket && vizSocket.readyState === WebSocket.OPEN) {
                    return;
                }

                const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
                vizSocket = new WebSocket(`${protocol}//${location.host}/api/graph/ws`);

                vizSocket.onopen = () => {
                    console.log('Connected to graph WebSocket');
                    vizSocket.send(JSON.stringify({
                        request_type: 'subscribe',
                        graph_id: null  // Subscribe to all graphs
                    }));
                };

                vizSocket.onmessage = (event) => {
                    try {
                        const update = JSON.parse(event.data);
                        handleGraphUpdate(update);
                    } catch (e) {
                        console.error('Error parsing graph update:', e);
                    }
                };

                vizSocket.onclose = () => {
                    console.log('Disconnected from graph WebSocket');
                };

                vizSocket.onerror = (error) => {
                    console.error('Graph WebSocket error:', error);
                };
            }

            // Load graph data
            function loadGraph() {
                const graphType = graphTypeSelect.value;
                fetch(`/api/graph/sprotty/${graphType}-graph`)
                    .then(response => response.json())
                    .then(data => {
                        if (data.model) {
                            // Use the Sprotty model directly
                            sprottyInstance.setModel(data.model);
                            console.log('Graph rendered:', data.model.id);
                        } else if (data.graph) {
                            // Fallback to regular graph forma
                            currentGraph = data.graph;
                            const sprottyModel = convertToSprottyModel(currentGraph);
                            sprottyInstance.setModel(sprottyModel);
                            console.log('Graph rendered:', currentGraph.name);
                        } else {
                            console.error('No graph data received:', data);
                        }
                    })
                    .catch(error => {
                        console.error('Error loading graph:', error);
                    });
            }

            // Handle graph update from WebSocke
            function handleGraphUpdate(update) {
                // Get current graph type from dropdown
                const graphType = graphTypeSelect.value;
                const expectedGraphId = `${graphType}-graph`;

                // Only process updates for the current graph type
                if (update.graph_id === expectedGraphId) {
                    if (update.update_type === 'FullUpdate' && update.graph) {
                        currentGraph = update.graph;
                        renderGraph(currentGraph);
                    } else if (update.update_type === 'NodeUpdated' && update.node) {
                        // Update node in current graph
                        if (currentGraph) {
                            const nodeIndex = currentGraph.nodes.findIndex(n => n.id === update.node.id);
                            if (nodeIndex >= 0) {
                                currentGraph.nodes[nodeIndex] = update.node;
                                renderGraph(currentGraph);
                            }
                        }
                    } else if (update.update_type === 'EdgeUpdated' && update.edge) {
                        // Update edge in current graph
                        if (currentGraph) {
                            const edgeIndex = currentGraph.edges.findIndex(e => e.id === update.edge.id);
                            if (edgeIndex >= 0) {
                                currentGraph.edges[edgeIndex] = update.edge;
                                renderGraph(currentGraph);
                            }
                        }
                    }
                }
            }

            // Convert MCP graph to Sprotty model forma
            function convertToSprottyModel(graph) {
                if (!graph) return null;

                const model = {
                    id: graph.id,
                    type: 'graph',
                    children: []
                };

                // Add nodes
                for (const node of graph.nodes) {
                    const sprottyNode = {
                        id: node.id,
                        type: 'node',
                        label: node.name,
                        status: node.status,
                        cssClasses: [`type-${node.node_type}`, `status-${node.status}`],
                        properties: node.properties || {},
                        children: []
                    };

                    // Add a label child for better positioning
                    const labelElement = {
                        id: `${node.id}-label`,
                        type: 'label',
                        text: node.name,
                        cssClasses: ['sprotty-label'],
                        properties: {}
                    };

                    sprottyNode.children.push(labelElement);

                    model.children.push(sprottyNode);
                }

                // Add edges
                for (const edge of graph.edges) {
                    const sprottyEdge = {
                        id: edge.id,
                        type: 'edge',
                        source: edge.source,
                        target: edge.target,
                        cssClasses: [`type-${edge.edge_type}`],
                        properties: edge.properties || {}
                    };

                    model.children.push(sprottyEdge);
                }

                return model;
            }

            // Render graph using Sprotty
            function renderGraph(graph) {
                if (!sprottyInstance) {
                    console.error('Sprotty not initialized');
                    return;
                }

                const sprottyModel = convertToSprottyModel(graph);
                if (sprottyModel) {
                    sprottyInstance.setModel(sprottyModel);
                    console.log('Graph rendered:', graph.name);
                }
            }

            // Handle keyboard shortcuts
            function handleKeyboardShortcuts(event) {
                // Only handle shortcuts when visualization is visible
                if (vizContainer.style.display !== 'block') return;

                switch(event.key) {
                    case '?': // Toggle shortcut help
                        toggleShortcutHelp();
                        break;
                    case '+': // Zoom in
                    case '=':
                        if (sprottyInstance) {
                            const newZoom = sprottyInstance.svg.property('__zoom').k * 1.2;
                            sprottyInstance.svg.call(
                                sprottyInstance.zoom.scaleTo,
                                Math.min(4, newZoom)
                            );
                        }
                        break;
                    case '-': // Zoom ou
                        if (sprottyInstance) {
                            const newZoom = sprottyInstance.svg.property('__zoom').k / 1.2;
                            sprottyInstance.svg.call(
                                sprottyInstance.zoom.scaleTo,
                                Math.max(0.1, newZoom)
                            );
                        }
                        break;
                    case '0': // Reset zoom
                        if (sprottyInstance) {
                            sprottyInstance.svg.call(sprottyInstance.zoom.transform, d3.zoomIdentity);
                        }
                        break;
                    case 'f': // Focus selected
                        focusSelectedNode();
                        break;
                    case 'Escape': // Clear selection
                        if (sprottyInstance) {
                            sprottyInstance.hideInfo();
                        }
                        break;
                    case '1': // Switch to workflow view
                    case '2': // Switch to agent view
                    case '3': // Switch to human input view
                    case '4': // Switch to LLM integration view
                        const viewIndex = parseInt(event.key) - 1;
                        if (viewIndex >= 0 && viewIndex < graphTypeSelect.options.length) {
                            graphTypeSelect.selectedIndex = viewIndex;
                            loadGraph();
                        }
                        break;
                }
            }

            // Focus on the selected node
            function focusSelectedNode() {
                if (!sprottyInstance) return;

                const selectedNode = sprottyInstance.nodes.find(
                    node => sprottyInstance.svg.select(`#group-${node.id} rect.selected`).size() > 0
                );

                if (selectedNode && selectedNode.x && selectedNode.y) {
                    const width = sprottyInstance.container.clientWidth;
                    const height = sprottyInstance.container.clientHeight;

                    // Calculate transform to center the node
                    const transform = d3.zoomIdentity
                        .translate(width/2, height/2)
                        .scale(1.5)
                        .translate(-selectedNode.x, -selectedNode.y);

                    // Apply the transform with a smooth transition
                    sprottyInstance.svg
                        .transition()
                        .duration(500)
                        .call(sprottyInstance.zoom.transform, transform);
                }
            }

            // Toggle display of keyboard shortcuts help
            function toggleShortcutHelp() {
                // Create the help panel if it doesn't exis
                let helpPanel = document.querySelector('.keyboard-shortcuts');

                if (!helpPanel) {
                    helpPanel = document.createElement('div');
                    helpPanel.className = 'keyboard-shortcuts';
                    helpPanel.innerHTML = `
                        <h3>Keyboard Shortcuts</h3>
                        <table>
                            <tr><td><kbd>?</kbd></td><td>Toggle this help</td></tr>
                            <tr><td><kbd>+</kbd>/<kbd>-</kbd></td><td>Zoom in/out</td></tr>
                            <tr><td><kbd>0</kbd></td><td>Reset zoom</td></tr>
                            <tr><td><kbd>f</kbd></td><td>Focus selected node</td></tr>
                            <tr><td><kbd>Esc</kbd></td><td>Clear selection</td></tr>
                            <tr><td><kbd>1</kbd>-<kbd>4</kbd></td><td>Switch graph view</td></tr>
                        </table>
                    `;
                    vizContainer.appendChild(helpPanel);
                }

                // Toggle visibility
                helpPanel.style.display = helpPanel.style.display === 'none' ? 'block' : 'none';
            }
            "#.to_string();
    }

    html += r#"
        </script>
    </body>
</html>"#;

    html
}

/// WebSocket connection handler with state
async fn ws_handler_with_state(
    ws: WebSocketUpgrade,
    state: Arc<WebServerState>,
    client_id: String,
) -> impl IntoResponse {
    debug!(
        "New WebSocket connection request from client: {}",
        client_id
    );

    // Upgrade the WebSocket connection
    ws.on_upgrade(move |socket| handle_socket(socket, state, client_id))
}

/// Handle WebSocket connection for terminal
async fn handle_socket(socket: WebSocket, state: Arc<WebServerState>, client_id: String) {
    let (mut sender, mut receiver) = socket.split();

    // Create a unique client ID
    let client_id = format!(
        "client-{}",
        uuid::Uuid::new_v4().to_string().split('-').next().unwrap()
    );

    // Create an mpsc channel for this client (not broadcast)
    let (tx, mut rx) = mpsc::channel::<Message>(100);

    // Register the clien
    {
        let mut clients = state.clients.write().await;
        clients.insert(client_id.clone(), tx.clone());
        debug!("Client connected: {}", client_id);
    }

    // Send welcome message
    if let Err(e) = sender
        .send(Message::Text(
            "Welcome to the MCP Agent Web Terminal!".to_string(),
        ))
        .awai
    {
        error!("Error sending welcome message: {}", e);
    }

    // Process messages from the channel
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            // Forward the message to the clien
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Process incoming messages from the WebSocke
    let clients_clone = state.clients.clone();
    let client_id_clone = client_id.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(Message::Text(text))) = receiver.next().await {
            debug!("Received message from client {}: {}", client_id_clone, text);

            // Echo the message to all clients
            let clients = clients_clone.lock().await;
            for (id, tx) in clients.iter() {
                if id != &client_id_clone {
                    // Don't send back to the originator
                    let _ = tx.send(format!("<{}>: {}", client_id_clone, text));
                }
            }
        }

        debug!("Client disconnected: {}", client_id_clone);
    });

    // Wait for either task to complete
    tokio::select! {
        _ = &mut recv_task => recv_task.abort(),
    }

    // Remove the clien
    let mut clients_lock = state.clients.write().await;
    clients_lock.remove(&client_id);
}

/// Authentication handler for JWT with state
#[cfg(feature = "terminal-full")]
async fn auth_handler_with_state(state: Arc<WebServerState>) -> Result<impl IntoResponse> {
    // Sample login form - in a real application, this would be a proper HTML form
    let login_form = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>MCP-Agent Login</title>
            <style>
                body { font-family: Arial, sans-serif; margin: 0; padding: 20px; }
                .login-form { max-width: 400px; margin: 0 auto; padding: 20px; border: 1px solid #ddd; border-radius: 5px; }
                .form-group { margin-bottom: 15px; }
                label { display: block; margin-bottom: 5px; }
                input { width: 100%; padding: 8px; border: 1px solid #ddd; border-radius: 4px; }
                button { padding: 10px 15px; background-color: #4CAF50; color: white; border: none; border-radius: 4px; cursor: pointer; }
                button:hover { background-color: #45a049; }
            </style>
        </head>
        <body>
            <div class="login-form">
                <h2>MCP-Agent Login</h2>
                <form action="/auth" method="post">
                    <div class="form-group">
                        <label for="username">Username:</label>
                        <input type="text" id="username" name="username" required>
                    </div>
                    <div class="form-group">
                        <label for="password">Password:</label>
                        <input type="password" id="password" name="password" required>
                    </div>
                    <button type="submit">Login</button>
                </form>
            </div>
        </body>
        </html>
    "#;

    // Return the login form
    Ok(Html(login_form))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::config::AuthConfig;

    #[tokio::test]
    async fn test_web_terminal_id() {
        let terminal = WebTerminal::new();

        let id = terminal.id().await.unwrap();
        assert_eq!(id, uuid::Uuid::new_v4().to_string());
    }

    #[tokio::test]
    async fn test_set_input_callback() {
        let mut terminal = WebTerminal::new();

        // Initially no callback
        assert!(terminal.input_callback.is_none());

        // Set a callback
        terminal.set_input_callback(|_, _| Ok(()));

        // Now there should be a callback
        assert!(terminal.input_callback.is_some());
    }

    #[test]
    fn test_web_terminal_config_default() {
        let config = WebTerminalConfig::default();
        assert_eq!(
            config.host,
            std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
        );
        assert_eq!(config.port, 8888);
        assert!(config.enable_visualization);
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
