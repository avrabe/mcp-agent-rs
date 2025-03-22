//! Graph Visualization Example
//!
//! This example demonstrates a simple graph visualization system using WebSocket
//! for real-time updates and a web interface for interaction.

use {
    axum::{
        extract::{State, WebSocketUpgrade},
        response::{Html, IntoResponse, Json},
        routing::{get, patch, post},
        Router,
    },
    futures::{SinkExt, StreamExt},
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, fmt, net::SocketAddr, sync::Arc, sync::Mutex, time::Duration},
    tokio::sync::mpsc,
    tokio::sync::{broadcast, RwLock},
    tokio::time::sleep,
    tracing::{debug, error, info},
    uuid::Uuid,
};

// Custom error type for the example
#[derive(Debug)]
pub enum Error {
    WebSocketError(String),
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::WebSocketError(msg) => write!(f, "WebSocket error: {}", msg),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

// Node structure for the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Node {
    id: String,
    name: String,
    #[serde(rename = "type")]
    node_type: String,
    status: String,
    properties: HashMap<String, serde_json::Value>,
}

// Edge structure for the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Edge {
    id: String,
    source: String,
    target: String,
    #[serde(rename = "type")]
    edge_type: String,
    properties: HashMap<String, serde_json::Value>,
}

// Graph structure
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Graph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

// Graph state management
struct GraphState {
    graph: Mutex<Graph>,
}

impl GraphState {
    fn new() -> Self {
        Self {
            graph: Mutex::new(Graph {
                nodes: Vec::new(),
                edges: Vec::new(),
            }),
        }
    }

    fn add_node(&self, node: Node) {
        let mut graph = self.graph.lock().unwrap();
        // Check if the node already exists
        if !graph.nodes.iter().any(|n| n.id == node.id) {
            graph.nodes.push(node);
        }
    }

    fn add_edge(&self, edge: Edge) {
        let mut graph = self.graph.lock().unwrap();
        // Check if the edge already exists
        if !graph.edges.iter().any(|e| e.id == edge.id) {
            graph.edges.push(edge);
        }
    }

    fn get_graph_json(&self) -> String {
        let graph = self.graph.lock().unwrap();
        serde_json::to_string(&*graph).unwrap_or_else(|_| "{}".to_string())
    }
}

pub struct GraphManager {
    graphs: RwLock<HashMap<String, Graph>>,
    tx: broadcast::Sender<GraphUpdate>,
}

impl GraphManager {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            graphs: RwLock::new(HashMap::new()),
            tx,
        }
    }

    pub async fn register_graph(&self, graph: Graph) -> Result<()> {
        let mut graphs = self.graphs.write().await;
        graphs.insert(graph.id.clone(), graph.clone());

        // Notify subscribers of the full graph update
        let update = GraphUpdate {
            graph_id: graph.id,
            update_type: GraphUpdateType::FullUpdate,
            graph: Some(graph),
            node: None,
            edge: None,
        };
        let _ = self.tx.send(update);

        Ok(())
    }

    pub async fn add_node(&self, graph_id: &str, node: GraphNode) -> Result<()> {
        let mut graphs = self.graphs.write().await;

        if let Some(graph) = graphs.get_mut(graph_id) {
            graph.nodes.push(node.clone());

            // Notify subscribers of the node addition
            let update = GraphUpdate {
                graph_id: graph_id.to_string(),
                update_type: GraphUpdateType::NodeAdded,
                graph: None,
                node: Some(node),
                edge: None,
            };
            let _ = self.tx.send(update);

            Ok(())
        } else {
            Err(Error::WebSocketError(format!(
                "Graph {} not found",
                graph_id
            )))
        }
    }

    pub async fn add_edge(&self, graph_id: &str, edge: GraphEdge) -> Result<()> {
        let mut graphs = self.graphs.write().await;

        if let Some(graph) = graphs.get_mut(graph_id) {
            graph.edges.push(edge.clone());

            // Notify subscribers of the edge addition
            let update = GraphUpdate {
                graph_id: graph_id.to_string(),
                update_type: GraphUpdateType::EdgeAdded,
                graph: None,
                node: None,
                edge: Some(edge),
            };
            let _ = self.tx.send(update);

            Ok(())
        } else {
            Err(Error::WebSocketError(format!(
                "Graph {} not found",
                graph_id
            )))
        }
    }

    pub async fn get_graph(&self, graph_id: &str) -> Option<Graph> {
        let graphs = self.graphs.read().await;
        graphs.get(graph_id).cloned()
    }

    pub async fn update_node(&self, graph_id: &str, node: GraphNode) -> Result<()> {
        let mut graphs = self.graphs.write().await;

        if let Some(graph) = graphs.get_mut(graph_id) {
            if let Some(index) = graph.nodes.iter().position(|n| n.id == node.id) {
                graph.nodes[index] = node.clone();

                // Notify subscribers of the node update
                let update = GraphUpdate {
                    graph_id: graph_id.to_string(),
                    update_type: GraphUpdateType::NodeUpdated,
                    graph: None,
                    node: Some(node),
                    edge: None,
                };
                let _ = self.tx.send(update);

                Ok(())
            } else {
                Err(Error::WebSocketError(format!(
                    "Node {} not found in graph {}",
                    node.id, graph_id
                )))
            }
        } else {
            Err(Error::WebSocketError(format!(
                "Graph {} not found",
                graph_id
            )))
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<GraphUpdate> {
        self.tx.subscribe()
    }
}

/// Application state
struct AppState {
    graph_manager: Arc<GraphManager>,
}

/// WebSocket handler for graph updates
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<GraphState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handle WebSocket connection
async fn handle_socket(socket: WebSocket, state: Arc<GraphState>) {
    let (mut sender, mut receiver) = socket.split();

    // Send initial graph state
    let graph_json = state.get_graph_json();
    if let Err(e) = sender.send(Message::Text(graph_json)).await {
        eprintln!("Error sending initial graph state: {}", e);
        return;
    }

    // Create a channel for sending updates to the WebSocket
    let (tx, mut rx) = mpsc::channel::<String>(100);

    // Spawn a task to forward messages from the channel to the WebSocket
    let forward_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Process incoming messages
    while let Some(result) = receiver.next().await {
        match result {
            Ok(Message::Text(text)) => {
                if text.starts_with("ADD_NODE:") {
                    let node_data = &text[9..];
                    if let Ok(node) = serde_json::from_str::<Node>(node_data) {
                        state.add_node(node);
                        // Send updated graph to all clients
                        let graph_json = state.get_graph_json();
                        let _ = tx.send(graph_json).await;
                    }
                } else if text.starts_with("ADD_EDGE:") {
                    let edge_data = &text[9..];
                    if let Ok(edge) = serde_json::from_str::<Edge>(edge_data) {
                        state.add_edge(edge);
                        // Send updated graph to all clients
                        let graph_json = state.get_graph_json();
                        let _ = tx.send(graph_json).await;
                    }
                }
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => {}
        }
    }

    // If we're here, the connection is closed
    forward_task.abort();
}

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
    let node = GraphNode {
        id: Uuid::new_v4().to_string(),
        name: node_name.to_string(),
        node_type: node_type.to_string(),
        status: status.to_string(),
        properties: HashMap::new(),
    };

    // Add node to graph
    match state.graph_manager.add_node("example-graph", node).await {
        Ok(_) => Json(serde_json::json!({
            "success": true,
            "id": node.id
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string()
        })),
    }
}

/// Handler for updating nodes in the graph
async fn update_node_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(node_id): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Get the graph
    let graph = match state.graph_manager.get_graph("example-graph").await {
        Some(g) => g,
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": "Graph not found"
            }));
        }
    };

    // Find the node
    let node = match graph.nodes.iter().find(|n| n.id == node_id) {
        Some(n) => n.clone(),
        None => {
            return Json(serde_json::json!({
                "success": false,
                "error": format!("Node {} not found", node_id)
            }));
        }
    };

    // Update the status if provided
    let status = payload
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or(&node.status);

    // Create updated node
    let updated_node = GraphNode {
        id: node.id,
        name: node.name,
        node_type: node.node_type,
        status: status.to_string(),
        properties: node.properties,
    };

    // Update the node
    match state
        .graph_manager
        .update_node("example-graph", updated_node)
        .await
    {
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
    let edge = GraphEdge {
        id: Uuid::new_v4().to_string(),
        source,
        target,
        edge_type: edge_type.to_string(),
        properties: HashMap::new(),
    };

    // Add edge to graph
    match state.graph_manager.add_edge("example-graph", edge).await {
        Ok(_) => Json(serde_json::json!({
            "success": true,
            "id": edge.id
        })),
        Err(e) => Json(serde_json::json!({
            "success": false,
            "error": e.to_string()
        })),
    }
}

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
}

/// Serve a simple HTML page that displays the graph visualization
async fn index_handler() -> impl IntoResponse {
    Html(include_str!("../static/graph_visualization.html").to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    info!("Starting graph visualization example");

    // Create graph manager
    let graph_manager = Arc::new(GraphManager::new());

    // Create sample graph
    create_sample_graph(graph_manager.clone()).await?;

    // Create application state
    let state = Arc::new(AppState {
        graph_manager: graph_manager.clone(),
    });

    // Start graph simulation in the background
    let sim_manager = graph_manager.clone();
    tokio::spawn(async move {
        simulate_graph_updates(sim_manager).await;
    });

    // Create router
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .route("/api/node", post(add_node_handler))
        .route("/api/node/:node_id", patch(update_node_handler))
        .route("/api/edge", post(add_edge_handler))
        .with_state(state);

    // Start server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}
