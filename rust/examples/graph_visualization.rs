//! Graph Visualization Example
//!
//! This example demonstrates a simple graph visualization system using WebSocket
//! for real-time updates and a web interface for interaction.

#[cfg(feature = "terminal-web")]
use {
    axum::extract::ws::{Message, WebSocket},
    axum::{
        extract::{State, WebSocketUpgrade},
        response::{Html, IntoResponse, Json},
        routing::{get, patch, post},
        Router,
    },
};

use {
    futures::SinkExt,
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, fmt, sync::Mutex},
    tokio::sync::{broadcast, RwLock},
    uuid::Uuid,
};

/// Update types for graph changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GraphUpdateType {
    FullUpdate,
    NodeAdded,
    EdgeAdded,
    NodeUpdated,
}

/// Update message for graph changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphUpdate {
    pub graph_id: String,
    pub update_type: GraphUpdateType,
    pub graph: Option<Graph>,
    pub node: Option<Node>,
    pub edge: Option<Edge>,
}

/// Node for API operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub status: String,
    pub properties: HashMap<String, serde_json::Value>,
}

/// Edge for API operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub edge_type: String,
    pub properties: HashMap<String, serde_json::Value>,
}

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

// Node representation
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Node {
    id: String,
    name: String,
    #[serde(rename = "type")]
    node_type: String,
    status: String,
    properties: HashMap<String, serde_json::Value>,
}

// Edge representation
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Edge {
    id: String,
    source: String,
    target: String,
    #[serde(rename = "type")]
    edge_type: String,
    properties: HashMap<String, serde_json::Value>,
}

// Graph representation
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Graph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

// Graph state for maintaining current graph
#[derive(Debug)]
struct GraphState {
    graph: Mutex<Graph>,
}

impl GraphState {
    fn new() -> Self {
        GraphState {
            graph: Mutex::new(Graph {
                nodes: Vec::new(),
                edges: Vec::new(),
            }),
        }
    }

    fn add_node(&self, node: Node) {
        let mut graph = self.graph.lock().unwrap();
        graph.nodes.push(node);
    }

    fn add_edge(&self, edge: Edge) {
        let mut graph = self.graph.lock().unwrap();
        graph.edges.push(edge);
    }

    fn get_graph_json(&self) -> String {
        let graph = self.graph.lock().unwrap();
        serde_json::to_string(&*graph).unwrap()
    }
}

pub struct GraphManager {
    graphs: RwLock<HashMap<String, Graph>>,
    tx: broadcast::Sender<GraphUpdate>,
}

impl Default for GraphManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphManager {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        GraphManager {
            graphs: RwLock::new(HashMap::new()),
            tx,
        }
    }

    pub async fn register_graph(&self, graph: Graph) -> Result<()> {
        let graph_id = Uuid::new_v4().to_string();
        let mut graphs = self.graphs.write().await;
        graphs.insert(graph_id.clone(), graph.clone());

        let update = GraphUpdate {
            graph_id,
            update_type: GraphUpdateType::FullUpdate,
            graph: Some(graph),
            node: None,
            edge: None,
        };

        self.tx
            .send(update)
            .map_err(|e| Error::WebSocketError(e.to_string()))?;
        Ok(())
    }

    pub async fn add_node(&self, graph_id: &str, node: GraphNode) -> Result<()> {
        let mut graphs = self.graphs.write().await;
        let graph = graphs.get_mut(graph_id).ok_or_else(|| {
            Error::WebSocketError(format!("Graph with ID {} not found", graph_id))
        })?;

        let node = Node {
            id: node.id,
            name: node.name,
            node_type: node.node_type,
            status: node.status,
            properties: node.properties,
        };

        graph.nodes.push(node.clone());

        let update = GraphUpdate {
            graph_id: graph_id.to_string(),
            update_type: GraphUpdateType::NodeAdded,
            graph: None,
            node: Some(node),
            edge: None,
        };

        self.tx
            .send(update)
            .map_err(|e| Error::WebSocketError(e.to_string()))?;
        Ok(())
    }

    pub async fn add_edge(&self, graph_id: &str, edge: GraphEdge) -> Result<()> {
        let mut graphs = self.graphs.write().await;
        let graph = graphs.get_mut(graph_id).ok_or_else(|| {
            Error::WebSocketError(format!("Graph with ID {} not found", graph_id))
        })?;

        let edge = Edge {
            id: edge.id,
            source: edge.source,
            target: edge.target,
            edge_type: edge.edge_type,
            properties: edge.properties,
        };

        graph.edges.push(edge.clone());

        let update = GraphUpdate {
            graph_id: graph_id.to_string(),
            update_type: GraphUpdateType::EdgeAdded,
            graph: None,
            node: None,
            edge: Some(edge),
        };

        self.tx
            .send(update)
            .map_err(|e| Error::WebSocketError(e.to_string()))?;
        Ok(())
    }

    pub async fn get_graph(&self, graph_id: &str) -> Option<Graph> {
        let graphs = self.graphs.read().await;
        graphs.get(graph_id).cloned()
    }

    pub async fn update_node(&self, graph_id: &str, node: GraphNode) -> Result<()> {
        let mut graphs = self.graphs.write().await;
        let graph = graphs.get_mut(graph_id).ok_or_else(|| {
            Error::WebSocketError(format!("Graph with ID {} not found", graph_id))
        })?;

        // Find and update the node
        if let Some(existing_node) = graph.nodes.iter_mut().find(|n| n.id == node.id) {
            existing_node.name = node.name;
            existing_node.node_type = node.node_type;
            existing_node.status = node.status;
            existing_node.properties = node.properties;

            let update = GraphUpdate {
                graph_id: graph_id.to_string(),
                update_type: GraphUpdateType::NodeUpdated,
                graph: None,
                node: Some(existing_node.clone()),
                edge: None,
            };

            self.tx
                .send(update)
                .map_err(|e| Error::WebSocketError(e.to_string()))?;
            Ok(())
        } else {
            Err(Error::WebSocketError(format!(
                "Node with ID {} not found",
                node.id
            )))
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<GraphUpdate> {
        self.tx.subscribe()
    }
}

#[cfg(feature = "terminal-web")]
struct AppState {
    graph_manager: Arc<GraphManager>,
}

#[cfg(feature = "terminal-web")]
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<GraphState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

#[cfg(feature = "terminal-web")]
async fn handle_socket(socket: WebSocket, state: Arc<GraphState>) {
    let (mut sender, mut receiver) = socket.split();

    // Send initial graph state
    let graph_json = state.get_graph_json();
    if let Err(e) = sender.send(Message::Text(graph_json)).await {
        error!("Error sending initial graph state: {}", e);
        return;
    }

    // Create a channel for receiving graph updates
    let (tx, mut rx) = mpsc::channel(100);

    // Spawn a task to forward updates to the WebSocket
    let forward_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Process incoming WebSocket messages
    while let Some(result) = receiver.next().await {
        match result {
            Ok(Message::Text(text)) => {
                let msg = format!("Received message: {}", text);
                info!("{}", msg);
                // Process commands from the client
                // ... (command processing logic)
            }
            Ok(Message::Close(_)) | Err(_) => break,
            _ => (),
        }
    }

    // Cancel the forwarding task when the WebSocket is closed
    forward_task.abort();
}

#[cfg(feature = "terminal-web")]
async fn add_node_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let graph_id = payload["graphId"].as_str().unwrap_or_default();

    // Extract node details from the payload
    let node = GraphNode {
        id: payload["id"].as_str().unwrap_or_default().to_string(),
        name: payload["name"].as_str().unwrap_or_default().to_string(),
        node_type: payload["type"].as_str().unwrap_or_default().to_string(),
        status: payload["status"].as_str().unwrap_or_default().to_string(),
        properties: payload["properties"]
            .as_object()
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
    };

    match state.graph_manager.add_node(graph_id, node).await {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "error": e.to_string() })),
    }
}

#[cfg(feature = "terminal-web")]
async fn update_node_handler(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(node_id): axum::extract::Path<String>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let graph_id = payload["graphId"].as_str().unwrap_or_default();

    // Extract updated node details
    let updated_node = GraphNode {
        id: node_id,
        name: payload["name"].as_str().unwrap_or_default().to_string(),
        node_type: payload["type"].as_str().unwrap_or_default().to_string(),
        status: payload["status"].as_str().unwrap_or_default().to_string(),
        properties: payload["properties"]
            .as_object()
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
    };

    match state
        .graph_manager
        .update_node(graph_id, updated_node)
        .await
    {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "error": e.to_string() })),
    }
}

#[cfg(feature = "terminal-web")]
async fn add_edge_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    let graph_id = payload["graphId"].as_str().unwrap_or_default();

    // Extract edge details from the payload
    let edge = GraphEdge {
        id: payload["id"].as_str().unwrap_or_default().to_string(),
        source: payload["source"].as_str().unwrap_or_default().to_string(),
        target: payload["target"].as_str().unwrap_or_default().to_string(),
        edge_type: payload["type"].as_str().unwrap_or_default().to_string(),
        properties: payload["properties"]
            .as_object()
            .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
    };

    match state.graph_manager.add_edge(graph_id, edge).await {
        Ok(_) => Json(serde_json::json!({ "success": true })),
        Err(e) => Json(serde_json::json!({ "success": false, "error": e.to_string() })),
    }
}

#[cfg(feature = "terminal-web")]
async fn create_sample_graph(graph_manager: Arc<GraphManager>) -> Result<()> {
    // Create nodes
    let nodes = vec![
        GraphNode {
            id: "node1".to_string(),
            name: "Data Source".to_string(),
            node_type: "source".to_string(),
            status: "active".to_string(),
            properties: HashMap::new(),
        },
        GraphNode {
            id: "node2".to_string(),
            name: "Processor".to_string(),
            node_type: "processor".to_string(),
            status: "active".to_string(),
            properties: HashMap::new(),
        },
        GraphNode {
            id: "node3".to_string(),
            name: "Output".to_string(),
            node_type: "sink".to_string(),
            status: "pending".to_string(),
            properties: HashMap::new(),
        },
    ];

    // Create edges
    let edges = vec![
        GraphEdge {
            id: "edge1".to_string(),
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: "dataflow".to_string(),
            properties: HashMap::new(),
        },
        GraphEdge {
            id: "edge2".to_string(),
            source: "node2".to_string(),
            target: "node3".to_string(),
            edge_type: "dataflow".to_string(),
            properties: HashMap::new(),
        },
    ];

    // Register graph
    let graph_id = Uuid::new_v4().to_string();
    let empty_graph = Graph {
        nodes: Vec::new(),
        edges: Vec::new(),
    };

    // Register the empty graph first
    graph_manager.register_graph(empty_graph).await?;

    // Add nodes and edges
    for node in nodes {
        graph_manager.add_node(&graph_id, node).await?;
    }

    for edge in edges {
        graph_manager.add_edge(&graph_id, edge).await?;
    }

    Ok(())
}

#[cfg(feature = "terminal-web")]
async fn simulate_graph_updates(graph_manager: Arc<GraphManager>) {
    // Get the first graph ID (if any)
    let graph_id = {
        let graphs = graph_manager.graphs.read().await;
        graphs.keys().next().cloned()
    };

    if let Some(graph_id) = graph_id {
        loop {
            sleep(Duration::from_secs(5)).await;

            // Update a node
            let updated_node = GraphNode {
                id: "node3".to_string(),
                name: "Output".to_string(),
                node_type: "sink".to_string(),
                status: "active".to_string(), // Change status from pending to active
                properties: HashMap::new(),
            };

            if let Err(e) = graph_manager.update_node(&graph_id, updated_node).await {
                error!("Error updating node: {}", e);
            } else {
                info!("Node updated successfully");
            }

            sleep(Duration::from_secs(5)).await;

            // Add a new node
            let new_node = GraphNode {
                id: "node4".to_string(),
                name: "Analytics".to_string(),
                node_type: "processor".to_string(),
                status: "pending".to_string(),
                properties: HashMap::new(),
            };

            if let Err(e) = graph_manager.add_node(&graph_id, new_node).await {
                error!("Error adding node: {}", e);
            } else {
                info!("Node added successfully");
            }

            sleep(Duration::from_secs(5)).await;

            // Add a new edge
            let new_edge = GraphEdge {
                id: "edge3".to_string(),
                source: "node2".to_string(),
                target: "node4".to_string(),
                edge_type: "dataflow".to_string(),
                properties: HashMap::new(),
            };

            if let Err(e) = graph_manager.add_edge(&graph_id, new_edge).await {
                error!("Error adding edge: {}", e);
            } else {
                info!("Edge added successfully");
            }
        }
    }
}

#[cfg(feature = "terminal-web")]
async fn index_handler() -> impl IntoResponse {
    // Serve a simple HTML page with the visualization
    Html(include_str!("../static/graph_visualization.html"))
}

#[tokio::main]
#[cfg(feature = "terminal-web")]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(tracing::Level::INFO)
            .finish(),
    )
    .expect("Failed to set tracing subscriber");

    // Create graph manager
    let graph_manager = Arc::new(GraphManager::new());

    // Create sample graph
    create_sample_graph(graph_manager.clone()).await?;

    // Spawn a task to simulate graph updates
    tokio::spawn(simulate_graph_updates(graph_manager.clone()));

    // Create shared state
    let app_state = Arc::new(AppState {
        graph_manager: graph_manager.clone(),
    });

    // Create web server
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .route("/api/nodes", post(add_node_handler))
        .route("/api/nodes/:node_id", patch(update_node_handler))
        .route("/api/edges", post(add_edge_handler))
        .with_state(app_state);

    // Bind to address
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Starting server at http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

#[tokio::main]
#[cfg(not(feature = "terminal-web"))]
async fn main() -> Result<()> {
    println!("This example requires the 'terminal-web' feature to be enabled.");
    println!("Run with: cargo run --example graph_visualization --features terminal-web");
    Ok(())
}
