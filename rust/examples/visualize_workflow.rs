//! Workflow Visualization Example
//!
//! This example demonstrates how to use the graph visualization features.
//! Run with: cargo run --example visualize_workflow --features terminal-web
//!
//! Options:
//! --open-browser - Automatically open the browser to view the visualization

#[cfg(feature = "terminal-web")]
use colored::Colorize;
#[cfg(feature = "terminal-web")]
use mcp_agent::error::Result;
#[cfg(feature = "terminal-web")]
use mcp_agent::terminal::initialize_visualization;
#[cfg(feature = "terminal-web")]
use mcp_agent::terminal::{
    config::{AuthConfig, AuthMethod, TerminalConfig, WebTerminalConfig},
    graph::{Graph, GraphEdge, GraphManager, GraphNode},
    TerminalSystem,
};
#[cfg(feature = "terminal-web")]
use std::collections::HashMap;
#[cfg(feature = "terminal-web")]
use std::env;
#[cfg(feature = "terminal-web")]
use std::net::{IpAddr, Ipv4Addr};
#[cfg(feature = "terminal-web")]
use std::sync::Arc;
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

    // Check for open-browser flag
    let should_open_browser = env::args().any(|arg| arg == "--open-browser");

    println!("Starting workflow visualization example...");

    // Create terminal configuration with web visualization enabled
    let mut web_config = WebTerminalConfig::default();
    web_config.host = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    web_config.port = 8888;
    web_config.enable_visualization = true;
    web_config.auth_config = AuthConfig {
        auth_method: AuthMethod::Jwt,
        jwt_secret: "secret-key-for-jwt-token-generation".to_string(),
        token_expiration_secs: 3600,
        username: "admin".to_string(),
        password: "password".to_string(),
        allow_anonymous: true,
        require_authentication: false,
    };

    let config = TerminalConfig {
        console_terminal_enabled: true,
        web_terminal_enabled: true,
        web_terminal_config: web_config,
        ..Default::default()
    };

    // Create and start the terminal system
    let terminal = TerminalSystem::new(config);
    println!("Starting terminal system...");
    terminal.start().await?;

    // Give the server a moment to start up
    println!("Waiting for server to start...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Get the web address
    let web_address = terminal
        .web_terminal_address()
        .await
        .expect("Web terminal should be available");

    // Construct the full visualization URL
    let vis_url = format!("http://{}/vis", web_address);

    println!(
        "\n{}",
        "Workflow Visualization Server Started"
            .bright_green()
            .bold()
    );
    println!(
        "View visualization at: {}",
        vis_url.bright_blue().underline()
    );

    // Open browser if requested
    if should_open_browser {
        println!("Opening browser automatically...");
        match webbrowser::open(&vis_url) {
            Ok(_) => println!("Browser opened successfully"),
            Err(e) => println!("Failed to open browser: {}", e),
        }
    } else {
        println!("Run with --open-browser flag to automatically open in your browser");
    }

    // Initialize visualization and get the graph manager
    println!("Initializing visualization...");
    let graph_manager = initialize_visualization(
        &terminal,
        None,
        vec![],
        Vec::<Arc<dyn std::any::Any + Send + Sync>>::new(),
        None::<Arc<dyn std::any::Any + Send + Sync>>,
    )
    .await;

    println!("Creating workflow graph...");
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
    println!("Registering graph...");
    graph_manager.register_graph(graph.clone()).await?;

    // Ensure the graph is available for visualization
    println!("Setting active graph...");

    // Get the current graph
    let graph_opt = graph_manager.get_graph("workflow-1").await;
    if let Some(graph) = graph_opt {
        // Update the graph to set it as active
        graph_manager.update_graph("workflow-1", graph).await?;
    }

    // Add nodes to the graph
    println!("Adding nodes to graph...");
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
    println!("\n{}", "Simulating workflow execution...".yellow());
    sleep(Duration::from_secs(2)).await;

    // Start the workflow
    println!("{}", "Starting workflow...".yellow());
    update_node_status(&graph_manager, "workflow-1", "start", "running").await?;
    sleep(Duration::from_secs(1)).await;
    update_node_status(&graph_manager, "workflow-1", "start", "completed").await?;

    // Process data
    println!("{}", "Processing data...".yellow());
    update_node_status(&graph_manager, "workflow-1", "process-data", "running").await?;
    sleep(Duration::from_secs(3)).await;
    update_node_status(&graph_manager, "workflow-1", "process-data", "completed").await?;

    // Make decision
    println!("{}", "Making decision...".yellow());
    update_node_status(&graph_manager, "workflow-1", "make-decision", "running").await?;
    sleep(Duration::from_secs(1)).await;
    update_node_status(&graph_manager, "workflow-1", "make-decision", "completed").await?;

    // Execute both paths (in parallel in a real workflow)
    println!("{}", "Executing paths...".yellow());
    update_node_status(&graph_manager, "workflow-1", "path-a", "running").await?;
    update_node_status(&graph_manager, "workflow-1", "path-b", "running").await?;

    // Complete the paths at different times
    let vis_url_clone = vis_url.clone();
    tokio::spawn(async move {
        sleep(Duration::from_secs(1)).await;
        update_node_status(&graph_manager, "workflow-1", "path-a", "completed")
            .await
            .unwrap();

        sleep(Duration::from_secs(2)).await;
        update_node_status(&graph_manager, "workflow-1", "path-b", "completed")
            .await
            .unwrap();

        println!("\n{}", "Workflow execution complete!".bright_green().bold());
        println!(
            "The visualization remains available at: {}",
            vis_url_clone.bright_blue().underline()
        );
        println!("{}", "Press Ctrl+C to exit.".bright_yellow());
    });

    // Keep the program running
    println!("\n{}", "Press Ctrl+C to exit...".bright_yellow());
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
