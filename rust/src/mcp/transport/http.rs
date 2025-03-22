//! HTTP transport implementation for the MCP protocol.
//!
//! This module provides an HTTP-based transport for the MCP protocol using
//! the reqwest library for HTTP support.

#[cfg(feature = "transport-http")]
use log::{debug, error};
#[cfg(feature = "transport-http")]
use reqwest::{Client, StatusCode};
#[cfg(feature = "transport-http")]
use std::time::Duration;
#[cfg(feature = "transport-http")]
use url::Url;

use super::{Transport, TransportConfig, TransportFactory};
use crate::error::Error;
#[cfg(feature = "transport-http")]
use crate::mcp::types::{JsonRpcRequest as Request, JsonRpcResponse as Response, Message};

/// HTTP transport for MCP protocol
#[cfg(feature = "transport-http")]
pub struct HttpTransport {
    /// The HTTP client
    client: Client,
    /// The base URL for the API
    url: Url,
    /// Configuration
    config: TransportConfig,
}

#[cfg(feature = "transport-http")]
impl HttpTransport {
    /// Create a new HTTP transport
    pub fn new(config: TransportConfig) -> Result<Self, Error> {
        // Parse base URL
        let url = Url::parse(&config.url)
            .map_err(|_| Error::Internal(format!("Invalid HTTP URL: {}", config.url)))?;

        // Create HTTP client with timeout
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| Error::Internal(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            url,
            config,
        })
    }
}

#[cfg(feature = "transport-http")]
#[async_trait]
impl Transport for HttpTransport {
    async fn send_message(&self, message: Message) -> Result<(), Error> {
        // Build URL for message endpoint
        let url = self
            .url
            .join("/message")
            .map_err(|e| Error::Internal(format!("Failed to build message URL: {}", e)))?;

        debug!("Sending message to {}", url);

        // Convert to JSON and send
        let response = self
            .client
            .post(url)
            .json(&message)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("HTTP request failed: {}", e)))?;

        // Check response status
        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::Internal(format!(
                "HTTP request failed with status {}: {}",
                status, error_text
            )));
        }

        debug!("Message sent successfully");
        Ok(())
    }

    async fn send_request(&self, request: Request) -> Result<Response, Error> {
        // Build URL for request endpoint
        let url = self
            .url
            .join("/request")
            .map_err(|e| Error::Internal(format!("Failed to build request URL: {}", e)))?;

        debug!("Sending request to {}", url);

        // Convert Request to Message
        let message = request.to_message()?;

        // Send the request
        let response = self
            .client
            .post(url)
            .json(&message)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("HTTP request failed: {}", e)))?;

        // Check response status
        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(Error::Internal(format!(
                "HTTP request failed with status {}: {}",
                status, error_text
            )));
        }

        // Parse response
        let response_message: Message = response
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Failed to parse response JSON: {}", e)))?;

        // Convert Message to Response
        let mcp_response = Response::from_message(&response_message).map_err(|e| {
            Error::Internal(format!("Failed to convert message to response: {}", e))
        })?;

        debug!("Request completed successfully");
        Ok(mcp_response)
    }

    async fn close(&self) -> Result<(), Error> {
        // HTTP is stateless, so there's nothing to close
        Ok(())
    }

    fn is_connected(&self) -> bool {
        // HTTP is stateless, so we're always "connected"
        true
    }
}

/// Factory for creating HTTP transports
#[cfg(feature = "transport-http")]
#[derive(Debug)]
pub struct HttpTransportFactory {
    config: TransportConfig,
}

#[cfg(feature = "transport-http")]
impl HttpTransportFactory {
    /// Create a new HTTP transport factory
    ///
    /// # Arguments
    /// * `config` - The transport configuration to use for created clients.
    pub fn new(config: TransportConfig) -> Self {
        Self { config }
    }
}

#[cfg(feature = "transport-http")]
impl TransportFactory for HttpTransportFactory {
    fn create(&self) -> Result<Box<dyn Transport>, Error> {
        let transport = HttpTransport::new(self.config.clone())?;
        Ok(Box::new(transport))
    }
}

/// Empty implementation when the transport-http feature is not enabled
#[cfg(not(feature = "transport-http"))]
#[derive(Debug)]
pub struct HttpTransportFactory {
    _config: TransportConfig,
}

#[cfg(not(feature = "transport-http"))]
impl HttpTransportFactory {
    /// Creates a placeholder HTTP transport factory when the feature is disabled
    ///
    /// # Arguments
    /// * `config` - The transport configuration (not used when feature is disabled)
    pub fn new(config: TransportConfig) -> Self {
        Self { _config: config }
    }
}

#[cfg(not(feature = "transport-http"))]
impl TransportFactory for HttpTransportFactory {
    fn create(&self) -> Result<Box<dyn Transport>, Error> {
        Err(Error::Internal(
            "HTTP transport is not enabled. Enable the 'transport-http' feature.".to_string(),
        ))
    }
}
