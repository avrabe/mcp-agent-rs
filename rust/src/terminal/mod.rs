//! Terminal System
//!
//! This module provides a unified terminal interface layer that supports both
//! console and web-based terminals. It manages I/O synchronization between
//! multiple terminal interfaces and provides a clean API for the agent to
//! interact with terminals.
//!
//! Key components:
//!
//! - Terminal Router: Connects terminal interfaces to the agen
//! - Terminal Synchronizer: Manages I/O between terminals
//! - Console Terminal: Console-based implementation
//! - Web Terminal Server: Web-based implementation
//! - Graph visualization for workflows, agents, and LLM integration

pub mod config;
pub mod console;
pub mod graph;
pub mod router;
pub mod sync;
pub mod web;

use crate::error::{Error, Result};
use crate::mcp::agent::Agent;
use crate::terminal::graph::providers::{AsyncGraphDataProvider, GraphDataProvider};
use crate::workflow::engine::WorkflowEngine;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
// Comment out missing imports for now - these would be implemented based on LLM and human input systems
// use crate::llm::LlmProvider;
// use crate::human_input::HumanInputProvider;
use futures::future::BoxFuture;
use log::{debug, error, info, warn};
use std::fmt;
use std::io::Write;

/// Terminal configuration
pub use config::{AuthConfig, TerminalConfig, WebTerminalConfig};

/// Console terminal
pub use console::ConsoleTerminal;

/// Terminal router
pub use router::TerminalRouter;

/// Terminal synchronization
// pub use sync::{SyncTerminal, TerminalHandle};

/// Web terminal
pub use web::WebTerminal;

/// Graph visualization components
pub use graph::{Graph, GraphEdge, GraphManager, GraphNode, GraphUpdate, GraphUpdateType};

/// Core interface for terminal functionality
pub trait Terminal: Send + Sync + fmt::Debug {
    /// Return the terminal's unique identifier (non-async wrapper)
    fn id_sync(&self) -> Box<dyn std::future::Future<Output = Result<String>> + Send + Unpin + '_>;

    /// Start the terminal (non-async wrapper)
    fn start_sync(
        &mut self,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_>;

    /// Stop the terminal (non-async wrapper)
    fn stop_sync(&self) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_>;

    /// Display output to the terminal (non-async wrapper)
    fn display_sync(
        &self,
        output: &str,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_>;

    /// Echo input to the terminal (non-async wrapper)
    fn echo_input_sync(
        &self,
        input: &str,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_>;

    /// Execute a command on the terminal (non-async wrapper)
    fn execute_command_sync(
        &self,
        command: &str,
        tx: oneshot::Sender<String>,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_>;

    fn write(&mut self, s: &str) -> Result<()>;
    fn write_line(&mut self, s: &str) -> Result<()>;
    fn read_line(&mut self) -> Result<String>;
    fn flush(&mut self) -> Result<()>;
    fn read_password(&mut self, prompt: &str) -> Result<String>;
    fn read_secret(&mut self, prompt: &str) -> Result<String>;
}

/// Async terminal trait with full async methods
#[async_trait]
pub trait AsyncTerminal: Send + Sync + fmt::Debug {
    /// Return the terminal's unique identifier
    async fn id(&self) -> Result<String>;

    /// Start the terminal
    async fn start(&mut self) -> Result<()>;

    /// Stop the terminal
    async fn stop(&self) -> Result<()>;

    /// Display output to the terminal
    async fn display(&self, output: &str) -> Result<()>;

    /// Echo input to the terminal
    async fn echo_input(&self, input: &str) -> Result<()>;

    /// Execute a command on the terminal
    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<()>;
}

/// Helper extension trait that implements Terminal for any type that implements AsyncTerminal
pub trait TerminalExt: AsyncTerminal {
    fn as_terminal(&self) -> &dyn Terminal;
}

impl<T: AsyncTerminal + 'static> Terminal for T {
    fn id_sync(&self) -> Box<dyn std::future::Future<Output = Result<String>> + Send + Unpin + '_> {
        Box::new(Box::pin(async move { self.id().await }))
    }

    fn start_sync(
        &mut self,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        let fut = self.start();
        Box::new(Box::pin(fut))
    }

    fn stop_sync(&self) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        Box::new(Box::pin(async move { self.stop().await }))
    }

    fn display_sync(
        &self,
        output: &str,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        let output = output.to_string();
        Box::new(Box::pin(async move { self.display(&output).await }))
    }

    fn echo_input_sync(
        &self,
        input: &str,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        let input = input.to_string();
        Box::new(Box::pin(async move { self.echo_input(&input).await }))
    }

    fn execute_command_sync(
        &self,
        command: &str,
        tx: oneshot::Sender<String>,
    ) -> Box<dyn std::future::Future<Output = Result<()>> + Send + Unpin + '_> {
        let command = command.to_string();
        Box::new(Box::pin(
            async move { self.execute_command(&command, tx).await },
        ))
    }

    fn write(&mut self, s: &str) -> Result<()> {
        // Just print to stdout for now
        print!("{}", s);
        std::io::stdout().flush().map_err(|e| Error::from(e))?;
        Ok(())
    }

    fn write_line(&mut self, s: &str) -> Result<()> {
        // Just print to stdout for now
        println!("{}", s);
        Ok(())
    }

    fn read_line(&mut self) -> Result<String> {
        let mut result = String::new();
        std::io::stdin().read_line(&mut result)?;
        Ok(result.trim().to_string())
    }

    fn flush(&mut self) -> Result<()> {
        std::io::stdout().flush().map_err(|e| Error::from(e))?;
        Ok(())
    }

    fn read_password(&mut self, prompt: &str) -> Result<String> {
        self.write(prompt)?;
        self.flush()?;

        let mut password = String::new();
        std::io::stdin().read_line(&mut password)?;

        Ok(password.trim().to_string())
    }

    fn read_secret(&mut self, prompt: &str) -> Result<String> {
        self.read_password(prompt)
    }
}

impl<T: AsyncTerminal + 'static> TerminalExt for T {
    fn as_terminal(&self) -> &dyn Terminal {
        self
    }
}

/// Async helper functions to make working with dyn Terminal easier
pub mod terminal_helpers {
    use super::*;

    pub async fn id(terminal: &dyn Terminal) -> Result<String> {
        terminal.id_sync().await
    }

    pub async fn start(terminal: &mut dyn Terminal) -> Result<()> {
        terminal.start_sync().await
    }

    pub async fn stop(terminal: &dyn Terminal) -> Result<()> {
        terminal.stop_sync().await
    }

    pub async fn display(terminal: &dyn Terminal, output: &str) -> Result<()> {
        terminal.display_sync(output).await
    }

    pub async fn echo_input(terminal: &dyn Terminal, input: &str) -> Result<()> {
        terminal.echo_input_sync(input).await
    }

    pub async fn execute_command(
        terminal: &dyn Terminal,
        command: &str,
        tx: oneshot::Sender<String>,
    ) -> Result<()> {
        terminal.execute_command_sync(command, tx).await
    }
}

/// Terminal types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalType {
    /// Console terminal
    Console,
    /// Web terminal
    Web,
}

impl ToString for TerminalType {
    fn to_string(&self) -> String {
        match self {
            TerminalType::Console => "console".to_string(),
            TerminalType::Web => "web".to_string(),
        }
    }
}

/// Terminal system that manages all terminal interfaces
#[derive(Clone, Debug)]
pub struct TerminalSystem {
    /// Terminal router
    router: Arc<router::TerminalRouter>,
}

impl TerminalSystem {
    /// Create a new terminal system with the provided configuration
    pub fn new(config: config::TerminalConfig) -> Self {
        Self {
            router: Arc::new(router::TerminalRouter::new(config)),
        }
    }

    /// Start the terminal system
    pub async fn start(&self) -> Result<()> {
        self.router.start().await
    }

    /// Stop the terminal system
    pub async fn stop(&self) -> Result<()> {
        self.router.stop().await
    }

    /// Toggle the web terminal on or off
    pub async fn toggle_web_terminal(&self, enabled: bool) -> Result<()> {
        self.router.toggle_web_terminal(enabled).await
    }

    /// Write data to all connected terminals
    pub async fn write(&self, data: &str) -> Result<()> {
        self.router.write(data).await
    }

    /// Read the next input from any terminal (blocks until input is available)
    pub async fn read(&self) -> Result<String> {
        self.router.read().await
    }

    /// Check if a terminal is enabled
    pub async fn is_terminal_enabled(&self, terminal_type: TerminalType) -> bool {
        self.router
            .is_terminal_enabled(&terminal_type.to_string())
            .await
    }

    /// Get the web terminal address if enabled
    pub async fn web_terminal_address(&self) -> Option<String> {
        self.router.web_terminal_address().await
    }
}

/// Initialize visualization components for the terminal system
pub async fn initialize_visualization(
    terminal_system: &TerminalSystem,
    workflow_engine: Option<Arc<WorkflowEngine>>,
    agents: Vec<Arc<Agent>>,
    // Comment out missing imports for now - these would be implemented based on LLM and human input systems
    // llm_providers: Vec<Arc<dyn LlmProvider>>,
    // human_input_provider: Option<Arc<dyn HumanInputProvider>>,
) -> Arc<GraphManager> {
    let graph_manager = Arc::new(GraphManager::new());

    // Create providers module in graph namespace
    graph::create_providers_module();

    // Initialize workflow visualization if workflow engine is provided
    if let Some(engine) = workflow_engine {
        let workflow_provider = Arc::new(graph::providers::WorkflowGraphProvider::new(engine));

        // Call setup_tracking on the Arc-wrapped provider
        match workflow_provider
            .setup_tracking_boxed(graph_manager.clone())
            .await
        {
            Ok(_) => debug!("Workflow provider setup tracking successfully"),
            Err(e) => error!("Failed to setup tracking for workflow provider: {}", e),
        }
    }

    // Initialize agent visualization if agents are provided
    if !agents.is_empty() {
        let agent_provider = Arc::new(graph::providers::AgentGraphProvider::new());

        for agent in agents {
            agent_provider.add_agent(agent).await;
        }

        // Call setup_tracking on the Arc-wrapped provider
        match agent_provider
            .setup_tracking_boxed(graph_manager.clone())
            .await
        {
            Ok(_) => debug!("Agent provider setup tracking successfully"),
            Err(e) => error!("Failed to setup tracking for agent provider: {}", e),
        }
    }

    // Initialize LLM integration visualization if providers are available
    // if !llm_providers.is_empty() {
    //     let llm_provider = Arc::new(graph::providers::LlmIntegrationGraphProvider::new());
    //
    //     for provider in llm_providers {
    //         llm_provider.add_llm_provider(provider).await;
    //     }
    //
    //     llm_provider.setup_tracking(graph_manager.clone()).await.ok();
    // }

    // Initialize human input visualization if provider is available
    // if let Some(provider) = human_input_provider {
    //     let human_provider = Arc::new(graph::providers::HumanInputGraphProvider::new(Some(provider)));
    //     human_provider.setup_tracking(graph_manager.clone()).await.ok();
    // }

    // Register the graph manager with the web terminal
    if let Err(e) = terminal_system
        .router
        .register_graph_manager(graph_manager.clone())
        .await
    {
        error!(
            "Failed to register graph manager with the terminal router: {}",
            e
        );
        warn!("Visualization may not be fully functional due to registration failure");
    } else {
        debug!("Graph manager successfully registered with the terminal router");
    }

    graph_manager
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_terminal_system_default() {
        // Create a default terminal system (console only)
        let config = config::TerminalConfig::console_only();
        let system = TerminalSystem::new(config);

        // Check initial state (no terminals are active until started)
        assert!(!system.is_terminal_enabled(TerminalType::Console).await);
        assert!(!system.is_terminal_enabled(TerminalType::Web).await);

        // Start the system
        let result = system.start().await;
        assert!(result.is_ok());

        // Check that only console is enabled
        assert!(system.is_terminal_enabled(TerminalType::Console).await);
        assert!(!system.is_terminal_enabled(TerminalType::Web).await);

        // Stop the system
        let result = system.stop().await;
        assert!(result.is_ok());
    }
}
