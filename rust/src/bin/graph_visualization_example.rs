//! Graph Visualization Example
//!
//! This example demonstrates the graph visualization capabilities,
//! with a basic visualization of a graph structure without direct integration
//! with the workflow engine.

#[cfg(feature = "terminal-web")]
use {
    axum::{
        extract::{Path, State, WebSocketUpgrade},
        response::{Html, IntoResponse, Json},
        routing::{get, post},
        Router,
    },
    futures::{SinkExt, StreamExt},
    serde::{Deserialize, Serialize},
    std::collections::HashMap,
    std::fmt,
    std::net::SocketAddr,
    std::sync::Arc,
    std::time::Duration,
    tokio::sync::mpsc,
    tokio::sync::RwLock,
    tokio::time::sleep,
    tracing::{error, info},
    uuid::Uuid,
};

// Custom error type for the example
#[cfg(feature = "terminal-web")]
#[derive(Debug)]
pub enum Error {
    TerminalError(String),
    WebSocketError(String),
}

#[cfg(feature = "terminal-web")]
impl std::error::Error for Error {}

#[cfg(feature = "terminal-web")]
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TerminalError(msg) => write!(f, "Terminal error: {}", msg),
            Error::WebSocketError(msg) => write!(f, "WebSocket error: {}", msg),
        }
    }
}

// Type alias for our Result type
pub type Result<T> = std::result::Result<T, Error>;

// Return type when returning our custom error through '?' operator
type CustomResult<T> = std::result::Result<T, Error>;

#[cfg(feature = "terminal-web")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub id: String,
    pub name: String,
    pub graph_type: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub properties: HashMap<String, String>,
}

#[cfg(feature = "terminal-web")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub name: String,
    pub node_type: String,
    pub status: String,
    pub properties: HashMap<String, String>,
}

#[cfg(feature = "terminal-web")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub edge_type: String,
    pub properties: HashMap<String, String>,
}

#[cfg(feature = "terminal-web")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphUpdate {
    pub graph_id: String,
    pub update_type: GraphUpdateType,
    pub graph: Option<Graph>,
    pub node: Option<GraphNode>,
    pub edge: Option<GraphEdge>,
}

#[cfg(feature = "terminal-web")]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum GraphUpdateType {
    FullUpdate,
    NodeAdded,
    NodeUpdated,
    EdgeAdded,
}

#[cfg(feature = "terminal-web")]
pub struct GraphManager {
    graphs: RwLock<HashMap<String, Graph>>,
    update_channels: RwLock<Vec<mpsc::Sender<GraphUpdate>>>,
}

#[cfg(feature = "terminal-web")]
impl Default for GraphManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphManager {
    pub fn new() -> Self {
        Self {
            graphs: RwLock::new(HashMap::new()),
            update_channels: RwLock::new(Vec::new()),
        }
    }

    pub async fn register_graph(&self, graph: Graph) -> Result<()> {
        let graph_id = graph.id.clone();
        {
            let mut graphs = self.graphs.write().await;
            graphs.insert(graph_id.clone(), graph.clone());
        }

        // Notify subscribers of the full graph update
        self.notify_update(GraphUpdate {
            graph_id,
            update_type: GraphUpdateType::FullUpdate,
            graph: Some(graph),
            node: None,
            edge: None,
        })
        .await;

        Ok(())
    }

    pub async fn add_node(&self, graph_id: &str, node: GraphNode) -> Result<()> {
        let node_clone = node.clone();
        let result = {
            let mut graphs = self.graphs.write().await;

            if let Some(graph) = graphs.get_mut(graph_id) {
                graph.nodes.push(node);
                Ok(())
            } else {
                Err(Error::TerminalError(format!(
                    "Graph {} not found",
                    graph_id
                )))
            }
        };

        // If successful, notify subscribers of the node addition
        if result.is_ok() {
            self.notify_update(GraphUpdate {
                graph_id: graph_id.to_string(),
                update_type: GraphUpdateType::NodeAdded,
                graph: None,
                node: Some(node_clone),
                edge: None,
            })
            .await;
        }

        result
    }

    pub async fn add_edge(&self, graph_id: &str, edge: GraphEdge) -> Result<()> {
        let edge_clone = edge.clone();
        let result = {
            let mut graphs = self.graphs.write().await;

            if let Some(graph) = graphs.get_mut(graph_id) {
                graph.edges.push(edge);
                Ok(())
            } else {
                Err(Error::TerminalError(format!(
                    "Graph {} not found",
                    graph_id
                )))
            }
        };

        // If successful, notify subscribers of the edge addition
        if result.is_ok() {
            self.notify_update(GraphUpdate {
                graph_id: graph_id.to_string(),
                update_type: GraphUpdateType::EdgeAdded,
                graph: None,
                node: None,
                edge: Some(edge_clone),
            })
            .await;
        }

        result
    }

    async fn notify_update(&self, update: GraphUpdate) -> Result<()> {
        // Clone the channels to avoid holding the lock during await
        let channels_clone = {
            let channels = self.update_channels.read().await;
            channels.clone()
        };

        for channel in channels_clone.iter() {
            if let Err(e) = channel.send(update.clone()).await {
                error!("Failed to send graph update: {}", e);
            }
        }
        Ok(())
    }

    pub async fn get_graph(&self, graph_id: &str) -> Option<Graph> {
        let graphs = self.graphs.read().await;
        graphs.get(graph_id).cloned()
    }

    pub async fn update_node(&self, graph_id: &str, node: GraphNode) -> Result<()> {
        let node_clone = node.clone();
        let result = {
            let mut graphs = self.graphs.write().await;

            if let Some(graph) = graphs.get_mut(graph_id) {
                if let Some(index) = graph.nodes.iter().position(|n| n.id == node.id) {
                    graph.nodes[index] = node;
                    Ok(())
                } else {
                    Err(Error::TerminalError(format!(
                        "Node {} not found in graph {}",
                        node_clone.id, graph_id
                    )))
                }
            } else {
                Err(Error::TerminalError(format!(
                    "Graph {} not found",
                    graph_id
                )))
            }
        };

        // If successful, notify subscribers of the node update
        if result.is_ok() {
            self.notify_update(GraphUpdate {
                graph_id: graph_id.to_string(),
                update_type: GraphUpdateType::NodeUpdated,
                graph: None,
                node: Some(node_clone),
                edge: None,
            })
            .await;
        }

        result
    }

    pub async fn register_update_channel(&self, sender: mpsc::Sender<GraphUpdate>) {
        let mut channels = self.update_channels.write().await;
        channels.push(sender);
    }
}

#[cfg(not(feature = "terminal-web"))]
fn main() {
    println!("This example requires the terminal-web feature. Please enable it with --features terminal-web");
}

#[cfg(feature = "terminal-web")]
/// Serve a simple HTML page that displays the graph visualization
async fn index_handler() -> impl IntoResponse {
    Html(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <title>MCP Graph Visualization</title>
    <style>
        body {
            font-family: system-ui, -apple-system, sans-serif;
            margin: 0;
            padding: 0;
            background-color: #f5f5f5;
            color: #333;
        }
        .container {
            display: flex;
            height: 100vh;
        }
        .sidebar {
            width: 300px;
            background-color: #2c3e50;
            color: white;
            padding: 15px;
            box-shadow: 2px 0 5px rgba(0,0,0,0.1);
            overflow-y: auto;
        }
        .content {
            flex-grow: 1;
            padding: 20px;
            display: flex;
            flex-direction: column;
        }
        #graph-container {
            flex-grow: 1;
            border: 1px solid #ddd;
            border-radius: 5px;
            background-color: white;
            position: relative;
        }
        h1, h2 {
            margin-top: 0;
        }
        .btn {
            background-color: #3498db;
            color: white;
            border: none;
            padding: 8px 15px;
            margin: 5px 0;
            border-radius: 4px;
            cursor: pointer;
            font-size: 14px;
        }
        .btn:hover {
            background-color: #2980b9;
        }
        .console {
            height: 150px;
            margin-top: 15px;
            background-color: #2c3e50;
            color: #ecf0f1;
            border-radius: 5px;
            padding: 10px;
            font-family: monospace;
            overflow-y: auto;
        }
        .node {
            padding: 10px;
            border-radius: 5px;
            cursor: pointer;
            margin-bottom: 8px;
            border-left: 4px solid #3498db;
            background-color: #f9f9f9;
        }
        .node.selected {
            background-color: #e0f0ff;
        }
        .control-panel {
            margin-bottom: 15px;
            padding: 15px;
            background-color: #ecf0f1;
            border-radius: 5px;
        }
        .actions {
            display: flex;
            flex-wrap: wrap;
            gap: 5px;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="sidebar">
            <h2>Graph Visualization</h2>
            <div id="node-list"></div>
            <div class="actions">
                <button class="btn" id="add-node">Add Node</button>
                <button class="btn" id="update-node">Update Status</button>
                <button class="btn" id="add-edge">Add Edge</button>
            </div>
        </div>
        <div class="content">
            <div class="control-panel">
                <h1>MCP Graph Visualization Demo</h1>
                <div class="actions">
                    <button class="btn" id="fit-to-screen">Fit to Screen</button>
                    <button class="btn" id="center-selection">Center Selected</button>
                    <button class="btn" id="apply-layout">Apply Layout</button>
                </div>
            </div>
            <div id="graph-container"></div>
            <div class="console" id="console-output"></div>
        </div>
    </div>

    <script>
        // Console logging
        function log(message) {
            const consoleOutput = document.getElementById('console-output');
            const logEntry = document.createElement('div');
            logEntry.textContent = `[${new Date().toLocaleTimeString()}] ${message}`;
            consoleOutput.appendChild(logEntry);
            consoleOutput.scrollTop = consoleOutput.scrollHeight;
        }

        // Initialize the WebSocket connection
        const socket = new WebSocket(`ws://${window.location.host}/ws`);
        
        socket.onopen = () => {
            log('WebSocket connection established');
        };
        
        socket.onmessage = (event) => {
            const data = JSON.parse(event.data);
            log(`Received update: ${data.update_type}`);
            
            if (data.update_type === "FullUpdate" && data.graph) {
                // Update the node list
                updateNodeList(data.graph);
            } else if (data.update_type === "NodeAdded" && data.node) {
                // Add a new node
                addNodeToList(data.node);
            } else if (data.update_type === "NodeUpdated" && data.node) {
                // Update an existing node
                updateNodeInList(data.node);
            } else if (data.update_type === "EdgeAdded" && data.edge) {
                log(`Edge added: ${data.edge.source} -> ${data.edge.target}`);
            }
        };
        
        socket.onclose = () => {
            log('WebSocket connection closed');
        };

        socket.onerror = (error) => {
            log(`WebSocket error: ${error}`);
        };

        // Update the node list in the sidebar
        function updateNodeList(graph) {
            if (!graph || !graph.nodes) return;
            
            const nodeList = document.getElementById('node-list');
            nodeList.innerHTML = '';
            
            graph.nodes.forEach(node => {
                const nodeElement = document.createElement('div');
                nodeElement.className = 'node';
                nodeElement.setAttribute('data-id', node.id);
                nodeElement.textContent = `${node.name} (${node.status})`;
                
                nodeElement.addEventListener('click', () => {
                    document.querySelectorAll('.node').forEach(n => n.classList.remove('selected'));
                    nodeElement.classList.add('selected');
                });
                
                nodeList.appendChild(nodeElement);
            });
            
            log(`Updated node list with ${graph.nodes.length} nodes`);
        }
        
        // Add a new node to the list
        function addNodeToList(node) {
            const nodeList = document.getElementById('node-list');
            const nodeElement = document.createElement('div');
            nodeElement.className = 'node';
            nodeElement.setAttribute('data-id', node.id);
            nodeElement.textContent = `${node.name} (${node.status})`;
            
            nodeElement.addEventListener('click', () => {
                document.querySelectorAll('.node').forEach(n => n.classList.remove('selected'));
                nodeElement.classList.add('selected');
            });
            
            nodeList.appendChild(nodeElement);
            log(`Added new node: ${node.id}`);
        }
        
        // Update an existing node in the list
        function updateNodeInList(node) {
            const nodeElement = document.querySelector(`.node[data-id="${node.id}"]`);
            if (nodeElement) {
                nodeElement.textContent = `${node.name} (${node.status})`;
                log(`Updated node: ${node.id}`);
            }
        }

        // Setup UI interaction
        document.getElementById('add-node').addEventListener('click', () => {
            const nodeName = prompt("Enter node name:", `Node ${Date.now()}`);
            if (!nodeName) return;
            
            fetch('/api/node', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    name: nodeName,
                    node_type: 'demo',
                    status: 'idle'
                })
            })
            .then(response => response.json())
            .then(data => {
                if (data.success) {
                    log(`Added node: ${nodeName}`);
                } else {
                    log(`Error adding node: ${data.error}`);
                }
            })
            .catch(error => log(`Error adding node: ${error}`));
        });

        document.getElementById('update-node').addEventListener('click', () => {
            const selectedNode = document.querySelector('.node.selected');
            if (!selectedNode) {
                log('No node selected');
                return;
            }
            
            const nodeId = selectedNode.getAttribute('data-id');
            const statuses = ['idle', 'running', 'completed', 'failed'];
            const randomStatus = statuses[Math.floor(Math.random() * statuses.length)];
            
            fetch(`/api/node/${nodeId}`, {
                method: 'PATCH',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    status: randomStatus
                })
            })
            .then(response => response.json())
            .then(data => {
                if (data.success) {
                    log(`Updated node status: ${nodeId} -> ${randomStatus}`);
                } else {
                    log(`Error updating node: ${data.error}`);
                }
            })
            .catch(error => log(`Error updating node: ${error}`));
        });

        document.getElementById('add-edge').addEventListener('click', () => {
            const nodes = document.querySelectorAll('.node');
            if (nodes.length < 2) {
                log('Need at least 2 nodes to create an edge');
                return;
            }
            
            // Select two random nodes that don't have an edge yet
            const nodeArray = Array.from(nodes);
            const source = nodeArray[Math.floor(Math.random() * nodeArray.length)];
            let target;
            do {
                target = nodeArray[Math.floor(Math.random() * nodeArray.length)];
            } while (source === target);
            
            const sourceId = source.getAttribute('data-id');
            const targetId = target.getAttribute('data-id');
            
            fetch('/api/edge', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    source: sourceId,
                    target: targetId,
                    edge_type: 'connects'
                })
            })
            .then(response => response.json())
            .then(data => {
                if (data.success) {
                    log(`Added edge: ${sourceId} -> ${targetId}`);
                } else {
                    log(`Error adding edge: ${data.error}`);
                }
            })
            .catch(error => log(`Error adding edge: ${error}`));
        });

        document.getElementById('fit-to-screen').addEventListener('click', () => {
            log('Fit to screen requested');
        });

        document.getElementById('center-selection').addEventListener('click', () => {
            const selectedNode = document.querySelector('.node.selected');
            if (!selectedNode) {
                log('No node selected');
                return;
            }
            
            const elementId = selectedNode.getAttribute('data-id');
            log(`Centering on node: ${elementId}`);
        });

        document.getElementById('apply-layout').addEventListener('click', () => {
            log('Applying layout');
        });
    </script>
</body>
</html>"#,
    )
}

#[cfg(feature = "terminal-web")]
/// WebSocket handler for graph updates
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

#[cfg(feature = "terminal-web")]
/// Handle WebSocket connection
async fn handle_socket(socket: axum::extract::ws::WebSocket, state: Arc<AppState>) {
    let (tx, mut rx) = mpsc::channel(100);

    // Register the channel with the graph manager
    state.graph_manager.register_update_channel(tx).await;

    // Create a task to forward updates to the client
    let (mut sender, mut receiver) = socket.split();

    let forward_task = tokio::spawn(async move {
        while let Some(update) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&update) {
                if let Err(e) = sender.send(axum::extract::ws::Message::Text(json)).await {
                    error!("Error sending update to WebSocket: {}", e);
                    break;
                }
            }
        }
    });

    // Handle incoming messages (mostly for ping/pong)
    let receive_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let axum::extract::ws::Message::Close(_) = msg {
                break;
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = forward_task => {},
        _ = receive_task => {},
    }
}

#[cfg(feature = "terminal-web")]
/// Handler for adding nodes to the graph
async fn add_node_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Extract node data
    let node_name = payload
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Unnamed Node");
    let node_type = payload
        .get("node_type")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    let status = payload
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("idle");

    // Create new node with UUID
    let node_id = Uuid::new_v4().to_string();
    let node = GraphNode {
        id: node_id.clone(),
        name: node_name.to_string(),
        node_type: node_type.to_string(),
        status: status.to_string(),
        properties: HashMap::new(),
    };

    // Add node to graph
    match state.graph_manager.add_node("example-graph", node).await {
        Ok(_) => Json(serde_json::json!({
            "success": true,
            "id": node_id
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string()
        })),
    }
}

#[cfg(feature = "terminal-web")]
/// Handler for updating nodes in the graph
async fn update_node_handler(
    Path(node_id): Path<String>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Extract node data
    let node_name = payload
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let node_type = payload
        .get("node_type")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let status = payload
        .get("status")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Get current node from graph
    let graph = match state.graph_manager.get_graph("example-graph").await {
        Some(g) => g,
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Graph not found"
            }));
        }
    };

    // Find the node in the graph
    let current_node = match graph.nodes.iter().find(|n| n.id == node_id) {
        Some(n) => n.clone(),
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Node not found"
            }));
        }
    };

    // Create updated node
    let updated_node = GraphNode {
        id: node_id,
        name: node_name.unwrap_or(current_node.name),
        node_type: node_type.unwrap_or(current_node.node_type),
        status: status.unwrap_or(current_node.status),
        properties: current_node.properties,
    };

    // Update node
    match state
        .graph_manager
        .update_node("example-graph", updated_node)
        .await
    {
        Ok(_) => Json(serde_json::json!({
            "success": true
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string()
        })),
    }
}

#[cfg(feature = "terminal-web")]
/// Handler for adding edges to the graph
async fn add_edge_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Extract edge data
    let source = match payload.get("source").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Source node ID is required"
            }));
        }
    };

    let target = match payload.get("target").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Target node ID is required"
            }));
        }
    };

    let edge_type = payload
        .get("edge_type")
        .and_then(|v| v.as_str())
        .unwrap_or("default");

    // Create new edge with UUID
    let edge_id = Uuid::new_v4().to_string();
    let edge = GraphEdge {
        id: edge_id.clone(),
        source,
        target,
        edge_type: edge_type.to_string(),
        properties: HashMap::new(),
    };

    // Add edge to graph
    match state.graph_manager.add_edge("example-graph", edge).await {
        Ok(_) => Json(serde_json::json!({
            "success": true,
            "id": edge_id
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string()
        })),
    }
}

#[cfg(feature = "terminal-web")]
/// Application state
struct AppState {
    graph_manager: Arc<GraphManager>,
}

#[cfg(feature = "terminal-web")]
/// Create a sample graph with some initial nodes and edges
async fn create_sample_graph(graph_manager: Arc<GraphManager>) -> Result<()> {
    // Create an example graph
    let graph = Graph {
        id: "example-graph".to_string(),
        name: "Example Graph".to_string(),
        graph_type: "demo".to_string(),
        nodes: vec![
            GraphNode {
                id: "node1".to_string(),
                name: "Start".to_string(),
                node_type: "process".to_string(),
                status: "completed".to_string(),
                properties: HashMap::new(),
            },
            GraphNode {
                id: "node2".to_string(),
                name: "Process Data".to_string(),
                node_type: "process".to_string(),
                status: "running".to_string(),
                properties: HashMap::new(),
            },
            GraphNode {
                id: "node3".to_string(),
                name: "Decision".to_string(),
                node_type: "decision".to_string(),
                status: "idle".to_string(),
                properties: HashMap::new(),
            },
        ],
        edges: vec![
            GraphEdge {
                id: "edge1".to_string(),
                source: "node1".to_string(),
                target: "node2".to_string(),
                edge_type: "flow".to_string(),
                properties: HashMap::new(),
            },
            GraphEdge {
                id: "edge2".to_string(),
                source: "node2".to_string(),
                target: "node3".to_string(),
                edge_type: "flow".to_string(),
                properties: HashMap::new(),
            },
        ],
        properties: HashMap::new(),
    };

    // Register the graph
    graph_manager.register_graph(graph).await?;

    Ok(())
}

#[cfg(feature = "terminal-web")]
/// Simulate real-time updates by periodically changing node statuses
async fn simulate_graph_updates(graph_manager: Arc<GraphManager>) {
    // Wait a bit for the system to start
    sleep(Duration::from_secs(3)).await;

    // Update node2 status to completed
    info!("Updating node2 status to completed");
    let updated_node = GraphNode {
        id: "node2".to_string(),
        name: "Process Data".to_string(),
        node_type: "process".to_string(),
        status: "completed".to_string(),
        properties: HashMap::new(),
    };

    if let Err(e) = graph_manager
        .update_node("example-graph", updated_node)
        .await
    {
        error!("Failed to update node2: {}", e);
    }

    // After a while, start node3
    sleep(Duration::from_secs(5)).await;
    info!("Updating node3 status to running");
    let updated_node = GraphNode {
        id: "node3".to_string(),
        name: "Decision".to_string(),
        node_type: "decision".to_string(),
        status: "running".to_string(),
        properties: HashMap::new(),
    };

    if let Err(e) = graph_manager
        .update_node("example-graph", updated_node)
        .await
    {
        error!("Failed to update node3: {}", e);
    }

    // Add a new node after a delay
    sleep(Duration::from_secs(7)).await;
    info!("Adding node4");
    let new_node = GraphNode {
        id: "node4".to_string(),
        name: "Final Step".to_string(),
        node_type: "process".to_string(),
        status: "idle".to_string(),
        properties: HashMap::new(),
    };

    if let Err(e) = graph_manager.add_node("example-graph", new_node).await {
        error!("Failed to add node4: {}", e);
    }

    // Add edge from node3 to node4
    sleep(Duration::from_secs(1)).await;
    info!("Adding edge from node3 to node4");
    let new_edge = GraphEdge {
        id: "edge3".to_string(),
        source: "node3".to_string(),
        target: "node4".to_string(),
        edge_type: "flow".to_string(),
        properties: HashMap::new(),
    };

    if let Err(e) = graph_manager.add_edge("example-graph", new_edge).await {
        error!("Failed to add edge: {}", e);
    }

    // The rest will be controlled by human interaction via the UI
}

#[cfg(feature = "terminal-web")]
#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    // Create the graph manager
    let graph_manager = Arc::new(GraphManager::new());

    // Create the app state
    let app_state = Arc::new(AppState {
        graph_manager: graph_manager.clone(),
    });

    // Create example graph
    create_sample_graph(graph_manager.clone()).await?;

    // Create HTTP server
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .route("/nodes", post(add_node_handler))
        .route("/nodes/:id", post(update_node_handler))
        .route("/edges", post(add_edge_handler))
        .with_state(app_state);

    // Bind to 127.0.0.1:3000
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Starting server on http://{}", addr);

    // Use appropriate server binding based on features
    #[cfg(feature = "transport-ws")]
    {
        let server = axum_server::bind(addr).serve(app.into_make_service());
        info!("Server started. Open http://{} in your browser", addr);
        server.await?;
    }

    #[cfg(not(feature = "transport-ws"))]
    {
        let server = axum::Server::bind(&addr).serve(app.into_make_service());
        info!("Server started. Open http://{} in your browser", addr);
        server.await?;
    }

    Ok(())
}
