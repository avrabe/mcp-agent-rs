use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Represents a single tool that can be invoked by a model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    /// Unique identifier for the tool
    pub name: String,

    /// Human-readable description of functionality
    pub description: String,

    /// JSON Schema defining expected parameters
    pub input_schema: Value,

    /// Optional properties describing tool behavior
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<HashMap<String, Value>>,
}

impl Tool {
    /// Creates a new tool with the given name, description, and input schema
    pub fn new(name: &str, description: &str, input_schema: Value) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            input_schema,
            annotations: None,
        }
    }

    /// Adds an annotation to the tool
    pub fn with_annotation(mut self, key: &str, value: Value) -> Self {
        let annotations = self.annotations.get_or_insert_with(HashMap::new);
        annotations.insert(key.to_string(), value);
        self
    }
}

/// Represents different content types for tool results
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ToolResultContent {
    /// Text content
    #[serde(rename = "text")]
    Text {
        /// The text content
        text: String,
    },

    /// Image content
    #[serde(rename = "image")]
    Image {
        /// Base64-encoded image data
        data: String,
        /// MIME type of the image
        mime_type: String,
    },

    /// Audio content
    #[serde(rename = "audio")]
    Audio {
        /// Base64-encoded audio data
        data: String,
        /// MIME type of the audio
        mime_type: String,
    },

    /// Resource content
    #[serde(rename = "resource")]
    Resource {
        /// The resource data
        resource: ResourceContent,
    },
}

/// Represents a resource included in a tool result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceContent {
    /// URI for the resource
    pub uri: String,

    /// MIME type of the resource
    pub mime_type: String,

    /// Optional text content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// Optional binary content (base64 encoded)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

/// Represents the result of a tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// List of content items in the result
    pub content: Vec<ToolResultContent>,

    /// Whether the tool execution resulted in an error
    #[serde(default)]
    pub is_error: bool,
}

impl ToolResult {
    /// Creates a new success result with text content
    pub fn text(text: &str) -> Self {
        Self {
            content: vec![ToolResultContent::Text {
                text: text.to_string(),
            }],
            is_error: false,
        }
    }

    /// Creates a new error result with text content
    pub fn error(text: &str) -> Self {
        Self {
            content: vec![ToolResultContent::Text {
                text: text.to_string(),
            }],
            is_error: true,
        }
    }

    /// Adds text content to the result
    pub fn with_text(mut self, text: &str) -> Self {
        self.content.push(ToolResultContent::Text {
            text: text.to_string(),
        });
        self
    }

    /// Adds image content to the result
    pub fn with_image(mut self, data: &str, mime_type: &str) -> Self {
        self.content.push(ToolResultContent::Image {
            data: data.to_string(),
            mime_type: mime_type.to_string(),
        });
        self
    }
}
