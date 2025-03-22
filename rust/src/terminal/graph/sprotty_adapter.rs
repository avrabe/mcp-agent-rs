//! Adapter for Sprotty frontend integration
//!
//! This module provides the necessary adapters and utilities to integrate
//! the graph visualization models with the Eclipse Sprotty frontend library.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info};

use super::models::{
    SprottyEdge, SprottyElement, SprottyGraph, SprottyGraphLayout, SprottyNode, SprottyRoot, SprottyStatus,
};
use super::workflow::WorkflowGraphProvider;
use crate::error::Error;

/// Sprotty action types for frontend communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "arguments")]
pub enum SprottyAction {
    /// Request to load a model
    #[serde(rename = "loadModel")]
    LoadModel(LoadModelRequest),

    /// Response with a loaded model
    #[serde(rename = "setModel")]
    SetModel(SprottyRoot),

    /// Request to update a model element
    #[serde(rename = "updateElement")]
    UpdateElement(SprottyElement),

    /// Request to remove a model element
    #[serde(rename = "removeElement")]
    RemoveElement(RemoveElementRequest),

    /// Request to center a model
    #[serde(rename = "centerElements")]
    CenterElements(CenterElementsRequest),

    /// Request to fit model to screen
    #[serde(rename = "fitToScreen")]
    FitToScreen(FitToScreenRequest),

    /// Request to layout model
    #[serde(rename = "layout")]
    Layout(LayoutRequest),

    /// Action to select an element
    #[serde(rename = "selectElement")]
    SelectElement(SelectElementRequest),
    
    /// Error response
    #[serde(rename = "error")]
    Error(ErrorResponse),
}

/// Error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// Error message
    pub message: String,
}

/// Request to load a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadModelRequest {
    /// ID of the model to load
    #[serde(rename = "modelId")]
    pub model_id: String,
    
    /// Options for loading the model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<LoadModelOptions>,
}

/// Options for loading a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadModelOptions {
    /// Layout algorithm to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    
    /// Whether to fit the model to the screen
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "fitToScreen")]
    pub fit_to_screen: Option<bool>,
}

/// Request to remove an element
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveElementRequest {
    /// ID of the element to remove
    #[serde(rename = "elementId")]
    pub element_id: String,
}

/// Request to center specific elements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CenterElementsRequest {
    /// IDs of the elements to center
    #[serde(rename = "elementIds")]
    pub element_ids: Vec<String>,
    
    /// Whether to fit elements to viewport
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "fitToScreen")]
    pub fit_to_screen: Option<bool>,
}

/// Request to fit model to screen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitToScreenRequest {
    /// Padding around the model (in pixels)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub padding: Option<f64>,
    
    /// Max zoom level to use
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "maxZoom")]
    pub max_zoom: Option<f64>,
}

/// Request to layout a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutRequest {
    /// Layout algorithm to use
    #[serde(skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<String>,
    
    /// Layout direction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
}

/// Request to select an element
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectElementRequest {
    /// ID of the element to select
    #[serde(rename = "elementId")]
    pub element_id: String,
    
    /// Whether to select multiple elements
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "selectMultiple")]
    pub select_multiple: Option<bool>,
}

/// Convert a Sprotty model update to a Sprotty action
pub fn convert_update_to_action(update: &super::models::SprottyModelUpdate) -> SprottyAction {
    match update {
        super::models::SprottyModelUpdate::SetModel(model) => SprottyAction::SetModel(model.clone()),
        super::models::SprottyModelUpdate::UpdateElement(element) => {
            SprottyAction::UpdateElement(element.clone())
        }
        super::models::SprottyModelUpdate::RemoveElement(element_id) => {
            SprottyAction::RemoveElement(RemoveElementRequest {
                element_id: element_id.clone(),
            })
        }
    }
}

/// Process a Sprotty action from the frontend
pub async fn process_sprotty_action(
    action: SprottyAction,
    graph_manager: Arc<super::GraphManager>,
) -> Result<Option<SprottyAction>, Error> {
    match &action {
        SprottyAction::LoadModel(request) => {
            // Load the requested model
            debug!("Loading model: {}", request.model_id);
            
            // Extract model ID - this is the graph ID
            let graph_id = &request.model_id;
            
            // Check if this is a workflow graph
            if graph_id.starts_with("workflow-") {
                // Get the workflow provider and let it handle this action
                let workflow_provider = Arc::new(WorkflowGraphProvider::new(
                    Arc::new(crate::terminal::workflow::WorkflowEngine::new())
                ));
                
                if let Some(response) = workflow_provider.handle_sprotty_action(&action, graph_id, graph_manager.clone()).await? {
                    return Ok(Some(response));
                }
            }
            
            // Standard handling
            if let Some(graph) = graph_manager.get_graph(graph_id).await {
                // Convert the graph to a Sprotty model
                let sprotty_model = super::models::convert_to_sprotty_model(&graph);
                
                // Apply any specified options
                if let Some(options) = &request.options {
                    // Handle layout option
                    if let Some(layout) = &options.layout {
                        debug!("Applying layout: {}", layout);
                        // Here we would apply the layout algorithm, but for now we just pass the model
                    }
                    
                    // Handle fit to screen option
                    if let Some(fit) = options.fit_to_screen {
                        if fit {
                            debug!("Fitting model to screen");
                            // This will be handled by the frontend, just log
                        }
                    }
                }
                
                Ok(Some(SprottyAction::SetModel(sprotty_model)))
            } else {
                error!("Graph not found: {}", graph_id);
                Ok(Some(SprottyAction::Error(ErrorResponse {
                    message: format!("Graph not found: {}", graph_id),
                })))
            }
        }
        SprottyAction::SelectElement(request) => {
            // Handle element selection - in a real implementation, this might update
            // some UI state or trigger other actions
            debug!("Selected element: {}", request.element_id);
            
            // Extract the graph ID from the element ID
            // Element IDs should be in the format "graph-id:element-id"
            let parts: Vec<&str> = request.element_id.split(':').collect();
            if parts.len() >= 2 {
                let graph_id = parts[0];
                
                // Check if this is a workflow graph
                if graph_id.starts_with("workflow-") {
                    // Get the workflow provider and let it handle this action
                    let workflow_provider = Arc::new(WorkflowGraphProvider::new(
                        Arc::new(crate::terminal::workflow::WorkflowEngine::new())
                    ));
                    
                    if let Some(response) = workflow_provider.handle_sprotty_action(&action, graph_id, graph_manager.clone()).await? {
                        return Ok(Some(response));
                    }
                }
            }
            
            Ok(None)
        }
        SprottyAction::CenterElements(request) => {
            // This operation requires frontend action, so we pass it through
            debug!("Centering elements: {:?}", request.element_ids);
            
            // Check if we can determine the graph ID from any element ID
            if let Some(element_id) = request.element_ids.first() {
                let parts: Vec<&str> = element_id.split(':').collect();
                if parts.len() >= 2 {
                    let graph_id = parts[0];
                    
                    // Check if this is a workflow graph
                    if graph_id.starts_with("workflow-") {
                        // Get the workflow provider and let it handle this action
                        let workflow_provider = Arc::new(WorkflowGraphProvider::new(
                            Arc::new(crate::terminal::workflow::WorkflowEngine::new())
                        ));
                        
                        if let Some(response) = workflow_provider.handle_sprotty_action(&action, graph_id, graph_manager.clone()).await? {
                            return Ok(Some(response));
                        }
                    }
                }
            }
            
            Ok(Some(action))
        }
        SprottyAction::FitToScreen(_) => {
            // Pass this action back to the frontend
            debug!("Fitting to screen");
            Ok(Some(action))
        }
        SprottyAction::Layout(request) => {
            // This operation needs both backend and frontend coordination
            debug!("Applying layout: {:?}", request.algorithm);
            
            // Check if we're given a model ID
            if let Some(metadata) = &request.algorithm {
                // Check if the metadata contains a graph ID
                if metadata.contains(":") {
                    let parts: Vec<&str> = metadata.split(':').collect();
                    let graph_id = parts[0];
                    
                    // Check if this is a workflow graph
                    if graph_id.starts_with("workflow-") {
                        // Get the workflow provider and let it handle this action
                        let workflow_provider = Arc::new(WorkflowGraphProvider::new(
                            Arc::new(crate::terminal::workflow::WorkflowEngine::new())
                        ));
                        
                        if let Some(response) = workflow_provider.handle_sprotty_action(&action, graph_id, graph_manager.clone()).await? {
                            return Ok(Some(response));
                        }
                    }
                }
            }
            
            Ok(Some(action))
        }
        _ => {
            // For other actions, we just ignore them for now
            debug!("Ignoring action: {:?}", action);
            Ok(None)
        }
    }
} 