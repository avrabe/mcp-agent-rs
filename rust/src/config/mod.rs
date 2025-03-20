use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs::File;
use std::io::Read;

use crate::utils::error::{McpError, McpResult};

/// Settings for MCP Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// MCP-specific settings
    #[serde(default)]
    pub mcp: McpSettings,
    
    /// Logger settings
    #[serde(default)]
    pub logger: LoggerSettings,
    
    /// Execution engine settings
    #[serde(default)]
    pub execution_engine: String,
}

/// Settings for MCP
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct McpSettings {
    /// MCP server configurations
    #[serde(default)]
    pub servers: HashMap<String, McpServerSettings>,
}

/// Settings for a specific MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerSettings {
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
    pub auth: Option<McpServerAuthSettings>,
    
    /// Read timeout in seconds
    pub read_timeout_seconds: Option<u64>,
}

/// Authentication settings for an MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerAuthSettings {
    /// Authentication type
    #[serde(default)]
    pub auth_type: String,
    
    /// API key
    pub api_key: Option<String>,
    
    /// Token
    pub token: Option<String>,
    
    /// Username
    pub username: Option<String>,
    
    /// Password
    pub password: Option<String>,
}

/// Logger settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoggerSettings {
    /// Log transports (file, console)
    #[serde(default)]
    pub transports: Vec<String>,
    
    /// Log level
    #[serde(default = "default_log_level")]
    pub level: String,
    
    /// Log file path
    pub path: Option<String>,
    
    /// Show progress indicators
    #[serde(default)]
    pub show_progress: bool,
    
    /// Path settings for dynamic log paths
    pub path_settings: Option<LogPathSettings>,
}

/// Settings for dynamic log file paths
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogPathSettings {
    /// Pattern for the log file path
    pub path_pattern: String,
    
    /// Unique ID type (timestamp, session_id)
    pub unique_id: String,
    
    /// Timestamp format
    pub timestamp_format: Option<String>,
}

fn default_transport() -> String {
    "stdio".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

/// Load settings from a YAML file
pub fn load_settings<P: AsRef<Path>>(path: P) -> McpResult<Settings> {
    let mut file = File::open(path).map_err(|e| McpError::Config(format!("Failed to open config file: {}", e)))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).map_err(|e| McpError::Config(format!("Failed to read config file: {}", e)))?;
    
    let settings: Settings = serde_yaml::from_str(&contents)
        .map_err(|e| McpError::Config(format!("Failed to parse config file: {}", e)))?;
    
    Ok(settings)
}

/// Get settings, optionally from a specific file
pub fn get_settings(config_path: Option<&str>) -> McpResult<Settings> {
    match config_path {
        Some(path) => load_settings(path),
        None => {
            // Try to find config file in common locations
            let default_paths = vec![
                "mcp_agent.config.yaml",
                "config/mcp_agent.config.yaml",
                "../mcp_agent.config.yaml",
            ];
            
            for path in default_paths {
                if Path::new(path).exists() {
                    return load_settings(path);
                }
            }
            
            // Return default settings if no config file is found
            Ok(Settings {
                mcp: McpSettings::default(),
                logger: LoggerSettings::default(),
                execution_engine: "asyncio".to_string(),
            })
        }
    }
} 