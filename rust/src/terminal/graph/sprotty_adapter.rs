//! Adapter for Sprotty frontend integration
//!
//! This module provides the necessary adapters and utilities to integrate
//! the graph visualization models with the Eclipse Sprotty frontend library.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::debug;

use super::models::{convert_to_sprotty_model, SprottyElement, SprottyRoot};
use super::workflow::WorkflowGraphProvider;
use crate::error::Result;
use crate::terminal::graph::GraphManager;
use crate::workflow::engine::WorkflowEngine;

/// Sprotty action types for frontend communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SprottyAction {
    /// Request to load a model
    #[serde(rename = "loadModel")]
    LoadModel(LoadModelRequest),

    /// Request to select a node
    #[serde(rename = "selectNode")]
    SelectNode(String),

    /// Request to select an edge
    #[serde(rename = "selectEdge")]
    SelectEdge(String),

    /// Request to layout a model
    #[serde(rename = "layout")]
    Layout(LayoutRequest),

    /// Request to fit model to screen
    #[serde(rename = "fitToScreen")]
    FitToScreen(FitToScreenRequest),

    /// Request to center a model
    #[serde(rename = "center")]
    Center,

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
        super::models::SprottyModelUpdate::SetModel(model) => {
            SprottyAction::SetModel(model.clone())
        }
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
#[allow(clippy::too_many_lines)]
pub async fn process_sprotty_action(
    action: &SprottyAction,
    graph_id: &str,
    graph_manager: Arc<GraphManager>,
    workflow_engine: Arc<WorkflowEngine>,
) -> Result<Option<SprottyAction>> {
    match action {
        SprottyAction::LoadModel(request) => {
            // Load the requested model
            debug!("Loading model: {}", request.model_id);

            if graph_id.starts_with("workflow-") {
                // Get the workflow provider and let it handle this action
                let workflow_provider =
                    Arc::new(WorkflowGraphProvider::new(workflow_engine.clone()));

                // Convert Option<Error> to Result<GraphUpdate, Error>
                let update_result = workflow_provider
                    .handle_sprotty_action(action, graph_id, graph_manager.clone())
                    .await;

                if let Ok(update) = update_result {
                    if let Some(graph) = update.graph {
                        return Ok(Some(SprottyAction::SetModel(convert_to_sprotty_model(
                            &graph,
                        ))));
                    }
                }
            }

            // Try to load the graph directly from the manager
            if let Some(graph) = graph_manager.get_graph(graph_id).await {
                return Ok(Some(SprottyAction::SetModel(convert_to_sprotty_model(
                    &graph,
                ))));
            }

            Ok(Some(action.clone()))
        }
        SprottyAction::Layout(request) => {
            // This operation needs both backend and frontend coordination
            debug!("Applying layout: {:?}", request.algorithm);

            if let Some(metadata) = &request.algorithm {
                // Check if the metadata contains a graph ID
                debug!("Layout with algorithm: {}", metadata);

                if graph_id.starts_with("workflow-") {
                    // Get the workflow provider and let it handle this action
                    let workflow_provider =
                        Arc::new(WorkflowGraphProvider::new(workflow_engine.clone()));

                    let update_result = workflow_provider
                        .handle_sprotty_action(action, graph_id, graph_manager.clone())
                        .await;

                    if let Ok(update) = update_result {
                        if let Some(graph) = update.graph {
                            return Ok(Some(SprottyAction::SetModel(convert_to_sprotty_model(
                                &graph,
                            ))));
                        }
                    }
                }

                // Additional layout logic can be added here
            }

            Ok(Some(action.clone()))
        }
        SprottyAction::FitToScreen(request) => {
            // Pass this action back to the frontend
            debug!("Fitting to screen");

            if graph_id.starts_with("workflow-") {
                let workflow_provider =
                    Arc::new(WorkflowGraphProvider::new(workflow_engine.clone()));

                let update_result = workflow_provider
                    .handle_sprotty_action(action, graph_id, graph_manager.clone())
                    .await;

                if let Ok(update) = update_result {
                    if let Some(graph) = update.graph {
                        return Ok(Some(SprottyAction::SetModel(convert_to_sprotty_model(
                            &graph,
                        ))));
                    }
                }
            }

            Ok(Some(action.clone()))
        }
        _ => Ok(None),
    }
}

/// Adapter for handling Sprotty frontend actions and updating the graph
#[derive(Debug)]
pub struct SprottyAdapter {
    /// The workflow engine instance
    workflow_engine: Arc<WorkflowEngine>,
}

impl SprottyAdapter {
    /// Creates a new SprottyAdapter instance
    pub fn new(workflow_engine: Arc<WorkflowEngine>) -> Self {
        Self { workflow_engine }
    }

    /// Handles Sprotty action requests and returns appropriate responses
    pub async fn handle_action(
        &self,
        action: &SprottyAction,
        graph_id: &str,
        graph_manager: Arc<GraphManager>,
    ) -> Result<Option<SprottyAction>> {
        process_sprotty_action(
            action,
            graph_id,
            graph_manager,
            self.workflow_engine.clone(),
        )
        .await
    }

    /// Processes Sprotty actions and performs appropriate operations
    pub async fn handle_sprotty_action(
        &self,
        _action: &SprottyAction,
        _graph_id: &str,
        _graph_manager: Arc<GraphManager>,
    ) -> Option<SprottyAction> {
        // This implementation has been moved to workflow.rs
        None
    }
}
