//! Simple Visualization Example
//!
//! This example demonstrates a minimal setup for the graph visualization system.
//! Run with: cargo run --example simple_visualization --features terminal-web

#[cfg(feature = "terminal-web")]
mod visualization_example {
    use mcp_agent::error::Result;
    use mcp_agent::terminal::{
        config::{AuthConfig, AuthMethod, TerminalConfig, WebTerminalConfig},
        graph::{Graph, GraphEdge, GraphNode},
        initialize_visualization, TerminalSystem,
    };
    use std::collections::HashMap;
    use std::net::IpAddr;
    use std::str::FromStr;

    use std::time::Duration;
    use tokio::time::sleep;

    pub async fn run() -> Result<()> {
        // Initialize logging
        tracing_subscriber::fmt::init();

        println!("Starting simple visualization example...");

        // Create terminal configuration with web visualization enabled
        let mut config = TerminalConfig::default();
        config.console_terminal_enabled = true;

        // Configure web terminal
        let mut web_config = WebTerminalConfig::default();
        web_config.host = IpAddr::from_str("127.0.0.1").unwrap();
        web_config.port = 8080;
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

        config.web_terminal_config = web_config;

        // Create and start terminal system
        let terminal_system = TerminalSystem::new(config);
        terminal_system.start().await?;

        // Initialize visualization
        let graph_manager = initialize_visualization(
            &terminal_system,
            None,   // No workflow engine
            vec![], // No agents
        )
        .await;

        // Create a simple graph
        let graph = Graph {
            id: "simple-graph".to_string(),
            name: "Simple Visualization Demo".to_string(),
            graph_type: "demo".to_string(),
            nodes: vec![
                GraphNode {
                    id: "node-1".to_string(),
                    name: "Start".to_string(),
                    node_type: "start".to_string(),
                    status: "completed".to_string(),
                    properties: HashMap::new(),
                },
                GraphNode {
                    id: "node-2".to_string(),
                    name: "Process".to_string(),
                    node_type: "process".to_string(),
                    status: "active".to_string(),
                    properties: HashMap::new(),
                },
                GraphNode {
                    id: "node-3".to_string(),
                    name: "End".to_string(),
                    node_type: "end".to_string(),
                    status: "pending".to_string(),
                    properties: HashMap::new(),
                },
            ],
            edges: vec![
                GraphEdge {
                    id: "edge-1".to_string(),
                    source: "node-1".to_string(),
                    target: "node-2".to_string(),
                    edge_type: "flow".to_string(),
                    properties: HashMap::new(),
                },
                GraphEdge {
                    id: "edge-2".to_string(),
                    source: "node-2".to_string(),
                    target: "node-3".to_string(),
                    edge_type: "flow".to_string(),
                    properties: HashMap::new(),
                },
            ],
            properties: HashMap::new(),
        };

        // Register graph
        graph_manager.register_graph(graph).await?;

        // Keep the example running to allow time to view in browser
        println!("Visualization is running at http://127.0.0.1:8080/visualization");
        println!("Press Ctrl+C to exit");

        // Wait for user to exit
        loop {
            sleep(Duration::from_secs(1)).await;
        }
    }
}

#[cfg(not(feature = "terminal-web"))]
fn main() {
    println!("This example requires the terminal-web feature");
    println!("Run with: cargo run --example simple_visualization --features terminal-web");
}

#[cfg(feature = "terminal-web")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    visualization_example::run().await?;
    Ok(())
}
