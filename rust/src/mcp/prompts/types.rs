//! Types for the Prompts module
//!
//! This module defines the core types used throughout the MCP Prompts system.
//! It includes PromptId, PromptParameter, PromptVersion, PromptCategory,
//! PromptParameters, and RenderedPrompt.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A unique identifier for a prompt template
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PromptId(pub String);

impl PromptId {
    /// Create a new prompt ID
    pub fn new(id: &str) -> Self {
        Self(id.to_string())
    }

    /// Generate a random prompt ID
    pub fn generate() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

/// A parameter for a prompt template
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromptParameter {
    /// Parameter name
    pub name: String,
    /// Parameter description
    pub description: String,
    /// Whether the parameter is required
    pub required: bool,
    /// Default value, if any
    pub default_value: Option<String>,
    /// Possible enum values, if restricted
    pub enum_values: Option<Vec<String>>,
}

impl PromptParameter {
    /// Create a new required parameter
    pub fn required(name: &str, description: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            required: true,
            default_value: None,
            enum_values: None,
        }
    }

    /// Create a new optional parameter with a default value
    pub fn optional(name: &str, description: &str, default_value: &str) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            required: false,
            default_value: Some(default_value.to_string()),
            enum_values: None,
        }
    }

    /// Create a new enum parameter with allowed values
    pub fn enumeration(name: &str, description: &str, values: Vec<&str>) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            required: true,
            default_value: values.first().map(|v| v.to_string()),
            enum_values: Some(values.into_iter().map(|v| v.to_string()).collect()),
        }
    }
}

/// The version of a prompt template
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PromptVersion(pub String);

impl PromptVersion {
    /// Create a new prompt version
    pub fn new(version: &str) -> Self {
        Self(version.to_string())
    }

    /// Get the latest version
    pub fn latest() -> Self {
        Self("latest".to_string())
    }
}

/// Category of a prompt template
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PromptCategory(pub String);

impl PromptCategory {
    /// Create a new prompt category
    pub fn new(category: &str) -> Self {
        Self(category.to_string())
    }
}

/// A set of parameters for rendering a prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptParameters(pub HashMap<String, String>);

impl PromptParameters {
    /// Create a new empty set of parameters
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Create from a list of key-value pairs
    pub fn from_list(params: &[(&str, &str)]) -> Self {
        let mut map = HashMap::new();
        for (key, value) in params {
            map.insert(key.to_string(), value.to_string());
        }
        Self(map)
    }

    /// Add a parameter
    pub fn add(&mut self, key: &str, value: &str) -> &mut Self {
        self.0.insert(key.to_string(), value.to_string());
        self
    }

    /// Get a parameter value
    pub fn get(&self, key: &str) -> Option<&String> {
        self.0.get(key)
    }
}

impl Default for PromptParameters {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a rendered prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedPrompt {
    /// The prompt ID
    pub id: PromptId,
    /// The prompt text
    pub text: String,
    /// The parameters used
    pub parameters: PromptParameters,
    /// The version used
    pub version: PromptVersion,
}

impl RenderedPrompt {
    /// Create a new rendered prompt
    pub fn new(
        id: PromptId,
        text: String,
        parameters: PromptParameters,
        version: PromptVersion,
    ) -> Self {
        Self {
            id,
            text,
            parameters,
            version,
        }
    }
}
