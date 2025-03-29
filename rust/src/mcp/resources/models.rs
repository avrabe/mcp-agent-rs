use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A resource definition representing a piece of context data for the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    /// Unique identifier for the resource using URI format
    pub uri: String,
    /// Human-readable name for the resource
    pub name: String,
    /// Optional description of the resource
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// MIME type of the resource content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Size of the resource in bytes (if known)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Additional metadata for the resource
    #[serde(skip_serializing_if = "HashMap::is_empty", default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

impl Resource {
    /// Creates a new resource with the given URI and name
    pub fn new(uri: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            name: name.into(),
            description: None,
            mime_type: None,
            size: None,
            metadata: HashMap::new(),
        }
    }

    /// Sets the description for the resource
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the MIME type for the resource
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    /// Sets the size for the resource
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Adds metadata to the resource
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Into<serde_json::Value>,
    ) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// The contents of a resource
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResourceContents {
    /// Text-based resource content
    Text(TextContent),
    /// Binary resource content
    Binary(BinaryContent),
}

/// Text-based resource content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextContent {
    /// URI of the resource
    pub uri: String,
    /// MIME type of the content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// The text content
    pub text: String,
}

impl TextContent {
    /// Creates new text content for a resource
    pub fn new(uri: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            mime_type: None,
            text: text.into(),
        }
    }

    /// Sets the MIME type for the content
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }
}

/// Binary resource content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryContent {
    /// URI of the resource
    pub uri: String,
    /// MIME type of the content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Base64-encoded binary data
    pub blob: String,
}

impl BinaryContent {
    /// Creates new binary content for a resource
    pub fn new(uri: impl Into<String>, data: Vec<u8>) -> Self {
        Self {
            uri: uri.into(),
            mime_type: None,
            blob: base64::engine::general_purpose::STANDARD.encode(data),
        }
    }

    /// Sets the MIME type for the content
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    /// Decodes the binary data
    pub fn decode(&self) -> Result<Vec<u8>, base64::DecodeError> {
        base64::engine::general_purpose::STANDARD.decode(&self.blob)
    }
}

/// Response type for a resource read operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReadResponse {
    /// Array of resource contents
    pub contents: Vec<ResourceContents>,
}

/// Response type for a resource list operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceListResponse {
    /// Array of resources
    pub resources: Vec<Resource>,
    /// Optional cursor for pagination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// List resources request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceListParams {
    /// Optional cursor for pagination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Read resource request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceReadParams {
    /// URI of the resource to read
    pub uri: String,
}

/// Subscribe to resource request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSubscribeParams {
    /// URI of the resource to subscribe to
    pub uri: String,
}

/// Unsubscribe from resource request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUnsubscribeParams {
    /// URI of the resource to unsubscribe from
    pub uri: String,
}

/// Resource updated notification parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUpdatedParams {
    /// URI of the resource that was updated
    pub uri: String,
}
