use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde_json::json;
use tokio::time;

// Empty test that can run without the terminal-web feature
#[cfg(test)]
mod tests {
    #[test]
    fn test_simple_visualization_compiles() {
        // This empty test allows cargo test to succeed even without the terminal-web feature
        assert!(true);
    }
}

#[cfg(feature = "terminal-web")]
mod terminal_web_code {
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use anyhow::Result;
    use serde_json::json;
    use tokio::time;

    use mcp_agent::terminal::config::AuthConfig;
    use mcp_agent::terminal::graph::{Graph, GraphEdge, GraphNode, GraphUpdateType};
    use mcp_agent::terminal::{initialize_terminal, initialize_visualization, TerminalConfig};

    pub async fn run_visualization() -> Result<()> {
        // Setup terminal config with visualization enabled
        let terminal_config = TerminalConfig {
            console_enabled: true,
            web_terminal_enabled: true,
            web_terminal_host: "127.0.0.1".to_string(),
            web_terminal_port: 8080,
            auth_config: AuthConfig::default(),
            visualization_enabled: true,
        };

        // Initialize the terminal system
        let terminal_system = initialize_terminal(terminal_config).await?;

        // Initialize visualization (without specific providers)
        let graph_manager = initialize_visualization(
            &terminal_system,
            None,   // No workflow engine
            vec![], // No additional providers
            vec![], // No agents
            None,   // No LLM provider
        )
        .await;

        if let Some(graph_manager) = graph_manager {
            println!("Visualization system started at http://127.0.0.1:8080");
            println!("Press Ctrl+C to exit");

            // Create a sample workflow graph
            let workflow_graph = create_sample_workflow_graph();

            // Register the graph
            graph_manager
                .register_graph(workflow_graph.clone())
                .await
                .unwrap();

            // Create a sample agent graph
            let agent_graph = create_sample_agent_graph();

            // Register the graph
            graph_manager.register_graph(agent_graph).await.unwrap();

            // Simulate activity on the graph
            let graph_manager_clone = graph_manager.clone();
            tokio::spawn(async move {
                let mut interval = time::interval(Duration::from_secs(3));

                let nodes = vec!["task-1", "task-2", "task-3", "task-4", "task-5"];

                let statuses = vec!["idle", "active", "completed", "failed"];

                let mut i = 0;

                loop {
                    interval.tick().await;

                    let node_id = nodes[i % nodes.len()];
                    let status = statuses[(i + 1) % statuses.len()];

                    // Update a random node status
                    let mut node = GraphNode {
                        id: node_id.to_string(),
                        name: format!("Task {}", i % nodes.len() + 1),
                        node_type: "task".to_string(),
                        status: status.to_string(),
                        properties: HashMap::new(),
                    };

                    // Add some properties
                    node.properties.insert("progress".to_string(), json!(0.5));
                    node.properties
                        .insert("importance".to_string(), json!("high"));

                    // Update the node
                    graph_manager_clone
                        .update_node("workflow-graph", node)
                        .await
                        .unwrap_or_else(|e| {
                            println!("Error updating node: {}", e);
                        });

                    i += 1;
                }
            });

            // Keep the program running
            loop {
                time::sleep(Duration::from_secs(1)).await;
            }
        } else {
            println!("Failed to start visualization system");
        }

        Ok(())
    }

    fn create_sample_workflow_graph() -> Graph {
        let mut graph = Graph {
            id: "workflow-graph".to_string(),
            name: "Sample Workflow".to_string(),
            graph_type: "workflow".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            properties: HashMap::new(),
        };

        // Add properties
        graph.properties.insert(
            "description".to_string(),
            json!("A sample workflow graph for visualization demo"),
        );

        // Add nodes
        let task1 = GraphNode {
            id: "task-1".to_string(),
            name: "Task 1".to_string(),
            node_type: "task".to_string(),
            status: "completed".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("duration".to_string(), json!(5));
                props.insert("progress".to_string(), json!(1.0));
                props
            },
        };

        let task2 = GraphNode {
            id: "task-2".to_string(),
            name: "Task 2".to_string(),
            node_type: "task".to_string(),
            status: "active".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("duration".to_string(), json!(10));
                props.insert("progress".to_string(), json!(0.5));
                props
            },
        };

        let task3 = GraphNode {
            id: "task-3".to_string(),
            name: "Task 3".to_string(),
            node_type: "task".to_string(),
            status: "idle".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("duration".to_string(), json!(7));
                props.insert("progress".to_string(), json!(0.0));
                props
            },
        };

        let task4 = GraphNode {
            id: "task-4".to_string(),
            name: "Task 4".to_string(),
            node_type: "task".to_string(),
            status: "idle".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("duration".to_string(), json!(3));
                props.insert("progress".to_string(), json!(0.0));
                props
            },
        };

        let task5 = GraphNode {
            id: "task-5".to_string(),
            name: "Task 5".to_string(),
            node_type: "task".to_string(),
            status: "idle".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("duration".to_string(), json!(6));
                props.insert("progress".to_string(), json!(0.0));
                props.insert("importance".to_string(), json!("high"));
                props
            },
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
            id: "edge-1-3".to_string(),
            source: "task-1".to_string(),
            target: "task-3".to_string(),
            edge_type: "dependency".to_string(),
            properties: HashMap::new(),
        };

        let edge3 = GraphEdge {
            id: "edge-2-4".to_string(),
            source: "task-2".to_string(),
            target: "task-4".to_string(),
            edge_type: "dependency".to_string(),
            properties: HashMap::new(),
        };

        let edge4 = GraphEdge {
            id: "edge-3-5".to_string(),
            source: "task-3".to_string(),
            target: "task-5".to_string(),
            edge_type: "dependency".to_string(),
            properties: HashMap::new(),
        };

        let edge5 = GraphEdge {
            id: "edge-4-5".to_string(),
            source: "task-4".to_string(),
            target: "task-5".to_string(),
            edge_type: "dependency".to_string(),
            properties: HashMap::new(),
        };

        // Add all nodes and edges to the graph
        graph.nodes.push(task1);
        graph.nodes.push(task2);
        graph.nodes.push(task3);
        graph.nodes.push(task4);
        graph.nodes.push(task5);

        graph.edges.push(edge1);
        graph.edges.push(edge2);
        graph.edges.push(edge3);
        graph.edges.push(edge4);
        graph.edges.push(edge5);

        graph
    }

    fn create_sample_agent_graph() -> Graph {
        let mut graph = Graph {
            id: "agent-graph".to_string(),
            name: "Agent System".to_string(),
            graph_type: "agent".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            properties: HashMap::new(),
        };

        // Add properties
        graph.properties.insert(
            "description".to_string(),
            json!("A sample agent graph for visualization demo"),
        );

        // Add nodes
        let agent1 = GraphNode {
            id: "agent-1".to_string(),
            name: "User Interface Agent".to_string(),
            node_type: "agent".to_string(),
            status: "active".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("role".to_string(), json!("UI"));
                props.insert("version".to_string(), json!("1.0"));
                props
            },
        };

        let agent2 = GraphNode {
            id: "agent-2".to_string(),
            name: "Data Processing Agent".to_string(),
            node_type: "agent".to_string(),
            status: "active".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("role".to_string(), json!("Processor"));
                props.insert("version".to_string(), json!("2.1"));
                props
            },
        };

        let agent3 = GraphNode {
            id: "agent-3".to_string(),
            name: "Storage Agent".to_string(),
            node_type: "agent".to_string(),
            status: "active".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("role".to_string(), json!("Storage"));
                props.insert("version".to_string(), json!("1.5"));
                props
            },
        };

        // Add edges
        let edge1 = GraphEdge {
            id: "edge-a1-a2".to_string(),
            source: "agent-1".to_string(),
            target: "agent-2".to_string(),
            edge_type: "communication".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("protocol".to_string(), json!("HTTP"));
                props
            },
        };

        let edge2 = GraphEdge {
            id: "edge-a2-a3".to_string(),
            source: "agent-2".to_string(),
            target: "agent-3".to_string(),
            edge_type: "communication".to_string(),
            properties: {
                let mut props = HashMap::new();
                props.insert("protocol".to_string(), json!("gRPC"));
                props
            },
        };

        // Add all nodes and edges to the graph
        graph.nodes.push(agent1);
        graph.nodes.push(agent2);
        graph.nodes.push(agent3);

        graph.edges.push(edge1);
        graph.edges.push(edge2);

        graph
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(feature = "terminal-web")]
    {
        terminal_web_code::run_visualization().await?;
    }

    #[cfg(not(feature = "terminal-web"))]
    {
        println!("This example requires the 'terminal-web' feature to be enabled.");
        println!(
            "Please rebuild with: cargo build --example simple_visualization --features terminal-web"
        );
    }

    Ok(())
}
