/// Factory for creating HTTP transport instances.
/// This factory creates configured HTTP transport clients for connecting to MCP servers.
#[derive(Debug)]
pub struct HttpTransportFactory {
    _config: TransportConfig,
}

impl HttpTransportFactory {
    /// Creates a new HTTP transport factory with the provided configuration.
    ///
    /// # Arguments
    /// * `config` - The transport configuration to use for created clients.
    pub fn new(config: TransportConfig) -> Self {
        Self { _config: config }
    }
}

/// HTTP transport implementation for MCP protocol.
///
/// This module provides HTTP transport for the MCP protocol,
/// including authentication and proper JSON-RPC message handling.

use async_trait::async_trait;
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::mcp::auth::{AuthCredentials, AuthMethod, AuthService, AuthToken};
use crate::mcp::jsonrpc::JsonRpcHandler;
use crate::mcp::schema::SchemaValidator;
use crate::utils::error::{McpError, McpResult};

/// Configuration for HTTP transport
#[derive(Debug, Clone)]
pub struct HttpTransportConfig {
    /// Base URL for the HTTP endpoint
    pub base_url: String,
    /// Authentication method
    pub auth_method: Option<AuthMethod>,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// User agent
    pub user_agent: String,
    /// Additional headers
    pub headers: std::collections::HashMap<String, String>,
    /// Whether to validate messages against schema
    pub validate_schema: bool,
}

impl Default for HttpTransportConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            auth_method: None,
            timeout_secs: 30,
            user_agent: format!("mcp-agent-rust/{}", env!("CARGO_PKG_VERSION")),
            headers: std::collections::HashMap::new(),
            validate_schema: true,
        }
    }
}

/// HTTP transport for MCP protocol
pub struct HttpTransport {
    /// HTTP client
    client: Client,
    /// Transport configuration
    config: HttpTransportConfig,
    /// JSON-RPC handler
    handler: JsonRpcHandler,
    /// Authentication service
    auth_service: AuthService,
    /// Active credential ID
    credential_id: Option<String>,
    /// Schema validator
    validator: Option<SchemaValidator>,
}

impl HttpTransport {
    /// Creates a new HTTP transport with the given configuration
    pub fn new(config: HttpTransportConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        let validator = if config.validate_schema {
            Some(SchemaValidator::new())
        } else {
            None
        };

        Self {
            client,
            config,
            handler: JsonRpcHandler::new(),
            auth_service: AuthService::new(),
            credential_id: None,
            validator,
        }
    }

    /// Sets the credentials for authentication
    pub async fn set_credentials(&mut self, credentials: AuthCredentials) -> McpResult<()> {
        self.auth_service.add_credentials(credentials.clone()).await?;
        self.credential_id = Some(credentials.id);
        Ok(())
    }

    /// Builds a request with proper authentication
    async fn build_request(&self, method: reqwest::Method, endpoint: &str) -> McpResult<RequestBuilder> {
        let url = format!("{}/{}", self.config.base_url, endpoint);
        let mut request = self.client.request(method, &url);

        // Add common headers
        request = request.header("User-Agent", &self.config.user_agent);
        request = request.header("Content-Type", "application/json");
        request = request.header("Accept", "application/json");

        // Add custom headers
        for (name, value) in &self.config.headers {
            request = request.header(name, value);
        }

        // Add authentication if configured
        if let Some(cred_id) = &self.credential_id {
            let token = self.auth_service.get_token(cred_id).await?;
            request = request.header("Authorization", token.header_value());
        }

        Ok(request)
    }

    /// Sends a JSON-RPC request
    pub async fn send_request(&self, request: serde_json::Value) -> McpResult<serde_json::Value> {
        // Validate against schema if enabled
        if let Some(validator) = &self.validator {
            validator.validate_request(&request)?;
        }

        // Build and send the request
        let http_request = self.build_request(reqwest::Method::POST, "jsonrpc").await?
            .json(&request)
            .build()?;

        let response = self.client.execute(http_request).await?;

        // Handle HTTP errors
        if !response.status().is_success() {
            return Err(self.handle_error_response(response).await);
        }

        // Parse the response
        let response_json: serde_json::Value = response.json().await?;

        // Validate the response
        if let Some(validator) = &self.validator {
            validator.validate_response(&response_json)?;
        }

        Ok(response_json)
    }

    /// Handles error responses from the server
    async fn handle_error_response(&self, response: Response) -> McpError {
        let status = response.status();
        
        // Try to extract error details from the response body
        let error_body = match response.text().await {
            Ok(body) => body,
            Err(_) => "Unknown error".to_string(),
        };

        // Create appropriate error based on status code
        match status {
            StatusCode::UNAUTHORIZED => {
                // Authentication error
                McpError::AuthenticationError(format!("Unauthorized: {}", error_body))
            }
            StatusCode::FORBIDDEN => {
                // Authorization error
                McpError::AuthorizationError(format!("Forbidden: {}", error_body))
            }
            StatusCode::NOT_FOUND => {
                // Resource not found
                McpError::NotFound(format!("Not found: {}", error_body))
            }
            StatusCode::TOO_MANY_REQUESTS => {
                // Rate limit exceeded
                McpError::RateLimitExceeded(format!("Rate limit exceeded: {}", error_body))
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                // Server error
                McpError::ServerError(format!("Server error: {}", error_body))
            }
            _ => {
                // Generic HTTP error
                McpError::TransportError(format!("HTTP error {}: {}", status.as_u16(), error_body))
            }
        }
    }

    /// Processes a JSON-RPC message through the handler
    pub async fn process_message(&self, message: &[u8]) -> McpResult<Option<Vec<u8>>> {
        self.handler.process_json_message(message).await
    }

    /// Registers a method with the JSON-RPC handler
    pub async fn register_method<F>(&self, name: &str, handler: F) -> McpResult<()>
    where
        F: Fn(Option<serde_json::Value>) -> McpResult<serde_json::Value> + Send + Sync + 'static,
    {
        self.handler.register_method(name, handler).await
    }

    /// Registers a notification handler with the JSON-RPC handler
    pub async fn register_notification<F>(&self, name: &str, handler: F) -> McpResult<()>
    where
        F: Fn(Option<serde_json::Value>) -> McpResult<serde_json::Value> + Send + Sync + 'static,
    {
        self.handler.register_notification(name, handler).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_auth_header() {
        // Create a transport with API key auth
        let config = HttpTransportConfig {
            base_url: "http://localhost:8080".to_string(),
            auth_method: Some(AuthMethod::ApiKey),
            ..Default::default()
        };

        let mut transport = HttpTransport::new(config);
        
        // Add API key credentials
        let credentials = AuthCredentials::api_key("test-api-key".to_string());
        let cred_id = credentials.id.clone();
        transport.set_credentials(credentials).await.unwrap();
        
        // Build a request and check the auth header
        let request = transport.build_request(reqwest::Method::GET, "test").await.unwrap();
        let headers = request.build().unwrap().headers();
        
        assert!(headers.contains_key("Authorization"));
        let auth_header = headers.get("Authorization").unwrap().to_str().unwrap();
        assert_eq!(auth_header, "ApiKey test-api-key");
    }
} 