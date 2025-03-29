#[cfg(test)]
mod tests {
    use crate::mcp::tools::handler::ToolsProvider;
    use crate::mcp::tools::{BasicToolProvider, Tool, ToolResult, ToolResultContent};
    use crate::utils::error::McpError;
    use serde_json::json;

    #[tokio::test]
    async fn test_tool_registration() {
        // Create a tool provider
        let provider = BasicToolProvider::new();

        // Create a tool
        let tool = Tool::new(
            "test_tool",
            "A test tool",
            json!({
                "type": "object",
                "properties": {
                    "input": {
                        "type": "string"
                    }
                }
            }),
        );

        // Register the tool
        provider
            .register_tool(tool, |params| {
                let input = params
                    .get("input")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default");
                Ok(ToolResult::text(&format!("Processed: {}", input)))
            })
            .unwrap();

        // List tools
        let response = provider.list_tools(None).await.unwrap();
        assert_eq!(response.tools.len(), 1);
        assert_eq!(response.tools[0].name, "test_tool");
    }

    #[tokio::test]
    async fn test_tool_call() {
        // Create a tool provider
        let provider = BasicToolProvider::new();

        // Create a tool
        let tool = Tool::new(
            "echo",
            "Echoes the input",
            json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string"
                    }
                }
            }),
        );

        // Register the tool
        provider
            .register_tool(tool, |params| {
                let message = params
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("No message");
                Ok(ToolResult::text(message))
            })
            .unwrap();

        // Call the tool
        let result = provider
            .call_tool(
                "echo",
                &json!({
                    "message": "Hello, world!"
                }),
            )
            .await
            .unwrap();

        assert_eq!(result.is_error, false);
        assert_eq!(result.content.len(), 1);
        if let ToolResultContent::Text { text } = &result.content[0] {
            assert_eq!(text, "Hello, world!");
        } else {
            panic!("Expected text content");
        }
    }

    #[tokio::test]
    async fn test_unknown_tool() {
        // Create a tool provider
        let provider = BasicToolProvider::new();

        // Call an unknown tool
        let result = provider.call_tool("unknown", &json!({})).await;

        assert!(result.is_err());
        if let Err(error) = result {
            if let McpError::NotFound(_) = error {
                // Expected error
            } else {
                panic!("Expected not found error, got: {:?}", error);
            }
        }
    }

    #[tokio::test]
    async fn test_tool_error() {
        // Create a tool provider
        let provider = BasicToolProvider::new();

        // Create a tool that can return an error
        let tool = Tool::new(
            "fallible",
            "A tool that can fail",
            json!({
                "type": "object",
                "properties": {
                    "should_fail": {
                        "type": "boolean"
                    }
                }
            }),
        );

        // Register the tool
        provider
            .register_tool(tool, |params| {
                let should_fail = params
                    .get("should_fail")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if should_fail {
                    Ok(ToolResult::error("Tool execution failed"))
                } else {
                    Ok(ToolResult::text("Tool executed successfully"))
                }
            })
            .unwrap();

        // Call the tool with success
        let success_result = provider
            .call_tool(
                "fallible",
                &json!({
                    "should_fail": false
                }),
            )
            .await
            .unwrap();

        assert_eq!(success_result.is_error, false);

        // Call the tool with failure
        let error_result = provider
            .call_tool(
                "fallible",
                &json!({
                    "should_fail": true
                }),
            )
            .await
            .unwrap();

        assert_eq!(error_result.is_error, true);
        if let ToolResultContent::Text { text } = &error_result.content[0] {
            assert_eq!(text, "Tool execution failed");
        } else {
            panic!("Expected text content");
        }
    }
}
