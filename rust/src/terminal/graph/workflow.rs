//! Workflow graph visualization module.
//!
//! This module provides functionality for visualizing workflows in the terminal system.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

use crate::error::{Error, Result};
use crate::mcp::types::Message;
use crate::workflow::engine::WorkflowEngine;

use super::models::{convert_to_sprotty_model, SprottyStatus};
use super::sprotty_adapter::{process_sprotty_action, SprottyAction};
use super::{
    Graph, GraphDataProvider, GraphEdge, GraphManager, GraphNode, GraphUpdate, GraphUpdateType,
};

/// A graph data provider for workflow visualization
#[derive(Debug)]
pub struct WorkflowGraphProvider {
    /// Provider name
    name: String,
    /// Workflow engine reference
    workflow_engine: Arc<WorkflowEngine>,
    /// Cache of workflow states
    workflow_states: Arc<RwLock<HashMap<String, WorkflowState>>>,
}

/// Workflow state tracked for visualization
#[derive(Debug, Clone)]
struct WorkflowState {
    /// Workflow ID
    id: String,
    /// Workflow name
    name: String,
    /// Workflow nodes (task nodes)
    nodes: HashMap<String, WorkflowNodeState>,
    /// Workflow edges (dependencies)
    edges: HashMap<String, WorkflowEdgeState>,
    /// Layout information
    layout: Option<WorkflowLayout>,
}

/// Workflow layout information
#[derive(Debug, Clone)]
struct WorkflowLayout {
    /// Layout algorithm
    algorithm: String,
    /// Layout direction
    direction: String,
    /// Node positions
    node_positions: HashMap<String, (f64, f64)>,
}

/// Workflow node state
#[derive(Debug, Clone)]
struct WorkflowNodeState {
    /// Node ID
    id: String,
    /// Node name
    name: String,
    /// Node type (task type)
    node_type: String,
    /// Node status
    status: String,
    /// Node properties
    properties: HashMap<String, serde_json::Value>,
}

/// Workflow edge state
#[derive(Debug, Clone)]
struct WorkflowEdgeState {
    /// Edge ID
    id: String,
    /// Source node ID
    source: String,
    /// Target node ID
    target: String,
    /// Edge type
    edge_type: String,
    /// Edge properties
    properties: HashMap<String, serde_json::Value>,
}

impl WorkflowGraphProvider {
    /// Create a new workflow graph provider
    pub fn new(workflow_engine: Arc<WorkflowEngine>) -> Self {
        Self {
            name: "workflow-provider".to_string(),
            workflow_engine,
            workflow_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize the provider and start tracking workflow updates
    pub async fn initialize(&self, graph_manager: Arc<GraphManager>) -> Result<()> {
        info!("Initializing workflow graph provider: {}", self.name);

        // Register ourselves with the manager
        graph_manager.register_provider(Arc::new(self.clone()));

        // Subscribe to workflow events
        let engine = self.workflow_engine.clone();
        let states = self.workflow_states.clone();
        let manager = graph_manager.clone();
        let provider_name = self.name.clone();

        tokio::spawn(async move {
            let mut workflow_events = engine.subscribe_events().await;

            while let Ok(event) = workflow_events.recv().await {
                match event {
                    Message::WorkflowCreated(wf) => {
                        debug!("Workflow created: {}", wf.id);

                        // Initialize workflow state
                        let mut states_write = states.write().await;
                        let state = WorkflowState {
                            id: wf.id.clone(),
                            name: wf.name.clone(),
                            nodes: HashMap::new(),
                            edges: HashMap::new(),
                            layout: Some(WorkflowLayout {
                                algorithm: "layered".to_string(),
                                direction: "DOWN".to_string(),
                                node_positions: HashMap::new(),
                            }),
                        };
                        states_write.insert(wf.id.clone(), state);

                        // Create initial graph
                        let graph_id = format!("workflow-{}", wf.id);
                        let graph = create_workflow_graph(
                            &graph_id,
                            &wf.id,
                            &wf.name,
                            HashMap::new(),
                            vec![],
                        );

                        if let Err(e) = manager.register_graph(graph).await {
                            error!("Failed to register workflow graph: {}", e);
                        }
                    }
                    Message::WorkflowTaskAdded(task) => {
                        if let Some(workflow_id) = &task.workflow_id {
                            debug!("Task added to workflow {}: {}", workflow_id, task.id);

                            let mut states_write = states.write().await;
                            if let Some(state) = states_write.get_mut(workflow_id) {
                                // Create node properties from task fields
                                let mut properties = HashMap::new();
                                properties.insert(
                                    "task_type".to_string(),
                                    serde_json::json!(task.task_type.clone()),
                                );
                                if let Some(desc) = &task.description {
                                    properties.insert(
                                        "description".to_string(),
                                        serde_json::json!(desc.clone()),
                                    );
                                }

                                // Add node to state
                                let node_state = WorkflowNodeState {
                                    id: task.id.clone(),
                                    name: task.name.clone(),
                                    node_type: task.task_type.clone(),
                                    status: "pending".to_string(),
                                    properties,
                                };
                                state.nodes.insert(task.id.clone(), node_state.clone());

                                // Update graph
                                let graph_id = format!("workflow-{}", workflow_id);
                                let node = GraphNode {
                                    id: task.id.clone(),
                                    name: task.name.clone(),
                                    node_type: task.task_type.clone(),
                                    status: "pending".to_string(),
                                    properties: node_state.properties.clone(),
                                };

                                let update = GraphUpdate {
                                    graph_id: graph_id.clone(),
                                    update_type: GraphUpdateType::NodeAdded,
                                    graph: None,
                                    node: Some(node),
                                    edge: None,
                                };

                                if let Err(e) = manager.notify_update(update).await {
                                    error!("Failed to add node to workflow graph: {}", e);
                                }

                                // Add edges for dependencies
                                if let Some(deps) = &task.dependencies {
                                    for dep_id in deps {
                                        let edge_id = format!("{}-{}", dep_id, task.id);
                                        let edge_state = WorkflowEdgeState {
                                            id: edge_id.clone(),
                                            source: dep_id.clone(),
                                            target: task.id.clone(),
                                            edge_type: "dependency".to_string(),
                                            properties: HashMap::new(),
                                        };
                                        state.edges.insert(edge_id.clone(), edge_state);

                                        let edge = GraphEdge {
                                            id: edge_id,
                                            source: dep_id.clone(),
                                            target: task.id.clone(),
                                            edge_type: "dependency".to_string(),
                                            properties: HashMap::new(),
                                        };

                                        let update = GraphUpdate {
                                            graph_id: graph_id.clone(),
                                            update_type: GraphUpdateType::EdgeAdded,
                                            graph: None,
                                            node: None,
                                            edge: Some(edge),
                                        };

                                        if let Err(e) = manager.notify_update(update).await {
                                            error!("Failed to add edge to workflow graph: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Message::WorkflowTaskStateChanged(task) => {
                        if let Some(workflow_id) = &task.workflow_id {
                            if let Some(status) = &task.status {
                                debug!(
                                    "Task status changed in workflow {}: {} -> {}",
                                    workflow_id, task.id, status
                                );

                                let mut states_write = states.write().await;
                                if let Some(state) = states_write.get_mut(workflow_id) {
                                    if let Some(node) = state.nodes.get_mut(&task.id) {
                                        node.status = status.clone();

                                        // Update graph node
                                        let graph_id = format!("workflow-{}", workflow_id);
                                        let updated_node = GraphNode {
                                            id: task.id.clone(),
                                            name: node.name.clone(),
                                            node_type: node.node_type.clone(),
                                            status: status.clone(),
                                            properties: node.properties.clone(),
                                        };

                                        let update = GraphUpdate {
                                            graph_id,
                                            update_type: GraphUpdateType::NodeUpdated,
                                            graph: None,
                                            node: Some(updated_node),
                                            edge: None,
                                        };

                                        if let Err(e) = manager.notify_update(update).await {
                                            error!(
                                                "Failed to update node in workflow graph: {}",
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Message::WorkflowDeleted(wf_id) => {
                        debug!("Workflow deleted: {}", wf_id);

                        // Remove from our state
                        let mut states_write = states.write().await;
                        states_write.remove(&wf_id);

                        // Unregister the graph
                        let graph_id = format!("workflow-{}", wf_id);
                        if let Err(e) = manager.unregister_graph(&graph_id).await {
                            error!("Failed to unregister workflow graph: {}", e);
                        }
                    }
                    _ => {}
                }
            }
        });

        info!("Workflow graph provider initialized: {}", self.name);
        Ok(())
    }

    /// Handle Sprotty actions specific to workflow graphs
    pub async fn handle_sprotty_action(
        &self,
        action: &SprottyAction,
        graph_id: &str,
        graph_manager: Arc<GraphManager>,
    ) -> Result<Option<SprottyAction>> {
        // Only handle actions for workflow graphs
        if !graph_id.starts_with("workflow-") {
            return Ok(None);
        }

        match action {
            SprottyAction::Layout(request) => {
                debug!("Handling layout request for workflow graph: {}", graph_id);

                // Extract workflow ID from graph ID
                let workflow_id = graph_id.strip_prefix("workflow-").unwrap_or(graph_id);

                // Update the layout in our state
                let mut states = self.workflow_states.write().await;
                if let Some(state) = states.get_mut(workflow_id) {
                    if let Some(layout) = &mut state.layout {
                        if let Some(algorithm) = &request.algorithm {
                            layout.algorithm = algorithm.clone();
                        }
                        if let Some(direction) = &request.direction {
                            layout.direction = direction.clone();
                        }
                    } else {
                        state.layout = Some(WorkflowLayout {
                            algorithm: request
                                .algorithm
                                .clone()
                                .unwrap_or_else(|| "layered".to_string()),
                            direction: request
                                .direction
                                .clone()
                                .unwrap_or_else(|| "DOWN".to_string()),
                            node_positions: HashMap::new(),
                        });
                    }

                    // Pass the layout request back to the frontend
                    return Ok(Some(action.clone()));
                }
            }
            SprottyAction::CenterElements(request) => {
                // Pass center elements request back to the frontend
                return Ok(Some(action.clone()));
            }
            SprottyAction::FitToScreen(request) => {
                // Pass fit to screen request back to the frontend
                return Ok(Some(action.clone()));
            }
            _ => {
                // Default handling by process_sprotty_action
            }
        }

        Ok(None)
    }
}

/// Create a workflow graph from workflow state
fn create_workflow_graph(
    graph_id: &str,
    workflow_id: &str,
    workflow_name: &str,
    nodes: HashMap<String, WorkflowNodeState>,
    edges: Vec<WorkflowEdgeState>,
) -> Graph {
    let graph_nodes = nodes
        .values()
        .map(|n| GraphNode {
            id: n.id.clone(),
            name: n.name.clone(),
            node_type: n.node_type.clone(),
            status: n.status.clone(),
            properties: n.properties.clone(),
        })
        .collect();

    let graph_edges = edges
        .iter()
        .map(|e| GraphEdge {
            id: e.id.clone(),
            source: e.source.clone(),
            target: e.target.clone(),
            edge_type: e.edge_type.clone(),
            properties: e.properties.clone(),
        })
        .collect();

    let mut properties = HashMap::new();
    properties.insert(
        "workflow_id".to_string(),
        serde_json::json!(workflow_id.to_string()),
    );
    properties.insert(
        "layout",
        serde_json::json!({
            "algorithm": "layered",
            "direction": "DOWN"
        }),
    );

    Graph {
        id: graph_id.to_string(),
        name: format!("Workflow: {}", workflow_name),
        graph_type: "workflow".to_string(),
        nodes: graph_nodes,
        edges: graph_edges,
        properties,
    }
}

impl Clone for WorkflowGraphProvider {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            workflow_engine: self.workflow_engine.clone(),
            workflow_states: self.workflow_states.clone(),
        }
    }
}

#[async_trait]
impl GraphDataProvider for WorkflowGraphProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn graph_type(&self) -> &str {
        "workflow"
    }

    async fn generate_graph(&self) -> Result<Graph, Error> {
        // Create a basic graph with current workflow state
        let states = self.workflow_states.read().await;

        if states.is_empty() {
            // Create an empty workflow graph
            return Ok(Graph {
                id: "workflow-empty".to_string(),
                name: "Empty Workflow".to_string(),
                graph_type: "workflow".to_string(),
                nodes: Vec::new(),
                edges: Vec::new(),
                properties: HashMap::new(),
            });
        }

        // For simplicity, just use the first workflow
        let (workflow_id, state) = states.iter().next().unwrap();
        let graph_id = format!("workflow-{}", workflow_id);

        // Convert nodes to a vec of GraphEdgeState
        let edges_vec = state.edges.values().cloned().collect();

        // Create the graph
        Ok(create_workflow_graph(
            &graph_id,
            workflow_id,
            &state.name,
            state.nodes.clone(),
            edges_vec,
        ))
    }

    async fn setup_tracking(&self, graph_manager: Arc<GraphManager>) -> Result<(), Error> {
        info!("Setting up workflow graph tracking");

        // Subscribe to workflow events and update the graph accordingly
        let provider = self.clone();
        let manager = graph_manager.clone();

        tokio::spawn(async move {
            // This would set up a subscription to workflow engine events
            // For now, we'll just create a simple demo workflow
            let mut graph = Graph::new("workflow-demo", "Demo Workflow");
            graph.graph_type = "workflow".to_string();

            // Add some nodes
            let node1 = GraphNode {
                id: "task1".to_string(),
                name: "Start Task".to_string(),
                node_type: "task".to_string(),
                status: "completed".to_string(),
                properties: HashMap::new(),
            };

            let node2 = GraphNode {
                id: "task2".to_string(),
                name: "Process Data".to_string(),
                node_type: "task".to_string(),
                status: "running".to_string(),
                properties: HashMap::new(),
            };

            graph.nodes.push(node1);
            graph.nodes.push(node2);

            // Add an edge
            let edge = GraphEdge {
                id: "edge1".to_string(),
                source: "task1".to_string(),
                target: "task2".to_string(),
                edge_type: "dependency".to_string(),
                properties: HashMap::new(),
            };

            graph.edges.push(edge);

            // Register the graph
            if let Err(e) = manager.register_graph(graph).await {
                error!("Failed to register workflow graph: {}", e);
            }
        });

        Ok(())
    }
}
