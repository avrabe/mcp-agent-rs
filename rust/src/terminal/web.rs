// Terminal Web Module
//! Web-based terminal implementation
//!
//! This module provides a terminal interface accessible via a web browser.
//! It includes a WebSocket-based terminal, authentication, and graph visualization.
//!
//! Note: Full WebSocket server functionality requires the `transport-ws` feature.

use futures::SinkExt;
use futures::StreamExt;
use std::collections::HashMap;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{
    extract::ws::Message,
    http::{header, HeaderMap},
    routing::get,
    Router,
};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};

use crate::error::{Error, Result};
use crate::terminal::config::WebTerminalConfig;
use crate::terminal::graph;
use crate::terminal::graph::Graph;
use crate::terminal::{AsyncTerminal, Terminal};
use crate::workflow::signal::SignalHandler;

// Use conditional compilation for axum_server imports (requires transport-ws feature)

// Define a fallback Server type when transport-ws is not available
#[cfg(not(feature = "transport-ws"))]
mod server_fallback {
    pub struct Server;

    impl Server {
        pub fn bind(_addr: std::net::SocketAddr) -> Self {
            Self
        }

        pub async fn serve<S>(&self, _svc: S) -> Result<(), crate::error::Error>
        where
            S: std::fmt::Debug + Send + 'static,
        {
            Err(crate::error::Error::NotImplemented(
                "Server requires the transport-ws feature".to_string(),
            ))
        }
    }
}

#[cfg(not(feature = "transport-ws"))]
use server_fallback::Server;

/// A message to be sent to/from the terminal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalMessage {
    /// The unique message ID.
    pub id: String,
    /// The message content.
    pub content: String,
    /// The time when the message was created.
    pub timestamp: u64,
    /// The source of the message (user, system, etc.).
    pub source: String,
}

/// Static CSS headers for caching
static STATUS_CSS_HEADERS: once_cell::sync::Lazy<HeaderMap> = once_cell::sync::Lazy::new(|| {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "text/css".parse().unwrap());
    headers.insert(
        header::CACHE_CONTROL,
        "public, max-age=31536000".parse().unwrap(),
    );
    headers
});

/// Static JS headers for caching
static STATUS_JS_HEADERS: once_cell::sync::Lazy<HeaderMap> = once_cell::sync::Lazy::new(|| {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "application/javascript".parse().unwrap(),
    );
    headers.insert(
        header::CACHE_CONTROL,
        "public, max-age=31536000".parse().unwrap(),
    );
    headers
});

/// Messages for graph visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SprottyMessage {
    #[serde(rename = "type")]
    pub type_field: String,

    #[serde(rename = "graphType", skip_serializing_if = "Option::is_none")]
    pub graph_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>, // Use String for serialized JSON
}

/// Authentication request for JWT token
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthRequest {
    username: String,
    password: String,
}

/// Authentication response with JWT token
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthResponse {
    success: bool,
    token: Option<String>,
    message: Option<String>,
}

/// Static CSS content
static TERMINAL_CSS: &str = r#"/* Terminal CSS will go here */"#;

/// Static JavaScript content
static TERMINAL_JS: &str = r#"/* Terminal JavaScript will go here */"#;

/// The static HTML content for the terminal
static TERMINAL_HTML: &str = r#"<!DOCTYPE html>
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
        </style>
    </head>
    <body>
        <h1>MCP Agent Web Terminal</h1>
        <div class="controls">
            <button id="clear-btn">Clear</button>
            <button id="toggle-viz">Show Visualization</button>
            <select id="graph-type">
                <option value="workflow">Workflow</option>
                <option value="agent">Agent System</option>
                <option value="human">Human Input</option>
                <option value="llm">LLM Integration</option>
            </select>
            <button id="reset-zoom">Reset View</button>
        </div>
        <div id="visualization" class="visualization">
            <div id="sprotty-container" class="sprotty"></div>
            <div class="visualization-controls" aria-label="Visualization Controls">
                <button id="zoom-in" title="Zoom In">+</button>
                <button id="zoom-out" title="Zoom Out">-</button>
                <button id="zoom-reset" title="Reset View">⟲</button>
                <button id="fit-to-screen" title="Fit to Screen">⛶</button>
            </div>
        </div>
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
                });

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
                }
            });
        </script>
    </body>
</html>"#;

/// Client ID type alias
type ClientId = String;

/// Information about a connected client
#[derive(Debug, Clone)]
struct ClientInfo {
    /// ID of the client
    id: ClientId,
    /// Last activity timestamp
    last_active: u64,
    /// Sender for WebSocket messages
    sender: Option<mpsc::UnboundedSender<Message>>,
}

/// Input callback function type
type InputCallback = Arc<dyn Fn(&str, &str) -> Result<()> + Send + Sync>;

/// Used for JWT claims for authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// The subject (username).
    pub sub: String,
    /// The expiration time.
    pub exp: u64,
}

/// Messages for graph visualization
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SprottyModelUpdate {
    #[serde(rename = "type")]
    pub update_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<WebSprottyGraph>,
}

/// Add a CommandHandler type definition
pub type CommandHandler = dyn Fn(&str) -> Result<String> + Send + Sync;

/// Define WebSprottyGraph and related structs before WebTerminalState
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSprottyGraph {
    /// Unique identifier for the graph
    pub id: String,
    /// Type of the graph element
    #[serde(rename = "type")]
    pub type_field: String,
    /// Child elements in the graph
    pub children: Vec<SprottyElement>,
}

/// An element in the Sprotty graph model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprottyElement {
    /// The unique identifier for the element.
    pub id: String,
    /// The type of the element.
    #[serde(rename = "type")]
    pub type_field: String,
    /// The position of the element in the graph.
    pub position: Position,
}

/// The position of an element in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    /// The X coordinate.
    pub x: f64,
    /// The Y coordinate.
    pub y: f64,
}

/// Token information for JWT authentication
#[derive(Debug, Clone)]
pub struct TokenInfo {
    /// Expiration timestamp for the token
    pub expires: u64,
    /// Username associated with the token
    pub username: String,
}

/// State for the web terminal
pub struct WebTerminalState {
    /// Map of connected clients
    pub clients: HashMap<ClientId, ClientInfo>,
    /// Terminal configuration
    pub config: WebTerminalConfig,
    /// Callbacks for client input
    pub callbacks: HashMap<ClientId, Arc<dyn Fn(&str, &str) -> Result<String> + Send + Sync>>,
    /// Shared state objects
    pub shared_states: HashMap<String, Arc<dyn std::any::Any + Send + Sync>>,
    /// Graph manager
    pub graph_manager: Option<Arc<graph::GraphManager>>,
    /// Command handlers
    pub command_handlers: HashMap<String, Arc<dyn Fn(&str, &str) -> Result<String> + Send + Sync>>,
    /// Authentication tokens
    pub tokens: HashMap<String, String>,
    /// Server address
    pub server_address: Option<SocketAddr>,
    /// Running state
    pub running: bool,
}

impl std::fmt::Debug for WebTerminalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebTerminalState")
            .field("clients", &self.clients)
            .field("config", &self.config)
            .field("callbacks_count", &self.callbacks.len())
            .field("shared_states", &self.shared_states)
            .field("graph_manager", &self.graph_manager)
            .field("command_handlers_count", &self.command_handlers.len())
            .field("tokens", &self.tokens)
            .field("server_address", &self.server_address)
            .field("running", &self.running)
            .finish()
    }
}

/// Web terminal implementation for the terminal interface
pub struct WebTerminal {
    /// The ID of this terminal
    id: String,
    /// The host to listen on
    host: String,
    /// The port to listen on
    port: u16,
    /// Whether the terminal is running
    is_running: AtomicBool,
    /// The shared state for the terminal
    state: Arc<Mutex<WebTerminalState>>,
    /// Sender for console log messages
    console_sender: Arc<Mutex<Option<UnboundedSender<TerminalMessage>>>>,
    /// Signal handler for handling OS signals
    signal_handler: Box<dyn SignalHandler>,
    /// Shutdown channel sender for stopping the server
    shutdown_tx: Mutex<Option<UnboundedSender<()>>>,
    /// Shutdown channel receiver
    shutdown_rx: Mutex<Option<UnboundedReceiver<()>>>,
    /// Broadcast channel sender
    broadcast_tx: broadcast::Sender<String>,
    /// Broadcast channel receiver
    broadcast_rx: broadcast::Receiver<String>,
}

impl Clone for WebTerminal {
    fn clone(&self) -> Self {
        let rx = self.broadcast_tx.subscribe();

        Self {
            id: self.id.clone(),
            host: self.host.clone(),
            port: self.port,
            is_running: AtomicBool::new(self.is_running.load(Ordering::SeqCst)),
            state: Arc::clone(&self.state),
            console_sender: Arc::clone(&self.console_sender),
            signal_handler: Box::new(crate::workflow::signal::NullSignalHandler::new()),
            shutdown_tx: Mutex::new(None),
            shutdown_rx: Mutex::new(None),
            broadcast_tx: self.broadcast_tx.clone(),
            broadcast_rx: rx,
        }
    }
}

impl std::fmt::Debug for WebTerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebTerminal")
            .field("id", &self.id)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("is_running", &self.is_running)
            .finish()
    }
}

impl WebTerminal {
    /// Create a new web terminal with the given configuration
    pub fn new(
        config: WebTerminalConfig,
        signal_handler: Box<dyn SignalHandler>,
        server_address: Option<SocketAddr>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = unbounded_channel::<()>();
        let (broadcast_tx, broadcast_rx) = broadcast::channel(100);

        let state = WebTerminalState {
            clients: HashMap::new(),
            config,
            callbacks: HashMap::new(),
            shared_states: HashMap::new(),
            graph_manager: None,
            command_handlers: HashMap::new(),
            tokens: HashMap::new(),
            server_address,
            running: false,
        };

        Self {
            id: "web".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8080,
            is_running: AtomicBool::new(false),
            state: Arc::new(Mutex::new(state)),
            console_sender: Arc::new(Mutex::new(None)),
            signal_handler,
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            shutdown_rx: Mutex::new(Some(shutdown_rx)),
            broadcast_tx,
            broadcast_rx,
        }
    }

    /// Set the terminal ID
    pub fn set_id(&mut self, id: String) {
        self.id = id;
    }

    /// Set the terminal configuration
    pub fn set_config(&mut self, config: WebTerminalConfig) {
        // Create a clone of state to avoid blocking
        let state_clone = self.state.clone();
        let config_clone = config;

        // Since this is a sync function, we can't use .await directly
        // Instead, we spawn a task that will update the state
        tokio::spawn(async move {
            let mut state = state_clone.lock().await;
            state.config = config_clone;
        });
    }

    /// Set an input callback function for handling terminal input
    pub fn set_input_callback(
        &mut self,
        callback: impl Fn(String, String) -> Result<String> + Send + Sync + 'static,
    ) -> Result<()> {
        // Create a wrapper that converts String to &str
        let callback_wrapper = move |input: &str, client_id: &str| -> Result<String> {
            callback(input.to_string(), client_id.to_string())
        };

        let callback_arc = Arc::new(callback_wrapper);

        // Create a clone of state to avoid blocking
        let state_clone = self.state.clone();

        // Since this is a sync function, we can't use .await directly
        // Instead, we spawn a task that will update the state
        tokio::spawn(async move {
            let mut state = state_clone.lock().await;
            state
                .callbacks
                .insert("system".to_string(), callback_arc.clone());
        });

        Ok(())
    }

    /// Set the graph manager for visualization
    pub async fn set_graph_manager(&mut self, graph_manager: Arc<graph::GraphManager>) {
        debug!("Setting graph manager in web terminal");

        // Store the graph manager locally
        let mut state = self.state.lock().await;
        state.graph_manager = Some(graph_manager.clone());

        // Log that we've set the graph manager
        debug!("Graph manager set in web terminal state");
    }

    /// Start the web terminal server
    pub async fn start(&mut self) -> Result<()> {
        if self.is_running.load(Ordering::SeqCst) {
            debug!("Web terminal server already running");
            return Ok(());
        }

        self.is_running.store(true, Ordering::SeqCst);

        // Create config from the current state
        let config = {
            let mut state_guard = self.state.lock().await;
            state_guard.running = true;
            state_guard.config.clone()
        };

        // Create the address to bind to
        let addr = SocketAddr::new(config.host, config.port);

        info!("Starting web terminal server on {}", addr);

        // Create the router
        let app = self.create_router("").await?;

        // Create a oneshot channel to notify when the server is done
        let (shutdown_tx, shutdown_rx) = mpsc::unbounded_channel::<()>();

        // Store the shutdown sender
        {
            let mut shutdown_guard = self.shutdown_tx.lock().await;
            *shutdown_guard = Some(shutdown_tx);
        }

        // Create a TCP listener
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| Error::from(format!("Failed to bind to address: {}", e)))?;

        // Get the actual bound address
        let server_addr = listener
            .local_addr()
            .map_err(|e| Error::from(format!("Failed to get local address: {}", e)))?;

        // Store the actual bound address
        {
            let mut state = self.state.lock().await;
            state.server_address = Some(server_addr);
            info!("Web terminal server bound to {}", server_addr);
        }

        // Create and start the server
        let server_task = tokio::spawn(async move {
            debug!("Web terminal server task started");

            // Use the axum::serve pattern
            let server = axum::serve(listener, app);

            // Wait for the server to complete or shutdown
            if let Err(e) = server.await {
                error!("Server error: {}", e);
            }

            debug!("Web terminal server stopped");
        });

        Ok(())
    }

    /// Broadcast a message to all connected clients
    pub async fn broadcast(&self, message: &str) -> Result<()> {
        // Send message to broadcast channel
        if let Err(e) = self.broadcast_tx.send(message.to_string()) {
            warn!("Failed to broadcast message: {}", e);
        }
        Ok(())
    }

    /// Get the graph manager (if enabled)
    pub async fn graph_manager(&self) -> Option<Arc<graph::GraphManager>> {
        let state = self.state.lock().await;
        state.graph_manager.clone()
    }

    /// Get a locked reference to the web terminal
    pub async fn lock(&self) -> impl std::ops::Deref<Target = WebTerminal> + '_ {
        struct WebTerminalGuard<'a> {
            terminal: &'a WebTerminal,
        }

        impl std::ops::Deref for WebTerminalGuard<'_> {
            type Target = WebTerminal;

            fn deref(&self) -> &Self::Target {
                self.terminal
            }
        }

        WebTerminalGuard { terminal: self }
    }

    /// Update the graph from JSON data
    pub async fn update_graph(&self, graph_data: &str) -> Result<()> {
        log::debug!("Updating graph with data: {}", graph_data);
        let state = self.state.lock().await;

        match serde_json::from_str::<WebSprottyGraph>(graph_data) {
            Ok(graph) => {
                if let Some(graph_manager) = &state.graph_manager {
                    // Create a new Graph to register with both id and name
                    let new_graph = Graph::new(&graph.id, &graph.id); // Using id as name too for simplicity
                    log::debug!("Registering graph with ID: {}", graph.id);
                    graph_manager.register_graph(new_graph).await?;
                    Ok(())
                } else {
                    Err("No graph manager available".into())
                }
            }
            Err(e) => {
                log::error!("Failed to parse graph data: {}", e);
                Err(format!("Failed to parse graph data: {}", e).into())
            }
        }
    }

    /// Broadcast a graph update to all connected clients
    async fn broadcast_graph_update(&self, graph_id: &str) -> Result<()> {
        let state = self.state.lock().await;

        if let Some(graph_manager) = &state.graph_manager {
            let graph_data = graph_manager.get_graph(graph_id).await;

            if let Some(graph) = graph_data {
                let graph_json = serde_json::to_string(&graph)?;
                let message = format!("GRAPH_UPDATE:{}", graph_json);

                if let Err(e) = self.broadcast_tx.send(message) {
                    warn!("Failed to broadcast graph update: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Set a command handler function for processing terminal commands
    pub async fn with_command_handler<F>(self, handler: F) -> Self
    where
        F: Fn(&str, &str) -> Result<String> + Send + Sync + 'static,
    {
        // Create Arc wrapper for handler
        let handler_arc = Arc::new(handler);

        // Update state
        {
            let mut state = self.state.lock().await;

            // Add the handler for the system client
            state
                .command_handlers
                .insert("system".to_string(), handler_arc.clone());

            // Collect client IDs to avoid borrowing issues
            let client_ids: Vec<String> = state.clients.keys().cloned().collect();

            // Add handlers for existing clients
            for client_id in client_ids {
                state
                    .command_handlers
                    .insert(client_id, handler_arc.clone());
            }
        }

        self
    }

    /// Subscribe to WebTerminal events
    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.broadcast_tx.subscribe()
    }

    /// Handle a command received from the WebTerminal
    pub async fn handle_command(&self, command: String) -> Result<String> {
        let (tx, rx) = oneshot::channel();
        self.execute_command(&command, tx).await?;
        match tokio::time::timeout(Duration::from_secs(5), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(Error::CommandError(
                "Failed to receive response".to_string(),
            )),
            Err(_) => Err(Error::CommandError(
                "Command execution timed out".to_string(),
            )),
        }
    }

    /// Generate static terminal HTML without capturing self
    fn get_terminal_html(config: &WebTerminalConfig) -> String {
        let mut html = TERMINAL_HTML.to_string();

        // If visualization is disabled, modify the HTML to remove visualization elements
        if !config.enable_visualization {
            // Simple string replacements to hide visualization UI elements
            html = html.replace(r#"<button id="toggle-viz">Show Visualization</button>"#, "");
            html = html.replace(
                r#"<select id="graph-type">
                <option value="workflow">Workflow</option>
                <option value="agent">Agent System</option>
                <option value="human">Human Input</option>
                <option value="llm">LLM Integration</option>
            </select>"#,
                "",
            );
            html = html.replace(r#"<button id="reset-zoom">Reset View</button>"#, "");
            html = html.replace(
                r#"<div id="visualization" class="visualization">
            <div id="sprotty-container" class="sprotty"></div>
            <div class="visualization-controls" aria-label="Visualization Controls">
                <button id="zoom-in" title="Zoom In">+</button>
                <button id="zoom-out" title="Zoom Out">-</button>
                <button id="zoom-reset" title="Reset View">⟲</button>
                <button id="fit-to-screen" title="Fit to Screen">⛶</button>
            </div>
        </div>"#,
                "",
            );
        }

        html
    }

    /// Generate the terminal HTML
    pub async fn generate_html(&self) -> String {
        let state = self.state.lock().await;
        Self::get_terminal_html(&state.config)
    }

    /// Initialize the web server
    async fn initialize_server(&self) -> Result<()> {
        // Don't start if not active
        if !self.is_running.load(Ordering::SeqCst) {
            debug!("Web terminal not active, skipping server initialization");
            return Ok(());
        }

        // Add all the handlers here
        debug!("Initializing web terminal server handlers");

        Ok(())
    }

    /// Create the HTTP router for the terminal web interface
    pub async fn create_router(&self, _prefix: &str) -> Result<Router> {
        debug!("Creating web terminal router");

        // Get the state
        let state = self.state.clone();

        // Create a simple router for now - we'll add more routes later
        let router = Router::new()
            .route("/", get(|| async { "MCP Terminal Server" }))
            .with_state(state);

        Ok(router)
    }

    /// Get a reference as a terminal guard
    pub fn guard(&self) -> &Self {
        self
    }

    /// Fix the stop method to match the trait
    pub async fn stop(&self) -> Result<()> {
        // Simply mark as not running and return
        // This is a simplified approach to avoid any blocking operations or locks
        self.is_running.store(false, Ordering::SeqCst);
        log::debug!("Web terminal server marked as stopped");

        // Don't try to acquire locks - just send shutdown signal if we can get the lock without blocking
        if let Ok(mut shutdown_guard) = self.shutdown_tx.try_lock() {
            if let Some(tx) = shutdown_guard.take() {
                let _ = tx.send(());
                log::debug!("Sent shutdown signal to web server");
            }
        }

        Ok(())
    }

    // Implementation of basic Terminal interface sync methods
    fn write(&mut self, s: &str) -> Result<()> {
        // Convert to async using tokio::spawn and return a result
        let message = s.to_string();
        let broadcast_tx = self.broadcast_tx.clone();

        tokio::spawn(async move {
            if let Err(e) = broadcast_tx.send(message) {
                error!("Failed to broadcast message: {}", e);
            }
        });

        Ok(())
    }

    fn write_line(&mut self, s: &str) -> Result<()> {
        // Add a newline and write
        let message = format!("{}\n", s);
        self.write(&message)
    }

    fn read_line(&mut self) -> Result<String> {
        // This is a synchronous method but our interaction is async
        // For now, return a placeholder and warn that this is not fully implemented
        warn!("read_line called synchronously on WebTerminal, returning empty string");
        Ok("".to_string())
    }

    fn flush(&mut self) -> Result<()> {
        // WebSocket-based output doesn't need flushing, so this is a no-op
        Ok(())
    }

    fn read_password(&mut self, prompt: &str) -> Result<String> {
        // Similar to read_line, this isn't really supported in a synchronous way
        warn!("read_password called synchronously on WebTerminal with prompt '{}', returning empty string", prompt);
        Ok("".to_string())
    }

    fn read_secret(&mut self, prompt: &str) -> Result<String> {
        // Similar to read_password
        warn!("read_secret called synchronously on WebTerminal with prompt '{}', returning empty string", prompt);
        Ok("".to_string())
    }

    fn as_terminal(&self) -> &dyn Terminal {
        self
    }

    /// Return the terminal address if the server is running
    pub async fn terminal_address(&self) -> Option<String> {
        let state = self.state.lock().await;

        if !state.running {
            return None;
        }

        // Use the actual bound address from state rather than the configured address
        if let Some(addr) = state.server_address {
            let protocol = "http"; // Assume HTTP for now
            let address = format!("{}://{}", protocol, addr);
            info!("Web terminal address: {}", address);
            Some(address)
        } else {
            // Fallback to the configured address
            let protocol = "http"; // Assume HTTP for now
            let host = state.config.host.to_string();
            let port = state.config.port;
            let address = format!("{}://{}:{}", protocol, host, port);
            debug!("Web terminal address (fallback): {}", address);
            Some(address)
        }
    }

    /// Display output to the terminal
    async fn display(&self, output: &str) -> Result<()> {
        // For now, just log the output
        debug!("WebTerminal display: {}", output);
        Ok(())
    }
}

impl Default for WebTerminal {
    /// Create a default Web Terminal instance
    fn default() -> Self {
        let (shutdown_tx, shutdown_rx) = unbounded_channel::<()>();
        let (broadcast_tx, broadcast_rx) = broadcast::channel(100);

        Self {
            id: "web".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8080,
            is_running: AtomicBool::new(false),
            state: Arc::new(Mutex::new(WebTerminalState {
                clients: HashMap::new(),
                config: WebTerminalConfig::default(),
                callbacks: HashMap::new(),
                shared_states: HashMap::new(),
                graph_manager: None,
                command_handlers: HashMap::new(),
                tokens: HashMap::new(),
                server_address: None,
                running: false,
            })),
            console_sender: Arc::new(Mutex::new(None)),
            signal_handler: Box::new(crate::workflow::signal::NullSignalHandler::new()),
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            shutdown_rx: Mutex::new(Some(shutdown_rx)),
            broadcast_tx,
            broadcast_rx,
        }
    }
}

#[async_trait]
impl AsyncTerminal for WebTerminal {
    async fn id(&self) -> Result<String> {
        Ok(self.id.clone())
    }

    async fn start(&mut self) -> Result<()> {
        // Call the struct method directly
        WebTerminal::start(self).await
    }

    async fn stop(&self) -> Result<()> {
        // Call the struct method directly
        WebTerminal::stop(self).await
    }

    async fn display(&self, output: &str) -> Result<()> {
        // Call the struct's display method directly
        WebTerminal::display(self, output).await
    }

    async fn echo_input(&self, input: &str) -> Result<()> {
        debug!("Echo input: {}", input);
        Ok(())
    }

    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<()> {
        info!("Executing command: {}", command);

        // For now just respond with a simple message
        if tx.send(format!("Command executed: {}", command)).is_err() {
            error!("Failed to send response");
        }
        Ok(())
    }

    /// Get the terminal address if it has one
    async fn terminal_address(&self) -> Option<String> {
        // Call the struct's terminal_address method directly
        WebTerminal::terminal_address(self).await
    }
}
