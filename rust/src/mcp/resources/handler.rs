use async_trait::async_trait;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::mcp::jsonrpc::JsonRpcMethod;
use crate::mcp::resources::error::ResourceError;
use crate::mcp::resources::models::*;
use crate::mcp::resources::provider::ResourceProvider;
use crate::mcp::types::{JsonRpcError, JsonRpcRequest, JsonRpcResponse};

/// Represents the capabilities of the resources system
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct ResourcesCapabilities {
    /// Whether subscriptions to individual resources are supported
    pub subscribe: bool,
    /// Whether notifications of list changes are supported
    pub list_changed: bool,
}

/// Handles resources-related JSON-RPC methods
#[derive(Debug)]
pub struct ResourcesHandler {
    /// Collection of resource providers
    providers: Arc<RwLock<Vec<Arc<dyn ResourceProvider>>>>,
    /// Subscriptions by client ID and resource URI
    subscriptions: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    /// Resource subscribers by resource URI
    subscribers: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    /// Capabilities
    capabilities: ResourcesCapabilities,
}

impl ResourcesHandler {
    /// Creates a new resources handler
    pub fn new(providers: Arc<RwLock<Vec<Arc<dyn ResourceProvider>>>>) -> Self {
        Self {
            providers,
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            subscribers: Arc::new(RwLock::new(HashMap::new())),
            capabilities: ResourcesCapabilities {
                subscribe: true,
                list_changed: true,
            },
        }
    }

    /// Returns the resources capabilities
    pub fn capabilities(&self) -> ResourcesCapabilities {
        self.capabilities
    }

    /// Sets the resources capabilities
    pub fn set_capabilities(&mut self, capabilities: ResourcesCapabilities) {
        self.capabilities = capabilities;
    }

    /// Lists all available resources
    async fn list_resources(
        &self,
        params: ResourceListParams,
    ) -> Result<ResourceListResponse, ResourceError> {
        let providers = self.providers.read().await;
        let mut resources = Vec::new();
        let mut next_cursor = None;

        for provider in providers.iter() {
            let provider_resources = provider.list_resources(params.cursor.clone()).await?;

            if let Some(cursor) = provider_resources.next_cursor {
                next_cursor = Some(cursor);
            }

            resources.extend(provider_resources.resources);
        }

        Ok(ResourceListResponse {
            resources,
            next_cursor,
        })
    }

    /// Reads a resource by URI
    async fn read_resource(
        &self,
        params: ResourceReadParams,
    ) -> Result<ResourceReadResponse, ResourceError> {
        let providers = self.providers.read().await;

        for provider in providers.iter() {
            if provider.can_handle_uri(&params.uri) {
                return provider.read_resource(&params.uri).await;
            }
        }

        Err(ResourceError::ResourceNotFound(params.uri))
    }

    /// Subscribes a client to a resource
    async fn subscribe_resource(
        &self,
        client_id: &str,
        params: ResourceSubscribeParams,
    ) -> Result<(), ResourceError> {
        if !self.capabilities.subscribe {
            return Err(ResourceError::CapabilityNotSupported(
                "subscribe".to_string(),
            ));
        }

        let mut subscriptions = self.subscriptions.write().await;
        let client_subscriptions = subscriptions
            .entry(client_id.to_string())
            .or_insert_with(HashSet::new);
        client_subscriptions.insert(params.uri.clone());

        Ok(())
    }

    /// Unsubscribes a client from a resource
    async fn unsubscribe_resource(
        &self,
        client_id: &str,
        params: ResourceUnsubscribeParams,
    ) -> Result<(), ResourceError> {
        if !self.capabilities.subscribe {
            return Err(ResourceError::CapabilityNotSupported(
                "subscribe".to_string(),
            ));
        }

        let mut subscriptions = self.subscriptions.write().await;
        if let Some(client_subscriptions) = subscriptions.get_mut(client_id) {
            client_subscriptions.remove(&params.uri);
        }

        Ok(())
    }

    /// Creates a notification for resource updates
    pub fn create_resource_updated_notification(&self, uri: &str) -> serde_json::Value {
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/resources/updated",
            "params": ResourceUpdatedParams {
                uri: uri.to_string(),
            }
        })
    }

    /// Creates a notification for resource list changes
    pub fn create_list_changed_notification(&self) -> serde_json::Value {
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/resources/list_changed"
        })
    }

    /// Returns the client IDs that are subscribed to a resource
    pub async fn get_subscribers_for_resource(&self, uri: &str) -> Vec<String> {
        let subscriptions = self.subscriptions.read().await;
        let mut subscribers = Vec::new();

        for (client_id, uris) in subscriptions.iter() {
            if uris.contains(uri) {
                subscribers.push(client_id.clone());
            }
        }

        subscribers
    }
}

#[async_trait]
impl JsonRpcMethod for ResourcesHandler {
    async fn handle(&self, request: JsonRpcRequest) -> Result<JsonRpcResponse, JsonRpcError> {
        match request.method.as_str() {
            "resources/list" => {
                let params_value = request.params.unwrap_or(json!({}));
                let params: ResourceListParams =
                    serde_json::from_value(params_value).map_err(|e| {
                        JsonRpcError::invalid_params(&format!("Invalid parameters: {}", e))
                    })?;

                match self.list_resources(params).await {
                    Ok(result) => Ok(JsonRpcResponse::success(json!(result), request.id)),
                    Err(e) => Err(e.into()),
                }
            }
            "resources/read" => {
                let params_value = request.params.unwrap_or_else(|| json!({}));
                let params: ResourceReadParams =
                    serde_json::from_value(params_value).map_err(|e| {
                        JsonRpcError::invalid_params(&format!("Invalid parameters: {}", e))
                    })?;

                match self.read_resource(params).await {
                    Ok(result) => Ok(JsonRpcResponse::success(json!(result), request.id)),
                    Err(e) => Err(e.into()),
                }
            }
            "resources/subscribe" => {
                if !self.capabilities.subscribe {
                    return Err(JsonRpcError::method_not_found("Method not supported"));
                }

                let params_value = request.params.unwrap_or_else(|| json!({}));
                let params: ResourceSubscribeParams = serde_json::from_value(params_value)
                    .map_err(|e| {
                        JsonRpcError::invalid_params(&format!("Invalid parameters: {}", e))
                    })?;

                // Extract client ID from the request context if available
                let client_id = request.client_id.unwrap_or_else(|| "anonymous".to_string());

                match self.subscribe_resource(&client_id, params).await {
                    Ok(_) => Ok(JsonRpcResponse::success(json!(true), request.id)),
                    Err(e) => Err(e.into()),
                }
            }
            "resources/unsubscribe" => {
                if !self.capabilities.subscribe {
                    return Err(JsonRpcError::method_not_found("Method not supported"));
                }

                let params_value = request.params.unwrap_or_else(|| json!({}));
                let params: ResourceUnsubscribeParams = serde_json::from_value(params_value)
                    .map_err(|e| {
                        JsonRpcError::invalid_params(&format!("Invalid parameters: {}", e))
                    })?;

                // Extract client ID from the request context if available
                let client_id = request.client_id.unwrap_or_else(|| "anonymous".to_string());

                match self.unsubscribe_resource(&client_id, params).await {
                    Ok(_) => Ok(JsonRpcResponse::success(json!(true), request.id)),
                    Err(e) => Err(e.into()),
                }
            }
            _ => Err(JsonRpcError::method_not_found("Method not found")),
        }
    }
}

#[cfg(test)]
mod tests {

    // Tests for the resources handler
}
