//! Schema validation for MCP protocol messages
//!
//! This module provides validation of MCP protocol messages against the
//! official JSON Schema. It ensures that all messages conform to the MCP specification.

use crate::utils::error::{McpError, McpResult};
use jsonschema::{Draft, JSONSchema};
use serde_json::Value;
use std::sync::Arc;
use tracing::{debug, error, warn};

/// Schema validator for MCP protocol messages
#[derive(Debug, Clone)]
pub struct SchemaValidator {
    /// The compiled JSON schema for general MCP messages
    message_schema: Arc<JSONSchema>,
    /// The compiled JSON schema for JSON-RPC requests
    request_schema: Arc<JSONSchema>,
    /// The compiled JSON schema for JSON-RPC responses
    response_schema: Arc<JSONSchema>,
    /// The compiled JSON schema for JSON-RPC notifications
    notification_schema: Arc<JSONSchema>,
    /// The compiled JSON schema for JSON-RPC errors
    error_schema: Arc<JSONSchema>,
}

impl Default for SchemaValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemaValidator {
    /// Creates a new schema validator with compiled schemas
    pub fn new() -> Self {
        // These are the core schema definitions inline for simplicity
        // In a production environment, these should be loaded from files
        let message_schema_json = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": "MCP Message Schema",
            "type": "object",
            "required": ["jsonrpc"],
            "properties": {
                "jsonrpc": {
                    "type": "string",
                    "enum": ["2.0"]
                }
            }
        }"#;

        let request_schema_json = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": "JSON-RPC Request Schema",
            "type": "object",
            "required": ["jsonrpc", "method", "id"],
            "properties": {
                "jsonrpc": {
                    "type": "string",
                    "enum": ["2.0"]
                },
                "method": {
                    "type": "string"
                },
                "params": {
                    "oneOf": [
                        { "type": "object" },
                        { "type": "array" },
                        { "type": "null" }
                    ]
                },
                "id": {
                    "oneOf": [
                        { "type": "string" },
                        { "type": "number" },
                        { "type": "null" }
                    ]
                }
            }
        }"#;

        let response_schema_json = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": "JSON-RPC Response Schema",
            "type": "object",
            "required": ["jsonrpc", "id"],
            "oneOf": [
                {
                    "required": ["result"],
                    "properties": {
                        "jsonrpc": {"type": "string", "enum": ["2.0"]},
                        "result": {},
                        "id": {
                            "oneOf": [
                                {"type": "string"},
                                {"type": "number"},
                                {"type": "null"}
                            ]
                        }
                    },
                    "not": {"required": ["error"]}
                },
                {
                    "required": ["error"],
                    "properties": {
                        "jsonrpc": {"type": "string", "enum": ["2.0"]},
                        "error": {
                            "type": "object",
                            "required": ["code", "message"],
                            "properties": {
                                "code": {"type": "integer"},
                                "message": {"type": "string"},
                                "data": {}
                            }
                        },
                        "id": {
                            "oneOf": [
                                {"type": "string"},
                                {"type": "number"},
                                {"type": "null"}
                            ]
                        }
                    },
                    "not": {"required": ["result"]}
                }
            ]
        }"#;

        let notification_schema_json = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": "JSON-RPC Notification Schema",
            "type": "object",
            "required": ["jsonrpc", "method"],
            "properties": {
                "jsonrpc": {
                    "type": "string",
                    "enum": ["2.0"]
                },
                "method": {
                    "type": "string"
                },
                "params": {
                    "oneOf": [
                        { "type": "object" },
                        { "type": "array" },
                        { "type": "null" }
                    ]
                }
            },
            "not": {"required": ["id"]}
        }"#;

        let error_schema_json = r#"{
            "$schema": "http://json-schema.org/draft-07/schema#",
            "title": "JSON-RPC Error Schema",
            "type": "object",
            "required": ["code", "message"],
            "properties": {
                "code": {"type": "integer"},
                "message": {"type": "string"},
                "data": {}
            }
        }"#;

        // Compile the schemas
        let compile_schema = |schema_str: &str| -> Arc<JSONSchema> {
            let schema: Value = serde_json::from_str(schema_str).expect("Invalid schema JSON");
            let compiled = JSONSchema::options()
                .with_draft(Draft::Draft7)
                .compile(&schema)
                .expect("Failed to compile schema");
            Arc::new(compiled)
        };

        Self {
            message_schema: compile_schema(message_schema_json),
            request_schema: compile_schema(request_schema_json),
            response_schema: compile_schema(response_schema_json),
            notification_schema: compile_schema(notification_schema_json),
            error_schema: compile_schema(error_schema_json),
        }
    }

    /// Validates a generic JSON-RPC message
    pub fn validate_message(&self, message: &Value) -> McpResult<()> {
        match self.message_schema.validate(message) {
            Ok(_) => Ok(()),
            Err(errors) => {
                let error_messages: Vec<String> = errors
                    .map(|e| format!("{} at {}", e, e.instance_path))
                    .collect();
                let message = format!("Invalid JSON-RPC message: {}", error_messages.join(", "));
                error!("{}", message);
                Err(McpError::ValidationError(message))
            }
        }
    }

    /// Validates a JSON-RPC request
    pub fn validate_request(&self, request: &Value) -> McpResult<()> {
        // First validate as a message
        self.validate_message(request)?;

        // Then validate as a request
        match self.request_schema.validate(request) {
            Ok(_) => {
                debug!("Request validation passed");
                Ok(())
            }
            Err(errors) => {
                let error_messages: Vec<String> = errors
                    .map(|e| format!("{} at {}", e, e.instance_path))
                    .collect();
                let message = format!("Invalid JSON-RPC request: {}", error_messages.join(", "));
                warn!("{}", message);
                Err(McpError::ValidationError(message))
            }
        }
    }

    /// Validates a JSON-RPC response
    pub fn validate_response(&self, response: &Value) -> McpResult<()> {
        // First validate as a message
        self.validate_message(response)?;

        // Then validate as a response
        match self.response_schema.validate(response) {
            Ok(_) => {
                debug!("Response validation passed");
                Ok(())
            }
            Err(errors) => {
                let error_messages: Vec<String> = errors
                    .map(|e| format!("{} at {}", e, e.instance_path))
                    .collect();
                let message = format!("Invalid JSON-RPC response: {}", error_messages.join(", "));
                warn!("{}", message);
                Err(McpError::ValidationError(message))
            }
        }
    }

    /// Validates a JSON-RPC notification
    pub fn validate_notification(&self, notification: &Value) -> McpResult<()> {
        // First validate as a message
        self.validate_message(notification)?;

        // Then validate as a notification
        match self.notification_schema.validate(notification) {
            Ok(_) => {
                debug!("Notification validation passed");
                Ok(())
            }
            Err(errors) => {
                let error_messages: Vec<String> = errors
                    .map(|e| format!("{} at {}", e, e.instance_path))
                    .collect();
                let message = format!(
                    "Invalid JSON-RPC notification: {}",
                    error_messages.join(", ")
                );
                warn!("{}", message);
                Err(McpError::ValidationError(message))
            }
        }
    }

    /// Validates a JSON-RPC error object
    pub fn validate_error(&self, error: &Value) -> McpResult<()> {
        match self.error_schema.validate(error) {
            Ok(_) => {
                debug!("Error validation passed");
                Ok(())
            }
            Err(errors) => {
                let error_messages: Vec<String> = errors
                    .map(|e| format!("{} at {}", e, e.instance_path))
                    .collect();
                let message = format!("Invalid JSON-RPC error: {}", error_messages.join(", "));
                warn!("{}", message);
                Err(McpError::ValidationError(message))
            }
        }
    }

    /// Validates a batch of JSON-RPC requests
    pub fn validate_batch(&self, batch: &Value) -> McpResult<()> {
        if !batch.is_array() {
            return Err(McpError::ValidationError(
                "Batch must be an array".to_string(),
            ));
        }

        let batch_array = batch.as_array().unwrap();
        if batch_array.is_empty() {
            return Err(McpError::ValidationError(
                "Batch cannot be empty".to_string(),
            ));
        }

        for (i, item) in batch_array.iter().enumerate() {
            // Try to validate as request first
            let req_result = self.validate_request(item);
            if req_result.is_ok() {
                continue;
            }

            // Try to validate as notification
            let notif_result = self.validate_notification(item);
            if notif_result.is_ok() {
                continue;
            }

            // If both validations fail, return an error
            return Err(McpError::ValidationError(format!(
                "Invalid item at index {}: neither a valid request nor a notification",
                i
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_valid_request() {
        let validator = SchemaValidator::new();
        let request = json!({
            "jsonrpc": "2.0",
            "method": "test_method",
            "params": {"key": "value"},
            "id": "1"
        });

        assert!(validator.validate_request(&request).is_ok());
    }

    #[test]
    fn test_invalid_request() {
        let validator = SchemaValidator::new();
        let request = json!({
            "jsonrpc": "1.0", // Invalid version
            "method": "test_method",
            "id": "1"
        });

        assert!(validator.validate_request(&request).is_err());
    }

    #[test]
    fn test_valid_response() {
        let validator = SchemaValidator::new();
        let response = json!({
            "jsonrpc": "2.0",
            "result": {"key": "value"},
            "id": "1"
        });

        assert!(validator.validate_response(&response).is_ok());
    }

    #[test]
    fn test_valid_error_response() {
        let validator = SchemaValidator::new();
        let response = json!({
            "jsonrpc": "2.0",
            "error": {
                "code": -32600,
                "message": "Invalid Request"
            },
            "id": "1"
        });

        assert!(validator.validate_response(&response).is_ok());
    }

    #[test]
    fn test_invalid_response() {
        let validator = SchemaValidator::new();
        let response = json!({
            "jsonrpc": "2.0",
            "result": {"key": "value"},
            "error": { // Can't have both result and error
                "code": -32600,
                "message": "Invalid Request"
            },
            "id": "1"
        });

        assert!(validator.validate_response(&response).is_err());
    }

    #[test]
    fn test_valid_notification() {
        let validator = SchemaValidator::new();
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "test_notification",
            "params": {"key": "value"}
        });

        assert!(validator.validate_notification(&notification).is_ok());
    }

    #[test]
    fn test_invalid_notification() {
        let validator = SchemaValidator::new();
        let notification = json!({
            "jsonrpc": "2.0",
            "method": "test_notification",
            "id": "1" // Notifications should not have an ID
        });

        assert!(validator.validate_notification(&notification).is_err());
    }

    #[test]
    fn test_valid_batch() {
        let validator = SchemaValidator::new();
        let batch = json!([
            {
                "jsonrpc": "2.0",
                "method": "test_method",
                "params": {"key": "value"},
                "id": "1"
            },
            {
                "jsonrpc": "2.0",
                "method": "test_notification",
                "params": {"key": "value"}
            }
        ]);

        assert!(validator.validate_batch(&batch).is_ok());
    }

    #[test]
    fn test_invalid_batch() {
        let validator = SchemaValidator::new();
        let batch = json!([
            {
                "jsonrpc": "2.0",
                "method": "test_method",
                "params": {"key": "value"},
                "id": "1"
            },
            {
                "jsonrpc": "1.0", // Invalid version
                "method": "test_notification"
            }
        ]);

        assert!(validator.validate_batch(&batch).is_err());
    }

    #[test]
    fn test_empty_batch() {
        let validator = SchemaValidator::new();
        let batch = json!([]);

        assert!(validator.validate_batch(&batch).is_err());
    }
}
