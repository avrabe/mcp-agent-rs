use crate::mcp::types::JsonRpcError;
use thiserror::Error;

/// Errors that can occur in the resources system
#[derive(Debug, Error)]
pub enum ResourceError {
    /// Resource not found
    #[error("Resource not found: {0}")]
    ResourceNotFound(String),

    /// Invalid URI format
    #[error("Invalid URI: {0}")]
    InvalidUri(String),

    /// Invalid parameters for template
    #[error("Invalid template parameters: {0}")]
    InvalidTemplateParameters(String),

    /// Security violation (path traversal, etc.)
    #[error("Security violation: {0}")]
    SecurityViolation(String),

    /// Resource is too large
    #[error("Resource is too large: {0}")]
    ResourceTooLarge(String),

    /// I/O error
    #[error("I/O error: {0}")]
    IoError(String),

    /// Capability not supported
    #[error("Capability not supported: {0}")]
    CapabilityNotSupported(String),

    /// Serialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    DeserializationError(String),
}

impl From<ResourceError> for JsonRpcError {
    fn from(error: ResourceError) -> Self {
        match error {
            ResourceError::ResourceNotFound(_) => {
                JsonRpcError::new(-32002, "Resource not found", Some(error.to_string().into()))
            }
            ResourceError::InvalidUri(_) => {
                JsonRpcError::new(-32602, "Invalid URI", Some(error.to_string().into()))
            }
            ResourceError::InvalidTemplateParameters(_) => {
                JsonRpcError::new(-32602, "Invalid parameters", Some(error.to_string().into()))
            }
            ResourceError::SecurityViolation(_) => {
                JsonRpcError::new(-32001, "Security violation", Some(error.to_string().into()))
            }
            ResourceError::ResourceTooLarge(_) => {
                JsonRpcError::new(-32001, "Resource too large", Some(error.to_string().into()))
            }
            ResourceError::IoError(_) => {
                JsonRpcError::new(-32603, "I/O error", Some(error.to_string().into()))
            }
            ResourceError::CapabilityNotSupported(_) => {
                JsonRpcError::new(-32601, "Method not found", Some(error.to_string().into()))
            }
            ResourceError::SerializationError(_) => {
                JsonRpcError::new(-32603, "Internal error", Some(error.to_string().into()))
            }
            ResourceError::DeserializationError(_) => {
                JsonRpcError::new(-32700, "Parse error", Some(error.to_string().into()))
            }
        }
    }
}

impl From<std::io::Error> for ResourceError {
    fn from(error: std::io::Error) -> Self {
        ResourceError::IoError(error.to_string())
    }
}

impl From<serde_json::Error> for ResourceError {
    fn from(error: serde_json::Error) -> Self {
        if error.is_data() {
            ResourceError::DeserializationError(error.to_string())
        } else {
            ResourceError::SerializationError(error.to_string())
        }
    }
}
