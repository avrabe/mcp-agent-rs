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
    
    /// Alias for update_status for API compatibility
    pub fn set_status(&mut self, status: &str) {
        self.update_status(status);
    }
    
    /// Set a single metadata value
    pub fn set_metadata<T: Into<serde_json::Value>>(&mut self, key: &str, value: T) {
        self.metadata.insert(key.to_string(), value.into());
        self.updated_at = Some(Utc::now());
    }
    
    /// Set error information
    pub fn set_error<S: Into<String>>(&mut self, message: S) {
        self.record_error("WorkflowError", &message.into());
    }
    
    /// Get a reference to the metadata for API compatibility
    pub fn metadata(&self) -> &HashMap<String, serde_json::Value> {
        &self.metadata
    }
    
    /// Get the status for API compatibility
    pub fn status(&self) -> &str {
        &self.status
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
    
    /// Create a success result with the given output
    pub fn success<T: Into<serde_json::Value>>(output: T) -> Self {
        let mut result = Self::with_value(output.into());
        result.start_time = Some(Utc::now() - chrono::Duration::seconds(1));
        result.end_time = Some(Utc::now());
        result.metadata.insert("status".to_string(), serde_json::Value::String("success".to_string()));
        result
    }
    
    /// Create a failed result with the given error message
    pub fn failed<S: Into<String>>(error_message: S) -> Self {
        let mut result = Self::new();
        result.start_time = Some(Utc::now() - chrono::Duration::seconds(1));
        result.end_time = Some(Utc::now());
        result.metadata.insert("status".to_string(), serde_json::Value::String("failed".to_string()));
        result.metadata.insert("error".to_string(), serde_json::Value::String(error_message.into()));
        result
    }
    
    /// Create a partial result with the given success and failure counts
    pub fn partial<T: Into<serde_json::Value>>(partial_output: T, success_count: usize, failure_count: usize) -> Self {
        let mut result = Self::with_value(partial_output.into());
        result.start_time = Some(Utc::now() - chrono::Duration::seconds(1));
        result.end_time = Some(Utc::now());
        result.metadata.insert("status".to_string(), serde_json::Value::String("partial".to_string()));
        result.metadata.insert("success_count".to_string(), serde_json::Value::Number(serde_json::Number::from(success_count)));
        result.metadata.insert("failure_count".to_string(), serde_json::Value::Number(serde_json::Number::from(failure_count)));
        result
    }
    
    /// Check if the result was successful
    pub fn is_success(&self) -> bool {
        self.metadata.get("status")
            .and_then(|v| v.as_str())
            .map(|s| s == "success")
            .unwrap_or(false)
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
    
    /// Get the output value as a formatted string
    pub fn output(&self) -> String {
        match &self.value {
            Some(v) => serde_json::to_string_pretty(v).unwrap_or_else(|_| "Error formatting output".to_string()),
            None => "No output available".to_string(),
        }
    }
    
    /// Get the error message if present
    pub fn error(&self) -> Option<String> {
        self.metadata.get("error")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    }
} 