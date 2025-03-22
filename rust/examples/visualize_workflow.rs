//! Workflow Visualization Example
//!
//! This example demonstrates how to use the graph visualization features.
//! Run with: cargo run --example visualize_workflow --features terminal-web

use std::net::IpAddr;
use std::thread::sleep;
use std::time::Duration;

#[cfg(feature = "terminal-web")]
use mcp_agent::error::Result;
#[cfg(feature = "terminal-web")]
use mcp_agent::terminal::{
    TerminalSystem,
    config::{AuthConfig, AuthMethod, TerminalConfig, WebTerminalConfig},
    graph::{
        Graph, GraphEdge, GraphManager, GraphNode,
        models::{SprottyEdge, SprottyGraph, SprottyNode, SprottyStatus},
    },
};

#[cfg(not(feature = "terminal-web"))]
fn main() {
    println!("This example requires the terminal-web feature.");
    println!("Run with: cargo run --example visualize_workflow --features terminal-web");
}

#[cfg(feature = "terminal-web")]
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    println!("Starting workflow visualization example...");

    // Create terminal configuration with web visualization enabled
    let mut config = TerminalConfig::web_only();
    config.web_terminal_config.port = 8080;
    config.web_terminal_config.enable_visualization = true;
    config.web_terminal_config.auth_config.allow_anonymous = true;

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

    // Create a graph manager manually (normally this would be done by initialize_visualization)
    let graph_manager = GraphManager::new();

    // Create a simple workflow graph
    let mut graph = Graph::new("workflow-1", "Sample Workflow");

    // Add task nodes
    graph.add_node(GraphNode {
        id: "task1".to_string(),
        name: "Data Collection".to_string(),
        node_type: "task".to_string(),
        status: "idle".to_string(),
        properties: Default::default(),
    });

    graph.add_node(GraphNode {
        id: "task2".to_string(),
        name: "Data Processing".to_string(),
        node_type: "task".to_string(),
        status: "idle".to_string(),
        properties: Default::default(),
    });

    graph.add_node(GraphNode {
        id: "task3".to_string(),
        name: "Analysis".to_string(),
        node_type: "task".to_string(),
        status: "idle".to_string(),
        properties: Default::default(),
    });

    graph.add_node(GraphNode {
        id: "task4".to_string(),
        name: "Visualization".to_string(),
        node_type: "task".to_string(),
        status: "idle".to_string(),
        properties: Default::default(),
    });

    // Add edges to show task dependencies
    graph.add_edge(GraphEdge {
        id: "edge1".to_string(),
        source: "task1".to_string(),
        target: "task2".to_string(),
        edge_type: "dependency".to_string(),
        properties: Default::default(),
    });

    graph.add_edge(GraphEdge {
        id: "edge2".to_string(),
        source: "task2".to_string(),
        target: "task3".to_string(),
        edge_type: "dependency".to_string(),
        properties: Default::default(),
    });

    graph.add_edge(GraphEdge {
        id: "edge3".to_string(),
        source: "task2".to_string(),
        target: "task4".to_string(),
        edge_type: "dependency".to_string(),
        properties: Default::default(),
    });

    // Register the graph with the graph manager
    graph_manager
        .register_graph(graph)
        .await
        .expect("Failed to register graph");

    // Simulate workflow execution
    println!("Simulating workflow execution...");

    // Task 1: Data Collection (starts immediately)
    println!("Starting task: Data Collection");
    update_node_status(&graph_manager, "workflow-1", "task1", "running").await?;
    sleep(Duration::from_secs(2));

    // Task 1: Complete
    println!("Completed task: Data Collection");
    update_node_status(&graph_manager, "workflow-1", "task1", "completed").await?;
    sleep(Duration::from_secs(1));

    // Task 2: Data Processing
    println!("Starting task: Data Processing");
    update_node_status(&graph_manager, "workflow-1", "task2", "running").await?;
    sleep(Duration::from_secs(3));

    // Task 2: Complete
    println!("Completed task: Data Processing");
    update_node_status(&graph_manager, "workflow-1", "task2", "completed").await?;
    sleep(Duration::from_secs(1));

    // Task 3 & 4: Start in parallel
    println!("Starting tasks: Analysis and Visualization");
    update_node_status(&graph_manager, "workflow-1", "task3", "running").await?;
    update_node_status(&graph_manager, "workflow-1", "task4", "running").await?;
    sleep(Duration::from_secs(3));

    // Task 4: Complete
    println!("Completed task: Visualization");
    update_node_status(&graph_manager, "workflow-1", "task4", "completed").await?;
    sleep(Duration::from_secs(1));

    // Task 3: Failed (simulating an error)
    println!("Task failed: Analysis");
    update_node_status(&graph_manager, "workflow-1", "task3", "failed").await?;

    println!("\nWorkflow execution simulation complete.");
    println!("The graph visualization should be visible in your browser.");
    println!("Press Ctrl+C to exit.");

    // Keep the application running
    loop {
        sleep(Duration::from_secs(1));
    }
}

#[cfg(feature = "terminal-web")]
async fn update_node_status(
    graph_manager: &GraphManager,
    graph_id: &str,
    node_id: &str,
    status: &str,
) -> Result<()> {
    // Get the current graph
    let mut graph = graph_manager
        .get_graph(graph_id)
        .await
        .expect("Graph should exist")
        .clone();

    // Find and update the node
    for node in &mut graph.nodes {
        if node.id == node_id {
            node.status = status.to_string();
            break;
        }
    }

    // Update the graph in the manager
    graph_manager.update_graph(&graph).await?;

    Ok(())
}
