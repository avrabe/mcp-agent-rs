# MCP Agent Graph Visualization

This document explains how to use the graph visualization features in MCP Agent, which allow you to visualize task workflows, state machines, or any other graph-based data structure in real-time through a web browser.

## Overview

The MCP Agent includes a powerful graph visualization system based on the Eclipse Sprotty framework. This allows you to:

1. Create and manage graphs representing workflows or other processes
2. Visualize task states in real-time (idle, running, completed, failed)
3. Track dependencies between tasks with visual edges
4. Monitor execution progress through a web interface

## Getting Started

### 1. Enable Visualization in Your Application

To enable visualization in your MCP Agent application, configure the terminal system with visualization enabled:

```rust
use mcp_agent::{
    error::Result,
    terminal::{
        config::{TerminalConfig},
        TerminalSystem,
    },
};

#[tokio::main]
async fn main() -> Result<()> {
    // Create terminal configuration with web visualization enabled
    let mut config = TerminalConfig::web_only();
    config.web_terminal_config.port = 8080;
    config.web_terminal_config.enable_visualization = true;
    config.web_terminal_config.auth_config.allow_anonymous = true;

    // Create and start the terminal system
    let terminal = TerminalSystem::new(config);
    terminal.start().await?;

    println!("Web terminal started at {}", terminal.web_terminal_address().await?);

    // Your application code...

    Ok(())
}
```

### 2. Create and Register Graphs

```rust
use mcp_agent::terminal::graph::{Graph, GraphNode, GraphEdge, GraphManager};

// Get the graph manager from the terminal system
let graph_manager = terminal.graph_manager().await
    .expect("Graph manager should be available");

// Create a graph
let mut graph = Graph::new("workflow-1", "My Workflow");

// Add nodes
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

// Add an edge
graph.add_edge(GraphEdge {
    id: "edge1".to_string(),
    source: "task1".to_string(),
    target: "task2".to_string(),
    edge_type: "dependency".to_string(),
    properties: Default::default(),
});

// Register the graph
graph_manager.register_graph(graph).await?;
```

### 3. Update Node Status During Execution

```rust
// Update a node's status
async fn update_node_status(
    graph_manager: &GraphManager,
    graph_id: &str,
    node_id: &str,
    status: &str,
) -> Result<()> {
    // Get the current graph
    let mut graph = graph_manager.get_graph(graph_id).await?
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

// Example usage during task execution
update_node_status(&graph_manager, "workflow-1", "task1", "running").await?;
// ... task execution ...
update_node_status(&graph_manager, "workflow-1", "task1", "completed").await?;
```

## Node Status Visualization

The visualization system represents different node statuses with distinct visual styles:

| Status    | Color  | Icon | Description                 |
| --------- | ------ | ---- | --------------------------- |
| idle      | Gray   | ○    | Task not yet started        |
| waiting   | Yellow | ⋯    | Task waiting to start       |
| running   | Blue   | ⟳    | Task currently executing    |
| completed | Green  | ✓    | Task completed successfully |
| failed    | Red    | ✗    | Task execution failed       |

## Advanced Usage

### Custom Node Properties

You can attach custom properties to nodes to store additional information:

```rust
use std::collections::HashMap;

let mut properties = HashMap::new();
properties.insert("duration".to_string(), "120s".to_string());
properties.insert("priority".to_string(), "high".to_string());

graph.add_node(GraphNode {
    id: "task1".to_string(),
    name: "Data Collection".to_string(),
    node_type: "task".to_string(),
    status: "idle".to_string(),
    properties,
});
```

### Multiple Graphs

You can manage multiple graphs simultaneously:

```rust
let workflow_graph = Graph::new("workflow-1", "Main Workflow");
let system_graph = Graph::new("system-1", "System Components");

graph_manager.register_graph(workflow_graph).await?;
graph_manager.register_graph(system_graph).await?;
```

## Example

See the `examples/visualize_workflow.rs` file for a complete example of how to create and update a graph visualization.

## Using the Web Interface

1. Start your application with visualization enabled
2. Open your browser to http://localhost:8080 (or the configured port)
3. The visualization appears at the bottom of the terminal interface
4. You can use the control buttons to reset, zoom, or auto-layout the graph

## Troubleshooting

- If no graph appears, check that you've registered at least one graph with the graph manager
- If updates aren't reflected, verify that you're calling `update_graph` after modifying nodes
- If the web interface doesn't load, ensure the terminal system started successfully

## Requirements

The visualization system requires the `terminal-web` feature to be enabled:

```
cargo run --example visualize_workflow --features terminal-web
```
