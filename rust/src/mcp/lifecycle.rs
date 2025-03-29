//! Message lifecycle management for MCP protocol
//!
//! This module implements the lifecycle management components of the MCP protocol,
//! including initialization, capability negotiation, and session control.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, warn};

use crate::mcp::jsonrpc::JsonRpcHandler;
use crate::mcp::types::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use crate::utils::error::{McpError, McpResult};

/// Session state for an MCP connection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionState {
    /// Initial state, before any initialization
    Initial,
    /// Initializing, sent initialize request
    Initializing,
    /// Ready, initialization complete
    Ready,
    /// Session shutting down
    ShuttingDown,
    /// Session terminated
    Terminated,
}

/// MCP server capabilities that can be negotiated during initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Server name
    pub name: String,
    /// Server version
    pub version: String,
    /// Supported protocol version
    pub protocol_version: String,
    /// Supported primitives
    pub primitives: Vec<String>,
    /// Batch processing support
    #[serde(default)]
    pub batch_processing: bool,
    /// Custom capabilities
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<serde_json::Value>,
}

impl Default for ServerCapabilities {
    fn default() -> Self {
        Self {
            name: "MCP Server".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: "0.1".to_string(),
            primitives: vec!["prompt".to_string()],
            batch_processing: true,
            custom: None,
        }
    }
}

/// MCP client initialization parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Client name
    pub name: String,
    /// Client version
    pub version: String,
    /// Required protocol version
    pub protocol_version: String,
    /// Required primitives
    #[serde(default)]
    pub required_primitives: Vec<String>,
    /// Additional client info
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<serde_json::Value>,
}

/// Initialize request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    /// Client information
    pub client: ClientInfo,
    /// Capabilities requested by the client
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<serde_json::Value>,
}

/// Initialize result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    /// Server capabilities
    pub capabilities: ServerCapabilities,
    /// Server information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_info: Option<serde_json::Value>,
}

/// Interface for session lifecycle events
pub trait SessionListener: Send + Sync {
    /// Called when a session is initialized
    fn on_initialized(&self, session_id: &str, params: &InitializeParams) -> McpResult<()>;

    /// Called when a session is terminated
    fn on_terminated(&self, session_id: &str) -> McpResult<()>;
}

/// Lifecycle manager for MCP sessions
#[derive(Clone)]
pub struct LifecycleManager {
    /// Server capabilities
    capabilities: ServerCapabilities,
    /// Active sessions and their states
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,
    /// Session listeners
    listeners: Arc<RwLock<Vec<Box<dyn SessionListener>>>>,
}

impl std::fmt::Debug for LifecycleManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LifecycleManager")
            .field("capabilities", &self.capabilities)
            .field("sessions", &self.sessions)
            .field(
                "listeners",
                &format!("<{} listeners>", Arc::strong_count(&self.listeners)),
            )
            .finish()
    }
}

impl LifecycleManager {
    /// Creates a new lifecycle manager with the specified capabilities
    pub fn new(capabilities: ServerCapabilities) -> Self {
        Self {
            capabilities,
            sessions: Arc::new(RwLock::new(HashMap::new())),
            listeners: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Registers lifecycle methods with the JSON-RPC handler
    pub async fn register_methods(&self, handler: &JsonRpcHandler) -> McpResult<()> {
        let lifecycle_clone = self.clone();

        // Register initialize method
        handler
            .register_method("initialize", move |params| {
                let lifecycle = lifecycle_clone.clone();
                let params = params.ok_or_else(|| {
                    McpError::InvalidParams("Missing initialization parameters".to_string())
                })?;

                let init_params: InitializeParams =
                    serde_json::from_value(params).map_err(|e| {
                        McpError::InvalidParams(format!("Invalid initialization parameters: {}", e))
                    })?;

                // Generate a unique session ID
                let session_id = uuid::Uuid::new_v4().to_string();

                // Store session state
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let mut sessions = lifecycle.sessions.write().await;
                        sessions.insert(session_id.clone(), SessionState::Initializing);

                        // Prepare initialization result
                        let result = InitializeResult {
                            capabilities: lifecycle.capabilities.clone(),
                            server_info: None,
                        };

                        // Notify listeners
                        lifecycle
                            .notify_initialized(&session_id, &init_params)
                            .await?;

                        // Mark session as ready
                        sessions.insert(session_id, SessionState::Ready);

                        let json_result = serde_json::to_value(result).map_err(|e| {
                            McpError::SerializationError(format!(
                                "Failed to serialize result: {}",
                                e
                            ))
                        })?;

                        Ok::<_, McpError>(json_result)
                    })
                })?;

                Ok(result)
            })
            .await?;

        // Register shutdown method
        let lifecycle_clone = self.clone();
        handler
            .register_method("shutdown", move |params| {
                let lifecycle = lifecycle_clone.clone();

                // Extract session ID from params
                let params = params.ok_or_else(|| {
                    McpError::InvalidParams("Missing shutdown parameters".to_string())
                })?;

                let session_id = params
                    .get("session_id")
                    .and_then(|id| id.as_str())
                    .ok_or_else(|| {
                        McpError::InvalidParams("Missing session_id parameter".to_string())
                    })?
                    .to_string();

                // Update session state
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        let mut sessions = lifecycle.sessions.write().await;

                        if let Some(state) = sessions.get(&session_id) {
                            if *state != SessionState::Ready {
                                return Err(McpError::InvalidState(
                                    "Session not in Ready state".to_string(),
                                ));
                            }

                            sessions.insert(session_id.clone(), SessionState::ShuttingDown);
                            Ok(serde_json::json!({"success": true}))
                        } else {
                            Err(McpError::NotFound(format!(
                                "Session not found: {}",
                                session_id
                            )))
                        }
                    })
                })?;

                Ok(result)
            })
            .await?;

        // Register exit notification
        let lifecycle_clone = self.clone();
        handler
            .register_notification("exit", move |params| {
                let lifecycle = lifecycle_clone.clone();

                // Extract session ID from params
                let params = params.ok_or_else(|| {
                    McpError::InvalidParams("Missing exit parameters".to_string())
                })?;

                let session_id = params
                    .get("session_id")
                    .and_then(|id| id.as_str())
                    .ok_or_else(|| {
                        McpError::InvalidParams("Missing session_id parameter".to_string())
                    })?
                    .to_string();

                // Update session state and notify listeners
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
        let mut sessions = lifecycle.sessions.write().await;

        if let Some(state) = sessions.get(&session_id) {
if *state != SessionState::ShuttingDown {
warn!("Exit notification received for session not in ShuttingDown state: {}", session_id);
}

// Mark session as terminated
sessions.insert(session_id.clone(), SessionState::Terminated);

// Notify listeners
lifecycle.notify_terminated(&session_id).await?;

// Remove session
sessions.remove(&session_id);
        } else {
warn!("Exit notification received for unknown session: {}", session_id);
        }
        Ok::<_, McpError>(())
    })
                })?;
                Ok(serde_json::Value::Null)
            })
            .await?;

        Ok(())
    }

    /// Adds a session listener
    pub async fn add_listener<L: SessionListener + 'static>(&self, listener: L) {
        let mut listeners = self.listeners.write().await;
        listeners.push(Box::new(listener));
    }

    /// Gets the current state of a session
    pub async fn get_session_state(&self, session_id: &str) -> Option<SessionState> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// Notifies all listeners that a session was initialized
    async fn notify_initialized(
        &self,
        session_id: &str,
        params: &InitializeParams,
    ) -> McpResult<()> {
        let listeners = self.listeners.read().await;
        for listener in listeners.iter() {
            if let Err(e) = listener.on_initialized(session_id, params) {
                error!("Error in session initialization listener: {}", e);
                // Continue with other listeners even if one fails
            }
        }
        Ok(())
    }

    /// Notifies all listeners that a session was terminated
    async fn notify_terminated(&self, session_id: &str) -> McpResult<()> {
        let listeners = self.listeners.read().await;
        for listener in listeners.iter() {
            if let Err(e) = listener.on_terminated(session_id) {
                error!("Error in session termination listener: {}", e);
                // Continue with other listeners even if one fails
            }
        }
        Ok(())
    }

    /// Creates a client-side initialize request
    pub fn create_initialize_request(client_info: ClientInfo) -> JsonRpcRequest {
        let params = InitializeParams {
            client: client_info,
            capabilities: None,
        };

        JsonRpcRequest::new(
            "initialize",
            Some(serde_json::to_value(params).unwrap()),
            serde_json::Value::String(uuid::Uuid::new_v4().to_string()),
        )
    }

    /// Creates a client-side shutdown request
    pub fn create_shutdown_request(session_id: &str) -> JsonRpcRequest {
        JsonRpcRequest::new(
            "shutdown",
            Some(serde_json::json!({"session_id": session_id})),
            serde_json::Value::String(uuid::Uuid::new_v4().to_string()),
        )
    }

    /// Creates a client-side exit notification
    pub fn create_exit_notification(session_id: &str) -> JsonRpcNotification {
        JsonRpcNotification::new("exit", Some(serde_json::json!({"session_id": session_id})))
    }

    /// Handles an initialize response from the server
    pub fn handle_initialize_response(response: &JsonRpcResponse) -> McpResult<InitializeResult> {
        if let Some(error) = &response.error {
            return Err(McpError::RpcError(error.code, error.message.clone()));
        }

        let result = response.result.as_ref().ok_or_else(|| {
            McpError::InvalidResponse("Missing result in initialize response".to_string())
        })?;

        let init_result: InitializeResult =
            serde_json::from_value(result.clone()).map_err(|e| {
                McpError::DeserializationError(format!(
                    "Failed to deserialize InitializeResult: {}",
                    e
                ))
            })?;

        Ok(init_result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicBool, Ordering};

    struct TestSessionListener {
        initialized: Arc<AtomicBool>,
        terminated: Arc<AtomicBool>,
    }

    impl SessionListener for TestSessionListener {
        fn on_initialized(&self, _session_id: &str, _params: &InitializeParams) -> McpResult<()> {
            self.initialized.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn on_terminated(&self, _session_id: &str) -> McpResult<()> {
            self.terminated.store(true, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_lifecycle_methods() {
        // Create a lifecycle manager with default capabilities
        let lifecycle = LifecycleManager::new(ServerCapabilities::default());

        // Set up a listener to track initialization and termination
        let initialized = Arc::new(AtomicBool::new(false));
        let terminated = Arc::new(AtomicBool::new(false));

        let listener = TestSessionListener {
            initialized: initialized.clone(),
            terminated: terminated.clone(),
        };

        lifecycle.add_listener(listener).await;

        // Generate a unique session ID
        let session_id = uuid::Uuid::new_v4().to_string();

        // Create initialization parameters
        let client_info = ClientInfo {
            name: "Test Client".to_string(),
            version: "1.0".to_string(),
            protocol_version: "0.1".to_string(),
            required_primitives: vec![],
            custom: None,
        };

        let init_params = InitializeParams {
            client: client_info,
            capabilities: None,
        };

        // Test initialization
        {
            // Set session state to initializing
            let mut sessions = lifecycle.sessions.write().await;
            sessions.insert(session_id.clone(), SessionState::Initializing);

            // Drop the lock before calling notify_initialized
            drop(sessions);

            // Notify listeners of initialization
            lifecycle
                .notify_initialized(&session_id, &init_params)
                .await
                .unwrap();

            // Mark session as ready
            let mut sessions = lifecycle.sessions.write().await;
            sessions.insert(session_id.clone(), SessionState::Ready);
        }

        // Verify that the listener was notified of initialization
        assert!(initialized.load(Ordering::SeqCst));
        assert!(!terminated.load(Ordering::SeqCst));

        // Verify the session state
        let state = lifecycle.get_session_state(&session_id).await.unwrap();
        assert_eq!(state, SessionState::Ready);

        // Test shutdown
        {
            let mut sessions = lifecycle.sessions.write().await;
            sessions.insert(session_id.clone(), SessionState::ShuttingDown);
        }

        // Verify the session state
        let state = lifecycle.get_session_state(&session_id).await.unwrap();
        assert_eq!(state, SessionState::ShuttingDown);

        // Test exit (session termination)
        {
            // Mark session as terminated
            let mut sessions = lifecycle.sessions.write().await;
            sessions.insert(session_id.clone(), SessionState::Terminated);

            // Drop the lock before calling notify_terminated
            drop(sessions);

            // Notify listeners of termination
            lifecycle.notify_terminated(&session_id).await.unwrap();

            // Remove session
            let mut sessions = lifecycle.sessions.write().await;
            sessions.remove(&session_id);
        }

        // Verify that the listener was notified of termination
        assert!(terminated.load(Ordering::SeqCst));

        // Verify the session was removed
        let state = lifecycle.get_session_state(&session_id).await;
        assert!(state.is_none());
    }
}
