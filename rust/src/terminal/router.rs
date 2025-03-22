//! Terminal Router implementation
//!
//! Manages I/O routing between terminal interfaces and the agen

use log::{debug, error, info, trace, warn};
use std::sync::Arc;

use super::console::ConsoleTerminal;
use super::sync::TerminalSynchronizer;
// use super::web::WebTerminalServer;
use super::config::TerminalConfig;
use super::config::WebTerminalConfig;
use super::AsyncTerminal;
use super::Terminal;
use crate::error::{Error, Result};
use tokio::sync::mpsc;

use tokio::sync::Mutex as TokioMutex;

use super::graph::GraphManager;
use super::web::WebTerminal;
use crate::mcp::agent::Agent;
use crate::workflow::engine::WorkflowEngine;

use once_cell::sync::OnceCell;

use super::AsyncTerminal;
use crate::terminal::terminal_helpers;
use tokio::sync::mpsc;

// Global terminal router instance
static TERMINAL_ROUTER: OnceCell<Arc<TerminalRouter>> = OnceCell::new();

/// Terminal router for managing multiple terminals
#[derive(Debug)]
pub struct TerminalRouter {
    /// Terminal configuration
    config: TerminalConfig,
    /// Terminal synchronizer for broadcasting to all terminals
    synchronizer: Arc<TokioMutex<TerminalSynchronizer>>,
    /// Console terminal (optional)
    console_terminal: TokioMutex<Option<Arc<dyn Terminal>>>,
    /// Web terminal (optional)
    web_terminal: TokioMutex<Option<Arc<dyn Terminal>>>,
    /// Graph visualization manager (optional)
    graph_manager: TokioMutex<Option<Arc<GraphManager>>>,
    /// Default terminal
    default_terminal: String,
}

impl TerminalRouter {
    /// Create a new terminal router
    pub fn new(config: TerminalConfig) -> Self {
        let synchronizer = Arc::new(TokioMutex::new(TerminalSynchronizer::new()));

        Self {
            config,
            synchronizer,
            console_terminal: TokioMutex::new(None),
            web_terminal: TokioMutex::new(None),
            graph_manager: TokioMutex::new(None),
            default_terminal: "console".to_string(),
        }
    }

    /// Start terminal(s) based on configuration
    pub async fn start(&self) -> Result<()> {
        info!("Starting terminal router");

        // Initialize console terminal if enabled
        if self.config.console_terminal_enabled {
            debug!("Initializing console terminal");
            let mut console = ConsoleTerminal::new("console".to_string());

            // Start the console terminal
            console.start().await?;

            // Create an Arc<dyn Terminal> from the ConsoleTerminal
            let console_arc: Arc<dyn Terminal> = Arc::new(console);

            // Register terminal with synchronizer
            let mut synchronizer = self.synchronizer.lock().await;
            let term_id = console_arc.id_sync().await?;
            synchronizer.register_terminal(console_arc.clone()).await?;
            drop(synchronizer);

            let mut console_lock = self.console_terminal.lock().await;
            *console_lock = Some(console_arc);
        }

        // Initialize web terminal if enabled
        if self.config.web_terminal_enabled {
            debug!("Initializing web terminal");
            // Use the WebTerminalConfig directly from the config
            let web_config = self.config.web_terminal_config.clone();

            let mut web = WebTerminal::new();
            web.set_config(web_config);
            web.set_id("web".to_string());

            web.start().await?;

            // Create an Arc<dyn Terminal> from the WebTerminal
            let web_arc: Arc<dyn Terminal> = Arc::new(web);

            // Register terminal with synchronizer
            let mut synchronizer = self.synchronizer.lock().await;
            let term_id = web_arc.id_sync().await?;
            synchronizer.register_terminal(web_arc.clone()).await?;
            drop(synchronizer);

            let mut web_lock = self.web_terminal.lock().await;
            *web_lock = Some(web_arc);
        }

        info!("Terminal router started");
        Ok(())
    }

    /// Stop all terminals
    pub async fn stop(&self) -> Result<()> {
        info!("Stopping terminal router");

        // Stop console terminal if it exists
        {
            let mut console_lock = self.console_terminal.lock().await;
            if let Some(console) = console_lock.take() {
                debug!("Stopping console terminal");
                let id = console.id_sync().await?;

                let mut_console = Arc::get_mut(&mut console.clone()).unwrap();
                mut_console.stop_sync().await?;

                // Unregister from synchronizer
                let mut synchronizer = self.synchronizer.lock().await;
                synchronizer.unregister_terminal(&id).await?;
            }
        }

        // Stop web terminal if it exists
        {
            let mut web_lock = self.web_terminal.lock().await;
            if let Some(web) = web_lock.take() {
                debug!("Stopping web terminal");
                let id = web.id_sync().await?;

                let mut_web: &mut dyn Terminal = Arc::get_mut(&mut web.clone()).unwrap();
                mut_web.stop_sync().await?;

                // Unregister from synchronizer
                let mut synchronizer = self.synchronizer.lock().await;
                synchronizer.unregister_terminal(&id).await?;
            }
        }

        info!("Terminal router stopped");
        Ok(())
    }

    /// Toggle web terminal on/off
    pub async fn toggle_web_terminal(&self, enable: bool) -> Result<()> {
        let mut web_lock = self.web_terminal.lock().await;

        if enable && web_lock.is_none() {
            debug!("Enabling web terminal");

            // Create and start web terminal
            // Use the WebTerminalConfig directly from the config
            let web_config = self.config.web_terminal_config.clone();

            let mut web = WebTerminal::new();
            web.set_config(web_config);
            web.set_id("web".to_string());

            web.start().await?;

            // Create an Arc<dyn Terminal> from the WebTerminal
            let web_arc: Arc<dyn Terminal> = Arc::new(web);

            // Register with synchronizer
            let mut synchronizer = self.synchronizer.lock().await;
            let term_id = web_arc.id_sync().await?;
            synchronizer.register_terminal(web_arc.clone()).await?;

            *web_lock = Some(web_arc);
        } else if !enable && web_lock.is_some() {
            debug!("Disabling web terminal");

            // Stop and remove web terminal
            if let Some(web) = web_lock.take() {
                let id = web.id_sync().await?;

                let mut_web: &mut dyn Terminal = Arc::get_mut(&mut web.clone()).unwrap();
                mut_web.stop_sync().await?;

                // Unregister from synchronizer
                let mut synchronizer = self.synchronizer.lock().await;
                synchronizer.unregister_terminal(&id).await?;
            }
        }

        Ok(())
    }

    /// Write data to all terminals
    pub async fn write(&self, data: &str) -> Result<()> {
        debug!("Terminal router writing data: {}", data);

        // Write to console terminal if it exists
        {
            let console_lock = self.console_terminal.lock().await;
            if let Some(console) = &self.console_terminal.lock().await.as_ref() {
                if let Err(e) = console.display_sync(&data).await {
                    error!("Error writing to console terminal: {}", e);
                }
            }
        }

        // Write to web terminal if it exists
        {
            let web_lock = self.web_terminal.lock().await;
            if let Some(web) = &self.web_terminal.lock().await.as_ref() {
                if let Err(e) = web.display_sync(&data).await {
                    error!("Error writing to web terminal: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Read data from the first available terminal
    pub async fn read(&self) -> Result<String> {
        // In a real implementation, this would read from the active terminal
        // or provide a way to select which terminal to read from

        // Just a placeholder implementation
        Ok("Input not implemented".to_string())
    }

    /// Check if a specific terminal type is enabled
    pub async fn is_terminal_enabled(&self, terminal_type: &str) -> bool {
        match terminal_type {
            "console" => self.console_terminal.lock().await.is_some(),
            "web" => self.web_terminal.lock().await.is_some(),
            _ => false,
        }
    }

    /// Get the web terminal address if enabled
    pub async fn web_terminal_address(&self) -> Option<String> {
        if self.is_terminal_enabled("web").await {
            Some(format!(
                "{}:{}",
                self.config.web_terminal_config.host, self.config.web_terminal_config.port
            ))
        } else {
            None
        }
    }

    /// Initialize visualization for a terminal
    pub async fn initialize_visualization(
        &mut self,
        graph_manager: Arc<GraphManager>,
    ) -> Result<()> {
        info!("Initializing visualization with graph manager");

        // Store the graph manager
        let mut gm = self.graph_manager.lock().await;
        *gm = Some(graph_manager);

        Ok(())
    }

    /// Register a graph manager with the terminal router
    pub async fn register_graph_manager(&self, graph_manager: Arc<GraphManager>) -> Result<()> {
        // Store graph manager
        let mut gm = self.graph_manager.lock().await;
        *gm = Some(graph_manager.clone());

        // Notify web terminal if available
        let web_terminal = self.web_terminal.lock().await;
        if let Some(web_terminal) = &*web_terminal {
            // Set graph manager ID
            let graph_manager_id = format!("{}-{}", "default", Uuid::new_v4());
            let (tx, rx) = tokio::sync::oneshot::channel();

            match web_terminal
                .execute_command_sync(&format!("SET_GRAPH_MANAGER:{}", graph_manager_id), tx)
                .await
            {
                Ok(_) => match rx.await {
                    Ok(_) => {
                        info!("Graph manager registered with web terminal");
                        Ok(())
                    }
                    Err(e) => {
                        error!("Failed to receive response from web terminal: {}", e);
                        Err(Error::TerminalError(
                            "Failed to receive response from web terminal".to_string(),
                        ))
                    }
                },
                Err(e) => {
                    error!("Failed to register graph manager with web terminal: {}", e);
                    Err(e)
                }
            }
        } else {
            // Web terminal not available, but this is not an error
            info!("Web terminal not available for graph manager registration");
            Ok(())
        }
    }

    /// Register a console terminal
    pub async fn register_console_terminal(&self, terminal: ConsoleTerminal) -> Result<()> {
        let mut console_lock = self.console_terminal.lock().await;
        *console_lock = Some(Arc::new(terminal));
        Ok(())
    }

    /// Register a web terminal
    pub async fn register_web_terminal(&self, terminal: WebTerminal) -> Result<()> {
        let mut web_lock = self.web_terminal.lock().await;
        *web_lock = Some(Arc::new(terminal));
        Ok(())
    }

    /// Set the default terminal
    pub fn set_default_terminal(&mut self, terminal_type: &str) {
        self.default_terminal = terminal_type.to_string();
    }

    /// Get the console terminal
    pub async fn console_terminal(&self) -> Option<Arc<dyn Terminal>> {
        let console = self.console_terminal.lock().await;
        console.clone()
    }

    /// Get the web terminal
    pub async fn web_terminal(&self) -> Option<Arc<dyn Terminal>> {
        let web = self.web_terminal.lock().await;
        web.clone()
    }

    /// Get the default terminal
    pub async fn default_terminal(&self) -> Option<Arc<dyn Terminal>> {
        match self.default_terminal.as_str() {
            "console" => self.console_terminal().await,
            "web" => self.web_terminal().await,
            _ => self.console_terminal().await,
        }
    }

    /// Display output to all terminals
    pub async fn display_all(&self, output: &str) -> Result<()> {
        if let Some(console) = self.console_terminal().await {
            let _ = terminal_helpers::display(&*console, output).await;
        }

        if let Some(web) = self.web_terminal().await {
            let _ = terminal_helpers::display(&*web, output).await;
        }

        Ok(())
    }

    /// Execute a command on the default terminal
    pub async fn execute_command(&self, command: &str) -> Result<String> {
        let terminal = self
            .default_terminal()
            .await
            .ok_or_else(|| Error::TerminalError("No default terminal available".into()))?;
        let (tx, rx) = tokio::sync::oneshot::channel::<String>();
        terminal_helpers::execute_command(&*terminal, command, tx).await?;
        rx.await
            .map_err(|e| Error::TerminalError(format!("Failed to receive command result: {}", e)))
    }

    /// Execute a command on the web terminal with a graph manager
    pub async fn set_graph_manager(&self, graph_manager_id: &str) -> Result<()> {
        // Set graph manager ID for web terminal if available
        if let Some(web_terminal) = self.web_terminal().await {
            let (tx, rx) = tokio::sync::oneshot::channel();

            match web_terminal
                .execute_command_sync(&format!("SET_GRAPH_MANAGER:{}", graph_manager_id), tx)
                .await
            {
                Ok(_) => match rx.await {
                    Ok(response) => {
                        info!("Set graph manager response: {}", response);
                        Ok(())
                    }
                    Err(e) => {
                        error!("Failed to receive response from web terminal: {}", e);
                        Err(Error::TerminalError(
                            "Failed to receive response from web terminal".to_string(),
                        ))
                    }
                },
                Err(e) => {
                    error!("Failed to set graph manager for web terminal: {}", e);
                    Err(Error::TerminalError(format!(
                        "Failed to set graph manager: {}",
                        e
                    )))
                }
            }
        } else {
            warn!("Web terminal not available for setting graph manager");
            Ok(())
        }
    }

    async fn initialize_terminal(&self, terminal_id: &str) -> Result<()> {
        debug!("Initializing terminal: {}", terminal_id);

        // No graph provider to initialize
        Ok(())
    }

    pub async fn get_terminal(&self, id: &str) -> Result<Arc<dyn Terminal>> {
        let terminals = self.terminals.lock().await;

        if let Some(terminal) = terminals.get(id) {
            Ok(terminal.clone())
        } else {
            Err(Error::TerminalError(format!("Terminal not found: {}", id)))
        }
    }

    async fn broadcast_to_terminals(&self, message: &str) -> Result<()> {
        let terminals = self.terminals.lock().await;

        if terminals.is_empty() {
            return Err(Error::TerminalError(format!(
                "No terminals available to broadcast message"
            )));
        }

        for terminal in terminals.values() {
            if let Err(e) = terminal_helpers::display(terminal, message).await {
                error!("Failed to broadcast to terminal: {}", e);
            }
        }

        Ok(())
    }

    pub async fn send_input_to_terminal(&self, terminal_id: &str, input: &str) -> Result<()> {
        let terminals = self.terminals.lock().await;

        if let Some(terminal) = terminals.get(terminal_id) {
            let terminal = terminal.clone();
            drop(terminals);

            terminal_helpers::echo_input(&*terminal, input).await
        } else {
            Err(Error::TerminalError(format!(
                "Terminal not found: {}",
                terminal_id
            )))
        }
    }
}

impl Default for TerminalRouter {
    fn default() -> Self {
        Self::new(TerminalConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::super::config::{AuthConfig, AuthMethod};
    use super::*;

    #[tokio::test]
    async fn test_router_console_only() {
        let router = TerminalRouter::new(TerminalConfig::console_only());
        let result = router.start().await;
        assert!(result.is_ok());

        assert!(router.is_terminal_enabled("console").await);
        assert!(!router.is_terminal_enabled("web").await);

        let result = router.stop().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_router_toggle_web() {
        let config = TerminalConfig::dual_terminal();
        let router = TerminalRouter::new(config);
        let result = router.start().await;
        assert!(result.is_ok());

        assert!(router.is_terminal_enabled("console").await);
        assert!(router.is_terminal_enabled("web").await);

        let result = router.toggle_web_terminal(false).await;
        assert!(result.is_ok());
        assert!(!router.is_terminal_enabled("web").await);

        let result = router.stop().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_register_graph_manager() {
        let config = TerminalConfig::default();
        let router = TerminalRouter::new(config);
        let graph_manager = Arc::new(GraphManager::new());

        let result = router.register_graph_manager(graph_manager.clone()).await;
        assert!(result.is_ok());

        let stored_manager = router.graph_manager.lock().await;
        assert!(stored_manager.is_some());

        let id1 = graph_manager.id().await.unwrap();
        let id2 = stored_manager.as_ref().unwrap().id().await.unwrap();
        assert_eq!(id1, id2);
    }

    #[tokio::test]
    async fn test_register_and_get_terminals() {
        let config = TerminalConfig::default();
        let router = TerminalRouter::new(config);

        assert!(router.console_terminal().await.is_none());
        assert!(router.web_terminal().await.is_none());

        let console = ConsoleTerminal::new();
        router.register_console_terminal(console).await.unwrap();

        assert!(router.console_terminal().await.is_some());

        assert!(router.default_terminal().await.is_some());
    }
}

/// Initialize the terminal system with the given configuration
pub async fn initialize_terminal(config: TerminalConfig) -> Result<Arc<TerminalRouter>> {
    debug!("Initializing terminal system with config: {:?}", config);

    // Create the terminal router
    let router = TerminalRouter::new(config);

    // Start the router
    router.start().await?;

    // Create an Arc and set it as the global instance
    let router_arc = Arc::new(router);
    if let Err(_) = TERMINAL_ROUTER.set(router_arc.clone()) {
        error!("Failed to set global terminal router instance, it was already set");
    }

    // Return the router instance
    Ok(router_arc)
}

/// Initialize the visualization system
pub async fn initialize_visualization(
    config: &TerminalConfig,
    workflow_engine: Option<Arc<WorkflowEngine>>,
    agents: Vec<Arc<Agent>>,
) -> Result<Arc<GraphManager>> {
    info!("Initializing visualization system");

    // Create the graph manager
    let graph_manager = match super::graph::initialize_visualization(workflow_engine, agents).await
    {
        Ok(gm) => {
            info!("Graph manager successfully created");
            gm
        }
        Err(e) => {
            error!("Failed to initialize graph visualization: {}", e);
            return Err(e);
        }
    };

    // If web terminal is enabled, register the graph manager
    if config.web_terminal_enabled {
        if let Some(router) = TERMINAL_ROUTER.get() {
            match router.register_graph_manager(graph_manager.clone()).await {
                Ok(_) => {
                    info!("Graph manager successfully registered with terminal router");
                }
                Err(e) => {
                    // Log the error but continue - we still return the graph manager
                    error!(
                        "Failed to register graph manager with terminal router: {}",
                        e
                    );
                    warn!("Visualization may not be fully functional");
                }
            }
        } else {
            warn!("Terminal router not initialized; visualization registration skipped");
        }
    } else if config.web_terminal_config.enable_visualization {
        warn!(
            "Visualization is enabled in config, but web terminal is disabled; visualization will not be available"
        );
    }

    Ok(graph_manager)
}

/// Get the global terminal router instance
pub fn get_terminal_router() -> Option<Arc<TerminalRouter>> {
    TERMINAL_ROUTER.get().cloned()
}
