//! Authentication and authorization for MCP protocol
//!
//! This module implements the authorization framework specified in the MCP protocol
//! for HTTP-based implementations. It handles authentication, token management,
//! and authorization flows.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::utils::error::{McpError, McpResult};

/// Supported authentication methods
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthMethod {
    /// Bearer token authentication
    Bearer,
    /// API key authentication
    ApiKey,
    /// OAuth2 authentication
    OAuth2,
    /// Custom authentication method
    Custom(String),
}

impl std::fmt::Display for AuthMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bearer => write!(f, "Bearer"),
            Self::ApiKey => write!(f, "ApiKey"),
            Self::OAuth2 => write!(f, "OAuth2"),
            Self::Custom(name) => write!(f, "{}", name),
        }
    }
}

/// Token type for authentication
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthToken {
    /// The token value
    pub token: String,
    /// The type of token
    pub token_type: AuthMethod,
    /// When the token expires, in seconds since UNIX epoch
    pub expires_at: Option<u64>,
}

impl AuthToken {
    /// Creates a new authentication token
    pub fn new(token: String, token_type: AuthMethod, expires_in: Option<Duration>) -> Self {
        let expires_at = expires_in.map(|duration| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + duration.as_secs()
        });

        Self {
            token,
            token_type,
            expires_at,
        }
    }

    /// Creates a new bearer token
    pub fn bearer(token: String, expires_in: Option<Duration>) -> Self {
        Self::new(token, AuthMethod::Bearer, expires_in)
    }

    /// Creates a new API key token
    pub fn api_key(key: String) -> Self {
        Self::new(key, AuthMethod::ApiKey, None)
    }

    /// Checks if the token is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            now >= expires_at
        } else {
            false
        }
    }

    /// Returns the token with its type for use in HTTP headers
    pub fn header_value(&self) -> String {
        format!("{} {}", self.token_type, self.token)
    }
}

/// Authentication credentials for MCP protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthCredentials {
    /// The unique ID of these credentials
    pub id: String,
    /// The authentication method
    pub method: AuthMethod,
    /// Credentials specific to the authentication method
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Username for basic authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// Password for basic authentication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// OAuth2 client ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// OAuth2 client secret
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// OAuth2 scope
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// OAuth2 token endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_endpoint: Option<String>,
}

impl AuthCredentials {
    /// Creates new API key credentials
    pub fn api_key(api_key: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            method: AuthMethod::ApiKey,
            api_key: Some(api_key),
            username: None,
            password: None,
            client_id: None,
            client_secret: None,
            scope: None,
            token_endpoint: None,
        }
    }

    /// Creates new OAuth2 credentials
    pub fn oauth2(
        client_id: String,
        client_secret: String,
        token_endpoint: String,
        scope: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            method: AuthMethod::OAuth2,
            api_key: None,
            username: None,
            password: None,
            client_id: Some(client_id),
            client_secret: Some(client_secret),
            scope,
            token_endpoint: Some(token_endpoint),
        }
    }
}

/// Authentication service for MCP protocol
#[derive(Debug, Clone)]
pub struct AuthService {
    /// Tokens indexed by credential ID
    tokens: Arc<RwLock<std::collections::HashMap<String, AuthToken>>>,
    /// Credentials indexed by credential ID
    credentials: Arc<RwLock<std::collections::HashMap<String, AuthCredentials>>>,
}

impl Default for AuthService {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthService {
    /// Creates a new authentication service
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(std::collections::HashMap::new())),
            credentials: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Adds credentials to the service
    pub async fn add_credentials(&self, credentials: AuthCredentials) -> McpResult<()> {
        let mut creds = self.credentials.write().await;
        creds.insert(credentials.id.clone(), credentials);
        Ok(())
    }

    /// Gets credentials by ID
    pub async fn get_credentials(&self, id: &str) -> McpResult<AuthCredentials> {
        let creds = self.credentials.read().await;
        creds
            .get(id)
            .cloned()
            .ok_or_else(|| McpError::AuthenticationError(format!("Credentials not found: {}", id)))
    }

    /// Gets a token for credentials
    pub async fn get_token(&self, credential_id: &str) -> McpResult<AuthToken> {
        // Check if we already have a valid token
        {
            let tokens = self.tokens.read().await;
            if let Some(token) = tokens.get(credential_id) {
                if !token.is_expired() {
                    debug!("Using existing token for credential_id={}", credential_id);
                    return Ok(token.clone());
                }
            }
        }

        // Get credentials to create a new token
        let credentials = self.get_credentials(credential_id).await?;

        // Create a new token based on the credential type
        let token = match credentials.method {
            AuthMethod::ApiKey => {
                let api_key = credentials.api_key.ok_or_else(|| {
                    McpError::AuthenticationError("API key not provided".to_string())
                })?;
                AuthToken::api_key(api_key)
            }
            AuthMethod::OAuth2 => {
                // In a real implementation, this would do an OAuth2 flow
                // For now, we'll just create a dummy token
                warn!("OAuth2 token generation not fully implemented");
                let token_value = Uuid::new_v4().to_string();
                AuthToken::new(
                    token_value,
                    AuthMethod::OAuth2,
                    Some(Duration::from_secs(3600)),
                )
            }
            AuthMethod::Bearer => {
                // In a real implementation, this might involve a custom token fetch
                // For now, we'll just create a dummy token
                let token_value = Uuid::new_v4().to_string();
                AuthToken::bearer(token_value, Some(Duration::from_secs(3600)))
            }
            AuthMethod::Custom(name) => {
                // Custom auth methods would need their own implementation
                return Err(McpError::AuthenticationError(format!(
                    "Custom auth method not implemented: {}",
                    name
                )));
            }
        };

        // Store and return the token
        {
            let mut tokens = self.tokens.write().await;
            tokens.insert(credential_id.to_string(), token.clone());
        }

        info!("Created new token for credential_id={}", credential_id);
        Ok(token)
    }

    /// Validates a token from an HTTP Authorization header
    pub async fn validate_token(&self, auth_header: &str) -> McpResult<()> {
        // Parse the authorization header
        let parts: Vec<&str> = auth_header.splitn(2, ' ').collect();
        if parts.len() != 2 {
            return Err(McpError::AuthenticationError(
                "Invalid Authorization header format".to_string(),
            ));
        }

        let auth_type = parts[0];
        let token_value = parts[1];

        // Check if any of our stored tokens match
        let tokens = self.tokens.read().await;
        for token in tokens.values() {
            if token.token_type.to_string() == auth_type && token.token == token_value {
                // Found a matching token, check if it's expired
                if token.is_expired() {
                    return Err(McpError::AuthenticationError("Token expired".to_string()));
                }
                return Ok(());
            }
        }

        // No matching token found
        Err(McpError::AuthenticationError("Invalid token".to_string()))
    }

    /// Revokes a token for a credential
    pub async fn revoke_token(&self, credential_id: &str) -> McpResult<()> {
        let mut tokens = self.tokens.write().await;
        tokens.remove(credential_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_api_key_auth() {
        let service = AuthService::new();

        // Add API key credentials
        let credentials = AuthCredentials::api_key("test-api-key".to_string());
        let cred_id = credentials.id.clone();
        service.add_credentials(credentials).await.unwrap();

        // Get a token
        let token = service.get_token(&cred_id).await.unwrap();

        // Verify the token
        assert_eq!(token.token_type, AuthMethod::ApiKey);
        assert_eq!(token.token, "test-api-key");
        assert!(!token.is_expired());

        // Check header value
        let header = token.header_value();
        assert_eq!(header, "ApiKey test-api-key");

        // Validate using the header
        service.validate_token(&header).await.unwrap();
    }

    #[tokio::test]
    async fn test_token_expiration() {
        let service = AuthService::new();

        // Create a bearer token that expires immediately
        let token = AuthToken::bearer("expired-token".to_string(), Some(Duration::from_secs(0)));

        // Add credentials and manually insert the token
        let credentials = AuthCredentials::api_key("test-api-key".to_string());
        let cred_id = credentials.id.clone();
        service.add_credentials(credentials).await.unwrap();

        {
            let mut tokens = service.tokens.write().await;
            tokens.insert(cred_id.clone(), token);
        }

        // Wait a moment to ensure the token expires
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Get a token - this should create a new one since the old one is expired
        let new_token = service.get_token(&cred_id).await.unwrap();
        assert_ne!(new_token.token, "expired-token");
    }
}
