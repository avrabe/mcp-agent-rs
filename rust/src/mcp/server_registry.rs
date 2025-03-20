use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::timeout;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::mcp::connection::Connection;
use crate::utils::error::{McpError, McpResult};
use crate::mcp::protocol::McpProtocol;
use crate::mcp::types::{Message, MessageType, Priority, MessageId};
use serde_yaml;

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
        }
    }
}

/// Type alias for initialization hook function
pub type InitHook = Box<dyn Fn(&str, &Message) -> McpResult<()> + Send + Sync>;

/// Registry of MCP servers with configuration settings
pub struct ServerRegistry {
    /// Map of server name to server settings
    servers: HashMap<String, ServerConfig>,
    /// Map of server name to initialization hooks
    init_hooks: HashMap<String, InitHook>,
}

impl std::fmt::Debug for ServerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerRegistry")
            .field("servers", &self.servers)
            .field("init_hooks", &format!("{} hooks", self.init_hooks.len()))
            .finish()
    }
}

impl ServerRegistry {
    /// Creates a new server registry.
    pub fn new() -> Self {
        Self {
            servers: HashMap::new(),
            init_hooks: HashMap::new(),
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

    /// Connects to a server by name.
    pub async fn connect_to_server(&self, name: &str) -> McpResult<Connection> {
        let config = self.servers.get(name)
            .ok_or_else(|| McpError::ServerNotFound(format!("Server not found: {}", name)))?;
        
        // Create a new connection
        let connection = Connection::connect_stdio(
            &config.command,
            &config.args,
            &config.env,
            config.read_timeout,
        ).await?;
        
        Ok(connection)
    }

    /// Start a server based on its configuration
    pub async fn start_server(&self, server_name: &str) -> McpResult<Connection> {
        let config = self.servers.get(server_name)
            .ok_or_else(|| McpError::ServerNotFound(format!("Server not found: {}", server_name)))?;
        
        // Connect to the server based on transport type
        let connection = match config.transport {
            Transport::Stdio => {
                Connection::connect_stdio(
                    &config.command,
                    &config.args,
                    &config.env,
                    config.read_timeout,
                ).await?
            },
            Transport::Sse => {
                if let Some(url) = &config.url {
                    Connection::connect_sse(url, config.read_timeout).await?
                } else {
                    return Err(McpError::Config("SSE transport requires URL".to_string()));
                }
            },
        };
        
        // Run initialization hook if available
        if let Some(hook) = self.init_hooks.get(server_name) {
            let init_msg = Message::new(
                MessageType::Request,
                Priority::Normal,
                Vec::new(),
                None,
                None,
            );
            
            hook(server_name, &init_msg)?;
        }
        
        Ok(connection)
    }

    /// Get the configuration for a specific server
    pub fn get_server_config(&self, server_name: &str) -> McpResult<&ServerConfig> {
        self.servers.get(server_name)
            .ok_or_else(|| McpError::ServerNotFound(format!("Server not found: {}", server_name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    fn create_test_server_settings() -> ServerSettings {
        ServerSettings {
            name: Some("test-server".to_string()),
            description: Some("Test server".to_string()),
            transport: Transport::Stdio,
            command: Some("echo".to_string()),
            args: Some(vec!["test".to_string()]),
            read_timeout_seconds: Some(5),
            url: None,
            auth: None,
            roots: None,
            env: None,
        }
    }

    #[tokio::test]
    async fn test_server_registry_creation() {
        let mut settings = McpSettings {
            servers: HashMap::new(),
        };
        settings
            .servers
            .insert("test-server".to_string(), create_test_server_settings());

        let mut registry = ServerRegistry::new();
        registry.register_server("test-server", ServerConfig {
            command: "echo".to_string(),
            args: vec!["test".to_string()],
            env: HashMap::new(),
            read_timeout: Duration::from_secs(5),
            transport: Transport::Stdio,
            url: None,
        }).unwrap();
        
        assert!(registry.get_server_config("test-server").is_ok());
    }
    
    #[test]
    fn test_register_server() -> McpResult<()> {
        let mut registry = ServerRegistry::new();
        let config = ServerConfig {
            command: "test-command".to_string(),
            args: vec!["--arg1".to_string(), "--arg2".to_string()],
            env: HashMap::new(),
            read_timeout: Duration::from_secs(30),
            transport: Transport::Stdio,
            url: None,
        };
        
        registry.register_server("test-server", config)?;
        
        assert!(registry.servers.contains_key("test-server"));
        Ok(())
    }
    
    #[test]
    fn test_register_init_hook() -> McpResult<()> {
        let mut registry = ServerRegistry::new();
        let config = ServerConfig {
            command: "test-command".to_string(),
            args: vec![],
            env: HashMap::new(),
            read_timeout: Duration::from_secs(30),
            transport: Transport::Stdio,
            url: None,
        };
        
        registry.register_server("test-server", config)?;
        
        let hook = Box::new(|_: &str, _: &Message| -> McpResult<()> { Ok(()) });
        registry.register_init_hook("test-server", hook)?;
        
        assert!(registry.init_hooks.contains_key("test-server"));
        Ok(())
    }
} 