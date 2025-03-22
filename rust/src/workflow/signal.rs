use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{oneshot, Mutex};
use tokio::time::timeout;
use tracing::{debug, instrument};
use uuid::Uuid;

/// A signal that can be sent to or from a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSignal {
    /// Unique ID of the signal
    pub id: String,

    /// Name of the signal
    pub name: String,

    /// Description of the signal
    pub description: Option<String>,

    /// Payload for the signal
    pub payload: Option<serde_json::Value>,

    /// ID of the workflow this signal belongs to
    pub workflow_id: Option<String>,

    /// Time the signal was created
    pub created_at: DateTime<Utc>,
}

impl WorkflowSignal {
    /// Create a new signal
    pub fn new(name: &str, payload: Option<serde_json::Value>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_string(),
            description: None,
            payload,
            workflow_id: None,
            created_at: Utc::now(),
        }
    }

    /// Create a new signal with a description
    pub fn with_description(
        name: &str,
        description: &str,
        payload: Option<serde_json::Value>,
    ) -> Self {
        let mut signal = Self::new(name, payload);
        signal.description = Some(description.to_string());
        signal
    }

    /// Create a signal for a specific workflow
    pub fn for_workflow(name: &str, workflow_id: &str, payload: Option<serde_json::Value>) -> Self {
        let mut signal = Self::new(name, payload);
        signal.workflow_id = Some(workflow_id.to_string());
        signal
    }

    // Predefined signal constants
    /// Interrupt signal name
    pub const INTERRUPT: &'static str = "interrupt";

    /// Terminate signal name
    pub const TERMINATE: &'static str = "terminate";

    /// Pause signal name
    pub const PAUSE: &'static str = "pause";

    /// Resume signal name
    pub const RESUME: &'static str = "resume";

    /// Cancel signal name
    pub const CANCEL: &'static str = "cancel";
}

/// Registration of a signal handler
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalRegistration {
    /// Name of the signal
    pub signal_name: String,

    /// Unique name of the registration
    pub unique_name: String,

    /// ID of the workflow this signal belongs to
    pub workflow_id: Option<String>,
}

impl SignalRegistration {
    /// Create a new signal registration
    pub fn new(signal_name: &str, workflow_id: Option<&str>) -> Self {
        Self {
            signal_name: signal_name.to_string(),
            unique_name: Uuid::new_v4().to_string(),
            workflow_id: workflow_id.map(|id| id.to_string()),
        }
    }
}

/// A pending signal waiting for a value
#[derive(Debug)]
pub struct PendingSignal {
    /// Registration of the signal
    pub registration: SignalRegistration,

    /// Channel sender for resolving the waiting signal
    pub sender: oneshot::Sender<WorkflowSignal>,
}

/// Object-safe trait for signal handlers
pub trait SignalHandler: Send + Sync + std::fmt::Debug {
    /// Send a signal to the workflow (non-async wrapper)
    fn signal_boxed(&self, signal: WorkflowSignal) -> BoxFuture<Result<()>>;

    /// Wait for a signal of the specified type (non-async wrapper)
    fn wait_for_signal_boxed<'a>(
        &'a self,
        signal_type: &'a str,
        timeout_seconds: Option<u64>,
    ) -> BoxFuture<'a, Result<WorkflowSignal>>;
}

/// Async signal handler trait
#[async_trait]
pub trait AsyncSignalHandler: Send + Sync + std::fmt::Debug {
    /// Send a signal to the workflow
    async fn signal(&self, signal: WorkflowSignal) -> Result<()>;

    /// Wait for a signal of the specified type
    async fn wait_for_signal(
        &self,
        signal_type: &str,
        timeout_seconds: Option<u64>,
    ) -> Result<WorkflowSignal>;
}

/// Helper extension trait
pub trait SignalHandlerExt: AsyncSignalHandler {
    /// Convert to a SignalHandler trait object reference
    fn as_signal_handler(&self) -> &dyn SignalHandler;
}

impl<T: AsyncSignalHandler + 'static> SignalHandler for T {
    fn signal_boxed(&self, signal: WorkflowSignal) -> BoxFuture<Result<()>> {
        Box::pin(async move { T::signal(self, signal).await })
    }

    fn wait_for_signal_boxed<'a>(
        &'a self,
        signal_type: &'a str,
        timeout_seconds: Option<u64>,
    ) -> BoxFuture<'a, Result<WorkflowSignal>> {
        Box::pin(async move { T::wait_for_signal(self, signal_type, timeout_seconds).await })
    }
}

impl<T: AsyncSignalHandler + 'static> SignalHandlerExt for T {
    fn as_signal_handler(&self) -> &dyn SignalHandler {
        self
    }
}

/// Async helper functions to make working with dyn SignalHandler easier
pub mod signal_handler_helpers {
    use super::*;

    /// Send a signal using the provided handler
    pub async fn signal(handler: &dyn SignalHandler, signal: WorkflowSignal) -> Result<()> {
        handler.signal_boxed(signal).await
    }

    /// Wait for a signal of a specific type using the provided handler
    pub async fn wait_for_signal(
        handler: &dyn SignalHandler,
        signal_type: &str,
        timeout_seconds: Option<u64>,
    ) -> Result<WorkflowSignal> {
        handler
            .wait_for_signal_boxed(signal_type, timeout_seconds)
            .await
    }
}

/// Asynchronous signal handler
#[derive(Debug)]
pub struct DefaultSignalHandler {
    /// Map of signal name to pending signals
    pending_signals: Arc<Mutex<HashMap<String, Vec<PendingSignal>>>>,
}

impl DefaultSignalHandler {
    /// Create a new signal handler
    pub fn new() -> Self {
        Self {
            pending_signals: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new signal handler with predefined signal types
    pub fn new_with_signals(signal_types: Vec<&str>) -> Self {
        let handler = Self::new();
        // Signal types are just registered for documentation
        // The actual handling is done in the wait_for_signal method
        debug!("Signal handler initialized with types: {:?}", signal_types);
        handler
    }
}

#[async_trait]
impl AsyncSignalHandler for DefaultSignalHandler {
    #[instrument(skip(self), fields(signal.name = %signal.name, signal.id = %signal.id))]
    async fn signal(&self, signal: WorkflowSignal) -> Result<()> {
        debug!("Emitting signal: {}", signal.name);

        let mut pending_signals = self.pending_signals.lock().await;

        if let Some(handlers) = pending_signals.get_mut(&signal.name) {
            // Make a copy of the handlers to avoid mutable borrow issues
            let pending_handlers = std::mem::take(handlers);

            // Keep track of handlers that weren't matched
            let mut remaining_handlers = Vec::new();

            // Process each handler
            for handler in pending_handlers {
                let matched = match (&signal.workflow_id, &handler.registration.workflow_id) {
                    // If both have workflow IDs, they must match
                    (Some(signal_wf_id), Some(handler_wf_id)) => signal_wf_id == handler_wf_id,
                    // If signal has workflow ID but handler doesn't, it won't match
                    (Some(_), None) => false,
                    // Otherwise match (handler has workflow ID but signal doesn't, or neither has workflow ID)
                    _ => true,
                };

                if matched {
                    // We found a match, send the signal
                    debug!("Found matching handler for signal: {}", signal.name);
                    let _ = handler.sender.send(signal.clone());
                } else {
                    // No match, keep the handler
                    remaining_handlers.push(handler);
                }
            }

            // Restore the remaining handlers
            *handlers = remaining_handlers;

            Ok(())
        } else {
            // No handlers registered for this signal
            debug!("No handlers registered for signal: {}", signal.name);
            Ok(())
        }
    }

    #[instrument(skip(self), fields(signal.name = %signal_name))]
    async fn wait_for_signal(
        &self,
        signal_name: &str,
        timeout_seconds: Option<u64>,
    ) -> Result<WorkflowSignal> {
        debug!("Waiting for signal: {}", signal_name);

        let registration = SignalRegistration::new(signal_name, None);
        let (sender, receiver) = oneshot::channel();

        let pending_signal = PendingSignal {
            registration: registration.clone(),
            sender,
        };

        // Register the pending signal
        {
            let mut pending_signals = self.pending_signals.lock().await;
            pending_signals
                .entry(signal_name.to_string())
                .or_insert_with(Vec::new)
                .push(pending_signal);
        }

        // Wait for the signal with optional timeout
        let signal = match timeout_seconds {
            Some(secs) => {
                match timeout(Duration::from_secs(secs), receiver).await {
                    Ok(result) => result.map_err(|_| anyhow!("Signal channel closed"))?,
                    Err(_) => {
                        // Timeout occurred, remove the registration
                        let mut pending_signals = self.pending_signals.lock().await;
                        if let Some(handlers) = pending_signals.get_mut(signal_name) {
                            handlers.retain(|ps| {
                                ps.registration.unique_name != registration.unique_name
                            });
                            if handlers.is_empty() {
                                pending_signals.remove(signal_name);
                            }
                        }

                        return Err(anyhow!("Timeout waiting for signal: {}", signal_name));
                    }
                }
            }
            None => receiver
                .await
                .map_err(|_| anyhow!("Signal channel closed"))?,
        };

        debug!("Received signal: {}", signal_name);
        Ok(signal)
    }
}

impl Default for DefaultSignalHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_basic_signal() {
        let handler = DefaultSignalHandler::new();

        // Set up a task to wait for a signal
        let handler_clone = Arc::new(handler);
        let wait_task = tokio::spawn({
            let handler = Arc::clone(&handler_clone);
            async move { handler.wait_for_signal("test_signal", None).await }
        });

        // Give the wait task time to register
        sleep(Duration::from_millis(50)).await;

        // Send the signal
        let signal = WorkflowSignal::new("test_signal", Some(serde_json::json!({"value": "test"})));
        handler_clone.signal(signal.clone()).await.unwrap();

        // Check that the wait task received the signal
        let result = wait_task.await.unwrap();
        assert!(result.is_ok());

        let received_signal = result.unwrap();
        assert_eq!(received_signal.name, "test_signal");
        assert_eq!(
            received_signal.payload,
            Some(serde_json::json!({"value": "test"}))
        );
    }

    #[tokio::test]
    async fn test_signal_timeout() {
        let handler = DefaultSignalHandler::new();

        // Wait for a signal with a short timeout
        let result = handler.wait_for_signal("timeout_signal", Some(1)).await;

        // Should time out
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Timeout waiting for signal"));
    }
}
