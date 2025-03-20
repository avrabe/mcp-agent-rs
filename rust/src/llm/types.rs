use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use anyhow::Result;
use async_trait::async_trait;

/// Role of a message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    /// Message from the system
    System,
    /// Message from the user
    User,
    /// Message from the assistant
    Assistant,
    /// Message from a function call
    Function,
    /// Message from a tool
    Tool,
}

impl std::fmt::Display for MessageRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageRole::System => write!(f, "system"),
            MessageRole::User => write!(f, "user"),
            MessageRole::Assistant => write!(f, "assistant"),
            MessageRole::Function => write!(f, "function"),
            MessageRole::Tool => write!(f, "tool"),
        }
    }
}

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Role of the message sender
    pub role: MessageRole,
    /// Content of the message
    pub content: String,
    /// Optional metadata for the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl Message {
    /// Create a new system message
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::System,
            content: content.into(),
            metadata: None,
        }
    }
    
    /// Create a new user message
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::User,
            content: content.into(),
            metadata: None,
        }
    }
    
    /// Create a new assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: MessageRole::Assistant,
            content: content.into(),
            metadata: None,
        }
    }
    
    /// Add metadata to the message
    pub fn with_metadata(mut self, metadata: HashMap<String, serde_json::Value>) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Configuration for an LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Name of the model to use
    pub model: String,
    
    /// Base URL for the API
    pub api_url: String,
    
    /// API key for authentication (if required)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    
    /// Maximum number of tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    
    /// Temperature for sampling (higher = more random)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    
    /// Top-p sampling (nucleus sampling)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    
    /// Additional parameters
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub parameters: HashMap<String, serde_json::Value>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            model: "mistral".to_string(),
            api_url: "http://localhost:11434".to_string(),
            api_key: None,
            max_tokens: None,
            temperature: Some(0.7),
            top_p: Some(0.9),
            parameters: HashMap::new(),
        }
    }
}

/// A completion request to an LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Model to use for the request
    pub model: String,
    
    /// Messages in the conversation
    pub messages: Vec<Message>,
    
    /// Maximum number of tokens to generate
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    
    /// Temperature for sampling (higher = more random)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    
    /// Top-p sampling (nucleus sampling)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    
    /// Additional parameters
    #[serde(flatten)]
    pub parameters: HashMap<String, serde_json::Value>,
}

/// A completion response from an LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Completion {
    /// Generated content
    pub content: String,
    
    /// Model that generated the completion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    
    /// Number of tokens in the prompt
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    
    /// Number of tokens in the completion
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
    
    /// Total number of tokens used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
    
    /// Additional data from the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Trait for LLM clients
#[async_trait]
pub trait LlmClient: Send + Sync + 'static {
    /// Generate a completion from the LLM
    async fn complete(&self, request: CompletionRequest) -> Result<Completion>;
    
    /// Check if the model is available
    async fn is_available(&self) -> Result<bool>;
    
    /// Get the configuration for the client
    fn config(&self) -> &LlmConfig;
} 