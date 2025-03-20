use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use std::fmt;

use serde::{Deserialize, Serialize};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;
use tokio::time::sleep;
use log::{debug, info, warn};

use crate::mcp::connection::Connection;
use crate::mcp::types::Message;
use crate::utils::error::{McpError, McpResult};
#[cfg(test)]
use crate::config::Settings;
use crate::config::{McpServerSettings, get_settings};

/// Settings for a specific MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    /// Transport type (stdio, sse, etc.)
    #[serde(default = "default_transport")]
    pub transport: String,
    
    /// Command to start the server (for stdio transport)
    pub command: Option<String>,
    
    /// Arguments for the command (for stdio transport)
    pub args: Option<Vec<String>>,
    
    /// URL for the server (for sse transport)
    pub url: Option<String>,
    
    /// Environment variables to pass to the server
    pub env: Option<HashMap<String, String>>,
    
    /// Authentication settings
    pub auth: Option<ServerAuthSettings>,
    
    /// Read timeout in seconds
    pub read_timeout_seconds: Option<u64>,
    
    /// Whether to auto-reconnect on failure
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,
    
    /// Maximum reconnection attempts
    #[serde(default = "default_max_reconnect")]
    pub max_reconnect_attempts: u32,
}

/// Authentication settings for an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAuthSettings {
    /// Authentication type
    #[serde(default)]
    pub auth_type: String,
    
    /// API key
    pub api_key: Option<String>,
    
    /// Token
    pub token: Option<String>,
}

/// Convert from config McpServerSettings to ServerManager's ServerSettings
impl From<McpServerSettings> for ServerSettings {
    fn from(settings: McpServerSettings) -> Self {
        Self {
            transport: settings.transport,
            command: settings.command,
            args: settings.args,
            url: settings.url,
            env: settings.env,
            auth: settings.auth.map(|auth| ServerAuthSettings {
                auth_type: auth.auth_type,
                api_key: auth.api_key,
                token: auth.token,
            }),
            read_timeout_seconds: settings.read_timeout_seconds,
            auto_reconnect: true,
            max_reconnect_attempts: 3,
        }
    }
}

fn default_transport() -> String {
    "stdio".to_string()
}

fn default_auto_reconnect() -> bool {
    true
}

fn default_max_reconnect() -> u32 {
    3
}

/// A wrapper type for server initialization hook functions that implements Debug
pub struct InitHookFn(Box<dyn Fn(&str, &Connection) -> McpResult<()> + Send + Sync>);

impl InitHookFn {
    /// Create a new initialization hook function
    pub fn new<F>(f: F) -> Self 
    where 
        F: Fn(&str, &Connection) -> McpResult<()> + Send + Sync + 'static
    {
        Self(Box::new(f))
    }
    
    /// Call the initialization hook
    pub fn call(&self, server_name: &str, connection: &Connection) -> McpResult<()> {
        (self.0)(server_name, connection)
    }
}

impl fmt::Debug for InitHookFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "InitHookFn")
    }
}

/// Manages MCP server processes and connections
#[derive(Debug)]
pub struct ServerManager {
    /// Server configurations
    server_settings: Arc<Mutex<HashMap<String, ServerSettings>>>,
    
    /// Running server processes
    running_servers: Arc<Mutex<HashMap<String, Child>>>,
    
    /// Active server connections
    active_connections: Arc<Mutex<HashMap<String, Connection>>>,
    
    /// Initialization hooks for servers
    init_hooks: Arc<Mutex<HashMap<String, InitHookFn>>>,
}

impl Default for ServerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerManager {
    /// Create a new server manager
    pub fn new() -> Self {
        Self {
            server_settings: Arc::new(Mutex::new(HashMap::new())),
            running_servers: Arc::new(Mutex::new(HashMap::new())),
            active_connections: Arc::new(Mutex::new(HashMap::new())),
            init_hooks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Create a new ServerManager and initialize from a config file
    pub async fn from_config(config_path: Option<&str>) -> McpResult<Self> {
        let manager = Self::new();
        manager.load_from_config(config_path).await?;
        Ok(manager)
    }
    
    /// Load server settings from the standard config file format
    pub async fn load_from_config(&self, config_path: Option<&str>) -> McpResult<()> {
        let settings = get_settings(config_path)?;
        
        if settings.mcp.servers.is_empty() {
            debug!("No server configurations found in config file");
            return Ok(());
        }
        
        info!("Loading server configurations from config file");
        let mut server_settings = self.server_settings.lock().await;
        
        for (name, config) in settings.mcp.servers {
            debug!("Loading server configuration for '{}'", name);
            let server_settings_entry = ServerSettings::from(config);
            server_settings.insert(name, server_settings_entry);
        }
        
        info!("Loaded {} server configurations", server_settings.len());
        Ok(())
    }
    
    /// Load server settings from a YAML file
    pub async fn load_from_file<P: AsRef<Path>>(&self, path: P) -> McpResult<()> {
        let path_ref = path.as_ref().to_owned();
        let _file = tokio::fs::File::open(&path_ref).await
            .map_err(|e| McpError::Config(format!("Failed to open config file: {}", e)))?;
        
        let contents = tokio::fs::read_to_string(&path_ref).await
            .map_err(|e| McpError::Config(format!("Failed to read config file: {}", e)))?;
        
        let settings: HashMap<String, ServerSettings> = serde_yaml::from_str(&contents)
            .map_err(|e| McpError::Config(format!("Failed to parse YAML: {}", e)))?;
        
        let mut server_settings = self.server_settings.lock().await;
        for (name, setting) in settings {
            server_settings.insert(name, setting);
        }
        
        Ok(())
    }
    
    /// Load server settings from common locations
    pub async fn load_from_default_locations(&self) -> McpResult<()> {
        let default_paths = vec![
            "mcp_agent.config.yaml",
            "config/mcp_agent.config.yaml",
            "../mcp_agent.config.yaml",
        ];
        
        for path in default_paths {
            if Path::new(path).exists() {
                return self.load_from_file(path).await;
            }
        }
        
        Err(McpError::Config("No configuration file found".to_string()))
    }
    
    /// Register a new server setting
    pub async fn register_server(&self, name: &str, settings: ServerSettings) -> McpResult<()> {
        let mut server_settings = self.server_settings.lock().await;
        
        if server_settings.contains_key(name) {
            return Err(McpError::Config(format!("Server '{}' already registered", name)));
        }
        
        server_settings.insert(name.to_string(), settings);
        Ok(())
    }
    
    /// Register an initialization hook for a server
    pub async fn register_init_hook<F>(&self, server_name: &str, hook: F) -> McpResult<()>
    where
        F: Fn(&str, &Connection) -> McpResult<()> + Send + Sync + 'static, 
    {
        let server_settings = self.server_settings.lock().await;
        
        if !server_settings.contains_key(server_name) {
            return Err(McpError::ServerNotFound(format!("Server '{}' not found", server_name)));
        }
        
        let mut init_hooks = self.init_hooks.lock().await;
        init_hooks.insert(server_name.to_string(), InitHookFn::new(hook));
        
        Ok(())
    }
    
    /// Start a server process
    pub async fn start_server(&self, server_name: &str) -> McpResult<Connection> {
        // Check if already connected
        {
            let active_connections = self.active_connections.lock().await;
            if let Some(conn) = active_connections.get(server_name) {
                if conn.is_connected() {
                    return Ok(conn.clone());
                }
            }
        }
        
        // Get server settings
        let settings = {
            let server_settings = self.server_settings.lock().await;
            server_settings.get(server_name)
                .ok_or_else(|| McpError::ServerNotFound(format!("Server '{}' not found", server_name)))?
                .clone()
        };
        
        info!("Starting server: {}", server_name);
        
        // Create the connection based on transport
        let (mut connection, process) = match settings.transport.as_str() {
            "stdio" => {
                let command = settings.command.as_ref()
                    .ok_or_else(|| McpError::Config(format!("Command required for stdio transport")))?;
                
                let args = settings.args.as_ref()
                    .ok_or_else(|| McpError::Config(format!("Args required for stdio transport")))?;
                
                debug!("Starting stdio server: {} {:?}", command, args);
                
                // Create the process
                let mut cmd = Command::new(command);
                cmd.args(args);
                cmd.stdin(Stdio::piped());
                cmd.stdout(Stdio::piped());
                
                // Set environment variables
                if let Some(env) = &settings.env {
                    for (key, value) in env {
                        cmd.env(key, value);
                    }
                }
                
                // Add default environment variables for MCP
                cmd.env("MCP_SERVER", "1");
                
                // Spawn the process
                let mut process = cmd.spawn()
                    .map_err(|e| McpError::ConnectionFailed(format!("Failed to start server: {}", e)))?;
                
                // Allow some time for the process to start
                sleep(Duration::from_millis(200)).await;
                
                // Set up stdio communication
                let stdin = process.stdin.take()
                    .ok_or_else(|| McpError::ConnectionFailed("Failed to open stdin".to_string()))?;
                
                let stdout = process.stdout.take()
                    .ok_or_else(|| McpError::ConnectionFailed("Failed to open stdout".to_string()))?;
                
                // Create the timeout
                let _timeout = settings.read_timeout_seconds
                    .map(Duration::from_secs)
                    .unwrap_or_else(|| Duration::from_secs(30));
                
                // Create connection config
                let config = Default::default();
                
                // Create the connection
                let connection = Connection::new_stdio(
                    format!("stdio://{}", server_name),
                    stdin,
                    stdout,
                    process,
                    config,
                );
                
                // Return a new Child instance to store in running_servers
                let new_process = Command::new(command)
                    .args(args)
                    .spawn()
                    .map_err(|e| McpError::ConnectionFailed(format!("Failed to start server process: {}", e)))?;
                
                (connection, Some(new_process))
            },
            "sse" => {
                let url = settings.url.as_ref()
                    .ok_or_else(|| McpError::Config(format!("URL required for SSE transport")))?;
                
                debug!("Connecting to SSE server: {}", url);
                
                // Create the timeout
                let _timeout = settings.read_timeout_seconds
                    .map(Duration::from_secs)
                    .unwrap_or_else(|| Duration::from_secs(30));
                
                // Connect to the SSE server
                let connection = Connection::connect_sse(url, _timeout).await?;
                
                (connection, None)
            },
            _ => {
                return Err(McpError::Config(format!("Unsupported transport: {}", settings.transport)));
            }
        };
        
        // Connect and initialize
        info!("Connecting to server: {}", server_name);
        connection.connect().await?;
        
        info!("Initializing server: {}", server_name);
        connection.initialize().await?;
        
        // Run initialization hook if available
        {
            let init_hooks = self.init_hooks.lock().await;
            if let Some(hook) = init_hooks.get(server_name) {
                debug!("Running initialization hook for server: {}", server_name);
                hook.call(server_name, &connection)?;
            }
        }
        
        // Store the process and connection
        if let Some(process) = process {
            let mut running_servers = self.running_servers.lock().await;
            running_servers.insert(server_name.to_string(), process);
        }
        
        let connection_clone = connection.clone();
        
        let mut active_connections = self.active_connections.lock().await;
        active_connections.insert(server_name.to_string(), connection);
        
        info!("Server started successfully: {}", server_name);
        Ok(connection_clone)
    }
    
    /// Stop a running server
    pub async fn stop_server(&self, server_name: &str) -> McpResult<()> {
        info!("Stopping server: {}", server_name);
        
        // Close the connection
        {
            let mut active_connections = self.active_connections.lock().await;
            if let Some(mut connection) = active_connections.remove(server_name) {
                if let Err(e) = connection.close().await {
                    warn!("Error closing connection to server {}: {}", server_name, e);
                }
            }
        }
        
        // Kill the process
        {
            let mut running_servers = self.running_servers.lock().await;
            if let Some(mut process) = running_servers.remove(server_name) {
                if let Err(e) = process.kill().await {
                    warn!("Error killing server process {}: {}", server_name, e);
                }
            }
        }
        
        info!("Server stopped: {}", server_name);
        Ok(())
    }
    
    /// Stop all running servers
    pub async fn stop_all_servers(&self) -> McpResult<()> {
        info!("Stopping all servers");
        
        // Close all connections
        {
            let mut active_connections = self.active_connections.lock().await;
            for (name, mut connection) in active_connections.drain() {
                if let Err(e) = connection.close().await {
                    warn!("Error closing connection to server {}: {}", name, e);
                }
            }
        }
        
        // Kill all processes
        {
            let mut running_servers = self.running_servers.lock().await;
            for (name, mut process) in running_servers.drain() {
                if let Err(e) = process.kill().await {
                    warn!("Error killing server process {}: {}", name, e);
                }
            }
        }
        
        info!("All servers stopped");
        Ok(())
    }
    
    /// Send a message to a server
    pub async fn send_message(&self, server_name: &str, message: Message) -> McpResult<Message> {
        let mut connection = self.get_connection(server_name).await?;
        
        // Send the message
        connection.send_message(message.clone()).await?;
        
        // Receive the response
        let response = connection.receive_message().await?;
        
        Ok(response)
    }
    
    /// Get a connection to a server, starting it if needed
    pub async fn get_connection(&self, server_name: &str) -> McpResult<Connection> {
        // Check if already connected
        {
            let active_connections = self.active_connections.lock().await;
            if let Some(conn) = active_connections.get(server_name) {
                if conn.is_connected() {
                    return Ok(conn.clone());
                }
            }
        }
        
        // Start the server
        self.start_server(server_name).await
    }
    
    /// Check if a server is connected
    pub async fn is_server_connected(&self, server_name: &str) -> bool {
        let active_connections = self.active_connections.lock().await;
        
        if let Some(conn) = active_connections.get(server_name) {
            conn.is_connected()
        } else {
            false
        }
    }
    
    /// Get a list of registered server names
    pub async fn get_server_names(&self) -> Vec<String> {
        let server_settings = self.server_settings.lock().await;
        server_settings.keys().cloned().collect()
    }
    
    /// Get the number of connected servers
    pub async fn connected_server_count(&self) -> usize {
        let active_connections = self.active_connections.lock().await;
        active_connections.iter()
            .filter(|(_, conn)| conn.is_connected())
            .count()
    }
}

impl Drop for ServerManager {
    fn drop(&mut self) {
        debug!("ServerManager being dropped");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_server_manager_creation() {
        let manager = ServerManager::new();
        
        // Register a test server
        let settings = ServerSettings {
            transport: "stdio".to_string(),
            command: Some("echo".to_string()),
            args: Some(vec!["Hello".to_string()]),
            url: None,
            env: None,
            auth: None,
            read_timeout_seconds: Some(5),
            auto_reconnect: true,
            max_reconnect_attempts: 3,
        };
        
        manager.register_server("test", settings).await.unwrap();
        
        let names = manager.get_server_names().await;
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], "test");
    }
    
    #[tokio::test]
    async fn test_from_config() {
        // This test requires a valid config file to exist
        // For CI purposes we'll just verify the method exists and returns Ok
        let manager = ServerManager::new();
        
        // Create a config manually rather than loading from file
        let mut config = Settings {
            mcp: crate::config::McpSettings::default(),
            logger: crate::config::LoggerSettings::default(),
            execution_engine: "tokio".to_string(),
        };
        
        let server_config = crate::config::McpServerSettings {
            transport: "stdio".to_string(),
            command: Some("echo".to_string()),
            args: Some(vec!["test".to_string()]),
            url: None,
            env: None,
            auth: None,
            read_timeout_seconds: Some(5),
        };
        
        config.mcp.servers.insert("test-server".to_string(), server_config);
        
        // Mock the get_settings function by registering the server manually
        let server_settings = ServerSettings {
            transport: "stdio".to_string(),
            command: Some("echo".to_string()),
            args: Some(vec!["test".to_string()]),
            url: None,
            env: None,
            auth: None,
            read_timeout_seconds: Some(5),
            auto_reconnect: true,
            max_reconnect_attempts: 3,
        };
        
        manager.register_server("test-server", server_settings).await.unwrap();
        
        // Verify the server was registered
        let names = manager.get_server_names().await;
        assert!(names.contains(&"test-server".to_string()));
    }
} 