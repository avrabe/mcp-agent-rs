//! Sprotty-compatible models for graph visualization.
//!
//! These models are designed to be compatible with the Eclipse Sprotty
//! library for interactive graph visualization in the web client.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use super::{Graph, GraphNode, GraphEdge};

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
    /// Node element
    #[serde(rename = "node")]
    Node(SprottyNode),
    
    /// Edge element
    #[serde(rename = "edge")]
    Edge(SprottyEdge),
    
    /// Label element
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
    
    /// Node layout
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
    pub status: Option<String>,
    
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
    
    /// Label text
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
    
    /// Height
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
            css_classes: Some(vec![format!("type-{}", node.node_type), format!("status-{}", node.status)]),
            label: Some(node.name.clone()),
            status: Some(node.status.clone()),
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
pub fn convert_update_to_sprotty(
    update: &super::GraphUpdate,
) -> Option<SprottyModelUpdate> {
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
                Some(SprottyModelUpdate::UpdateElement(SprottyElement::Node(sprotty_node)))
            } else {
                None
            }
        }
        super::GraphUpdateType::EdgeAdded | super::GraphUpdateType::EdgeUpdated => {
            if let Some(edge) = &update.edge {
                let sprotty_edge = convert_edge_to_sprotty(edge);
                Some(SprottyModelUpdate::UpdateElement(SprottyElement::Edge(sprotty_edge)))
            } else {
                None
            }
        }
        super::GraphUpdateType::NodeRemoved => {
            if let Some(node) = &update.node {
                Some(SprottyModelUpdate::RemoveElement(node.id.clone()))
            } else {
                None
            }
        }
        super::GraphUpdateType::EdgeRemoved => {
            if let Some(edge) = &update.edge {
                Some(SprottyModelUpdate::RemoveElement(edge.id.clone()))
            } else {
                None
            }
        }
    }
}

/// Convert a single node to a Sprotty node
fn convert_node_to_sprotty(node: &GraphNode) -> SprottyNode {
    // Create a node element
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
        status: Some(node.status.clone()),
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
        properties: HashMap::from([
            ("color".to_string(), serde_json::Value::String(status_color.to_string())),
        ]),
    });
    
    // Add additional badges based on node properties
    if let Some(importance) = node.properties.get("importance") {
        if let Some(importance_val) = importance.as_f64() {
            let importance_text = match importance_val {
                i if i >= 0.8 => "H",
                i if i >= 0.5 => "M",
                _ => "L",
            };
            
            let importance_badge = SprottyElement::Label(SprottyLabel {
                id: format!("{}-importance", node.id),
                text: importance_text.to_string(),
                css_classes: Some(vec!["badge".to_string(), "importance-badge".to_string()]),
                position: Some(SprottyPosition { x: 45.0, y: -20.0 }), // Position on upper right
                size: None,
                properties: HashMap::new(),
            });
            sprotty_node.children.push(importance_badge);
        }
    }
    
    if let Some(progress) = node.properties.get("progress") {
        if let Some(progress_val) = progress.as_f64() {
            let progress_text = format!("{:.0}%", progress_val * 100.0);
            let progress_badge = SprottyElement::Label(SprottyLabel {
                id: format!("{}-progress", node.id),
                text: progress_text,
                css_classes: Some(vec!["badge".to_string(), "progress-badge".to_string()]),
                position: Some(SprottyPosition { x: 45.0, y: 20.0 }), // Position on lower right
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
        css_classes: Some(vec![
            format!("type-{}", edge.edge_type),
        ]),
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
    
    /// Update a single element
    #[serde(rename = "updateElement")]
    UpdateElement(SprottyElement),
    
    /// Remove an element
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
        
        // Verify the result
        assert_eq!(sprotty_model.id, "test-graph");
        assert_eq!(sprotty_model.model_type, "graph");
        assert_eq!(sprotty_model.children.len(), 2); // 1 node + 1 edge
    }
} 