#[cfg(feature = "terminal-web")]
use {
    anyhow::anyhow,
    mcp_agent::{
        init_telemetry,
        terminal::graph::{Graph, GraphEdge, GraphManager, GraphNode},
        TelemetryConfig,
    },
    serde_json::json,
    std::collections::HashMap,
    std::sync::Arc,
    tokio::time::{self, Duration},
};

#[tokio::main]
#[cfg(feature = "terminal-web")]
async fn main() -> anyhow::Result<()> {
    // Initialize telemetry
    let telemetry_config = TelemetryConfig {
        service_name: "graph-output-example".to_string(),
        enable_console: true,
        log_level: "info".to_string(),
        enable_opentelemetry: false,
        opentelemetry_endpoint: None,
    };

    // Handle init_telemetry error explicitly without using the ? operator
    if let Err(e) = init_telemetry(telemetry_config) {
        eprintln!("Warning: Failed to initialize telemetry: {}", e);
        // Continue execution, don't exit
    }

    // Create a graph manager
    let graph_manager = Arc::new(GraphManager::new());

    // Create a sample workflow graph
    let workflow_graph = create_sample_workflow_graph();

    // Register the graph with the manager
    graph_manager
        .register_graph(workflow_graph.clone())
        .await
        .map_err(|e| anyhow!("{}", e))?;

    // Output the graph as JSON
    let graph_json = serde_json::to_string_pretty(&workflow_graph)?;
    println!("\nWorkflow Graph JSON:\n{}", graph_json);

    // Create a sample agent graph
    let agent_graph = create_sample_agent_graph();

    // Register the agent graph
    graph_manager
        .register_graph(agent_graph.clone())
        .await
        .map_err(|e| anyhow!("{}", e))?;

    // Output the agent graph as JSON
    let agent_json = serde_json::to_string_pretty(&agent_graph)?;
    println!("\nAgent Graph JSON:\n{}", agent_json);

    // Simulate some graph updates
    println!("\nSimulating graph updates...");

    // Update workflow graph nodes
    for i in 0..3 {
        let mut node = workflow_graph.nodes[i].clone();

        // Update the node status based on a simple pattern
        match i % 3 {
            0 => {
                node.status = "running".to_string();
                node.properties.insert("progress".to_string(), json!(50));
            }
            1 => {
                node.status = "completed".to_string();
                node.properties.insert("progress".to_string(), json!(100));
            }
            _ => {
                node.status = "pending".to_string();
                node.properties.insert("progress".to_string(), json!(0));
            }
        }

        // Update the node in the graph manager
        graph_manager
            .update_node(&workflow_graph.id, node.clone())
            .await
            .map_err(|e| anyhow!("{}", e))?;
        println!("Updated node: {} to status: {}", node.id, node.status);

        // Sleep for a moment to simulate time passing
        time::sleep(Duration::from_millis(500)).await;
    }

    // Get the updated graph
    let updated_graph = graph_manager
        .get_graph(&workflow_graph.id)
        .await
        .ok_or_else(|| anyhow!("Updated graph not found"))?;

    // Output the updated graph
    let updated_json = serde_json::to_string_pretty(&updated_graph)?;
    println!("\nUpdated Workflow Graph JSON:\n{}", updated_json);

    // List all graphs
    let graph_ids = graph_manager.list_graphs().await;
    println!("\nAll registered graphs: {:?}", graph_ids);

    Ok(())
}

#[cfg(not(feature = "terminal-web"))]
fn main() {
    println!("This example requires the 'terminal-web' feature to be enabled.");
    println!("Please run with: cargo run --example graph_output_example --features terminal-web");
}

#[cfg(feature = "terminal-web")]
fn create_sample_workflow_graph() -> Graph {
    // Create nodes
    let nodes = vec![
        GraphNode {
            id: "task-1".to_string(),
            name: "Data Collection".to_string(),
            node_type: "task".to_string(),
            status: "idle".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("importance".to_string(), json!("high"));
                props.insert("duration".to_string(), json!(5000));
                props
            },
        },
        GraphNode {
            id: "task-2".to_string(),
            name: "Data Processing".to_string(),
            node_type: "task".to_string(),
            status: "idle".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("importance".to_string(), json!("medium"));
                props.insert("duration".to_string(), json!(3000));
                props
            },
        },
        GraphNode {
            id: "task-3".to_string(),
            name: "Result Analysis".to_string(),
            node_type: "task".to_string(),
            status: "idle".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("importance".to_string(), json!("high"));
                props.insert("duration".to_string(), json!(2000));
                props
            },
        },
    ];

    // Create edges
    let edges = vec![
        GraphEdge {
            id: "edge-1-2".to_string(),
            source: "task-1".to_string(),
            target: "task-2".to_string(),
            edge_type: "dependency".to_string(),
            properties: HashMap::new(),
        },
        GraphEdge {
            id: "edge-2-3".to_string(),
            source: "task-2".to_string(),
            target: "task-3".to_string(),
            edge_type: "dependency".to_string(),
            properties: HashMap::new(),
        },
    ];

    // Create the graph
    Graph {
        id: "workflow-graph".to_string(),
        name: "Sample Workflow".to_string(),
        graph_type: "workflow".to_string(),
        nodes,
        edges,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "created_at".to_string(),
                json!(chrono::Utc::now().to_rfc3339()),
            );
            props
        },
    }
}

#[cfg(feature = "terminal-web")]
fn create_sample_agent_graph() -> Graph {
    // Create nodes
    let nodes = vec![
        GraphNode {
            id: "agent-1".to_string(),
            name: "Orchestrator Agent".to_string(),
            node_type: "agent".to_string(),
            status: "active".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("role".to_string(), json!("orchestrator"));
                props
            },
        },
        GraphNode {
            id: "agent-2".to_string(),
            name: "Data Agent".to_string(),
            node_type: "agent".to_string(),
            status: "active".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("role".to_string(), json!("data-processing"));
                props
            },
        },
        GraphNode {
            id: "agent-3".to_string(),
            name: "Analysis Agent".to_string(),
            node_type: "agent".to_string(),
            status: "standby".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("role".to_string(), json!("analysis"));
                props
            },
        },
    ];

    // Create edges
    let edges = vec![
        GraphEdge {
            id: "edge-a1-a2".to_string(),
            source: "agent-1".to_string(),
            target: "agent-2".to_string(),
            edge_type: "communication".to_string(),
            properties: HashMap::new(),
        },
        GraphEdge {
            id: "edge-a1-a3".to_string(),
            source: "agent-1".to_string(),
            target: "agent-3".to_string(),
            edge_type: "communication".to_string(),
            properties: HashMap::new(),
        },
    ];

    // Create the graph
    Graph {
        id: "agent-graph".to_string(),
        name: "Sample Agent Network".to_string(),
        graph_type: "agent".to_string(),
        nodes,
        edges,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "created_at".to_string(),
                json!(chrono::Utc::now().to_rfc3339()),
            );
            props
        },
    }
}
