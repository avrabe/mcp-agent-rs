use crate::mcp::agent::Agent;
use crate::terminal::graph::providers::AgentGraphProvider;
use crate::terminal::graph::{Graph, GraphManager};

use std::sync::Arc;
use tracing::debug;

/// Set up agent visualization using the agent graph provider
pub fn setup_agent_visualization(agents: Vec<Arc<Agent>>, graph_manager: Arc<GraphManager>) {
    debug!("Setting up agent visualization for {} agents", agents.len());

    // Create the agent graph provider
    let agent_provider = Arc::new(AgentGraphProvider::new());

    // Add agents to the provider
    for agent in agents {
        let provider_clone = Arc::clone(&agent_provider);
        tokio::spawn(async move {
            provider_clone.add_agent(agent).await;
        });
    }

    // Register the provider with the graph manager
    graph_manager.register_provider(agent_provider);

    debug!("Agent visualization setup complete");
}
