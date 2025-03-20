use crate::mcp::connection::{Connection, ConnectionConfig};
use crate::mcp::executor::{AsyncioExecutor, Executor, ExecutorConfig, Signal, TaskResult};
use crate::mcp::server_registry::{McpSettings, ServerRegistry};
use crate::mcp::types::{Message, MessageType, Priority};
use crate::utils::error::{McpError, McpResult};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use uuid::Uuid;
use tokio::net::TcpStream;

/// Configuration for the MCP Agent
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Executor configuration
    pub executor_config: Option<ExecutorConfig>,
    /// MCP Protocol settings
    pub mcp_settings: Option<McpSettings>,
    /// Default timeout for operations in seconds
    pub default_timeout_secs: u64,
    /// Max number of reconnection attempts
    pub max_reconnect_attempts: u32,
    /// Base delay between reconnection attempts in milliseconds
    pub reconnect_base_delay_ms: u64,
    /// Max delay between reconnection attempts in milliseconds
    pub reconnect_max_delay_ms: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            executor_config: None,
            mcp_settings: None,
            default_timeout_secs: 30,
            max_reconnect_attempts: 5,
            reconnect_base_delay_ms: 100,
            reconnect_max_delay_ms: 5000,
        }
    }
}

/// The MCP Agent provides a high-level API for interacting with MCP services
/// It manages connections, task execution, and message handling
pub struct Agent {
    /// Agent configuration
    config: AgentConfig,
    /// Executor for running tasks
    executor: Arc<dyn Executor>,
    /// Server registry for managing connections
    server_registry: Arc<Mutex<ServerRegistry>>,
    /// Active connections to servers
    connections: Arc<Mutex<HashMap<String, Connection>>>,
}

// Add Debug implementation for Agent
impl std::fmt::Debug for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Agent")
            .field("config", &self.config)
            // Skip executor since it doesn't implement Debug
            .field("server_registry", &"<ServerRegistry>")
            .field("connections", &"<Connections>")
            .finish()
    }
}

impl Agent {
    /// Create a new Agent with the given configuration
    pub fn new(config: Option<AgentConfig>) -> Self {
        let config = config.unwrap_or_default();
        let executor = Arc::new(AsyncioExecutor::new(config.executor_config.clone()));
        let server_registry = Arc::new(Mutex::new(ServerRegistry::new()));
        
        Self {
            config,
            executor,
            server_registry,
            connections: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Connect to an MCP server
    pub async fn connect(&self, server_id: &str, address: &str) -> McpResult<()> {
        // For now, we'll just create a dummy connection since we need to adapt
        // to the existing ServerRegistry and Connection implementations

        // Connect to the server using TcpStream
        let stream = TcpStream::connect(address).await
            .map_err(|e| McpError::ConnectionFailed(e.to_string()))?;
            
        // Create a connection config with appropriate values
        let config = ConnectionConfig {
            keep_alive_interval: Duration::from_secs(30),
            keep_alive_timeout: Duration::from_secs(5),
            max_retries: 3,
            retry_delay_ms: 1000,
        };
        
        // Create a new connection
        let connection = Connection::new(address.to_string(), stream, config);
        
        // Store the connection
        let mut connections = self.connections.lock().await;
        connections.insert(server_id.to_string(), connection);
        
        Ok(())
    }
    
    /// Disconnect from an MCP server
    pub async fn disconnect(&self, server_id: &str) -> McpResult<()> {
        let mut connections = self.connections.lock().await;
        if let Some(mut connection) = connections.remove(server_id) {
            connection.close().await?;
            Ok(())
        } else {
            Err(McpError::ServerNotFound(server_id.to_string()))
        }
    }
    
    /// Send a message to a specific server
    pub async fn send_message(&self, server_id: &str, message: Message) -> McpResult<()> {
        let mut connections = self.connections.lock().await;
        if let Some(connection) = connections.get_mut(server_id) {
            connection.send_message(message).await
        } else {
            Err(McpError::ServerNotFound(server_id.to_string()))
        }
    }
    
    /// Create and send a request message to a server and wait for response
    pub async fn send_request(
        &self,
        server_id: &str, 
        payload: Vec<u8>,
        priority: Option<Priority>,
    ) -> McpResult<Message> {
        let mut connections = self.connections.lock().await;
        if let Some(connection) = connections.get_mut(server_id) {
            let request = Message::new(
                MessageType::Request,
                priority.unwrap_or(Priority::Normal),
                payload,
                None,
                None,
            );
            
            // Send the message
            connection.send_message(request.clone()).await?;
            
            // Wait for response
            connection.receive_message().await
        } else {
            Err(McpError::ServerNotFound(server_id.to_string()))
        }
    }
    
    /// Execute a task using the agent's executor
    pub async fn execute_task(
        &self,
        function: &str,
        args: serde_json::Value,
        timeout: Option<Duration>,
    ) -> McpResult<TaskResult> {
        self.executor.execute(None, function, args, timeout).await
    }
    
    /// Execute a task and stream the results
    pub async fn execute_task_stream(
        &self,
        function: &str,
        args: serde_json::Value,
        timeout: Option<Duration>,
    ) -> McpResult<tokio::sync::mpsc::Receiver<TaskResult>> {
        self.executor.execute_stream(None, function, args, timeout).await
    }
    
    /// Send a signal to a workflow
    pub async fn send_signal(&self, signal: Signal) -> McpResult<()> {
        self.executor.send_signal(signal).await
    }
    
    /// Wait for a signal from a workflow
    pub async fn wait_for_signal(
        &self,
        workflow_id: &str,
        signal_name: &str,
        timeout: Option<Duration>,
    ) -> McpResult<Signal> {
        self.executor.wait_for_signal(workflow_id, signal_name, timeout).await
    }
    
    /// Check if the agent is connected to a specific server
    pub async fn is_connected(&self, server_id: &str) -> bool {
        let connections = self.connections.lock().await;
        connections.contains_key(server_id)
    }
    
    /// Get the number of active connections
    pub async fn connection_count(&self) -> usize {
        let connections = self.connections.lock().await;
        connections.len()
    }
    
    /// Generate a unique ID for various operations
    pub fn generate_id(&self) -> String {
        Uuid::new_v4().to_string()
    }
    
    /// Connect to a test server without using ServerRegistry
    /// This is primarily for testing and demonstration purposes
    pub async fn connect_to_test_server(&self, server_id: &str, address: &str) -> McpResult<()> {
        println!("Connecting to test server at {}", address);
        
        // Connect to the server using TcpStream
        let stream = TcpStream::connect(address).await
            .map_err(|e| McpError::ConnectionFailed(e.to_string()))?;
            
        // Use simpler configuration for testing
        let config = ConnectionConfig {
            keep_alive_interval: Duration::from_secs(30),
            keep_alive_timeout: Duration::from_secs(5),
            max_retries: 1,
            retry_delay_ms: 100,
        };
        
        // Create a connection directly
        let connection = Connection::new(address.to_string(), stream, config);
        
        // Store the connection
        let mut connections = self.connections.lock().await;
        connections.insert(server_id.to_string(), connection);
        
        println!("Connected to test server {}", server_id);
        Ok(())
    }
    
    /// List server IDs for all connected servers
    pub async fn list_connections(&self) -> Vec<String> {
        let connections = self.connections.lock().await;
        connections.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_agent_creation() {
        let agent = Agent::new(None);
        assert_eq!(agent.connection_count().await, 0);
    }
    
    #[tokio::test]
    async fn test_agent_task_execution() {
        let agent = Agent::new(None);
        let args = serde_json::json!({ "value": 42 });
        
        let result = agent.execute_task("test_success", args.clone(), None).await.unwrap();
        assert!(result.is_success());
        assert_eq!(result.success_value(), Some(&args));
    }
} 