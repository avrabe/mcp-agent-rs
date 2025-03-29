use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A template for parameterized resources
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceTemplate {
    /// URI template with placeholders
    pub uri_template: String,
    /// Human-readable name
    pub name: String,
    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Expected MIME type of resources
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    /// Parameters that can be used in the template
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<TemplateParameter>,
}

impl ResourceTemplate {
    /// Creates a new resource template
    pub fn new(uri_template: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            uri_template: uri_template.into(),
            name: name.into(),
            description: None,
            mime_type: None,
            parameters: Vec::new(),
        }
    }

    /// Adds a description to the template
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the MIME type for the template
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    /// Adds a parameter to the template
    pub fn with_parameter(mut self, parameter: TemplateParameter) -> Self {
        self.parameters.push(parameter);
        self
    }

    /// Applies parameters to the template to create a concrete URI
    pub fn apply(&self, params: &HashMap<String, String>) -> String {
        let mut result = self.uri_template.clone();

        for (name, value) in params {
            let placeholder = format!("{{{}}}", name);
            result = result.replace(&placeholder, value);
        }

        result
    }

    /// Validates parameters against the template
    pub fn validate_params(&self, params: &HashMap<String, String>) -> Result<(), String> {
        // Check for required parameters
        for param in &self.parameters {
            if param.required && !params.contains_key(&param.name) {
                return Err(format!("Missing required parameter: {}", param.name));
            }
        }

        // Check that all provided parameters are defined in the template
        for name in params.keys() {
            if !self.parameters.iter().any(|p| &p.name == name) {
                return Err(format!("Unknown parameter: {}", name));
            }
        }

        Ok(())
    }
}

/// A parameter for a resource template
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateParameter {
    /// Parameter name
    pub name: String,
    /// Human-readable display name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Description of the parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the parameter is required
    #[serde(default)]
    pub required: bool,
    /// Default value for the parameter
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    /// Example values for auto-completion
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub examples: Vec<String>,
}

impl TemplateParameter {
    /// Creates a new template parameter
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            display_name: None,
            description: None,
            required: false,
            default_value: None,
            examples: Vec::new(),
        }
    }

    /// Sets the display name for the parameter
    pub fn with_display_name(mut self, display_name: impl Into<String>) -> Self {
        self.display_name = Some(display_name.into());
        self
    }

    /// Sets the description for the parameter
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets whether the parameter is required
    pub fn with_required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    /// Sets the default value for the parameter
    pub fn with_default(mut self, default_value: impl Into<String>) -> Self {
        self.default_value = Some(default_value.into());
        self
    }

    /// Adds an example value for the parameter
    pub fn with_example(mut self, example: impl Into<String>) -> Self {
        self.examples.push(example.into());
        self
    }
}

/// Response for listing resource templates
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceTemplateListResponse {
    /// Available resource templates
    pub resource_templates: Vec<ResourceTemplate>,
}
