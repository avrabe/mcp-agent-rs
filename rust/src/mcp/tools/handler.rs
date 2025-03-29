use crate::mcp::jsonrpc::JsonRpcHandler;
use crate::mcp::tools::models::{Tool, ToolResult};
use crate::mcp::types::JsonRpcError;
use crate::utils::error::{McpError, McpResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::sync::Arc;

/// Request parameters for listing tools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsParams {
    /// Optional cursor for pagination
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Response for listing tools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResponse {
    /// List of available tools
    pub tools: Vec<Tool>,

    /// Optional cursor for fetching next page
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

/// Request parameters for calling a tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolParams {
    /// Name of the tool to call
    pub name: String,

    /// Arguments to pass to the tool
    pub arguments: Value,
}

/// Handler trait for tools functionality
#[async_trait]
pub trait ToolsProvider: Send + Sync {
    /// Lists available tools
    async fn list_tools(&self, cursor: Option<&str>) -> McpResult<ListToolsResponse>;

    /// Calls a tool
    async fn call_tool(&self, name: &str, arguments: &Value) -> McpResult<ToolResult>;
}

/// Handler capabilities for tools
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsCapabilities {
    /// Whether the server supports notifying clients when the tool list changes
    #[serde(default)]
    pub list_changed: bool,
}

/// Handler for tools requests
pub struct ToolsHandler {
    /// Provider for tools functionality
    provider: Arc<dyn ToolsProvider>,

    /// Capabilities of the tools handler
    capabilities: ToolsCapabilities,
}

impl fmt::Debug for ToolsHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolsHandler")
            .field("capabilities", &self.capabilities)
            .finish_non_exhaustive()
    }
}

impl ToolsHandler {
    /// Creates a new tools handler with the given provider
    pub fn new(provider: Arc<dyn ToolsProvider>) -> Self {
        Self {
            provider,
            capabilities: ToolsCapabilities { list_changed: true },
        }
    }

    /// Creates a new tools handler with the given provider and capabilities
    pub fn with_capabilities(
        provider: Arc<dyn ToolsProvider>,
        capabilities: ToolsCapabilities,
    ) -> Self {
        Self {
            provider,
            capabilities,
        }
    }

    /// Returns the capabilities of this handler
    pub fn capabilities(&self) -> &ToolsCapabilities {
        &self.capabilities
    }

    /// Registers methods with the JSON-RPC handler
    pub async fn register_methods(
        &self,
        method_handler: &JsonRpcHandler,
    ) -> Result<(), JsonRpcError> {
        // Clone the provider for the closure
        let list_provider = self.provider.clone();
        method_handler
            .register_method("tools/list", move |params| {
                let params_value = params.unwrap_or(Value::Null);
                let params: ListToolsParams =
                    serde_json::from_value(params_value).map_err(|e| {
                        let msg = format!("Invalid params: {}", e);
                        McpError::InvalidParams(msg)
                    })?;

                let list_provider = list_provider.clone();
                let response = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        list_provider.list_tools(params.cursor.as_deref()).await
                    })
                })?;

                serde_json::to_value(response).map_err(|e| {
                    let msg = format!("Serialization error: {}", e);
                    McpError::SerializationError(msg)
                })
            })
            .await
            .map_err(|e| JsonRpcError::internal_error(&e.to_string()))?;

        // Clone the provider for the closure
        let call_provider = self.provider.clone();
        method_handler
            .register_method("tools/call", move |params| {
                let params_value = params.unwrap_or(Value::Null);
                let params: CallToolParams = serde_json::from_value(params_value).map_err(|e| {
                    let msg = format!("Invalid params: {}", e);
                    McpError::InvalidParams(msg)
                })?;

                let call_provider = call_provider.clone();
                let result = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(async {
                        call_provider
                            .call_tool(&params.name, &params.arguments)
                            .await
                    })
                })?;

                serde_json::to_value(result).map_err(|e| {
                    let msg = format!("Serialization error: {}", e);
                    McpError::SerializationError(msg)
                })
            })
            .await
            .map_err(|e| JsonRpcError::internal_error(&e.to_string()))?;

        Ok(())
    }
}

impl Clone for ToolsHandler {
    fn clone(&self) -> Self {
        Self {
            provider: self.provider.clone(),
            capabilities: self.capabilities.clone(),
        }
    }
}
