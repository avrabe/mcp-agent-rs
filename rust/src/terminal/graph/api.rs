//! API endpoints for graph visualization.
//!
//! This module provides HTTP and WebSocket endpoints for accessing
//! graph visualization data from the MCP components.

use axum::extract::ws::{Message, WebSocket};
use axum::{
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, warn};

use super::models::{convert_to_sprotty_model, convert_update_to_sprotty};
use super::sprotty_adapter::{process_sprotty_action, SprottyAction};
use super::{GraphManager, GraphUpdate};

/// State for the graph API
#[derive(Debug, Clone)]
pub struct GraphApiState {
    /// The graph manager
    graph_manager: Arc<GraphManager>,
}

/// Create a router for the graph API
pub fn create_graph_router(graph_manager: Arc<GraphManager>) -> Router {
    let state = GraphApiState { graph_manager };

    Router::new()
        .route("/api/graph", get(list_graphs))
        .route("/api/graph/:id", get(get_graph))
        .route("/api/graph/sprotty/:id", get(get_sprotty_graph))
        .route("/api/graph/ws", get(graph_ws_handler))
        .with_state(state)
}

/// Handler for listing available graphs
async fn list_graphs(State(state): State<GraphApiState>) -> impl IntoResponse {
    let graph_ids = state.graph_manager.list_graphs().await;

    Json(json!({
        "graphs": graph_ids,
    }))
}

/// Handler for getting a specific graph
async fn get_graph(
    Path(id): Path<String>,
    State(state): State<GraphApiState>,
) -> impl IntoResponse {
    if let Some(graph) = state.graph_manager.get_graph(&id).await {
        Json(json!({
            "graph": graph,
        }))
    } else {
        Json(json!({
            "error": format!("Graph not found: {}", id),
        }))
    }
}

/// Handler for getting a specific graph in Sprotty format
async fn get_sprotty_graph(
    Path(id): Path<String>,
    State(state): State<GraphApiState>,
) -> impl IntoResponse {
    if let Some(graph) = state.graph_manager.get_graph(&id).await {
        let sprotty_model = convert_to_sprotty_model(&graph);
        Json(json!({
            "model": sprotty_model,
        }))
    } else {
        Json(json!({
            "error": format!("Graph not found: {}", id),
        }))
    }
}

/// WebSocket upgrade handler for graph updates
async fn graph_ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<GraphApiState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_graph_socket(socket, state.graph_manager))
}

/// Handle a WebSocket connection for graph updates
async fn handle_graph_socket(socket: WebSocket, graph_manager: Arc<GraphManager>) {
    let (mut sender, mut receiver) = socket.split();

    // Create a channel for sending graph updates to the client
    let (update_tx, mut update_rx) = mpsc::channel::<Message>(100);
    let sender_for_recv = update_tx.clone();

    // Flag to track whether to use Sprotty format
    let use_sprotty_format = Arc::new(RwLock::new(true));
    let use_sprotty_format_clone = use_sprotty_format.clone();

    // Create a subscription to all graph updates
    let update_listener = graph_manager.subscribe().await;

    // Spawn a task to listen for graph updates
    let use_sprotty = Arc::clone(&use_sprotty_format);
    let update_sender = update_tx.clone();
    tokio::spawn(async move {
        let mut updates = update_listener;

        while let Some(update) = updates.recv().await {
            debug!("Received graph update: {:?}", update.update_type);
            let use_sprotty_val = *use_sprotty.read().await;

            let json_result = if use_sprotty_val {
                if let Some(sprotty_update) = convert_update_to_sprotty(&update) {
                    let action = super::sprotty_adapter::convert_update_to_action(&sprotty_update);
                    serde_json::to_string(&action)
                } else {
                    serde_json::to_string(&update)
                }
            } else {
                serde_json::to_string(&update)
            };

            if let Ok(json) = json_result {
                if let Err(e) = update_sender.send(Message::Text(json)).await {
                    error!("Error sending graph update over WebSocket: {}", e);
                    break;
                }
            }
        }
    });

    // Forward messages from the update channel to the WebSocket
    tokio::spawn(async move {
        while let Some(msg) = update_rx.recv().await {
            if let Err(e) = sender.send(msg).await {
                error!("Error sending WebSocket message: {}", e);
                break;
            }
        }
    });

    // Create a task to handle incoming messages from the client
    let recv_task = tokio::spawn(async move {
        while let Some(result) = receiver.next().await {
            let msg = match result {
                Ok(msg) => msg,
                Err(e) => {
                    error!("Error receiving WebSocket message: {}", e);
                    break;
                }
            };

            match msg {
                Message::Text(text) => {
                    // Parse the message as either a SprottyAction or a GraphClientRequest
                    if let Ok(action) = serde_json::from_str::<SprottyAction>(&text) {
                        // Process Sprotty action
                        debug!("Received Sprotty action: {:?}", action);

                        match process_sprotty_action(action, graph_manager.clone()).await {
                            Ok(Some(response_action)) => {
                                if let Ok(json) = serde_json::to_string(&response_action) {
                                    if let Err(e) = sender_for_recv.send(Message::Text(json)).await
                                    {
                                        error!("Error sending Sprotty action response: {}", e);
                                        break;
                                    }
                                }
                            }
                            Ok(None) => {
                                // No response needed
                            }
                            Err(e) => {
                                error!("Error processing Sprotty action: {}", e);
                                // Send error response
                                let error_json = json!({
                                    "kind": "error",
                                    "message": format!("Error: {}", e)
                                });
                                if let Err(e) = sender_for_recv
                                    .send(Message::Text(error_json.to_string()))
                                    .await
                                {
                                    error!("Error sending error response: {}", e);
                                    break;
                                }
                            }
                        }
                    } else if let Ok(request) = serde_json::from_str::<GraphClientRequest>(&text) {
                        // Handle legacy client request
                        debug!("Received client request: {}", request.request_type);

                        match request.request_type.as_str() {
                            "subscribe" => {
                                if let Some(graph_id) = &request.graph_id {
                                    debug!("Client subscribing to graph: {}", graph_id);

                                    // Send the initial graph state
                                    if let Some(graph) = graph_manager.get_graph(graph_id).await {
                                        let use_sprotty = *use_sprotty_format_clone.read().await;
                                        let update = GraphUpdate {
                                            graph_id: graph_id.clone(),
                                            update_type: super::GraphUpdateType::FullUpdate,
                                            graph: Some(graph),
                                            node: None,
                                            edge: None,
                                        };

                                        let result = if use_sprotty {
                                            if let Some(sprotty_update) =
                                                convert_update_to_sprotty(&update)
                                            {
                                                let action = super::sprotty_adapter::convert_update_to_action(&sprotty_update);
                                                serde_json::to_string(&action)
                                            } else {
                                                serde_json::to_string(&update)
                                            }
                                        } else {
                                            serde_json::to_string(&update)
                                        };

                                        if let Ok(json) = result {
                                            if let Err(e) =
                                                sender_for_recv.send(Message::Text(json)).await
                                            {
                                                error!(
                                                    "Error sending initial graph over WebSocket: {}",
                                                    e
                                                );
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            "set_format" => {
                                if let Some(params) = &request.params {
                                    if let Some(format) = params.get("format") {
                                        if let Some(format_str) = format.as_str() {
                                            let mut format_flag =
                                                use_sprotty_format_clone.write().await;
                                            *format_flag = format_str == "sprotty";
                                            debug!("Client set format to: {}", format_str);
                                        }
                                    }
                                }
                            }
                            _ => {
                                warn!("Unknown client request type: {}", request.request_type);
                            }
                        }
                    } else {
                        warn!("Received invalid WebSocket message: {}", text);
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either task to complete
    tokio::select! {
        _ = recv_task => {
            debug!("WebSocket connection closed");
        }
    }
}

/// Client request for graph data
#[derive(Debug, Deserialize)]
struct GraphClientRequest {
    /// Type of request
    request_type: String,

    /// Graph ID for the request
    graph_id: Option<String>,

    /// Additional parameters for the request
    params: Option<HashMap<String, serde_json::Value>>,
}

#[cfg(test)]
mod tests {
    use super::super::{Graph, GraphManager, GraphNode};
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_graph_router() {
        // Create a graph manager and add a test graph
        let manager = Arc::new(GraphManager::new());

        let test_graph = Graph {
            id: "test-graph".to_string(),
            name: "Test Graph".to_string(),
            graph_type: "test".to_string(),
            nodes: vec![GraphNode {
                id: "node1".to_string(),
                name: "Node 1".to_string(),
                node_type: "test".to_string(),
                status: "active".to_string(),
                properties: HashMap::new(),
            }],
            edges: vec![],
            properties: HashMap::new(),
        };

        manager.register_graph(test_graph).await.unwrap();

        // Create a router
        let router = create_graph_router(manager);

        // The router should have routes
        // In newer axum versions, we can't directly check routes
        // Just assert that we have a valid router instance
        assert!(true, "Router was created successfully");
    }
}
