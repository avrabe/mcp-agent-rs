//! Terminal configuration

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

/// Configuration for the terminal system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalConfig {
    /// Enable console terminal
    #[serde(default = "default_console_enabled")]
    pub console_terminal_enabled: bool,

    /// Enable web terminal
    #[serde(default = "default_web_enabled")]
    pub web_terminal_enabled: bool,

    /// Web terminal configuration
    #[serde(default)]
    pub web_terminal_config: WebTerminalConfig,

    /// Terminal input timeout (0 means no timeout)
    #[serde(default = "default_input_timeout")]
    pub input_timeout_secs: u64,

    /// Maximum message size for terminal I/O in bytes
    #[serde(default = "default_max_message_size")]
    pub max_message_size: usize,
}

/// Web terminal configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebTerminalConfig {
    /// Web terminal host address
    #[serde(default = "default_web_host")]
    pub host: IpAddr,

    /// Web terminal por
    #[serde(default = "default_web_port")]
    pub port: u16,

    /// Authentication configuration
    #[serde(default)]
    pub auth_config: AuthConfig,

    /// Enable visualization for the web terminal
    #[serde(default = "default_enable_visualization")]
    pub enable_visualization: bool,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Authentication method
    #[serde(default)]
    pub auth_method: AuthMethod,

    /// JWT secret for token-based authentication
    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,

    /// Token expiration time in seconds
    #[serde(default = "default_token_expiration")]
    pub token_expiration_secs: u64,

    /// Username for basic authentication
    #[serde(default = "default_username")]
    pub username: String,

    /// Password for basic authentication
    #[serde(default = "default_password")]
    pub password: String,

    /// Allow anonymous access (no authentication)
    #[serde(default)]
    pub allow_anonymous: bool,

    /// Require authentication for all requests
    #[serde(default = "default_require_auth")]
    pub require_authentication: bool,
}

/// Authentication methods
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// No authentication
    None,
    /// Basic username/password authentication
    Basic,
    /// JWT token-based authentication
    Jwt,
}

impl Default for AuthMethod {
    fn default() -> Self {
        AuthMethod::Jwt
    }
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self {
            console_terminal_enabled: default_console_enabled(),
            web_terminal_enabled: default_web_enabled(),
            web_terminal_config: WebTerminalConfig::default(),
            input_timeout_secs: default_input_timeout(),
            max_message_size: default_max_message_size(),
        }
    }
}

impl Default for WebTerminalConfig {
    fn default() -> Self {
        Self {
            host: default_web_host(),
            port: default_web_port(),
            auth_config: AuthConfig::default(),
            enable_visualization: default_enable_visualization(),
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            auth_method: AuthMethod::default(),
            jwt_secret: default_jwt_secret(),
            token_expiration_secs: default_token_expiration(),
            username: default_username(),
            password: default_password(),
            allow_anonymous: false,
            require_authentication: default_require_auth(),
        }
    }
}

impl TerminalConfig {
    /// Create a new terminal config
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a terminal config with only console enabled
    pub fn console_only() -> Self {
        Self {
            console_terminal_enabled: true,
            web_terminal_enabled: false,
            ..Default::default()
        }
    }

    /// Create a terminal config with only web terminal enabled
    pub fn web_only() -> Self {
        Self {
            console_terminal_enabled: false,
            web_terminal_enabled: true,
            ..Default::default()
        }
    }

    /// Create a terminal config with both console and web terminal enabled
    pub fn dual_terminal() -> Self {
        Self {
            console_terminal_enabled: true,
            web_terminal_enabled: true,
            ..Default::default()
        }
    }

    /// Get the web terminal socket address
    pub fn web_socket_addr(&self) -> SocketAddr {
        SocketAddr::new(self.web_terminal_config.host, self.web_terminal_config.port)
    }

    /// Get the input timeout as a Duration
    pub fn input_timeout(&self) -> Option<Duration> {
        if self.input_timeout_secs == 0 {
            None
        } else {
            Some(Duration::from_secs(self.input_timeout_secs))
        }
    }
}

fn default_console_enabled() -> bool {
    true
}

fn default_web_enabled() -> bool {
    false
}

fn default_web_host() -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
}

fn default_web_port() -> u16 {
    8888
}

fn default_require_auth() -> bool {
    true
}

fn default_jwt_secret() -> String {
    // In production, this would be randomly generated and stored securely
    "change_this_to_a_secure_random_value".to_string()
}

fn default_token_expiration() -> u64 {
    86400 // 24 hours
}

fn default_username() -> String {
    "admin".to_string()
}

fn default_password() -> String {
    "change_me".to_string()
}

fn default_input_timeout() -> u64 {
    0 // No timeout by defaul
}

fn default_max_message_size() -> usize {
    16 * 1024 // 16 KB
}

fn default_enable_visualization() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TerminalConfig::default();
        assert!(config.console_terminal_enabled);
        assert!(!config.web_terminal_enabled);
        assert_eq!(
            config.web_terminal_config.host,
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
        );
        assert_eq!(config.web_terminal_config.port, 8888);
        assert!(
            config
                .web_terminal_config
                .auth_config
                .require_authentication
        );
    }

    #[test]
    fn test_web_socket_addr() {
        let config = TerminalConfig::default();
        let addr = config.web_socket_addr();
        assert_eq!(addr.to_string(), "127.0.0.1:8888");
    }

    #[test]
    fn test_dual_terminal() {
        let config = TerminalConfig::dual_terminal();
        assert!(config.console_terminal_enabled);
        assert!(config.web_terminal_enabled);
    }

    #[test]
    fn test_input_timeout() {
        let mut config = TerminalConfig::default();
        assert_eq!(config.input_timeout(), None);

        config.input_timeout_secs = 30;
        assert_eq!(config.input_timeout(), Some(Duration::from_secs(30)));
    }
}
