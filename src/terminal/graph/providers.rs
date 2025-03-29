use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde_json::Value;
use futures::StreamExt;
use tracing::{debug, error, info};
use chrono::{Utc, Duration};

use crate::error::Result;
use crate::workflow::{Workflow, WorkflowEngine, WorkflowState, task::WorkflowTask};
use crate::mcp::agent::Agent;

// These will be implemented later
// use crate::llm::LlmProvider;
// use crate::human_input::HumanInputProvider;

use super::{Graph, GraphNode, GraphEdge, GraphManager, GraphUpdateType};

#[tokio::test]
async fn test_human_input_state_updates() {
    let provider = HumanInputGraphProvider::new();
    let graph_manager = Arc::new(GraphManager::new());
    provider
        .setup_tracking(graph_manager.clone())
        .await
        .unwrap();

    let input_id = "test-input";
    let state = InputState {
        status: "waiting".to_string(),
        last_input: Some(Utc::now()),
        timeout: Some(Utc::now() + Duration::seconds(30)),
        description: Some("Test input".to_string()),
        required: true,
    };

    assert!(provider.update_input_state(input_id, state).await.is_ok());

    // Verify the state was updated
    let states = provider.input_states.read().await;
    assert!(states.contains_key(input_id));
    assert_eq!(states.get(input_id).unwrap().status, "waiting");
}

#[tokio::test]
async fn test_provider_event_handlers() {
    let provider = LlmIntegrationGraphProvider::new();
    let graph_manager = Arc::new(GraphManager::new());
    provider
        .setup_tracking(graph_manager.clone())
        .await
        .unwrap();

    let provider_id = "test-provider";
    let handler = provider.create_event_handler(provider_id).await;

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
async fn test_human_input_event_handlers() {
    let provider = HumanInputGraphProvider::new();
    let graph_manager = Arc::new(GraphManager::new());
    provider
        .setup_tracking(graph_manager.clone())
        .await
        .unwrap();

    let input_id = "test-input";
    let handler = provider.create_event_handler(input_id).await;

    let state = InputState {
        status: "waiting".to_string(),
        last_input: Some(Utc::now()),
        timeout: Some(Utc::now() + Duration::seconds(30)),
        description: Some("Test input".to_string()),
        required: true,
    };

    assert!(handler.handle_state_change(state).await.is_ok());
    assert!(handler.handle_timeout().await.is_ok());
} 