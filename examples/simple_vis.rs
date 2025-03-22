use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio::time;

use mcp_agent::error::Result;
use mcp_agent::terminal::{TerminalConfig, initialize_terminal, initialize_visualization};
use mcp_agent::terminal::config::AuthConfig;
use mcp_agent::terminal::graph::{Graph, GraphNode, GraphEdge};

#[tokio::main]
async fn main() -> Result<()> {
    // Setup terminal config with visualization enabled
    let terminal_config = TerminalConfig {
        console_enabled: true,
        web_terminal_enabled: true,
        web_terminal_host: "127.0.0.1".to_string(),
        web_terminal_port: 8080,
        auth_config: AuthConfig::default(),
    };
    
    // Initialize the terminal system
    let terminal_system = initialize_terminal(terminal_config).await?;
    
    // Initialize visualization (without specific providers)
    let graph_manager = initialize_visualization(
        &terminal_system,
        None, // No workflow engine
        vec![], // No additional providers
        vec![], // No agents
        None, // No LLM provider
    ).await;
    
    if let Some(graph_manager) = graph_manager {
        println!("Visualization system started at http://127.0.0.1:8080");
        println!("Press Ctrl+C to exit");
        
        // Create a sample workflow graph
        let mut workflow_graph = Graph {
            id: "workflow-graph".to_string(),
            name: "Sample Workflow".to_string(),
            graph_type: "workflow".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            properties: HashMap::new(),
        };
        
        // Add a few nodes
        let task1 = GraphNode {
            id: "task-1".to_string(),
            name: "Task 1".to_string(),
            node_type: "task".to_string(),
            status: "completed".to_string(),
            properties: HashMap::new(),
        };
        
        let task2 = GraphNode {
            id: "task-2".to_string(),
            name: "Task 2".to_string(),
            node_type: "task".to_string(),
            status: "active".to_string(),
            properties: HashMap::new(),
        };
        
        let task3 = GraphNode {
            id: "task-3".to_string(),
            name: "Task 3".to_string(),
            node_type: "task".to_string(),
            status: "idle".to_string(),
            properties: HashMap::new(),
        };
        
        // Add edges
        let edge1 = GraphEdge {
            id: "edge-1-2".to_string(),
            source: "task-1".to_string(),
            target: "task-2".to_string(),
            edge_type: "dependency".to_string(),
            properties: HashMap::new(),
        };
        
        let edge2 = GraphEdge {
            id: "edge-2-3".to_string(),
            source: "task-2".to_string(),
            target: "task-3".to_string(),
            edge_type: "dependency".to_string(),
            properties: HashMap::new(),
        };
        
        // Add all nodes and edges to the graph
        workflow_graph.nodes.push(task1);
        workflow_graph.nodes.push(task2);
        workflow_graph.nodes.push(task3);
        
        workflow_graph.edges.push(edge1);
        workflow_graph.edges.push(edge2);
        
        // Register the graph
        graph_manager.register_graph(workflow_graph).await.unwrap();
        
        // Keep the program running
        loop {
            time::sleep(Duration::from_secs(1)).await;
        }
    } else {
        println!("Failed to start visualization system");
    }
    
    Ok(())
} 