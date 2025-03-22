//! Simple Graph Visualization Example
//!
//! This example demonstrates a basic graph visualization system using WebSocket
//! for real-time updates and a web interface for interaction.

use {
    axum::{
        extract::WebSocketUpgrade,
        response::{Html, IntoResponse},
        routing::get,
        Router,
    },
    serde::{Deserialize, Serialize},
    std::{collections::HashMap, sync::Arc, time::Duration},
    tokio::sync::RwLock,
    tokio::time::sleep,
    tracing::{error, info},
    uuid::Uuid,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Node {
    id: String,
    label: String,
    status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Edge {
    id: String,
    source: String,
    target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Graph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

struct GraphState {
    graph: RwLock<Graph>,
}

impl GraphState {
    fn new() -> Self {
        Self {
            graph: RwLock::new(Graph {
                nodes: vec![
                    Node {
                        id: "1".to_string(),
                        label: "Start".to_string(),
                        status: "completed".to_string(),
                    },
                    Node {
                        id: "2".to_string(),
                        label: "Process".to_string(),
                        status: "running".to_string(),
                    },
                ],
                edges: vec![Edge {
                    id: Uuid::new_v4().to_string(),
                    source: "1".to_string(),
                    target: "2".to_string(),
                }],
            }),
        }
    }

    async fn add_node(&self, label: String) -> String {
        let id = Uuid::new_v4().to_string();
        let node = Node {
            id: id.clone(),
            label,
            status: "idle".to_string(),
        };

        let mut graph = self.graph.write().await;
        graph.nodes.push(node);
        id
    }

    async fn add_edge(&self, source: String, target: String) -> String {
        let id = Uuid::new_v4().to_string();
        let edge = Edge {
            id: id.clone(),
            source,
            target,
        };

        let mut graph = self.graph.write().await;
        graph.edges.push(edge);
        id
    }

    async fn update_node_status(&self, id: String, status: String) {
        let mut graph = self.graph.write().await;
        if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == id) {
            node.status = status;
        }
    }

    async fn get_graph(&self) -> Graph {
        self.graph.read().await.clone()
    }
}

async fn ws_handler(ws: WebSocketUpgrade, state: Arc<GraphState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: axum::extract::ws::WebSocket, state: Arc<GraphState>) {
    // Send initial graph state
    let graph = state.get_graph().await;
    if let Ok(json) = serde_json::to_string(&graph) {
        if let Err(e) = socket.send(axum::extract::ws::Message::Text(json)).await {
            error!("Failed to send initial graph state: {}", e);
            return;
        }
    }

    // Start update loop
    let mut interval = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = interval.tick() => {
                let graph = state.get_graph().await;
                if let Ok(json) = serde_json::to_string(&graph) {
                    if let Err(e) = socket.send(axum::extract::ws::Message::Text(json)).await {
                        error!("Failed to send graph update: {}", e);
                        break;
                    }
                }
            }
        }
    }
}

async fn index_handler() -> impl IntoResponse {
    Html(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Simple Graph Visualization</title>
    <script src="https://d3js.org/d3.v7.min.js"></script>
    <style>
        body {
            margin: 0;
            padding: 20px;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
        }

        #graph {
            width: 100%;
            height: 600px;
            border: 1px solid #ddd;
            border-radius: 4px;
        }

        .node circle {
            fill: #fff;
            stroke: #2196f3;
            stroke-width: 2px;
        }

        .node text {
            font-size: 12px;
        }

        .node.completed circle {
            stroke: #4caf50;
        }

        .node.running circle {
            stroke: #2196f3;
        }

        .node.idle circle {
            stroke: #9e9e9e;
        }

        .link {
            fill: none;
            stroke: #999;
            stroke-width: 2px;
            marker-end: url(#arrowhead);
        }

        #arrowhead {
            fill: #999;
        }
    </style>
</head>
<body>
    <h1>Simple Graph Visualization</h1>
    <div id="graph"></div>

    <script>
        let nodes = [];
        let edges = [];
        let simulation;

        // Initialize D3.js visualization
        function initializeGraph() {
            const container = d3.select('#graph');
            const width = container.node().clientWidth;
            const height = container.node().clientHeight;

            const svg = container.append('svg')
                .attr('width', width)
                .attr('height', height);

            // Define arrow marker
            svg.append('defs').append('marker')
                .attr('id', 'arrowhead')
                .attr('viewBox', '-0 -5 10 10')
                .attr('refX', 20)
                .attr('refY', 0)
                .attr('orient', 'auto')
                .attr('markerWidth', 6)
                .attr('markerHeight', 6)
                .append('path')
                .attr('d', 'M0,-5L10,0L0,5');

            // Create container for the graph
            const g = svg.append('g');

            // Initialize force simulation
            simulation = d3.forceSimulation(nodes)
                .force('link', d3.forceLink(edges).id(d => d.id).distance(100))
                .force('charge', d3.forceManyBody().strength(-300))
                .force('center', d3.forceCenter(width / 2, height / 2))
                .on('tick', ticked);

            function ticked() {
                // Update link positions
                const link = g.selectAll('.link')
                    .data(edges)
                    .join('path')
                    .attr('class', 'link')
                    .attr('d', d => `M${d.source.x},${d.source.y}L${d.target.x},${d.target.y}`);

                // Update node positions
                const node = g.selectAll('.node')
                    .data(nodes)
                    .join('g')
                    .attr('class', d => `node ${d.status}`)
                    .attr('transform', d => `translate(${d.x},${d.y})`);

                node.selectAll('circle')
                    .data(d => [d])
                    .join('circle')
                    .attr('r', 10);

                node.selectAll('text')
                    .data(d => [d])
                    .join('text')
                    .attr('dy', -15)
                    .attr('text-anchor', 'middle')
                    .text(d => d.label);
            }

            // Zoom behavior
            const zoom = d3.zoom()
                .scaleExtent([0.1, 4])
                .on('zoom', (event) => {
                    g.attr('transform', event.transform);
                });

            svg.call(zoom);
        }

        // WebSocket connection
        const ws = new WebSocket(`ws://${window.location.host}/ws`);

        ws.onmessage = (event) => {
            const data = JSON.parse(event.data);
            nodes = data.nodes;
            edges = data.edges;
            simulation.nodes(nodes);
            simulation.force('link').links(edges);
            simulation.alpha(1).restart();
        };

        // Initialize the graph visualization
        initializeGraph();
    </script>
</body>
</html>"#,
    )
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    info!("Starting simple graph visualization example");

    // Create graph state
    let state = Arc::new(GraphState::new());

    // Simulate graph updates
    let update_state = state.clone();
    tokio::spawn(async move {
        // Wait for initial setup
        sleep(Duration::from_secs(3)).await;

        // Update node 2 to completed
        update_state
            .update_node_status("2".to_string(), "completed".to_string())
            .await;

        // Add node 3
        sleep(Duration::from_secs(3)).await;
        let node3_id = update_state.add_node("Decision".to_string()).await;
        update_state
            .add_edge("2".to_string(), node3_id.clone())
            .await;

        // Update node 3 status
        sleep(Duration::from_secs(3)).await;
        update_state
            .update_node_status(node3_id, "running".to_string())
            .await;
    });

    // Create router
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .with_state(state);

    // Start server
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
    info!("Server listening on http://{}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
