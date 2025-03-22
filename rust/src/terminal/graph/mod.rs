//! Graph visualization module for the terminal system.
//!
//! This module provides functionality to visualize workflow graphs, agent relationships,
//! LLM integration points, and human input components. It integrates with the web terminal
//! to provide an interactive visualization that can be toggled on/off.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

use self::models::SprottyStatus;
use crate::error::{Error, Result};
use crate::mcp::agent::Agent;
use crate::workflow::engine::WorkflowEngine;

pub mod api;
pub mod models;
pub mod providers;
pub mod sprotty_adapter;
// pub mod traversal;

// Internal modules
mod agent;
mod workflow;

// Re-export key components from submodules
pub use api::create_graph_router;
pub use models::{
    convert_to_sprotty_model, SprottyEdge, SprottyGraph, SprottyGraphLayout, SprottyNode,
    SprottyRoot, TaskNode,
};
pub use providers::{
    AgentGraphProvider, GraphDataProvider, HumanInputGraphProvider, LlmIntegrationGraphProvider,
    WorkflowGraphProvider,
};
pub use sprotty_adapter::{convert_update_to_action, process_sprotty_action, SprottyAction};

/// Represents a node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    /// Unique identifier for the node
    pub id: String,

    /// Display name for the node
    pub name: String,

    /// Type of node (workflow, task, agent, etc.)
    pub node_type: String,

    /// Current status of the node (idle, active, completed, etc.)
    pub status: String,

    /// Additional properties for the node
    pub properties: HashMap<String, serde_json::Value>,
}

/// Represents an edge in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    /// Unique identifier for the edge
    pub id: String,

    /// Source node ID
    pub source: String,

    /// Target node ID
    pub target: String,

    /// Type of edge (data, control, etc.)
    pub edge_type: String,

    /// Additional properties for the edge
    pub properties: HashMap<String, serde_json::Value>,
}

/// Represents a complete graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    /// Unique identifier for the graph
    pub id: String,

    /// Display name for the graph
    pub name: String,

    /// Type of graph (workflow, agent, etc.)
    pub graph_type: String,

    /// Nodes in the graph
    pub nodes: Vec<GraphNode>,

    /// Edges in the graph
    pub edges: Vec<GraphEdge>,

    /// Additional properties for the graph
    pub properties: HashMap<String, serde_json::Value>,
}

impl Graph {
    /// Create a new graph with the given ID and name
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            graph_type: "default".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            properties: HashMap::new(),
        }
    }
}

/// Graph visualization manager responsible for tracking and updating graphs
#[derive(Debug)]
pub struct GraphManager {
    /// Registered graphs, by ID
    graphs: RwLock<HashMap<String, Graph>>,

    /// Channels for sending graph updates to subscribers
    update_channels: RwLock<Vec<mpsc::Sender<GraphUpdate>>>,
}

/// Graph update even
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphUpdate {
    /// ID of the graph being updated
    pub graph_id: String,

    /// Type of update
    pub update_type: GraphUpdateType,

    /// Updated graph data
    pub graph: Option<Graph>,

    /// Updated node data (for node-level updates)
    pub node: Option<GraphNode>,

    /// Updated edge data (for edge-level updates)
    pub edge: Option<GraphEdge>,
}

/// Types of graph updates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GraphUpdateType {
    /// Full graph update
    FullUpdate,

    /// Node added
    NodeAdded,

    /// Node updated
    NodeUpdated,

    /// Node removed
    NodeRemoved,

    /// Edge added
    EdgeAdded,

    /// Edge updated
    EdgeUpdated,

    /// Edge removed
    EdgeRemoved,
}

impl GraphManager {
    /// Create a new graph manager
    pub fn new() -> Self {
        Self {
            graphs: RwLock::new(HashMap::new()),
            update_channels: RwLock::new(Vec::new()),
        }
    }

    /// Get the unique identifier for this graph manager
    pub async fn id(&self) -> Result<String> {
        // Generate or return a fixed ID
        Ok("graph-manager".to_string())
    }

    /// Register a new graph
    pub async fn register_graph(&self, graph: Graph) -> Result<()> {
        let mut graphs = self.graphs.write().await;
        graphs.insert(graph.id.clone(), graph.clone());

        // Notify subscribers of the new graph
        self.notify_update(GraphUpdate {
            graph_id: graph.id.clone(),
            update_type: GraphUpdateType::FullUpdate,
            graph: Some(graph),
            node: None,
            edge: None,
        })
        .await;

        Ok(())
    }

    /// Unregister a graph
    pub async fn unregister_graph(&self, graph_id: &str) -> Result<()> {
        let mut graphs = self.graphs.write().await;
        if graphs.remove(graph_id).is_none() {
            return Err(Error::TerminalError(format!(
                "Graph {} not found",
                graph_id
            )));
        }

        Ok(())
    }

    /// Add a node to a graph
    pub async fn add_node(&self, graph_id: &str, node: GraphNode) -> Result<()> {
        let mut graphs = self.graphs.write().await;

        if let Some(graph) = graphs.get_mut(graph_id) {
            // Add the node to the graph
            graph.nodes.push(node.clone());

            // Notify subscribers of the node addition
            self.notify_update(GraphUpdate {
                graph_id: graph_id.to_string(),
                update_type: GraphUpdateType::NodeAdded,
                graph: None,
                node: Some(node),
                edge: None,
            })
            .await;

            Ok(())
        } else {
            Err(Error::TerminalError(format!(
                "Graph {} not found",
                graph_id
            )))
        }
    }

    /// Add an edge to a graph
    pub async fn add_edge(&self, graph_id: &str, edge: GraphEdge) -> Result<()> {
        let mut graphs = self.graphs.write().await;

        if let Some(graph) = graphs.get_mut(graph_id) {
            // Add the edge to the graph
            graph.edges.push(edge.clone());

            // Notify subscribers of the edge addition
            self.notify_update(GraphUpdate {
                graph_id: graph_id.to_string(),
                update_type: GraphUpdateType::EdgeAdded,
                graph: None,
                node: None,
                edge: Some(edge),
            })
            .await;

            Ok(())
        } else {
            Err(Error::TerminalError(format!(
                "Graph {} not found",
                graph_id
            )))
        }
    }

    /// Update a graph node
    pub async fn update_node(&self, graph_id: &str, node: GraphNode) -> Result<()> {
        let mut graphs = self.graphs.write().await;

        if let Some(graph) = graphs.get_mut(graph_id) {
            // Find and update the node
            let node_idx = graph.nodes.iter().position(|n| n.id == node.id);

            match node_idx {
                Some(idx) => {
                    graph.nodes[idx] = node.clone();

                    // Notify subscribers of the node update
                    self.notify_update(GraphUpdate {
                        graph_id: graph_id.to_string(),
                        update_type: GraphUpdateType::NodeUpdated,
                        graph: None,
                        node: Some(node),
                        edge: None,
                    })
                    .await;

                    Ok(())
                }
                None => Err(Error::TerminalError(format!(
                    "Node {} not found in graph {}",
                    node.id, graph_id
                ))),
            }
        } else {
            Err(Error::TerminalError(format!(
                "Graph {} not found",
                graph_id
            )))
        }
    }

    /// Notify subscribers of a graph update directly
    pub async fn notify_update(&self, update: GraphUpdate) -> Result<()> {
        let channels = self.update_channels.read().await;

        for channel in channels.iter() {
            // Attempt to send the update, ignoring errors from closed channels
            if let Err(e) = channel.send(update.clone()).await {
                error!("Failed to send graph update: {}", e);
                // We don't return an error here, as we want to try to send to all channels
            }
        }

        Ok(())
    }

    /// Subscribe to graph updates
    pub async fn subscribe(&self) -> mpsc::Receiver<GraphUpdate> {
        let (tx, rx) = mpsc::channel(100);

        let mut channels = self.update_channels.write().await;
        channels.push(tx);

        rx
    }

    /// Get a graph by ID
    pub async fn get_graph(&self, graph_id: &str) -> Option<Graph> {
        let graphs = self.graphs.read().await;
        graphs.get(graph_id).cloned()
    }

    /// List all registered graphs
    pub async fn list_graphs(&self) -> Vec<String> {
        let graphs = self.graphs.read().await;
        graphs.keys().cloned().collect()
    }

    /// Get a graph as a SprottyGraph
    pub async fn get_sprotty_graph(&self, graph_id: &str) -> Option<SprottyGraph> {
        let mcp_graph = self.get_graph(graph_id).await?;

        // Create a new SprottyGraph
        let mut sprotty_graph = SprottyGraph::new(mcp_graph.id.clone(), mcp_graph.name.clone());

        // Add nodes
        for node in &mcp_graph.nodes {
            let mut properties = node.properties.clone();
            properties.insert(
                "type".to_string(),
                serde_json::json!(node.node_type.clone()),
            );

            let sprotty_node = SprottyNode {
                id: node.id.clone(),
                children: Vec::new(),
                layout: None,
                position: None,
                size: None,
                css_classes: Some(vec![
                    format!("type-{}", node.node_type),
                    format!("status-{}", node.status),
                ]),
                label: Some(node.name.clone()),
                status: Some(SprottyStatus::from(node.status.clone())),
                properties,
            };

            sprotty_graph.add_node(sprotty_node);
        }

        // Add edges
        for edge in &mcp_graph.edges {
            let mut properties = edge.properties.clone();
            properties.insert(
                "type".to_string(),
                serde_json::json!(edge.edge_type.clone()),
            );

            let sprotty_edge = SprottyEdge {
                id: edge.id.clone(),
                source: edge.source.clone(),
                target: edge.target.clone(),
                source_port: None,
                target_port: None,
                routing_points: Vec::new(),
                css_classes: Some(vec![format!("type-{}", edge.edge_type)]),
                label: None,
                properties,
            };

            sprotty_graph.add_edge(sprotty_edge);
        }

        Some(sprotty_graph)
    }

    /// Register a graph provider
    pub fn register_provider(&self, provider: Arc<dyn GraphDataProvider + Send + Sync>) {
        debug!("Provider would be registered (not implemented)");
    }
}

/// Create the providers module with default implementations
pub fn create_providers_module() {
    // This function is called by the terminal module to ensure
    // all graph providers are initialized properly
    debug!("Initializing graph visualization providers");
    // The actual providers are already defined in the providers.rs module
    // This function exists to satisfy the reference in terminal/mod.rs
}

impl Default for GraphManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Initialize visualization for the system
#[cfg(feature = "terminal-web")]
pub async fn initialize_visualization(
    workflow_engine: Option<Arc<WorkflowEngine>>,
    agents: Vec<Arc<Agent>>,
) -> Result<Arc<GraphManager>> {
    // Create a new graph manager
    let graph_manager = Arc::new(GraphManager::new());

    // Create and register the workflow provider if workflow_engine is available
    if let Some(workflow_engine) = workflow_engine {
        let workflow_provider = Arc::new(WorkflowGraphProvider::new(workflow_engine));
        graph_manager.register_provider(workflow_provider);
    }

    // Create and register the agent provider
    let agent_provider = Arc::new(AgentGraphProvider::new());
    graph_manager.register_provider(agent_provider);

    Ok(graph_manager)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_graph_manager_register() {
        let manager = GraphManager::new();

        let graph = Graph {
            id: "test-graph".to_string(),
            name: "Test Graph".to_string(),
            graph_type: "workflow".to_string(),
            nodes: vec![],
            edges: vec![],
            properties: HashMap::new(),
        };

        manager.register_graph(graph.clone()).await.unwrap();

        let retrieved = manager.get_graph("test-graph").await;
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "test-graph");
    }
}
