use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Html,
    routing::get,
    Router,
};
use futures::stream::StreamExt;
use futures::sink::SinkExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Node {
    id: String,
    label: String,
    #[serde(default)]
    properties: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Edge {
    id: String,
    source: String,
    target: String,
    label: String,
    #[serde(default)]
    properties: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Graph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

#[derive(Debug, Clone)]
struct GraphState {
    graph: Arc<Mutex<Graph>>,
    tx: broadcast::Sender<String>,
}

impl GraphState {
    fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self {
            graph: Arc::new(Mutex::new(Graph {
                nodes: Vec::new(),
                edges: Vec::new(),
            })),
            tx,
        }
    }

    async fn add_node(&self, label: String, properties: HashMap<String, String>) -> String {
        let id = Uuid::new_v4().to_string();
        let node = Node {
            id: id.clone(),
            label,
            properties,
        };
        let mut graph = self.graph.lock().await;
        graph.nodes.push(node);
        let json = serde_json::to_string(&*graph).unwrap();
        self.tx.send(json).unwrap();
        id
    }

    async fn add_edge(&self, source: String, target: String, label: String, properties: HashMap<String, String>) -> String {
        let id = Uuid::new_v4().to_string();
        let edge = Edge {
            id: id.clone(),
            source,
            target,
            label,
            properties,
        };
        let mut graph = self.graph.lock().await;
        graph.edges.push(edge);
        let json = serde_json::to_string(&*graph).unwrap();
        self.tx.send(json).unwrap();
        id
    }

    async fn get_graph_json(&self) -> String {
        let graph = self.graph.lock().await;
        serde_json::to_string(&*graph).unwrap()
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<GraphState>>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<GraphState>) {
    let mut rx = state.tx.subscribe();
    let (mut sender, mut receiver) = socket.split();

    // Send initial graph state
    let json = state.get_graph_json().await;
    let _ = sender.send(Message::Text(json)).await;

    // Forward graph updates to the WebSocket
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        while let Some(Ok(_msg)) = receiver.next().await {
            // Handle any incoming WebSocket messages here if needed
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}

async fn index_handler() -> Html<String> {
    let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>Graph Visualization</title>
    <script src="https://d3js.org/d3.v7.min.js"></script>
    <style>
        body {
            margin: 0;
            padding: 20px;
            font-family: Arial, sans-serif;
        }
        #graph {
            width: 100%;
            height: 600px;
            border: 1px solid #ccc;
            border-radius: 4px;
        }
        .node {
            cursor: pointer;
        }
        .node circle {
            stroke: #fff;
            stroke-width: 2px;
        }
        .node text {
            font-size: 12px;
        }
        .link {
            stroke: #999;
            stroke-opacity: 0.6;
            stroke-width: 2px;
        }
        .link-label {
            font-size: 10px;
            fill: #666;
        }
    </style>
</head>
<body>
    <h1>Graph Visualization</h1>
    <div id="graph"></div>

    <script>
        const width = document.getElementById('graph').clientWidth;
        const height = document.getElementById('graph').clientHeight;

        const svg = d3.select('#graph')
            .append('svg')
            .attr('width', width)
            .attr('height', height);

        const g = svg.append('g');

        // Add zoom behavior
        const zoom = d3.zoom()
            .scaleExtent([0.1, 4])
            .on('zoom', (event) => {
                g.attr('transform', event.transform);
            });

        svg.call(zoom);

        // Create force simulation
        const simulation = d3.forceSimulation()
            .force('link', d3.forceLink().id(d => d.id).distance(150))
            .force('charge', d3.forceManyBody().strength(-500))
            .force('center', d3.forceCenter(width / 2, height / 2));

        function updateGraph(data) {
            // Remove existing elements
            g.selectAll('.link').remove();
            g.selectAll('.node').remove();

            // Create links
            const links = g.selectAll('.link')
                .data(data.edges)
                .enter()
                .append('g')
                .attr('class', 'link');

            links.append('line')
                .style('stroke', '#999')
                .style('stroke-width', 2);

            links.append('text')
                .attr('class', 'link-label')
                .text(d => d.label)
                .attr('text-anchor', 'middle');

            // Create nodes
            const nodes = g.selectAll('.node')
                .data(data.nodes)
                .enter()
                .append('g')
                .attr('class', 'node')
                .call(d3.drag()
                    .on('start', dragstarted)
                    .on('drag', dragged)
                    .on('end', dragended));

            nodes.append('circle')
                .attr('r', 20)
                .style('fill', d => d.properties.color || '#69b3a2');

            nodes.append('text')
                .text(d => d.label)
                .attr('text-anchor', 'middle')
                .attr('dy', 30);

            // Update simulation
            simulation
                .nodes(data.nodes)
                .on('tick', ticked);

            simulation.force('link')
                .links(data.edges);

            // Reheat the simulation
            simulation.alpha(1).restart();

            function ticked() {
                links.selectAll('line')
                    .attr('x1', d => d.source.x)
                    .attr('y1', d => d.source.y)
                    .attr('x2', d => d.target.x)
                    .attr('y2', d => d.target.y);

                links.selectAll('text')
                    .attr('x', d => (d.source.x + d.target.x) / 2)
                    .attr('y', d => (d.source.y + d.target.y) / 2);

                nodes
                    .attr('transform', d => `translate(${d.x},${d.y})`);
            }
        }

        function dragstarted(event, d) {
            if (!event.active) simulation.alphaTarget(0.3).restart();
            d.fx = d.x;
            d.fy = d.y;
        }

        function dragged(event, d) {
            d.fx = event.x;
            d.fy = event.y;
        }

        function dragended(event, d) {
            if (!event.active) simulation.alphaTarget(0);
            d.fx = null;
            d.fy = null;
        }

        // WebSocket connection
        const ws = new WebSocket(`ws://${window.location.host}/ws`);
        ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            updateGraph(data);
        };
    </script>
</body>
</html>"#;
    Html(html.to_string())
}

#[tokio::main]
async fn main() {
    let state = Arc::new(GraphState::new());
    let app_state = Arc::clone(&state);

    // Create a sample graph
    {
        let mut properties = HashMap::new();
        properties.insert("color".to_string(), "blue".to_string());
        let node1 = state.add_node("Node 1".to_string(), properties.clone()).await;

        properties.insert("color".to_string(), "red".to_string());
        let node2 = state.add_node("Node 2".to_string(), properties.clone()).await;

        properties.insert("color".to_string(), "green".to_string());
        let node3 = state.add_node("Node 3".to_string(), properties).await;

        let mut edge_props = HashMap::new();
        edge_props.insert("weight".to_string(), "1".to_string());
        state.add_edge(node1.clone(), node2.clone(), "Edge 1-2".to_string(), edge_props.clone()).await;
        state.add_edge(node2.clone(), node3.clone(), "Edge 2-3".to_string(), edge_props.clone()).await;
        state.add_edge(node3, node1, "Edge 3-1".to_string(), edge_props).await;
    }

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .with_state(app_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Starting server on http://localhost:3000");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
} 