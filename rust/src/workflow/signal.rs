use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::time::timeout;
use uuid::Uuid;
use async_trait::async_trait;
use anyhow::{Result, anyhow};
use std::time::Duration;
use tracing::{debug, info, warn, error, instrument};

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
    pub fn with_description(name: &str, description: &str, payload: Option<serde_json::Value>) -> Self {
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

/// Signal handler interface for workflow signals
#[async_trait]
pub trait SignalHandler: Send + Sync + std::fmt::Debug {
    /// Send a signal to the workflow
    async fn signal(&self, signal: WorkflowSignal) -> Result<()>;
    
    /// Wait for a signal with the given name
    async fn wait_for_signal(
        &self,
        signal_name: &str,
        workflow_id: Option<&str>,
        timeout: Option<Duration>,
    ) -> Result<WorkflowSignal>;
}

/// Asynchronous signal handler
#[derive(Debug)]
pub struct AsyncSignalHandler {
    /// Map of signal name to pending signals
    pending_signals: Arc<Mutex<HashMap<String, Vec<PendingSignal>>>>,
}

impl AsyncSignalHandler {
    /// Create a new signal handler
    pub fn new() -> Self {
        Self {
            pending_signals: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl SignalHandler for AsyncSignalHandler {
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
        workflow_id: Option<&str>,
        timeout_duration: Option<Duration>,
    ) -> Result<WorkflowSignal> {
        debug!("Waiting for signal: {}", signal_name);
        
        let registration = SignalRegistration::new(signal_name, workflow_id);
        let (sender, receiver) = oneshot::channel();
        
        let pending_signal = PendingSignal {
            registration: registration.clone(),
            sender,
        };
        
        // Register the pending signal
        {
            let mut pending_signals = self.pending_signals.lock().await;
            pending_signals.entry(signal_name.to_string())
                .or_insert_with(Vec::new)
                .push(pending_signal);
        }
        
        // Wait for the signal with optional timeout
        let signal = if let Some(duration) = timeout_duration {
            match timeout(duration, receiver).await {
                Ok(result) => result.map_err(|_| anyhow!("Signal channel closed"))?,
                Err(_) => {
                    // Timeout occurred, remove the registration
                    let mut pending_signals = self.pending_signals.lock().await;
                    if let Some(handlers) = pending_signals.get_mut(signal_name) {
                        handlers.retain(|ps| ps.registration.unique_name != registration.unique_name);
                        if handlers.is_empty() {
                            pending_signals.remove(signal_name);
                        }
                    }
                    
                    return Err(anyhow!("Timeout waiting for signal: {}", signal_name));
                }
            }
        } else {
            receiver.await.map_err(|_| anyhow!("Signal channel closed"))?
        };
        
        debug!("Received signal: {}", signal_name);
        Ok(signal)
    }
}

impl Default for AsyncSignalHandler {
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
        let handler = AsyncSignalHandler::new();
        
        // Set up a task to wait for a signal
        let handler_clone = Arc::new(handler);
        let wait_task = tokio::spawn({
            let handler = Arc::clone(&handler_clone);
            async move {
                handler.wait_for_signal("test_signal", None, Some(Duration::from_secs(1))).await
            }
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
        assert_eq!(received_signal.payload, Some(serde_json::json!({"value": "test"})));
    }
    
    #[tokio::test]
    async fn test_signal_timeout() {
        let handler = AsyncSignalHandler::new();
        
        // Wait for a signal with a short timeout
        let result = handler.wait_for_signal(
            "timeout_signal", 
            None, 
            Some(Duration::from_millis(100))
        ).await;
        
        // Should timeout
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Timeout waiting for signal"));
    }
} 