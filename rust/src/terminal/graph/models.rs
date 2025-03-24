//! Sprotty-compatible models for graph visualization.
//!
//! These models are designed to be compatible with the Eclipse Sprotty
//! library for interactive graph visualization in the web client.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{Graph, GraphEdge, GraphNode};

/// Root element for a Sprotty model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprottyRoot {
    /// Unique identifier for the model
    pub id: String,

    /// Type of the model ("graph")
    #[serde(rename = "type")]
    pub model_type: String,

    /// Children elements (nodes and edges)
    pub children: Vec<SprottyElement>,

    /// Layout options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<SprottyGraphLayout>,
}

/// Sprotty element - can be a node or edge
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SprottyElement {
    /// Node elemen
    #[serde(rename = "node")]
    Node(SprottyNode),

    /// Edge elemen
    #[serde(rename = "edge")]
    Edge(SprottyEdge),

    /// Label elemen
    #[serde(rename = "label")]
    Label(SprottyLabel),
}

/// Node element in a Sprotty model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprottyNode {
    /// Unique identifier
    pub id: String,

    /// Children elements (usually empty for nodes)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SprottyElement>,

    /// Node layou
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,

    /// Node position
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<SprottyPosition>,

    /// Node size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<SprottySize>,

    /// CSS classes for styling
    #[serde(rename = "cssClasses")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub css_classes: Option<Vec<String>>,

    /// Node label
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// Node status
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<SprottyStatus>,

    /// Additional node properties
    #[serde(flatten)]
    pub properties: HashMap<String, serde_json::Value>,
}

/// Edge element in a Sprotty model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprottyEdge {
    /// Unique identifier
    pub id: String,

    /// Source node ID
    pub source: String,

    /// Target node ID
    pub target: String,

    /// Source port ID (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_port: Option<String>,

    /// Target port ID (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_port: Option<String>,

    /// Edge routing points
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub routing_points: Vec<SprottyPosition>,

    /// CSS classes for styling
    #[serde(rename = "cssClasses")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub css_classes: Option<Vec<String>>,

    /// Edge label
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// Additional edge properties
    #[serde(flatten)]
    pub properties: HashMap<String, serde_json::Value>,
}

/// Label element in a Sprotty model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprottyLabel {
    /// Unique identifier
    pub id: String,

    /// Label tex
    pub text: String,

    /// CSS classes for styling
    #[serde(rename = "cssClasses")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub css_classes: Option<Vec<String>>,

    /// Label position
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<SprottyPosition>,

    /// Label size
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<SprottySize>,

    /// Additional label properties
    #[serde(flatten)]
    pub properties: HashMap<String, serde_json::Value>,
}

/// Position in Sprotty model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprottyPosition {
    /// X coordinate
    pub x: f64,

    /// Y coordinate
    pub y: f64,
}

/// Size in Sprotty model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprottySize {
    /// Width
    pub width: f64,

    /// Heigh
    pub height: f64,
}

/// Graph layout options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprottyGraphLayout {
    /// Layout algorithm
    pub algorithm: String,

    /// Layout direction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,

    /// Spacing between elements
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spacing: Option<f64>,
}

/// Status enum for node elements
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SprottyStatus {
    /// Node is idle (not started)
    Idle,
    /// Node is waiting (for dependencies)
    Waiting,
    /// Node is currently running
    Running,
    /// Node has completed successfully
    Completed,
    /// Node has failed
    Failed,
}

impl ToString for SprottyStatus {
    fn to_string(&self) -> String {
        match self {
            SprottyStatus::Idle => "idle".to_string(),
            SprottyStatus::Waiting => "waiting".to_string(),
            SprottyStatus::Running => "running".to_string(),
            SprottyStatus::Completed => "completed".to_string(),
            SprottyStatus::Failed => "failed".to_string(),
        }
    }
}

impl From<String> for SprottyStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "idle" => Self::Idle,
            "waiting" => Self::Waiting,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Idle, // Default to idle for unknown status
        }
    }
}

impl From<&str> for SprottyStatus {
    fn from(s: &str) -> Self {
        match s {
            "idle" => Self::Idle,
            "waiting" => Self::Waiting,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Idle, // Default to idle for unknown status
        }
    }
}

/// A wrapper for working with Sprotty graph models
#[derive(Debug, Clone, Serialize)]
pub struct SprottyGraph {
    /// Root element of the Sprotty model
    pub root: SprottyRoot,
    /// A map of node IDs to indices in the children array
    node_indices: HashMap<String, usize>,
    /// A map of edge IDs to indices in the children array
    edge_indices: HashMap<String, usize>,
}

impl SprottyGraph {
    /// Create a new Sprotty graph with the given ID and title
    pub fn new(id: String, title: String) -> Self {
        let root = SprottyRoot {
            id,
            model_type: "graph".to_string(),
            children: Vec::new(),
            layout: Some(SprottyGraphLayout {
                algorithm: "elklay".to_string(),
                direction: Some("DOWN".to_string()),
                spacing: Some(40.0),
            }),
        };

        Self {
            root,
            node_indices: HashMap::new(),
            edge_indices: HashMap::new(),
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, node: SprottyNode) {
        let id = node.id.clone();
        let element = SprottyElement::Node(node);
        self.root.children.push(element);
        self.node_indices.insert(id, self.root.children.len() - 1);
    }

    /// Add an edge to the graph
    pub fn add_edge(&mut self, edge: SprottyEdge) {
        let id = edge.id.clone();
        let element = SprottyElement::Edge(edge);
        self.root.children.push(element);
        self.edge_indices.insert(id, self.root.children.len() - 1);
    }

    /// Get a node by ID (as mutable)
    pub fn get_node_mut(&mut self, node_id: &str) -> Option<&mut SprottyNode> {
        if let Some(&index) = self.node_indices.get(node_id) {
            if let Some(SprottyElement::Node(node)) = self.root.children.get_mut(index) {
                return Some(node);
            }
        }
        None
    }

    /// Get an edge by ID (as mutable)
    pub fn get_edge_mut(&mut self, edge_id: &str) -> Option<&mut SprottyEdge> {
        if let Some(&index) = self.edge_indices.get(edge_id) {
            if let Some(SprottyElement::Edge(edge)) = self.root.children.get_mut(index) {
                return Some(edge);
            }
        }
        None
    }

    /// Get a node by ID
    pub fn get_node(&self, node_id: &str) -> Option<&SprottyNode> {
        if let Some(&index) = self.node_indices.get(node_id) {
            if let Some(SprottyElement::Node(node)) = self.root.children.get(index) {
                return Some(node);
            }
        }
        None
    }

    /// Get an edge by ID
    pub fn get_edge(&self, edge_id: &str) -> Option<&SprottyEdge> {
        if let Some(&index) = self.edge_indices.get(edge_id) {
            if let Some(SprottyElement::Edge(edge)) = self.root.children.get(index) {
                return Some(edge);
            }
        }
        None
    }

    /// Get all nodes
    pub fn nodes(&self) -> Vec<&SprottyNode> {
        self.root
            .children
            .iter()
            .filter_map(|child| {
                if let SprottyElement::Node(node) = child {
                    Some(node)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all edges
    pub fn edges(&self) -> Vec<&SprottyEdge> {
        self.root
            .children
            .iter()
            .filter_map(|child| {
                if let SprottyElement::Edge(edge) = child {
                    Some(edge)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all nodes in the graph
    pub fn get_all_nodes(&self) -> Vec<&SprottyNode> {
        let mut nodes = Vec::new();

        for child in &self.root.children {
            if let SprottyElement::Node(node) = child {
                nodes.push(node);
            }
        }

        nodes
    }

    /// Get all edges in the graph
    pub fn get_all_edges(&self) -> Vec<&SprottyEdge> {
        let mut edges = Vec::new();

        for child in &self.root.children {
            if let SprottyElement::Edge(edge) = child {
                edges.push(edge);
            }
        }

        edges
    }
}

impl Default for SprottyGraph {
    fn default() -> Self {
        Self::new("default".to_string(), "Default Graph".to_string())
    }
}

/// Extension trait for SprottyNode to create task node variants
pub trait SprottyNodeExt {
    /// Convert to TaskNode if this is a TaskNode varian
    fn as_task_node(&self) -> Option<&TaskNode>;
    /// Convert to TaskNode (mutable) if this is a TaskNode varian
    fn as_task_node_mut(&mut self) -> Option<&mut TaskNode>;
}

/// Task node for workflows
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    /// Unique identifier
    pub id: String,
    /// Task label
    pub label: String,
    /// Status of the task
    pub status: Option<SprottyStatus>,
    /// Position
    pub position: SprottyPosition,
    /// Additional metadata
    pub metadata: HashMap<String, serde_json::Value>,
    /// Child elements
    pub children: Vec<SprottyElement>,
}

impl SprottyNodeExt for SprottyNode {
    fn as_task_node(&self) -> Option<&TaskNode> {
        match self.properties.get("type") {
            Some(serde_json::Value::String(t)) if t == "task" => {
                // Reinterpret as TaskNode (this is a bit of a hack but works for our simple case)
                unsafe {
                    let ptr = self as *const SprottyNode as *const TaskNode;
                    Some(&*ptr)
                }
            }
            _ => None,
        }
    }

    fn as_task_node_mut(&mut self) -> Option<&mut TaskNode> {
        match self.properties.get("type") {
            Some(serde_json::Value::String(t)) if t == "task" => {
                // Reinterpret as TaskNode (this is a bit of a hack but works for our simple case)
                unsafe {
                    let ptr = self as *mut SprottyNode as *mut TaskNode;
                    Some(&mut *ptr)
                }
            }
            _ => None,
        }
    }
}

/// Convert an MCP Graph to a Sprotty model
pub fn convert_to_sprotty_model(graph: &Graph) -> SprottyRoot {
    let mut root = SprottyRoot {
        id: graph.id.clone(),
        model_type: "graph".to_string(),
        children: Vec::new(),
        layout: Some(SprottyGraphLayout {
            algorithm: "elklay".to_string(),
            direction: Some("DOWN".to_string()),
            spacing: Some(40.0),
        }),
    };

    // Add nodes
    for node in &graph.nodes {
        let node_element = SprottyElement::Node(SprottyNode {
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
            properties: node.properties.clone(),
        });

        root.children.push(node_element);
    }

    // Add edges
    for edge in &graph.edges {
        let edge_element = SprottyElement::Edge(SprottyEdge {
            id: edge.id.clone(),
            source: edge.source.clone(),
            target: edge.target.clone(),
            source_port: None,
            target_port: None,
            routing_points: Vec::new(),
            css_classes: Some(vec![format!("type-{}", edge.edge_type)]),
            label: None,
            properties: edge.properties.clone(),
        });

        root.children.push(edge_element);
    }

    root
}

/// Convert a graph update to a Sprotty model update
pub fn convert_update_to_sprotty(update: &super::GraphUpdate) -> Option<SprottyModelUpdate> {
    match update.update_type {
        super::GraphUpdateType::FullUpdate => {
            if let Some(graph) = &update.graph {
                let model = convert_to_sprotty_model(graph);
                Some(SprottyModelUpdate::SetModel(model))
            } else {
                None
            }
        }
        super::GraphUpdateType::NodeAdded | super::GraphUpdateType::NodeUpdated => {
            if let Some(node) = &update.node {
                let sprotty_node = convert_node_to_sprotty(node);
                Some(SprottyModelUpdate::UpdateElement(SprottyElement::Node(
                    sprotty_node,
                )))
            } else {
                None
            }
        }
        super::GraphUpdateType::EdgeAdded | super::GraphUpdateType::EdgeUpdated => {
            if let Some(edge) = &update.edge {
                let sprotty_edge = convert_edge_to_sprotty(edge);
                Some(SprottyModelUpdate::UpdateElement(SprottyElement::Edge(
                    sprotty_edge,
                )))
            } else {
                None
            }
        }
        super::GraphUpdateType::NodeRemoved => update
            .node
            .as_ref()
            .map(|node| SprottyModelUpdate::RemoveElement(node.id.clone())),
        super::GraphUpdateType::EdgeRemoved => update
            .edge
            .as_ref()
            .map(|edge| SprottyModelUpdate::RemoveElement(edge.id.clone())),
    }
}

/// Convert a single node to a Sprotty node
fn convert_node_to_sprotty(node: &GraphNode) -> SprottyNode {
    // Create a node elemen
    let mut sprotty_node = SprottyNode {
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
        properties: node.properties.clone(),
    };

    // Add type and status labels with better positioning
    let type_label = SprottyElement::Label(SprottyLabel {
        id: format!("{}-type", node.id),
        text: format!("Type: {}", node.node_type),
        css_classes: Some(vec!["type-label".to_string()]),
        position: Some(SprottyPosition { x: 0.0, y: -10.0 }), // Position above the node
        size: None,
        properties: HashMap::new(),
    });

    let status_label = SprottyElement::Label(SprottyLabel {
        id: format!("{}-status", node.id),
        text: format!("Status: {}", node.status),
        css_classes: Some(vec!["status-label".to_string()]),
        position: Some(SprottyPosition { x: 0.0, y: 10.0 }), // Position below the node
        size: None,
        properties: HashMap::new(),
    });

    // Add status indicator based on node status
    let status_color = match node.status.as_str() {
        "idle" => "#2a2a4a",
        "active" => "#2a4a2a",
        "completed" => "#2a4a4a",
        "failed" => "#4a2a2a",
        _ => "#2a2a2a",
    };

    let status_indicator = SprottyElement::Label(SprottyLabel {
        id: format!("{}-indicator", node.id),
        text: "●".to_string(),
        css_classes: Some(vec!["status-indicator".to_string()]),
        position: Some(SprottyPosition { x: -55.0, y: 0.0 }), // Position on left side of node
        size: None,
        properties: HashMap::from([(
            "color".to_string(),
            serde_json::Value::String(status_color.to_string()),
        )]),
    });

    // Add badges for any special properties
    if let Some(importance) = node.properties.get("importance") {
        if let Some(importance_str) = importance.as_str() {
            let badge = SprottyElement::Label(SprottyLabel {
                id: format!("{}-importance", node.id),
                text: importance_str.to_string(),
                css_classes: Some(vec!["badge".to_string(), "importance-badge".to_string()]),
                position: Some(SprottyPosition { x: 45.0, y: -20.0 }), // Position on upper righ
                size: None,
                properties: HashMap::new(),
            });
            sprotty_node.children.push(badge);
        }
    }

    // Add progress indicator if available
    if let Some(progress) = node.properties.get("progress") {
        if let Some(progress_val) = progress.as_f64() {
            let progress_text = format!("{:.0}%", progress_val * 100.0);
            let progress_badge = SprottyElement::Label(SprottyLabel {
                id: format!("{}-progress", node.id),
                text: progress_text,
                css_classes: Some(vec!["badge".to_string(), "progress-badge".to_string()]),
                position: Some(SprottyPosition { x: 45.0, y: 20.0 }), // Position on lower righ
                size: None,
                properties: HashMap::new(),
            });
            sprotty_node.children.push(progress_badge);
        }
    }

    sprotty_node.children.push(type_label);
    sprotty_node.children.push(status_label);
    sprotty_node.children.push(status_indicator);

    sprotty_node
}

/// Convert a single edge to a Sprotty edge
fn convert_edge_to_sprotty(edge: &GraphEdge) -> SprottyEdge {
    SprottyEdge {
        id: edge.id.clone(),
        source: edge.source.clone(),
        target: edge.target.clone(),
        source_port: None,
        target_port: None,
        routing_points: Vec::new(),
        css_classes: Some(vec![format!("type-{}", edge.edge_type)]),
        label: None,
        properties: edge.properties.clone(),
    }
}

/// Sprotty model update
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum SprottyModelUpdate {
    /// Set the entire model
    #[serde(rename = "setModel")]
    SetModel(SprottyRoot),

    /// Update a single elemen
    #[serde(rename = "updateElement")]
    UpdateElement(SprottyElement),

    /// Remove an elemen
    #[serde(rename = "removeElement")]
    RemoveElement(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_to_sprotty_model() {
        // Create a simple graph
        let mut node_props = HashMap::new();
        node_props.insert("priority".to_string(), serde_json::to_value(1).unwrap());

        let node = GraphNode {
            id: "node1".to_string(),
            name: "Node 1".to_string(),
            node_type: "task".to_string(),
            status: "active".to_string(),
            properties: node_props,
        };

        let edge = GraphEdge {
            id: "edge1".to_string(),
            source: "node1".to_string(),
            target: "node2".to_string(),
            edge_type: "dependency".to_string(),
            properties: HashMap::new(),
        };

        let graph = Graph {
            id: "test-graph".to_string(),
            name: "Test Graph".to_string(),
            graph_type: "workflow".to_string(),
            nodes: vec![node],
            edges: vec![edge],
            properties: HashMap::new(),
        };

        // Convert to Sprotty model
        let sprotty_model = convert_to_sprotty_model(&graph);

        // Verify the resul
        assert_eq!(sprotty_model.id, "test-graph");
        assert_eq!(sprotty_model.model_type, "graph");
        assert_eq!(sprotty_model.children.len(), 2); // 1 node + 1 edge

        if let SprottyElement::Node(node) = &sprotty_model.children[0] {
            assert_eq!(node.id, "node1");
            assert_eq!(node.label, Some("Node 1".to_string()));
            assert!(node
                .css_classes
                .as_ref()
                .unwrap()
                .contains(&"type-task".to_string()));
            assert!(node
                .css_classes
                .as_ref()
                .unwrap()
                .contains(&"status-active".to_string()));
        } else {
            panic!("Expected node element");
        }

        if let SprottyElement::Edge(edge) = &sprotty_model.children[1] {
            assert_eq!(edge.id, "edge1");
            assert_eq!(edge.source, "node1");
            assert_eq!(edge.target, "node2");
            assert!(edge
                .css_classes
                .as_ref()
                .unwrap()
                .contains(&"type-dependency".to_string()));
        } else {
            panic!("Expected edge element");
        }
    }
}
