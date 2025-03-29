use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::types::{
    PromptCategory, PromptId, PromptParameter, PromptParameters, PromptVersion, RenderedPrompt,
};
use crate::utils::error::{McpError, McpResult};

/// A template for generating prompts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    /// Unique identifier for the template
    pub id: PromptId,
    /// Human-readable name
    pub name: String,
    /// Template description
    pub description: String,
    /// The actual template text with parameter placeholders
    pub template: String,
    /// List of parameters accepted by this template
    pub parameters: Vec<PromptParameter>,
    /// Template version
    pub version: PromptVersion,
    /// Template category
    pub category: PromptCategory,
    /// When the template was created
    pub created_at: DateTime<Utc>,
    /// When the template was last updated
    pub updated_at: DateTime<Utc>,
}

impl PromptTemplate {
    /// Create a new prompt template with minimal information
    pub fn new(id: &str, template: &str) -> Self {
        let now = Utc::now();
        Self {
            id: PromptId::new(id),
            name: id.to_string(),
            description: String::new(),
            template: template.to_string(),
            parameters: Vec::new(),
            version: PromptVersion::latest(),
            category: PromptCategory::new("general"),
            created_at: now,
            updated_at: now,
        }
    }

    /// Create a new prompt template with all properties
    pub fn with_details(
        id: &str,
        name: &str,
        description: &str,
        template: &str,
        parameters: Vec<PromptParameter>,
        version: &str,
        category: &str,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: PromptId::new(id),
            name: name.to_string(),
            description: description.to_string(),
            template: template.to_string(),
            parameters,
            version: PromptVersion::new(version),
            category: PromptCategory::new(category),
            created_at: now,
            updated_at: now,
        }
    }

    /// Add a parameter to the template
    pub fn add_parameter(&mut self, parameter: PromptParameter) -> &mut Self {
        self.parameters.push(parameter);
        self.updated_at = Utc::now();
        self
    }

    /// Set the template text
    pub fn set_template(&mut self, template: &str) -> &mut Self {
        self.template = template.to_string();
        self.updated_at = Utc::now();
        self
    }

    /// Set the template description
    pub fn set_description(&mut self, description: &str) -> &mut Self {
        self.description = description.to_string();
        self.updated_at = Utc::now();
        self
    }

    /// Set the template category
    pub fn set_category(&mut self, category: &str) -> &mut Self {
        self.category = PromptCategory::new(category);
        self.updated_at = Utc::now();
        self
    }

    /// Set the template version
    pub fn set_version(&mut self, version: &str) -> &mut Self {
        self.version = PromptVersion::new(version);
        self.updated_at = Utc::now();
        self
    }

    /// Extract parameter names from the template text
    pub fn extract_parameters(&self) -> Vec<String> {
        let mut params = Vec::new();
        let mut template = self.template.as_str();

        while let Some(start) = template.find("{{") {
            if let Some(end) = template[start..].find("}}") {
                let param_name = template[start + 2..start + end].trim();
                params.push(param_name.to_string());
                template = &template[start + end + 2..];
            } else {
                break;
            }
        }

        params
    }

    /// Validate parameters against the template
    pub fn validate_parameters(&self, parameters: &PromptParameters) -> McpResult<()> {
        // Check required parameters
        for param in &self.parameters {
            if param.required
                && !parameters.0.contains_key(&param.name)
                && param.default_value.is_none()
            {
                return Err(McpError::ValidationError(format!(
                    "Missing required parameter: {}",
                    param.name
                )));
            }
        }

        // Check enum values
        for param in &self.parameters {
            if let Some(value) = parameters.0.get(&param.name) {
                if let Some(enum_values) = &param.enum_values {
                    if !enum_values.contains(value) {
                        return Err(McpError::ValidationError(format!(
                            "Invalid value for parameter {}: {}. Expected one of: {:?}",
                            param.name, value, enum_values
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// Render the template with the provided parameters
    pub fn render(&self, parameters: &PromptParameters) -> McpResult<RenderedPrompt> {
        // Validate parameters
        self.validate_parameters(parameters)?;

        // Create a map with default values
        let mut param_values = HashMap::new();
        for param in &self.parameters {
            if let Some(default) = &param.default_value {
                param_values.insert(param.name.clone(), default.clone());
            }
        }

        // Override with provided parameters
        for (key, value) in &parameters.0 {
            param_values.insert(key.clone(), value.clone());
        }

        // Render the template
        let mut result = self.template.clone();
        for (key, value) in &param_values {
            let placeholder = format!("{{{{{}}}}}", key);
            result = result.replace(&placeholder, value);
        }

        Ok(RenderedPrompt::new(
            self.id.clone(),
            result,
            parameters.clone(),
            self.version.clone(),
        ))
    }
}
