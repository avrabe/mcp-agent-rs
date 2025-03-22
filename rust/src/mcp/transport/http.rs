//! HTTP transport implementation for the MCP protocol.
//!
//! This module provides an HTTP-based transport for the MCP protocol using
//! the reqwest library for HTTP support.

#[cfg(feature = "transport-http")]
use reqwest::{Client, StatusCode};
#[cfg(feature = "transport-http")]
use tracing::{debug, error};
#[cfg(feature = "transport-http")]
use std::time::Duration;
#[cfg(feature = "transport-http")]
use url::Url;
#[cfg(feature = "transport-http")]
use async_trait::async_trait;

use super::{Transport, TransportConfig, TransportFactory};
use crate::error::{Error, Result};
use std::sync::Arc;

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
impl AsyncTransport for HttpTransport {
    async fn send_message(&self, message: Message) -> Result<()> {
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
            let error_msg = format!("HTTP error: {}", status);
            error!("{}", error_msg);
            return Err(Error::Protocol(error_msg));
        }

        Ok(())
    }

    async fn send_request(&self, request: Request) -> Result<Response> {
        // Convert request to Message
        let message = request.clone().into_message();

        // Build URL for request endpoint
        let url = self
            .url
            .join("/request")
            .map_err(|e| Error::Internal(format!("Failed to build request URL: {}", e)))?;

        debug!("Sending request to {}", url);

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
            let error_msg = format!("HTTP error: {}", status);
            error!("{}", error_msg);
            return Err(Error::Protocol(error_msg));
        }

        // Parse response
        let response_bytes = response
            .bytes()
            .await
            .map_err(|e| Error::Internal(format!("Failed to read response: {}", e)))?;

        let response_message: Message = serde_json::from_slice(&response_bytes)
            .map_err(|e| Error::Internal(format!("Failed to parse response: {}", e)))?;

        let mcp_response = Response::from_bytes(&response_bytes).map_err(|e| {
            Error::Internal(format!("Failed to convert message to response: {}", e))
        })?;

        Ok(mcp_response)
    }

    async fn send_batch_request(&self, batch_request: BatchRequest) -> Result<BatchResponse> {
        // Build URL for batch request endpoint
        let url = self
            .url
            .join("/batch")
            .map_err(|e| Error::Internal(format!("Failed to build batch URL: {}", e)))?;

        debug!("Sending batch request to {}", url);

        // Convert to JSON and send
        let response = self
            .client
            .post(url)
            .json(&batch_request)
            .send()
            .await
            .map_err(|e| Error::Internal(format!("HTTP request failed: {}", e)))?;

        // Check response status
        let status = response.status();
        if !status.is_success() {
            let error_msg = format!("HTTP error: {}", status);
            error!("{}", error_msg);
            return Err(Error::Protocol(error_msg));
        }

        // Parse response
        let response_bytes = response
            .bytes()
            .await
            .map_err(|e| Error::Internal(format!("Failed to read response: {}", e)))?;

        let batch_response: BatchResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| Error::Internal(format!("Failed to parse batch response: {}", e)))?;

        Ok(batch_response)
    }

    async fn close(&self) -> Result<()> {
        // HTTP doesn't have a persistent connection to close
        Ok(())
    }
}

#[cfg(feature = "transport-http")]
impl Transport for HttpTransport {
    fn send_message_boxed(
        &self,
        message: Message,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        Box::pin(self.send_message(message))
    }

    fn send_request_boxed(
        &self,
        request: Request,
    ) -> Box<dyn std::future::Future<Output = Result<Response>> + Send + Unpin + '_> {
        Box::pin(self.send_request(request))
    }

    fn send_batch_request_boxed(
        &self,
        batch_request: BatchRequest,
    ) -> Box<dyn std::future::Future<Output = Result<BatchResponse>> + Send + Unpin + '_> {
        Box::pin(self.send_batch_request(batch_request))
    }

    fn close_boxed(&self) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        Box::pin(self.close())
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
    fn create(&self) -> Result<Arc<dyn Transport>> {
        let transport = HttpTransport::new(self.config.clone())?;
        Ok(Arc::new(transport))
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
    fn create(&self) -> Result<Arc<dyn Transport>> {
        Err(Error::Internal(
            "HTTP transport is not enabled. Enable the 'transport-http' feature.".to_string(),
        ))
    }
}
