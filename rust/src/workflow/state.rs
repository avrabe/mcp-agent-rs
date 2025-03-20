use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Represents the current state of a workflow
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowState {
    /// Current status of the workflow
    pub status: String,
    
    /// Metadata for the workflow
    pub metadata: HashMap<String, serde_json::Value>,
    
    /// Timestamp of the last update
    pub updated_at: Option<DateTime<Utc>>,
    
    /// Error information if there was an error
    pub error: Option<HashMap<String, serde_json::Value>>,
    
    /// Name of the workflow
    pub name: Option<String>,
}

impl WorkflowState {
    /// Create a new workflow state
    pub fn new(name: Option<String>, metadata: Option<HashMap<String, serde_json::Value>>) -> Self {
        Self {
            status: "initialized".to_string(),
            metadata: metadata.unwrap_or_default(),
            updated_at: None,
            error: None,
            name,
        }
    }
    
    /// Record an error in the workflow state
    pub fn record_error(&mut self, error_type: &str, message: &str) {
        let mut error_map = HashMap::new();
        error_map.insert("type".to_string(), serde_json::Value::String(error_type.to_string()));
        error_map.insert("message".to_string(), serde_json::Value::String(message.to_string()));
        error_map.insert("timestamp".to_string(), serde_json::Value::Number(
            serde_json::Number::from_f64(Utc::now().timestamp_millis() as f64).unwrap()
        ));
        
        self.error = Some(error_map);
    }
    
    /// Update the workflow state with new metadata values
    pub fn update(&mut self, updates: HashMap<String, serde_json::Value>) {
        for (key, value) in updates {
            self.metadata.insert(key, value);
        }
        self.updated_at = Some(Utc::now());
    }
    
    /// Update the status of the workflow
    pub fn update_status(&mut self, status: &str) {
        self.status = status.to_string();
        self.updated_at = Some(Utc::now());
    }
}

/// Thread-safe shared workflow state
pub type SharedWorkflowState = Arc<Mutex<WorkflowState>>;

/// Result of a workflow execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowResult {
    /// The result value
    pub value: Option<serde_json::Value>,
    
    /// Metadata for the result
    pub metadata: HashMap<String, serde_json::Value>,
    
    /// Start time of the workflow
    pub start_time: Option<DateTime<Utc>>,
    
    /// End time of the workflow
    pub end_time: Option<DateTime<Utc>>,
}

impl WorkflowResult {
    /// Create a new workflow result
    pub fn new() -> Self {
        Self {
            value: None,
            metadata: HashMap::new(),
            start_time: None,
            end_time: None,
        }
    }
    
    /// Create a new workflow result with a value
    pub fn with_value(value: serde_json::Value) -> Self {
        Self {
            value: Some(value),
            metadata: HashMap::new(),
            start_time: None,
            end_time: None,
        }
    }
    
    /// Set the start time to now
    pub fn start(&mut self) {
        self.start_time = Some(Utc::now());
    }
    
    /// Set the end time to now
    pub fn complete(&mut self) {
        self.end_time = Some(Utc::now());
    }
    
    /// Calculate the duration in milliseconds
    pub fn duration_ms(&self) -> Option<i64> {
        match (self.start_time, self.end_time) {
            (Some(start), Some(end)) => Some((end - start).num_milliseconds()),
            _ => None,
        }
    }
} 