use crate::mcp::tools::handler::{ListToolsResponse, ToolsProvider};
use crate::mcp::tools::models::{Tool, ToolResult};
use crate::utils::error::{McpError, McpResult};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, RwLock};

/// Type for tool execution function
pub type ToolFunction = Arc<dyn Fn(&Value) -> McpResult<ToolResult> + Send + Sync>;

/// Basic implementation of the tools provider
pub struct BasicToolProvider {
    /// Map of tools by name
    tools: RwLock<HashMap<String, Tool>>,

    /// Map of tool handlers by name
    handlers:
        RwLock<HashMap<String, Arc<dyn Fn(&Value) -> Result<ToolResult, McpError> + Send + Sync>>>,
}

impl fmt::Debug for BasicToolProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BasicToolProvider")
            .field("tools_count", &self.tools.read().unwrap().len())
            .finish_non_exhaustive()
    }
}

impl BasicToolProvider {
    /// Creates a new basic tools provider
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            handlers: RwLock::new(HashMap::new()),
        }
    }

    /// Registers a new tool
    pub fn register_tool<F>(&self, tool: Tool, handler: F) -> McpResult<()>
    where
        F: Fn(&Value) -> Result<ToolResult, McpError> + Send + Sync + 'static,
    {
        let name = tool.name.clone();

        let mut tools = self
            .tools
            .write()
            .map_err(|_| McpError::Execution("Failed to acquire tools lock".to_string()))?;

        let mut handlers = self
            .handlers
            .write()
            .map_err(|_| McpError::Execution("Failed to acquire handlers lock".to_string()))?;

        tools.insert(name.clone(), tool);
        handlers.insert(name, Arc::new(handler));

        Ok(())
    }

    /// Unregisters a tool
    pub fn unregister_tool(&self, name: &str) -> McpResult<()> {
        let mut tools = self
            .tools
            .write()
            .map_err(|_| McpError::Execution("Failed to acquire tools lock".to_string()))?;

        let mut handlers = self
            .handlers
            .write()
            .map_err(|_| McpError::Execution("Failed to acquire handlers lock".to_string()))?;

        tools.remove(name);
        handlers.remove(name);

        Ok(())
    }
}

#[async_trait]
impl ToolsProvider for BasicToolProvider {
    async fn list_tools(&self, _cursor: Option<&str>) -> McpResult<ListToolsResponse> {
        let tools = self
            .tools
            .read()
            .map_err(|_| McpError::Execution("Failed to acquire tools lock".to_string()))?;

        let tools_vec: Vec<Tool> = tools.values().cloned().collect();

        Ok(ListToolsResponse {
            tools: tools_vec,
            next_cursor: None,
        })
    }

    async fn call_tool(&self, name: &str, arguments: &Value) -> McpResult<ToolResult> {
        let handlers = self
            .handlers
            .read()
            .map_err(|_| McpError::Execution("Failed to acquire handlers lock".to_string()))?;

        let handler = handlers
            .get(name)
            .ok_or_else(|| McpError::NotFound(format!("Tool '{}' not found", name)))?;

        handler(arguments)
    }
}

impl Default for BasicToolProvider {
    fn default() -> Self {
        Self::new()
    }
}
