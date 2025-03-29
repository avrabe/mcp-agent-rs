//! Workflow graph visualization module.
//!
//! This module provides functionality for visualizing workflows in the terminal system.

use crate::error::Result;
use crate::terminal::graph::providers::AsyncGraphDataProvider;
use crate::terminal::graph::{
    Graph, GraphEdge, GraphManager, GraphNode, GraphUpdate, GraphUpdateType,
};
use crate::workflow::engine::WorkflowEngine;
use crate::workflow::state::WorkflowState;

use async_trait::async_trait;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error};

use super::sprotty_adapter::SprottyAction;

/// A graph data provider for workflow visualization
#[derive(Debug)]
pub struct WorkflowGraphProvider {
    /// Provider name
    pub name: String,
    /// Workflow engine reference
    workflow_engine: Arc<WorkflowEngine>,
    /// Graph manager reference
    graph_manager: Arc<RwLock<Option<Arc<GraphManager>>>>,
}

impl WorkflowGraphProvider {
    /// Create a new workflow graph provider
    pub fn new(workflow_engine: Arc<WorkflowEngine>) -> Self {
        Self {
            name: "workflow".to_string(),
            workflow_engine,
            graph_manager: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a graph from the current workflow state
    async fn create_graph_from_workflow(&self) -> Result<Graph> {
        let mut graph = Graph {
            id: "workflow-graph".to_string(),
            name: "Workflow Graph".to_string(),
            graph_type: "workflow".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            properties: HashMap::new(),
        };

        // Get workflow state from the engine
        if let Ok(workflow_state) = self.workflow_engine.state().await {
            // Update graph properties
            graph.properties.insert(
                "workflow_id".to_string(),
                serde_json::to_value(workflow_state.name.as_deref().unwrap_or("unknown"))
                    .unwrap_or(serde_json::Value::Null),
            );

            graph.properties.insert(
                "status".to_string(),
                serde_json::to_value(workflow_state.status.clone())
                    .unwrap_or(serde_json::Value::Null),
            );

            // Create a node for the workflow itself
            let mut node_properties = HashMap::new();
            node_properties.insert(
                "status".to_string(),
                serde_json::Value::String(workflow_state.status.clone()),
            );
            if let Some(error) = &workflow_state.error {
                node_properties.insert(
                    "error".to_string(),
                    serde_json::Value::String(format!("{:?}", error)),
                );
            }

            let workflow_node = GraphNode {
                id: workflow_state
                    .name
                    .as_deref()
                    .unwrap_or("workflow")
                    .to_string(),
                name: workflow_state
                    .name
                    .as_deref()
                    .unwrap_or("Workflow")
                    .to_string(),
                node_type: "workflow".to_string(),
                status: workflow_state.status.clone(),
                properties: node_properties,
            };
            graph.nodes.push(workflow_node);

            // Add nodes for metadata entries
            for (key, value) in workflow_state.metadata.iter() {
                let node = GraphNode {
                    id: format!("metadata-{}", key),
                    name: key.clone(),
                    node_type: "metadata".to_string(),
                    status: "active".to_string(),
                    properties: {
                        let mut props = HashMap::new();
                        props.insert("value".to_string(), value.clone());
                        props
                    },
                };
                graph.nodes.push(node);

                // Add edge from workflow to metadata
                let edge = GraphEdge {
                    id: format!("workflow-metadata-{}", key),
                    source: workflow_state
                        .name
                        .as_deref()
                        .unwrap_or("workflow")
                        .to_string(),
                    target: format!("metadata-{}", key),
                    edge_type: "metadata".to_string(),
                    properties: HashMap::new(),
                };
                graph.edges.push(edge);
            }
        }

        Ok(graph)
    }

    /// Update the graph with the current workflow state
    pub async fn update_graph(&self, workflow_state: &WorkflowState) -> Result<Graph> {
        let mut graph = Graph {
            id: workflow_state
                .name
                .as_deref()
                .unwrap_or("workflow")
                .to_string(),
            name: workflow_state
                .name
                .as_deref()
                .unwrap_or("Workflow")
                .to_string(),
            graph_type: "workflow".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            properties: HashMap::new(),
        };

        // Create a node for the workflow itself
        let mut node_properties = HashMap::new();
        node_properties.insert(
            "status".to_string(),
            serde_json::Value::String(workflow_state.status.clone()),
        );
        if let Some(error) = &workflow_state.error {
            node_properties.insert(
                "error".to_string(),
                serde_json::Value::String(format!("{:?}", error)),
            );
        }

        let workflow_node = GraphNode {
            id: workflow_state
                .name
                .as_deref()
                .unwrap_or("workflow")
                .to_string(),
            name: workflow_state
                .name
                .as_deref()
                .unwrap_or("Workflow")
                .to_string(),
            node_type: "workflow".to_string(),
            status: workflow_state.status.clone(),
            properties: node_properties,
        };
        graph.nodes.push(workflow_node);

        // Add nodes for metadata entries
        for (key, value) in workflow_state.metadata.iter() {
            let node = GraphNode {
                id: format!("metadata-{}", key),
                name: key.clone(),
                node_type: "metadata".to_string(),
                status: "active".to_string(),
                properties: {
                    let mut props = HashMap::new();
                    props.insert("value".to_string(), value.clone());
                    props
                },
            };
            graph.nodes.push(node);

            // Add edge from workflow to metadata
            let edge = GraphEdge {
                id: format!("workflow-metadata-{}", key),
                source: workflow_state
                    .name
                    .as_deref()
                    .unwrap_or("workflow")
                    .to_string(),
                target: format!("metadata-{}", key),
                edge_type: "metadata".to_string(),
                properties: HashMap::new(),
            };
            graph.edges.push(edge);
        }

        Ok(graph)
    }

    pub async fn handle_state_change(&self, workflow_state: &WorkflowState) -> Result<()> {
        if let Some(manager) = self.graph_manager.read().await.as_ref() {
            let graph = self.update_graph(workflow_state).await?;
            let graph_id = graph.id.clone();
            manager.update_graph(&graph_id, graph).await?;
        }
        Ok(())
    }

    pub async fn handle_sprotty_action(
        &self,
        action: &SprottyAction,
        graph_id: &str,
        graph_manager: Arc<GraphManager>,
    ) -> Result<GraphUpdate> {
        // Setup tracking if not already done
        if self.graph_manager.read().await.is_none() {
            *self.graph_manager.write().await = Some(graph_manager.clone());
        }

        // Always use the current workflow state to create a new graph
        let workflow_state_result = self.workflow_engine.state().await;

        // Create empty graph as fallback
        let graph = match workflow_state_result {
            Ok(state) => self.update_graph(&state).await?,
            Err(_) => Graph {
                id: graph_id.to_string(),
                name: "Empty Workflow".to_string(),
                graph_type: "workflow".to_string(),
                nodes: Vec::new(),
                edges: Vec::new(),
                properties: HashMap::new(),
            },
        };

        // Return a graph update
        Ok(GraphUpdate {
            graph_id: graph_id.to_string(),
            update_type: GraphUpdateType::FullUpdate,
            graph: Some(graph),
            node: None,
            edge: None,
        })
    }
}

#[async_trait]
impl AsyncGraphDataProvider for WorkflowGraphProvider {
    async fn generate_graph(&self) -> Result<Graph> {
        self.create_graph_from_workflow().await
    }

    async fn setup_tracking(&self, graph_manager: Arc<GraphManager>) -> Result<()> {
        // Store graph manager reference
        *self.graph_manager.write().await = Some(graph_manager.clone());

        // Create an initial graph
        let graph = self.create_graph_from_workflow().await?;

        // Register with graph manager
        graph_manager.register_graph(graph.clone()).await?;

        // Set up a background task to periodically update the graph
        let engine = self.workflow_engine.clone();
        let graph_id = "workflow-graph".to_string();
        let graph_manager_clone = graph_manager.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));

            loop {
                interval.tick().await;

                // Check if workflow state has changed
                if let Ok(state) = engine.state().await {
                    debug!("Checking workflow state for updates: {:?}", state.status);

                    // Get the current graph
                    if let Some(graph) = graph_manager_clone.get_graph(&graph_id).await {
                        // Create a mutable copy for updates
                        let mut updated_graph = graph.clone();

                        // Update nodes based on current workflow state
                        for node in &mut updated_graph.nodes {
                            if node.node_type == "workflow" {
                                node.status = state.status.clone();
                                if let Some(error) = &state.error {
                                    node.properties.insert(
                                        "error".to_string(),
                                        serde_json::Value::String(format!("{:?}", error)),
                                    );
                                }
                            } else if node.node_type == "metadata" {
                                if let Some(value) = state.metadata.get(&node.name) {
                                    node.properties.insert("value".to_string(), value.clone());
                                }
                            }
                        }

                        // Update the graph in the manager
                        if let Err(e) = graph_manager_clone
                            .update_graph(&graph_id, updated_graph)
                            .await
                        {
                            error!("Failed to update workflow graph: {}", e);
                        }
                    }
                }
            }
        });

        Ok(())
    }
}

impl Clone for WorkflowGraphProvider {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            workflow_engine: self.workflow_engine.clone(),
            graph_manager: self.graph_manager.clone(),
        }
    }
}

impl GraphManager {
    /// Updates a graph with the provided ID with the given graph data
    pub async fn update_graph(&self, graph_id: &str, graph: Graph) -> Result<()> {
        let mut graphs = self.graphs.write().await;
        graphs.insert(graph_id.to_string(), graph);
        Ok(())
    }
}
