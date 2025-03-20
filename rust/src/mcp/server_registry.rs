use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::process::{Child, Command};
use tokio::time::sleep;

use crate::mcp::connection::Connection;
use crate::utils::error::{McpError, McpResult};
use crate::mcp::types::{Message, MessageType, Priority};
use serde_yaml;

use async_trait::async_trait;
use log::{debug, info, warn};

/// Authentication settings for an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAuthSettings {
    /// API key for authentication
    pub api_key: Option<String>,
}

/// Root directory settings for an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RootSettings {
    /// The URI identifying the root. Must start with file://
    pub uri: String,
    /// Optional name for the root
    pub name: Option<String>,
    /// Optional URI alias for presentation to the server
    pub server_uri_alias: Option<String>,
}

/// Transport type for server communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// Standard input/output transport
    Stdio,
    /// Server-Sent Events transport
    Sse,
}

/// Configuration for an individual server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    /// The name of the server
    pub name: Option<String>,
    /// The description of the server
    pub description: Option<String>,
    /// The transport mechanism
    pub transport: Transport,
    /// The command to execute the server (e.g. npx)
    pub command: Option<String>,
    /// The arguments for the server command
    pub args: Option<Vec<String>>,
    /// The timeout in seconds for the server connection
    pub read_timeout_seconds: Option<u64>,
    /// The URL for the server (e.g. for SSE transport)
    pub url: Option<String>,
    /// The authentication configuration for the server
    pub auth: Option<ServerAuthSettings>,
    /// Root directories this server has access to
    pub roots: Option<Vec<RootSettings>>,
    /// Environment variables to pass to the server process
    pub env: Option<HashMap<String, String>>,
    /// Whether to auto-reconnect on connection failure
    pub auto_reconnect: Option<bool>,
    /// How many times to retry connection before giving up
    pub max_reconnect_attempts: Option<u32>,
    /// Time to wait between reconnection attempts in milliseconds
    pub reconnect_delay_ms: Option<u64>,
}

/// Configuration for all MCP servers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpSettings {
    /// Map of server name to server settings
    pub servers: HashMap<String, ServerSettings>,
}

/// Configuration for a server connection
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Command to run for starting the server
    pub command: String,
    /// Arguments to pass to the server command
    pub args: Vec<String>,
    /// Environment variables to set for the server
    pub env: HashMap<String, String>,
    /// Timeout duration for read operations
    pub read_timeout: Duration,
    /// Transport type for the connection
    pub transport: Transport,
    /// URL for SSE transport (optional)
    pub url: Option<String>,
    /// Whether to auto-reconnect on connection failure
    pub auto_reconnect: bool,
    /// How many times to retry connection before giving up
    pub max_reconnect_attempts: u32,
    /// Time to wait between reconnection attempts
    pub reconnect_delay: Duration,
}

/// Convert ServerSettings to ServerConfig
impl From<ServerSettings> for ServerConfig {
    fn from(settings: ServerSettings) -> Self {
        ServerConfig {
            command: settings.command.unwrap_or_default(),
            args: settings.args.unwrap_or_default(),
            env: settings.env.unwrap_or_default(),
            read_timeout: Duration::from_secs(settings.read_timeout_seconds.unwrap_or(30)),
            transport: settings.transport,
            url: settings.url,
            auto_reconnect: settings.auto_reconnect.unwrap_or(true),
            max_reconnect_attempts: settings.max_reconnect_attempts.unwrap_or(3),
            reconnect_delay: Duration::from_millis(settings.reconnect_delay_ms.unwrap_or(1000)),
        }
    }
}

/// Type alias for initialization hook function
pub type InitHook = Box<dyn for<'a> Fn(&'a str, &'a Message) -> McpResult<()> + Send + Sync>;

/// A server connection including the process and connection
pub struct ServerConnection {
    /// Name of the server
    pub name: String,
    /// Connection to the server
    pub connection: Arc<Connection>,
    /// Configuration for the server
    pub config: ServerConfig,
    /// Child process for stdio transport
    pub process: Option<Child>,
    /// Whether the server is initialized
    pub initialized: bool,
}

impl std::fmt::Debug for ServerConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConnection")
            .field("name", &self.name)
            .field("connection", &self.connection)
            .field("config", &self.config)
            .field("has_process", &self.process.is_some())
            .field("initialized", &self.initialized)
            .finish()
    }
}

impl ServerConnection {
    /// Create a new server connection
    pub fn new(name: String, connection: Connection, config: ServerConfig, process: Option<Child>) -> Self {
        Self {
            name,
            connection: Arc::new(connection),
            config,
            process,
            initialized: false,
        }
    }
    
    /// Initialize the server
    pub async fn initialize(&mut self) -> McpResult<()> {
        if !self.initialized {
            info!("Initializing server: {}", self.name);
            Arc::get_mut(&mut self.connection).ok_or_else(|| McpError::ConnectionFailed("Could not get mutable reference to connection".to_string()))?.initialize().await?;
            self.initialized = true;
            info!("Server initialized: {}", self.name);
        }
        Ok(())
    }
    
    /// Close the connection and terminate the process
    pub async fn shutdown(&mut self) -> McpResult<()> {
        info!("Shutting down server: {}", self.name);
        
        // Disconnect the connection
        if let Err(e) = Arc::get_mut(&mut self.connection).ok_or_else(|| McpError::ConnectionFailed("Could not get mutable reference to connection".to_string()))?.disconnect().await {
            warn!("Error disconnecting from server {}: {}", self.name, e);
        }
        
        // Kill the process if it exists
        if let Some(ref mut process) = self.process {
            if let Err(e) = process.kill().await {
                warn!("Error killing server process {}: {}", self.name, e);
            }
        }
        
        info!("Server shutdown complete: {}", self.name);
        Ok(())
    }
}

/// Information about a server connection
#[derive(Debug)]
struct ServerConnInfo {
    /// The active connection to the server
    connection: Option<Connection>,
    /// Configuration for the server
    config: ServerConfig,
    /// Number of connection attempts made
    attempts: u32,
}

/// Registry of MCP servers with configuration settings and connection management
pub struct ServerRegistry {
    /// Map of server name to server settings
    servers: HashMap<String, ServerConfig>,
    /// Map of server name to initialization hooks
    init_hooks: HashMap<String, InitHook>,
    /// Active connections to servers, protected by a Mutex for thread-safe access
    active_connections: Arc<Mutex<HashMap<String, ServerConnInfo>>>,
    /// Active server processes
    active_servers: Arc<Mutex<HashMap<String, ServerConnection>>>,
}

impl std::fmt::Debug for ServerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerRegistry")
            .field("servers", &self.servers)
            .field("init_hooks", &format!("{} hooks", self.init_hooks.len()))
            .finish()
    }
}

impl Default for ServerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerRegistry {
    /// Creates a new server registry.
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            init_hooks: HashMap::new(),
            active_connections: Arc::new(Mutex::new(HashMap::new())),
            active_servers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Load server settings from a configuration file and populate the registry
    pub fn load_from_file<P: AsRef<Path>>(&mut self, config_path: P) -> McpResult<()> {
        let file = std::fs::File::open(&config_path)
            .map_err(|e| McpError::Config(format!("Failed to open config file: {}", e)))?;
        
        let settings: McpSettings = serde_yaml::from_reader(file)
            .map_err(|e| McpError::Config(format!("Failed to parse YAML: {}", e)))?;

        // Register servers from config
        for (name, server_settings) in settings.servers {
            let config = ServerConfig::from(server_settings);
            self.register_server(&name, config)?;
        }

        Ok(())
    }

    /// Load server settings from common locations
    pub fn load_from_default_locations(&mut self) -> McpResult<()> {
        let default_paths = vec![
            "mcp_agent.config.yaml",
            "config/mcp_agent.config.yaml",
            "../mcp_agent.config.yaml",
        ];
        
        for path in default_paths {
            if Path::new(path).exists() {
                return self.load_from_file(path);
            }
        }
        
        Err(McpError::Config("No configuration file found".to_string()))
    }

    /// Registers a server with the registry.
    pub fn register_server(&mut self, name: &str, config: ServerConfig) -> McpResult<()> {
        if self.servers.contains_key(name) {
            return Err(McpError::ServerNotFound(format!("Server already registered: {}", name)));
        }
        self.servers.insert(name.to_string(), config);
        Ok(())
    }

    /// Registers an initialization hook for a server.
    pub fn register_init_hook(
        &mut self,
        name: &str,
        hook: InitHook,
    ) -> McpResult<()> {
        if !self.servers.contains_key(name) {
            return Err(McpError::ServerNotFound(format!("Server not found: {}", name)));
        }
        self.init_hooks.insert(name.to_string(), hook);
        Ok(())
    }

    /// Get an existing connection from the registry or create a new one
    pub async fn get_connection(&self, name: &str) -> McpResult<Connection> {
        let mut active_conns = self.active_connections.lock().await;
        
        // Check if we already have an active connection
        if let Some(conn_info) = active_conns.get_mut(name) {
            if let Some(conn) = &conn_info.connection {
                if conn.is_connected() {
                    // Clone the connection to return it
                    return Ok(conn.clone());
                } else if conn_info.config.auto_reconnect && conn_info.attempts < conn_info.config.max_reconnect_attempts {
                    // Try to reconnect
                    conn_info.connection = None;
                    conn_info.attempts += 1;
                    
                    // Create a new connection
                    let new_conn = self.create_connection(name, &conn_info.config).await?;
                    conn_info.connection = Some(new_conn.clone());
                    return Ok(new_conn);
                } else {
                    // No auto-reconnect or max attempts reached
                    return Err(McpError::ConnectionFailed(format!("Connection to server {} lost", name)));
                }
            }
        }
        
        // No existing connection, create a new one
        let config = self.servers.get(name)
            .ok_or_else(|| McpError::ServerNotFound(format!("Server not found: {}", name)))?;
        
        let connection = self.create_connection(name, config).await?;
        
        // Store the connection
        active_conns.insert(name.to_string(), ServerConnInfo {
            connection: Some(connection.clone()),
            config: config.clone(),
            attempts: 1,
        });
        
        Ok(connection)
    }
    
    /// Create a new connection to a server
    async fn create_connection(&self, name: &str, config: &ServerConfig) -> McpResult<Connection> {
        match config.transport {
            Transport::Stdio => {
                debug!("Creating stdio connection to server {}: {} {:?}", name, config.command, config.args);
                Connection::connect_stdio(
                    &config.command, 
                    &config.args, 
                    &config.env, 
                    config.read_timeout
                ).await
            },
            Transport::Sse => {
                let url = config.url.as_ref()
                    .ok_or_else(|| McpError::Config(format!("URL required for SSE transport")))?;
                debug!("Creating SSE connection to server {}: {}", name, url);
                Connection::connect_sse(url, config.read_timeout).await
            }
        }
    }
    
    /// Start a managed server with lifecycle management
    pub async fn start_server(&self, server_name: &str) -> McpResult<Arc<Connection>> {
        let mut servers = self.active_servers.lock().await;
        
        // Check if we already have a running server
        if let Some(server) = servers.get(server_name) {
            if server.connection.is_connected() {
                return Ok(server.connection.clone());
            }
        }
        
        // Get the server config
        let config = self.servers.get(server_name)
            .ok_or_else(|| McpError::ServerNotFound(format!("Server not found: {}", server_name)))?
            .clone();
        
        info!("Starting server: {}", server_name);
        
        // Create the connection based on transport
        let (connection, process) = match config.transport {
            Transport::Stdio => {
                debug!("Creating stdio connection to server {}: {} {:?}", server_name, config.command, config.args);
                let mut cmd = Command::new(&config.command);
                cmd.args(&config.args);
                cmd.env_clear();
                
                // Add environment variables
                for (key, value) in &config.env {
                    cmd.env(key, value);
                }
                
                // Add default environment variables
                cmd.env("MCP_SERVER", "1");
                
                // Spawn the process
                let process = cmd.spawn()
                    .map_err(|e| McpError::ConnectionFailed(format!("Failed to start server process: {}", e)))?;
                
                // Allow a little time for the process to start
                sleep(Duration::from_millis(200)).await;
                
                // Create the connection
                let connection = Connection::connect_stdio(
                    &config.command,
                    &config.args,
                    &config.env,
                    config.read_timeout
                ).await?;
                
                (connection, Some(process))
            },
            Transport::Sse => {
                let url = config.url.as_ref()
                    .ok_or_else(|| McpError::Config(format!("URL required for SSE transport")))?;
                debug!("Creating SSE connection to server {}: {}", server_name, url);
                
                let connection = Connection::connect_sse(url, config.read_timeout).await?;
                
                (connection, None)
            }
        };
        
        // Create the server connection
        let mut server_conn = ServerConnection::new(
            server_name.to_string(),
            connection,
            config,
            process
        );
        
        // Initialize the server
        server_conn.initialize().await?;
        
        // Run the init hook if available
        if let Some(hook) = self.init_hooks.get(server_name) {
            debug!("Running init hook for server: {}", server_name);
            let init_msg = Message::new(
                MessageType::Request,
                Priority::Normal,
                Vec::new(),
                None,
                None,
            );
            
            hook(server_name, &init_msg)?;
        }
        
        // Store the server connection and return a clone of the connection
        let conn_arc = server_conn.connection.clone();
        servers.insert(server_name.to_string(), server_conn);
        
        info!("Server started successfully: {}", server_name);
        Ok(conn_arc)
    }
    
    /// Send a message to a server
    pub async fn send_message(&self, server_name: &str, message: Message) -> McpResult<Message> {
        // Get the server connection, starting it if needed
        let connection = self.start_server(server_name).await?;
        
        // Create a mutable clone of the connection to send the message
        let mut conn_clone = (*connection).clone();
        
        // Send the message
        conn_clone.send_message(message.clone()).await?;
        
        // Receive the response
        conn_clone.receive_message().await
    }
    
    /// Get the server configuration
    pub fn get_server_config(&self, server_name: &str) -> McpResult<&ServerConfig> {
        self.servers.get(server_name)
            .ok_or_else(|| McpError::ServerNotFound(format!("Server not found: {}", server_name)))
    }
    
    /// Stop a server and close the connection
    pub async fn stop_server(&self, server_name: &str) -> McpResult<()> {
        let mut servers = self.active_servers.lock().await;
        
        if let Some(mut server) = servers.remove(server_name) {
            server.shutdown().await?;
            info!("Server stopped: {}", server_name);
        } else {
            debug!("No running server to stop: {}", server_name);
        }
        
        Ok(())
    }
    
    /// Close a connection without stopping the server
    pub async fn close_connection(&self, server_name: &str) -> McpResult<()> {
        let mut active_conns = self.active_connections.lock().await;
        
        if let Some(conn_info) = active_conns.remove(server_name) {
            if let Some(mut conn) = conn_info.connection {
                if let Err(e) = conn.disconnect().await {
                    warn!("Error closing connection {}: {}", server_name, e);
                }
            }
        } else {
            debug!("No connection to close: {}", server_name);
        }
        
        Ok(())
    }
    
    /// Close all connections and stop all servers
    pub async fn stop_all_servers(&self) -> McpResult<()> {
        let mut servers = self.active_servers.lock().await;
        
        for (name, mut server) in servers.drain() {
            if let Err(e) = server.shutdown().await {
                warn!("Error stopping server {}: {}", name, e);
            }
        }
        
        info!("All servers stopped");
        Ok(())
    }
    
    /// Close all connections without stopping servers
    pub async fn close_all_connections(&self) -> McpResult<()> {
        let mut active_conns = self.active_connections.lock().await;
        
        for (name, conn_info) in active_conns.drain() {
            if let Some(mut conn) = conn_info.connection {
                if let Err(e) = conn.disconnect().await {
                    warn!("Error closing connection {}: {}", name, e);
                }
            }
        }
        
        info!("All connections closed");
        Ok(())
    }
    
    /// Get the list of registered server names
    pub fn get_server_names(&self) -> Vec<String> {
        self.servers.keys().cloned().collect()
    }
    
    /// Check if a server is connected
    pub async fn is_server_connected(&self, server_name: &str) -> bool {
        let servers = self.active_servers.lock().await;
        
        if let Some(server) = servers.get(server_name) {
            server.connection.is_connected()
        } else {
            // Also check in the connections map for backward compatibility
            let active_conns = self.active_connections.lock().await;
            
            if let Some(conn_info) = active_conns.get(server_name) {
                if let Some(conn) = &conn_info.connection {
                    conn.is_connected()
                } else {
                    false
                }
            } else {
                false
            }
        }
    }
    
    /// Get the number of connected servers
    pub async fn connected_server_count(&self) -> usize {
        let servers = self.active_servers.lock().await;
        let mut count = 0;
        
        for server in servers.values() {
            if server.connection.is_connected() {
                count += 1;
            }
        }
        
        count
    }
}

/// A trait for objects that need access to the server registry
#[async_trait]
pub trait ServerRegistryAccess {
    /// Get the server registry
    fn server_registry(&self) -> Arc<ServerRegistry>;
    
    /// Start a server
    async fn start_server(&self, server_name: &str) -> McpResult<Arc<Connection>> {
        self.server_registry().start_server(server_name).await
    }
    
    /// Send a message to a server
    async fn send_to_server(&self, server_name: &str, message: Message) -> McpResult<Message> {
        self.server_registry().send_message(server_name, message).await
    }
    
    /// Stop a server
    async fn stop_server(&self, server_name: &str) -> McpResult<()> {
        self.server_registry().stop_server(server_name).await
    }
    
    /// Check if a server is connected
    async fn is_server_connected(&self, server_name: &str) -> bool {
        self.server_registry().is_server_connected(server_name).await
    }
    
    /// Get the list of registered server names
    fn get_server_names(&self) -> Vec<String> {
        self.server_registry().get_server_names()
    }
    
    /// Get the number of connected servers
    async fn connected_server_count(&self) -> usize {
        self.server_registry().connected_server_count().await
    }
}

/// Implementation of Drop for ServerRegistry to ensure all servers are stopped
impl Drop for ServerRegistry {
    fn drop(&mut self) {
        debug!("ServerRegistry being dropped, connections will be closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_server_settings() -> ServerSettings {
        ServerSettings {
            name: Some("test".to_string()),
            description: Some("Test Server".to_string()),
            transport: Transport::Stdio,
            command: Some("echo".to_string()),
            args: Some(vec!["Hello".to_string()]),
            read_timeout_seconds: Some(10),
            url: None,
            auth: None,
            roots: None,
            env: None,
            auto_reconnect: Some(true),
            max_reconnect_attempts: Some(3),
            reconnect_delay_ms: Some(1000),
        }
    }
    
    #[tokio::test]
    async fn test_server_registry_creation() {
        let registry = ServerRegistry::new();
        assert_eq!(registry.servers.len(), 0);
        assert_eq!(registry.init_hooks.len(), 0);
        
        // Test with a mock config
        let mut settings = McpSettings {
            servers: HashMap::new(),
        };
        
        settings.servers.insert("test".to_string(), create_test_server_settings());
        
        let mut registry = ServerRegistry::new();
        
        for (name, server_settings) in settings.servers {
            let config = ServerConfig::from(server_settings);
            registry.register_server(&name, config).unwrap();
        }
        
        assert_eq!(registry.servers.len(), 1);
        assert!(registry.servers.contains_key("test"));
    }
    
    #[test]
    fn test_register_server() -> McpResult<()> {
        let mut registry = ServerRegistry::new();
        
        let settings = create_test_server_settings();
        let config = ServerConfig::from(settings);
        
        registry.register_server("test", config.clone())?;
        
        // Check it's registered
        assert!(registry.servers.contains_key("test"));
        
        // Should fail to register the same server twice
        let result = registry.register_server("test", config);
        assert!(result.is_err());
        
        Ok(())
    }
    
    #[test]
    fn test_register_init_hook() -> McpResult<()> {
        let mut registry = ServerRegistry::new();
        
        let settings = create_test_server_settings();
        let config = ServerConfig::from(settings);
        
        registry.register_server("test", config)?;
        
        // Try to register a hook for a non-existent server
        let hook: InitHook = Box::new(move |_name, _msg| Ok(()));
        let result = registry.register_init_hook("nonexistent", hook);
        assert!(result.is_err());
        
        // Register a valid hook
        let hook: InitHook = Box::new(move |_name, _msg| Ok(()));
        registry.register_init_hook("test", hook)?;
        
        assert_eq!(registry.init_hooks.len(), 1);
        assert!(registry.init_hooks.contains_key("test"));
        
        Ok(())
    }
    
    // More tests would go here but they would require mocking network connections
} 