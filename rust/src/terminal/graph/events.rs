//! Event handling for graph visualization system.
//!
//! This module provides event handlers for various graph visualization events,
//! including state changes, updates, and user interactions.

use crate::error::Result;
use crate::terminal::graph::{GraphManager, GraphNode, GraphUpdate, GraphUpdateType};
use std::sync::Arc;
use tracing::{debug, error, info};

#[derive(Debug)]
/// Event handler for LLM provider state changes
pub struct LlmProviderEventHandler {
    /// Graph manager for updating visualizations
    graph_manager: Arc<GraphManager>,
    /// Provider ID
    provider_id: String,
}

impl LlmProviderEventHandler {
    /// Create a new LLM provider event handler
    pub fn new(graph_manager: Arc<GraphManager>, provider_id: String) -> Self {
        Self {
            graph_manager,
            provider_id,
        }
    }

    /// Handle provider state change
    pub async fn handle_state_change(&self, state: super::providers::ProviderState) -> Result<()> {
        debug!(
            "Handling LLM provider state change for {}",
            self.provider_id
        );

        // Create a node update
        let mut properties = std::collections::HashMap::new();
        properties.insert(
            "model".to_string(),
            serde_json::to_value(state.model).unwrap_or_default(),
        );
        properties.insert(
            "last_request".to_string(),
            serde_json::to_value(state.last_request.map(|dt| dt.to_rfc3339())).unwrap_or_default(),
        );
        properties.insert(
            "error_count".to_string(),
            serde_json::to_value(state.error_count).unwrap_or_default(),
        );
        properties.insert(
            "success_count".to_string(),
            serde_json::to_value(state.success_count).unwrap_or_default(),
        );

        let node = GraphNode {
            id: self.provider_id.clone(),
            name: format!("LLM Provider {}", self.provider_id),
            node_type: "llm_provider".to_string(),
            status: state.status,
            properties,
        };

        // Create and send the update
        let update = GraphUpdate {
            graph_id: "llm_integration-graph".to_string(),
            update_type: GraphUpdateType::NodeUpdated,
            graph: None,
            node: Some(node),
            edge: None,
        };

        self.graph_manager.notify_update(update).await?;
        Ok(())
    }

    /// Handle provider error
    pub async fn handle_error(&self, error: &str) -> Result<()> {
        error!("LLM provider error for {}: {}", self.provider_id, error);

        // Update node status to error
        let node = GraphNode {
            id: self.provider_id.clone(),
            name: format!("LLM Provider {}", self.provider_id),
            node_type: "llm_provider".to_string(),
            status: "error".to_string(),
            properties: std::collections::HashMap::new(),
        };

        let update = GraphUpdate {
            graph_id: "llm_integration-graph".to_string(),
            update_type: GraphUpdateType::NodeUpdated,
            graph: None,
            node: Some(node),
            edge: None,
        };

        self.graph_manager.notify_update(update).await?;
        Ok(())
    }
}

#[derive(Debug)]
/// Event handler for human input state changes
pub struct HumanInputEventHandler {
    /// Graph manager for updating visualizations
    graph_manager: Arc<GraphManager>,
    /// Input ID
    input_id: String,
}

impl HumanInputEventHandler {
    /// Create a new human input event handler
    pub fn new(graph_manager: Arc<GraphManager>, input_id: String) -> Self {
        Self {
            graph_manager,
            input_id,
        }
    }

    /// Handle input state change
    pub async fn handle_state_change(&self, state: super::providers::InputState) -> Result<()> {
        debug!("Handling human input state change for {}", self.input_id);

        // Create a node update
        let mut properties = std::collections::HashMap::new();
        properties.insert(
            "timeout".to_string(),
            serde_json::to_value(state.timeout).unwrap_or_default(),
        );
        properties.insert(
            "required".to_string(),
            serde_json::to_value(state.required).unwrap_or_default(),
        );
        properties.insert(
            "last_input".to_string(),
            serde_json::to_value(state.last_input.map(|dt| dt.to_rfc3339())).unwrap_or_default(),
        );
        if let Some(desc) = &state.description {
            properties.insert(
                "description".to_string(),
                serde_json::to_value(desc).unwrap_or_default(),
            );
        }

        let node = GraphNode {
            id: self.input_id.clone(),
            name: format!("Input: {}", self.input_id),
            node_type: "human_input".to_string(),
            status: state.status,
            properties,
        };

        // Create and send the update
        let update = GraphUpdate {
            graph_id: "human_input-graph".to_string(),
            update_type: GraphUpdateType::NodeUpdated,
            graph: None,
            node: Some(node),
            edge: None,
        };

        self.graph_manager.notify_update(update).await?;
        Ok(())
    }

    /// Handle input timeout
    pub async fn handle_timeout(&self) -> Result<()> {
        info!("Human input timeout for {}", self.input_id);

        // Update node status to timeout
        let node = GraphNode {
            id: self.input_id.clone(),
            name: format!("Input: {}", self.input_id),
            node_type: "human_input".to_string(),
            status: "timeout".to_string(),
            properties: std::collections::HashMap::new(),
        };

        let update = GraphUpdate {
            graph_id: "human_input-graph".to_string(),
            update_type: GraphUpdateType::NodeUpdated,
            graph: None,
            node: Some(node),
            edge: None,
        };

        self.graph_manager.notify_update(update).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::graph::providers::{InputState, ProviderState};
    use chrono::Utc;

    #[tokio::test]
    async fn test_llm_provider_event_handler() {
        let graph_manager = Arc::new(GraphManager::new());
        let handler =
            LlmProviderEventHandler::new(graph_manager.clone(), "test-provider".to_string());

        let state = ProviderState {
            status: "active".to_string(),
            model: "test-model".to_string(),
            last_request: Some(Utc::now()),
            error_count: 0,
            success_count: 1,
        };

        assert!(handler.handle_state_change(state).await.is_ok());
        assert!(handler.handle_error("test error").await.is_ok());
    }

    #[tokio::test]
    async fn test_human_input_event_handler() {
        let graph_manager = Arc::new(GraphManager::new());
        let handler = HumanInputEventHandler::new(graph_manager.clone(), "test-input".to_string());

        let state = InputState {
            status: "waiting".to_string(),
            last_input: Some(Utc::now()),
            timeout: Some(Utc::now() + chrono::Duration::seconds(30)),
            description: Some("Test input".to_string()),
            required: true,
        };

        assert!(handler.handle_state_change(state).await.is_ok());
        assert!(handler.handle_timeout().await.is_ok());
    }
}
