use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    extract::State,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;

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

// WebSocket handler
async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<GraphState>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

// Handle WebSocket connection
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

// Handler for the index page
async fn index_handler() -> Html<String> {
    Html(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Graph Visualization</title>
    <style>
        body {
            font-family: Arial, sans-serif;
            margin: 0;
            padding: 0;
            display: flex;
            flex-direction: column;
            height: 100vh;
        }
        .container {
            display: flex;
            flex: 1;
            overflow: hidden;
        }
        .graph-panel {
            flex: 2;
            border-right: 1px solid #ccc;
            overflow: hidden;
            position: relative;
        }
        .terminal-panel {
            flex: 1;
            padding: 10px;
            background-color: #f5f5f5;
            display: flex;
            flex-direction: column;
        }
        .terminal-output {
            flex: 1;
            overflow-y: auto;
            background-color: #000;
            color: #00ff00;
            padding: 10px;
            font-family: monospace;
            margin-bottom: 10px;
        }
        .terminal-input {
            display: flex;
        }
        .terminal-input input {
            flex: 1;
            padding: 8px;
            font-family: monospace;
        }
        .terminal-input button {
            padding: 8px 12px;
            background-color: #4CAF50;
            color: white;
            border: none;
            cursor: pointer;
        }
        .node {
            fill: #69b3a2;
            stroke: #333;
            stroke-width: 1.5px;
        }
        .node.active {
            fill: #ff7f0e;
        }
        .node.completed {
            fill: #2ca02c;
        }
        .node.pending {
            fill: #d62728;
        }
        .link {
            stroke: #999;
            stroke-width: 1.5px;
            stroke-opacity: 0.6;
        }
        .node-label {
            font-size: 12px;
            text-anchor: middle;
            pointer-events: none;
        }
    </style>
    <script src="https://d3js.org/d3.v7.min.js"></script>
</head>
<body>
    <h1 style="text-align: center; padding: 10px; margin: 0;">Graph Visualization Example</h1>
    <div class="container">
        <div class="graph-panel" id="graph-container"></div>
        <div class="terminal-panel">
            <h3>Web Terminal</h3>
            <div class="terminal-output" id="terminal-output"></div>
            <div class="terminal-input">
                <input type="text" id="terminal-input" placeholder="Enter command...">
                <button id="send-btn">Send</button>
            </div>
        </div>
    </div>

    <script>
        // WebSocket connection
        let socket;
        let graphData = { nodes: [], edges: [] };
        
        // Initialize D3 visualization
        const width = document.getElementById('graph-container').clientWidth;
        const height = document.getElementById('graph-container').clientHeight;
        
        const svg = d3.select('#graph-container').append('svg')
            .attr('width', '100%')
            .attr('height', '100%')
            .attr('viewBox', [0, 0, width, height]);
            
        const g = svg.append('g');
        
        // Zoom behavior
        const zoom = d3.zoom()
            .scaleExtent([0.1, 4])
            .on('zoom', (event) => {
                g.attr('transform', event.transform);
            });
            
        svg.call(zoom);
        
        // Force simulation
        const simulation = d3.forceSimulation()
            .force('link', d3.forceLink().id(d => d.id).distance(100))
            .force('charge', d3.forceManyBody().strength(-300))
            .force('center', d3.forceCenter(width / 2, height / 2))
            .on('tick', ticked);
            
        let link = g.append('g')
            .attr('class', 'links')
            .selectAll('line');
            
        let node = g.append('g')
            .attr('class', 'nodes')
            .selectAll('circle');
            
        let label = g.append('g')
            .attr('class', 'labels')
            .selectAll('text');
            
        function ticked() {
            link
                .attr('x1', d => d.source.x)
                .attr('y1', d => d.source.y)
                .attr('x2', d => d.target.x)
                .attr('y2', d => d.target.y);
                
            node
                .attr('cx', d => d.x)
                .attr('cy', d => d.y);
                
            label
                .attr('x', d => d.x)
                .attr('y', d => d.y + 25);
        }
        
        function updateGraph(data) {
            // Parse the data if it's a string
            if (typeof data === 'string') {
                try {
                    data = JSON.parse(data);
                } catch (e) {
                    console.error('Error parsing graph data:', e);
                    return;
                }
            }
            
            // Update links
            link = link.data(data.edges, d => d.id);
            link.exit().remove();
            const linkEnter = link.enter().append('line')
                .attr('class', 'link');
            link = linkEnter.merge(link);
            
            // Update nodes
            node = node.data(data.nodes, d => d.id);
            node.exit().remove();
            const nodeEnter = node.enter().append('circle')
                .attr('class', d => `node ${d.status}`)
                .attr('r', 15)
                .call(d3.drag()
                    .on('start', dragstarted)
                    .on('drag', dragged)
                    .on('end', dragended));
            node = nodeEnter.merge(node);
            
            // Update labels
            label = label.data(data.nodes, d => d.id);
            label.exit().remove();
            const labelEnter = label.enter().append('text')
                .attr('class', 'node-label')
                .text(d => d.name);
            label = labelEnter.merge(label);
            
            // Update simulation
            simulation.nodes(data.nodes);
            simulation.force('link').links(data.edges);
            simulation.alpha(1).restart();
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
        function connectWebSocket() {
            const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
            const wsUrl = `${protocol}//${window.location.host}/ws`;
            
            socket = new WebSocket(wsUrl);
            
            socket.onopen = function() {
                appendToTerminal('Connected to server');
            };
            
            socket.onmessage = function(event) {
                try {
                    const data = JSON.parse(event.data);
                    graphData = data;
                    updateGraph(data);
                    appendToTerminal('Graph updated');
                } catch (e) {
                    appendToTerminal(event.data);
                }
            };
            
            socket.onclose = function() {
                appendToTerminal('Disconnected from server');
                // Try to reconnect after a delay
                setTimeout(connectWebSocket, 3000);
            };
            
            socket.onerror = function(error) {
                appendToTerminal('WebSocket error: ' + error.message);
            };
        }
        
        // Terminal functionality
        const terminalOutput = document.getElementById('terminal-output');
        const terminalInput = document.getElementById('terminal-input');
        const sendButton = document.getElementById('send-btn');
        
        function appendToTerminal(message) {
            const line = document.createElement('div');
            line.textContent = message;
            terminalOutput.appendChild(line);
            terminalOutput.scrollTop = terminalOutput.scrollHeight;
        }
        
        function sendCommand() {
            const command = terminalInput.value.trim();
            if (!command) return;
            
            appendToTerminal('> ' + command);
            
            if (command.startsWith('add-node ')) {
                const parts = command.substring(9).split(' ');
                if (parts.length >= 3) {
                    const id = parts[0];
                    const name = parts[1];
                    const type = parts[2];
                    const status = parts[3] || 'active';
                    
                    const node = {
                        id,
                        name,
                        type,
                        status,
                        properties: {}
                    };
                    
                    socket.send('ADD_NODE:' + JSON.stringify(node));
                } else {
                    appendToTerminal('Error: add-node requires id name type [status]');
                }
            } else if (command.startsWith('add-edge ')) {
                const parts = command.substring(9).split(' ');
                if (parts.length >= 3) {
                    const id = parts[0];
                    const source = parts[1];
                    const target = parts[2];
                    const type = parts[3] || 'default';
                    
                    const edge = {
                        id,
                        source,
                        target,
                        type,
                        properties: {}
                    };
                    
                    socket.send('ADD_EDGE:' + JSON.stringify(edge));
                } else {
                    appendToTerminal('Error: add-edge requires id source target [type]');
                }
            } else if (command === 'help') {
                appendToTerminal('Available commands:');
                appendToTerminal('  add-node <id> <name> <type> [status]');
                appendToTerminal('  add-edge <id> <source> <target> [type]');
                appendToTerminal('  help - show this help');
            } else {
                appendToTerminal('Unknown command. Type "help" for available commands.');
            }
            
            terminalInput.value = '';
        }
        
        sendButton.addEventListener('click', sendCommand);
        terminalInput.addEventListener('keypress', function(event) {
            if (event.key === 'Enter') {
                sendCommand();
            }
        });
        
        // Connect WebSocket when page loads
        window.addEventListener('load', connectWebSocket);
        
        // Resize handler for the graph
        window.addEventListener('resize', function() {
            const width = document.getElementById('graph-container').clientWidth;
            const height = document.getElementById('graph-container').clientHeight;
            svg.attr('viewBox', [0, 0, width, height]);
            simulation.force('center', d3.forceCenter(width / 2, height / 2));
            simulation.alpha(0.3).restart();
        });
    </script>
</body>
</html>"#
            .to_string(),
    )
}

#[tokio::main]
async fn main() {
    // Create shared state
    let graph_state = Arc::new(GraphState::new());

    // Create router
    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .with_state(graph_state);

    // Run server
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    println!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
