//! Terminal Router implementation
//!
//! Manages I/O routing between terminal interfaces and the agent

use std::sync::Arc;

use tracing::{debug, error, info};

use super::console::ConsoleTerminal;
use super::sync::TerminalSynchronizer;
use super::web::WebTerminalServer;
use super::Terminal;
use super::config::TerminalConfig;
use crate::error::Error;

use tokio::sync::Mutex;

/// Terminal Router connects terminal interfaces with the agent system
#[derive(Debug)]
pub struct TerminalRouter {
    /// Terminal Synchronizer manages I/O between terminals
    sync: Arc<TerminalSynchronizer>,
    /// Console terminal interface
    console: Mutex<Option<Arc<Mutex<ConsoleTerminal>>>>,
    /// Web terminal server interface
    web_terminal: Mutex<Option<Arc<Mutex<WebTerminalServer>>>>,
    /// Terminal configuration
    config: Mutex<TerminalConfig>,
}

impl TerminalRouter {
    /// Create a new terminal router with the provided configuration
    pub fn new(config: TerminalConfig) -> Self {
        Self {
            sync: Arc::new(TerminalSynchronizer::new()),
            console: Mutex::new(None),
            web_terminal: Mutex::new(None),
            config: Mutex::new(config),
        }
    }
    
    /// Start the terminal router with the configured terminal interfaces
    pub async fn start(&self) -> Result<(), Error> {
        info!("Starting terminal router");
        
        let config = self.config.lock().await.clone();
        
        // Initialize console terminal if enabled
        if config.console_enabled {
            debug!("Starting console terminal");
            let console = Arc::new(Mutex::new(ConsoleTerminal::new()));
            
            // Set up input callback
            {
                let sync = self.sync.clone();
                let mut term = console.lock().await;
                
                *term = term.clone().with_input_callback(move |input| {
                    let sync_clone = sync.clone();
                    
                    // Use a blocking executor for this callback to avoid tokio runtime issues
                    let _ = std::thread::spawn(move || {
                        // Create a local runtime for executing this async task
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .unwrap();
                        
                        rt.block_on(async {
                            if let Err(e) = sync_clone.broadcast_input(&input, "console").await {
                                error!("Failed to broadcast console input: {}", e);
                            }
                        });
                    });
                    
                    Ok(())
                });
                
                // Start the console terminal
                term.start().await?;
            }
            
            // Register with synchronizer
            self.sync.register_terminal(console.clone()).await?;
            let mut console_lock = self.console.lock().await;
            *console_lock = Some(console);
        }
        
        // Initialize web terminal if enabled
        if config.web_terminal_enabled {
            debug!("Starting web terminal server");
            let web_terminal = Arc::new(Mutex::new(WebTerminalServer::new(
                config.web_terminal_host.to_string(),
                config.web_terminal_port,
                config.auth_config.clone(),
            )));
            
            // Set up the web terminal server
            {
                let sync = self.sync.clone();
                let mut term = web_terminal.lock().await;
                
                // Configure input callback
                term.set_input_callback(move |input, client_id| {
                    let sync_clone = sync.clone();
                    let source = format!("web-{}", client_id);
                    
                    // Use a blocking executor for this callback to avoid tokio runtime issues
                    let _ = std::thread::spawn(move || {
                        // Create a local runtime for executing this async task
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .unwrap();
                        
                        rt.block_on(async {
                            if let Err(e) = sync_clone.broadcast_input(&input, &source).await {
                                error!("Failed to broadcast web input: {}", e);
                            }
                        });
                    });
                    
                    Ok(())
                });
                
                // Start the web terminal server
                term.start().await?;
            }
            
            // Register with synchronizer
            self.sync.register_terminal(web_terminal.clone()).await?;
            let mut web_lock = self.web_terminal.lock().await;
            *web_lock = Some(web_terminal);
        }
        
        info!("Terminal router started");
        Ok(())
    }
    
    /// Stop the terminal router and all terminal interfaces
    pub async fn stop(&self) -> Result<(), Error> {
        info!("Stopping terminal router");
        
        // Stop console terminal if it exists
        let console = {
            let mut console_lock = self.console.lock().await;
            console_lock.take()
        };
        
        if let Some(console) = &console {
            debug!("Stopping console terminal");
            let mut term = console.lock().await;
            term.stop().await?;
            self.sync.unregister_terminal("console").await?;
        }
        
        // Stop web terminal if it exists
        let web_terminal = {
            let mut web_lock = self.web_terminal.lock().await;
            web_lock.take()
        };
        
        if let Some(web_terminal) = &web_terminal {
            debug!("Stopping web terminal server");
            let mut term = web_terminal.lock().await;
            term.stop().await?;
            self.sync.unregister_terminal("web").await?;
        }
        
        info!("Terminal router stopped");
        Ok(())
    }
    
    /// Toggle the web terminal server on or off
    pub async fn toggle_web_terminal(&self, enabled: bool) -> Result<(), Error> {
        let mut config_lock = self.config.lock().await;
        let config = &mut *config_lock;
        
        if enabled && !self.is_terminal_enabled("web") {
            // Start web terminal if not running
            debug!("Starting web terminal server");
            let web_terminal = Arc::new(Mutex::new(WebTerminalServer::new(
                config.web_terminal_host.to_string(),
                config.web_terminal_port,
                config.auth_config.clone(),
            )));
            
            // Set up the web terminal server
            {
                let sync = self.sync.clone();
                let mut term = web_terminal.lock().await;
                
                // Configure input callback
                term.set_input_callback(move |input, client_id| {
                    let sync_clone = sync.clone();
                    let source = format!("web-{}", client_id);
                    
                    // Use a blocking executor for this callback to avoid tokio runtime issues
                    let _ = std::thread::spawn(move || {
                        // Create a local runtime for executing this async task
                        let rt = tokio::runtime::Builder::new_current_thread()
                            .enable_all()
                            .build()
                            .unwrap();
                        
                        rt.block_on(async {
                            if let Err(e) = sync_clone.broadcast_input(&input, &source).await {
                                error!("Failed to broadcast web input: {}", e);
                            }
                        });
                    });
                    
                    Ok(())
                });
                
                // Start the web terminal server
                term.start().await?;
            }
            
            // Register with synchronizer
            self.sync.register_terminal(web_terminal.clone()).await?;
            let mut web_lock = self.web_terminal.lock().await;
            *web_lock = Some(web_terminal);
            config.web_terminal_enabled = true;
            
        } else if !enabled && self.is_terminal_enabled("web") {
            // Stop web terminal if running
            debug!("Stopping web terminal server");
            let web_terminal = {
                let mut web_lock = self.web_terminal.lock().await;
                web_lock.take()
            };
            
            if let Some(web_terminal) = &web_terminal {
                let mut term = web_terminal.lock().await;
                term.stop().await?;
                self.sync.unregister_terminal("web").await?;
            }
            
            config.web_terminal_enabled = false;
        }
        
        Ok(())
    }
    
    /// Write data to all connected terminals
    pub async fn write(&self, data: &str) -> Result<(), Error> {
        self.sync.broadcast_output(data).await
    }
    
    /// Read the next input from any terminal (blocks until input is available)
    pub async fn read(&self) -> Result<String, Error> {
        self.sync.get_next_input().await
    }
    
    /// Check if a specific terminal type is enabled
    pub fn is_terminal_enabled(&self, terminal_type: &str) -> bool {
        match terminal_type {
            "console" => tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    self.console.lock().await.is_some()
                })
            }),
            "web" => tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    self.web_terminal.lock().await.is_some()
                })
            }),
            _ => false,
        }
    }
    
    /// Get the web terminal address if enabled
    pub fn web_terminal_address(&self) -> Option<String> {
        if self.is_terminal_enabled("web") {
            let config = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    self.config.lock().await.clone()
                })
            });
            Some(format!("{}:{}", config.web_terminal_host, config.web_terminal_port))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::config::{AuthConfig, AuthMethod};
    
    #[tokio::test]
    async fn test_router_console_only() {
        // Create a console-only configuration
        let config = TerminalConfig::console_only();
        
        // Create and start the router
        let router = TerminalRouter::new(config);
        let result = router.start().await;
        assert!(result.is_ok());
        
        // Check that only console is enabled
        assert!(router.is_terminal_enabled("console"));
        assert!(!router.is_terminal_enabled("web"));
        
        // Stop the router
        let result = router.stop().await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_router_toggle_web() {
        // Create a console-only configuration
        let config = TerminalConfig::console_only();
        
        // Create and start the router
        let router = TerminalRouter::new(config);
        let result = router.start().await;
        assert!(result.is_ok());
        
        // Initially, only console should be enabled
        assert!(router.is_terminal_enabled("console"));
        assert!(!router.is_terminal_enabled("web"));
        
        // Toggle web terminal on
        let result = router.toggle_web_terminal(true).await;
        assert!(result.is_ok());
        assert!(router.is_terminal_enabled("web"));
        
        // Check web terminal address
        let addr = router.web_terminal_address();
        assert!(addr.is_some());
        
        // Toggle web terminal off
        let result = router.toggle_web_terminal(false).await;
        assert!(result.is_ok());
        assert!(!router.is_terminal_enabled("web"));
        
        // Stop the router
        let result = router.stop().await;
        assert!(result.is_ok());
    }
} 