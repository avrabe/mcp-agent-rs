//! Workflow Visualization Example
//!
//! This example demonstrates how to use the graph visualization features.
//! Run with: cargo run --example visualize_workflow --features terminal-web

#[cfg(feature = "terminal-web")]
use mcp_agent::error::Result;
#[cfg(feature = "terminal-web")]
use mcp_agent::terminal::initialize_visualization;
#[cfg(feature = "terminal-web")]
use mcp_agent::terminal::{
    config::{AuthConfig, AuthMethod, TerminalConfig},
    graph::{Graph, GraphEdge, GraphManager, GraphNode},
    TerminalSystem,
};
#[cfg(feature = "terminal-web")]
use std::collections::HashMap;
#[cfg(feature = "terminal-web")]
use std::time::Duration;
#[cfg(feature = "terminal-web")]
use tokio::time::sleep;

#[cfg(not(feature = "terminal-web"))]
fn main() {
    println!("This example requires the terminal-web feature.");
    println!("Run with: cargo run --example visualize_workflow --features terminal-web");
}

#[cfg(feature = "terminal-web")]
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("Starting workflow visualization example...");

    // Create terminal configuration with web visualization enabled
    let mut config = TerminalConfig::web_only();
    config.web_terminal_config.port = 8080;
    config.web_terminal_config.enable_visualization = true;
    config.web_terminal_config.auth_config = AuthConfig {
        auth_method: AuthMethod::Jwt,
        jwt_secret: "secret-key-for-jwt-token-generation".to_string(),
        token_expiration_secs: 3600,
        username: "admin".to_string(),
        password: "password".to_string(),
        allow_anonymous: true,
        require_authentication: false,
    };

    // Create and start the terminal system
    let terminal = TerminalSystem::new(config);
    terminal.start().await?;

    // Get the web address
    let web_address = terminal
        .web_terminal_address()
        .await
        .expect("Web terminal should be available");

    println!("Web terminal started at {}", web_address);
    println!("Open this URL in your browser to see the visualization.");

    // Initialize visualization and get the graph manager
    let graph_manager = initialize_visualization(&terminal, None, vec![]).await;

    // Create a workflow graph
    let graph = Graph {
        id: "workflow-1".to_string(),
        name: "Workflow Example".to_string(),
        graph_type: "workflow".to_string(),
        nodes: Vec::new(),
        edges: Vec::new(),
        properties: HashMap::new(),
    };

    // Register the graph with the manager
    graph_manager.register_graph(graph.clone()).await?;

    // Add nodes to the graph
    let node1 = GraphNode {
        id: "start".to_string(),
        name: "Start".to_string(),
        node_type: "start".to_string(),
        status: "idle".to_string(),
        properties: HashMap::new(),
    };
    graph_manager.add_node("workflow-1", node1).await?;

    let node2 = GraphNode {
        id: "process-data".to_string(),
        name: "Process Data".to_string(),
        node_type: "task".to_string(),
        status: "idle".to_string(),
        properties: HashMap::new(),
    };
    graph_manager.add_node("workflow-1", node2).await?;

    let node3 = GraphNode {
        id: "make-decision".to_string(),
        name: "Make Decision".to_string(),
        node_type: "decision".to_string(),
        status: "idle".to_string(),
        properties: HashMap::new(),
    };
    graph_manager.add_node("workflow-1", node3).await?;

    let node4 = GraphNode {
        id: "path-a".to_string(),
        name: "Path A".to_string(),
        node_type: "task".to_string(),
        status: "idle".to_string(),
        properties: HashMap::new(),
    };
    graph_manager.add_node("workflow-1", node4).await?;

    let node5 = GraphNode {
        id: "path-b".to_string(),
        name: "Path B".to_string(),
        node_type: "task".to_string(),
        status: "idle".to_string(),
        properties: HashMap::new(),
    };
    graph_manager.add_node("workflow-1", node5).await?;

    // Add edges to connect the nodes
    let edge1 = GraphEdge {
        id: "edge-1".to_string(),
        source: "start".to_string(),
        target: "process-data".to_string(),
        edge_type: "flow".to_string(),
        properties: HashMap::new(),
    };
    graph_manager.add_edge("workflow-1", edge1).await?;

    let edge2 = GraphEdge {
        id: "edge-2".to_string(),
        source: "process-data".to_string(),
        target: "make-decision".to_string(),
        edge_type: "flow".to_string(),
        properties: HashMap::new(),
    };
    graph_manager.add_edge("workflow-1", edge2).await?;

    let edge3 = GraphEdge {
        id: "edge-3".to_string(),
        source: "make-decision".to_string(),
        target: "path-a".to_string(),
        edge_type: "flow".to_string(),
        properties: HashMap::new(),
    };
    graph_manager.add_edge("workflow-1", edge3).await?;

    let edge4 = GraphEdge {
        id: "edge-4".to_string(),
        source: "make-decision".to_string(),
        target: "path-b".to_string(),
        edge_type: "flow".to_string(),
        properties: HashMap::new(),
    };
    graph_manager.add_edge("workflow-1", edge4).await?;

    // Simulate workflow execution
    println!("Simulating workflow execution...");
    sleep(Duration::from_secs(2)).await;

    // Start the workflow
    println!("Starting workflow...");
    update_node_status(&graph_manager, "workflow-1", "start", "running").await?;
    sleep(Duration::from_secs(1)).await;
    update_node_status(&graph_manager, "workflow-1", "start", "completed").await?;

    // Process data
    println!("Processing data...");
    update_node_status(&graph_manager, "workflow-1", "process-data", "running").await?;
    sleep(Duration::from_secs(3)).await;
    update_node_status(&graph_manager, "workflow-1", "process-data", "completed").await?;

    // Make decision
    println!("Making decision...");
    update_node_status(&graph_manager, "workflow-1", "make-decision", "running").await?;
    sleep(Duration::from_secs(1)).await;
    update_node_status(&graph_manager, "workflow-1", "make-decision", "completed").await?;

    // Execute both paths (in parallel in a real workflow)
    println!("Executing paths...");
    update_node_status(&graph_manager, "workflow-1", "path-a", "running").await?;
    update_node_status(&graph_manager, "workflow-1", "path-b", "running").await?;

    // Complete the paths at different times
    tokio::spawn(async move {
        sleep(Duration::from_secs(1)).await;
        update_node_status(&graph_manager, "workflow-1", "path-a", "completed")
            .await
            .unwrap();

        sleep(Duration::from_secs(2)).await;
        update_node_status(&graph_manager, "workflow-1", "path-b", "completed")
            .await
            .unwrap();

        println!("Workflow execution complete!");
        println!("The visualization remains available at {}", web_address);
        println!("Press Ctrl+C to exit.");
    });

    // Keep the program running
    println!("Press Ctrl+C to exit...");
    tokio::signal::ctrl_c().await?;
    terminal.stop().await?;
    Ok(())
}

#[cfg(feature = "terminal-web")]
async fn update_node_status(
    graph_manager: &GraphManager,
    graph_id: &str,
    node_id: &str,
    status: &str,
) -> Result<()> {
    // Get the current node from the graph
    if let Some(graph) = graph_manager.get_graph(graph_id).await {
        // Find the node and create an updated copy
        for node in graph.nodes {
            if node.id == node_id {
                let mut updated_node = node.clone();
                updated_node.status = status.to_string();
                graph_manager.update_node(graph_id, updated_node).await?;
                break;
            }
        }
    }

    Ok(())
}
