use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

/// Signal name for human input in the workflow engine
pub const HUMAN_INPUT_SIGNAL_NAME: &str = "__human_input__";

/// A request for human input
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanInputRequest {
    /// Unique identifier for this request
    pub request_id: String,

    /// The prompt to show to the user
    pub prompt: String,

    /// Optional description of what the input is for
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional workflow ID if using workflow engine
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,

    /// Optional timeout in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,

    /// Additional request payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl HumanInputRequest {
    /// Create a new human input request
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            prompt: prompt.into(),
            description: None,
            workflow_id: None,
            timeout_seconds: None,
            metadata: None,
        }
    }

    /// Add a description to the request
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add a workflow ID to the request
    pub fn with_workflow_id(mut self, workflow_id: impl Into<String>) -> Self {
        self.workflow_id = Some(workflow_id.into());
        self
    }

    /// Add a timeout to the request
    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = Some(seconds);
        self
    }

    /// Add metadata to the request
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Get the timeout as a Duration
    pub fn timeout(&self) -> Option<Duration> {
        self.timeout_seconds.map(Duration::from_secs)
    }
}

/// A response to a human input request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanInputResponse {
    /// ID of the original request
    pub request_id: String,

    /// The input provided by the human
    pub response: String,

    /// Additional response payload
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl HumanInputResponse {
    /// Create a new human input response
    pub fn new(request_id: impl Into<String>, response: impl Into<String>) -> Self {
        Self {
            request_id: request_id.into(),
            response: response.into(),
            metadata: None,
        }
    }

    /// Add metadata to the response
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// A trait for human input handlers
#[async_trait]
pub trait HumanInputHandler: Send + Sync {
    /// Handle a human input request
    async fn handle_request(
        &self,
        request: HumanInputRequest,
    ) -> anyhow::Result<HumanInputResponse>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_human_input_request_builder() {
        let request = HumanInputRequest::new("Test prompt")
            .with_description("Test description")
            .with_workflow_id("workflow-123")
            .with_timeout(30);

        assert_eq!(request.prompt, "Test prompt");
        assert_eq!(request.description, Some("Test description".to_string()));
        assert_eq!(request.workflow_id, Some("workflow-123".to_string()));
        assert_eq!(request.timeout_seconds, Some(30));
        assert_eq!(request.timeout(), Some(Duration::from_secs(30)));
    }

    #[test]
    fn test_human_input_response_builder() {
        let response = HumanInputResponse::new("req-123", "Test response");

        assert_eq!(response.request_id, "req-123");
        assert_eq!(response.response, "Test response");
        assert!(response.metadata.is_none());

        let mut metadata = HashMap::new();
        metadata.insert(
            "test_key".to_string(),
            serde_json::Value::String("test_value".to_string()),
        );

        let response_with_metadata = response.with_metadata(metadata.clone());

        assert_eq!(response_with_metadata.request_id, "req-123");
        assert_eq!(response_with_metadata.response, "Test response");
        assert_eq!(response_with_metadata.metadata, Some(metadata));
    }
}
