//! Graph visualization module for the terminal system.
//!
//! This module provides functionality for visualizing various aspects of the system
//! as interactive graphs. It supports multiple types of visualizations:
//!
//! - Workflow graphs: Shows the current state of workflows and their transitions
//! - Agent graphs: Displays agent interactions and their current states
//! - LLM Integration graphs: Visualizes LLM provider states and connections
//! - Human Input graphs: Shows human input points and their states
//!
//! The module is built around the following components:
//!
//! - `GraphManager`: Central manager for all graph providers and visualizations
//! - `GraphProvider`: Trait for implementing different types of graph visualizations
//! - `Graph`: Core data structure for representing graph data
//! - `GraphNode`: Represents nodes in the graph
//! - `GraphEdge`: Represents connections between nodes
//! - Event Handlers: Handle real-time updates and state changes
//!
//! # Example
//!
//! ```rust
//! use crate::terminal::graph::{GraphManager, WorkflowGraphProvider, AgentGraphProvider};
//!
//! async fn setup_graph_visualization() -> Result<()> {
//!     // Create the graph manager
//!     let manager = GraphManager::new();
//!
//!     // Register providers
//!     let workflow_provider = WorkflowGraphProvider::new();
//!     let agent_provider = AgentGraphProvider::new();
//!
//!     manager.register_provider("workflow", workflow_provider).await?;
//!     manager.register_provider("agent", agent_provider).await?;
//!
//!     // Get all graphs
//!     let graphs = manager.get_all_graphs().await?;
//!
//!     Ok(())
//! }
//! ```

use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error};

use self::models::SprottyStatus;
use crate::error::{Error, Result};
use crate::mcp::agent::Agent;
use crate::workflow::engine::WorkflowEngine;

pub mod api;
pub mod events;
pub mod models;
pub mod providers;
pub mod sprotty_adapter;
// pub mod traversal;

// Internal modules
mod agent;
pub mod workflow;

// Re-export key components from submodules
pub use api::create_graph_router;
pub use events::{HumanInputEventHandler, LlmProviderEventHandler};
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

impl std::fmt::Display for GraphUpdateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str_value = match self {
            GraphUpdateType::FullUpdate => "full",
            GraphUpdateType::NodeAdded => "nodeAdded",
            GraphUpdateType::NodeUpdated => "nodeUpdated",
            GraphUpdateType::NodeRemoved => "nodeRemoved",
            GraphUpdateType::EdgeAdded => "edgeAdded",
            GraphUpdateType::EdgeUpdated => "edgeUpdated",
            GraphUpdateType::EdgeRemoved => "edgeRemoved",
        };
        write!(f, "{}", str_value)
    }
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
        let update = GraphUpdate {
            graph_id: graph.id.clone(),
            update_type: GraphUpdateType::FullUpdate,
            graph: Some(graph),
            node: None,
            edge: None,
        };

        self.notify_update(update).await?;
        Ok(())
    }

    /// Unregister a graph
    pub async fn unregister_graph(&self, graph_id: &str) -> Result<()> {
        let mut graphs = self.graphs.write().await;
        if graphs.remove(graph_id).is_none() {
            return Err(Error::TerminalError(format!(
                "Graph not found: {}",
                graph_id
            )));
        }

        Ok(())
    }

    /// Add a node to a graph
    pub async fn add_node(&self, graph_id: &str, node: GraphNode) -> Result<()> {
        let mut graphs = self.graphs.write().await;

        if let Some(graph) = graphs.get_mut(graph_id) {
            graph.nodes.push(node.clone());

            // Notify subscribers of the update
            let update = GraphUpdate {
                graph_id: graph_id.to_string(),
                update_type: GraphUpdateType::NodeAdded,
                graph: Some(graph.clone()),
                node: Some(node),
                edge: None,
            };

            drop(graphs); // Release lock before notification
            self.notify_update(update).await?;
            Ok(())
        } else {
            Err(Error::TerminalError(format!(
                "Graph not found: {}",
                graph_id
            )))
        }
    }

    /// Add an edge to a graph
    pub async fn add_edge(&self, graph_id: &str, edge: GraphEdge) -> Result<()> {
        let mut graphs = self.graphs.write().await;

        if let Some(graph) = graphs.get_mut(graph_id) {
            graph.edges.push(edge.clone());

            // Notify subscribers of the update
            let update = GraphUpdate {
                graph_id: graph_id.to_string(),
                update_type: GraphUpdateType::EdgeAdded,
                graph: Some(graph.clone()),
                node: None,
                edge: Some(edge),
            };

            drop(graphs); // Release lock before notification
            self.notify_update(update).await?;
            Ok(())
        } else {
            Err(Error::TerminalError(format!(
                "Graph not found: {}",
                graph_id
            )))
        }
    }

    /// Update a graph node
    pub async fn update_node(&self, graph_id: &str, node: GraphNode) -> Result<()> {
        let mut graphs = self.graphs.write().await;

        if let Some(graph) = graphs.get_mut(graph_id) {
            // Find the node and update it
            let found = graph
                .nodes
                .iter_mut()
                .find(|n| n.id == node.id)
                .ok_or_else(|| {
                    Error::TerminalError(format!("Node not found: {} in graph {}", node.id, graph_id))
                })?;

            *found = node.clone();

            // Notify subscribers of the update
            let update = GraphUpdate {
                graph_id: graph_id.to_string(),
                update_type: GraphUpdateType::NodeUpdated,
                graph: Some(graph.clone()),
                node: Some(node),
                edge: None,
            };

            drop(graphs); // Release lock before notification
            self.notify_update(update).await?;
            Ok(())
        } else {
            Err(Error::TerminalError(format!(
                "Graph not found: {}",
                graph_id
            )))
        }
    }

    /// Notify subscribers of a graph update directly
    pub async fn notify_update(&self, update: GraphUpdate) -> Result<()> {
        let channels = self.update_channels.read().await;

        // Create a properly formatted Sprotty-compatible update
        let sprotty_update = match &update.update_type {
            GraphUpdateType::FullUpdate => {
                if let Some(graph) = &update.graph {
                    // Convert the entire graph to a Sprotty-compatible format
                    let sprotty_model = models::convert_to_sprotty_model(graph);

                    // Create the update message
                    json!({
                        "type": "GRAPH_UPDATE",
                        "updateType": "full",
                        "graphId": update.graph_id,
                        "model": sprotty_model
                    })
                } else {
                    json!({
                        "type": "GRAPH_UPDATE",
                        "updateType": "full",
                        "graphId": update.graph_id,
                        "error": "No graph data provided"
                    })
                }
            }
            GraphUpdateType::NodeAdded | GraphUpdateType::NodeUpdated => {
                if let Some(node) = &update.node {
                    // Convert the node to a Sprotty-compatible node
                    let sprotty_node = models::convert_node_to_sprotty(node);
                    json!({
                        "type": "GRAPH_UPDATE",
                        "updateType": "nodeUpdate",
                        "graphId": update.graph_id,
                        "node": sprotty_node
                    })
                } else {
                    json!({
                        "type": "GRAPH_UPDATE",
                        "updateType": "nodeUpdate",
                        "graphId": update.graph_id,
                        "error": "No node data provided"
                    })
                }
            }
            GraphUpdateType::EdgeAdded | GraphUpdateType::EdgeUpdated => {
                if let Some(edge) = &update.edge {
                    // Convert the edge to a Sprotty-compatible edge
                    let sprotty_edge = models::convert_edge_to_sprotty(edge);
                    json!({
                        "type": "GRAPH_UPDATE",
                        "updateType": "edgeUpdate",
                        "graphId": update.graph_id,
                        "edge": sprotty_edge
                    })
                } else {
                    json!({
                        "type": "GRAPH_UPDATE",
                        "updateType": "edgeUpdate",
                        "graphId": update.graph_id,
                        "error": "No edge data provided"
                    })
                }
            }
            _ => {
                // For other update types, send a simple notification
                json!({
                    "type": "GRAPH_UPDATE",
                    "updateType": update.update_type.to_string(),
                    "graphId": update.graph_id
                })
            }
        };

        // Convert to a JSON string
        let update_json = serde_json::to_string(&sprotty_update).unwrap_or_else(|_| {
            "{\"type\":\"error\",\"message\":\"Failed to serialize update\"}".to_string()
        });

        // Also send the JSON string representation for web clients
        if let Some(web_update_fn) =
            crate::terminal::terminal_helpers::get_web_update_function().await
        {
            if let Err(e) = web_update_fn("graph_update", &update_json) {
                error!("Failed to send web graph update: {}", e);
            }
        }

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
    pub fn register_provider(&self, _provider: Arc<dyn GraphDataProvider + Send + Sync>) {
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
    _agents: Vec<Arc<Agent>>,
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

struct GraphManagerState {
    providers: Vec<Arc<dyn GraphDataProvider + Send + Sync>>,
    _agents: Vec<Arc<Agent>>,
    _workflow_instances: Vec<Arc<dyn std::any::Any + Send + Sync>>,
}
