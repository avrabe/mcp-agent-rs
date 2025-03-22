//! Terminal system initialization

use std::sync::Arc;

use super::config::TerminalConfig;
use super::graph::GraphManager;
use super::router::TerminalRouter;
use crate::Result;

/// Initialize the terminal system
pub fn initialize_terminal(config: TerminalConfig) -> Result<(TerminalRouter, GraphManager)> {
    // Create the graph manager
    let graph_manager = GraphManager::new();

    // Create and start the terminal router
    let mut router = TerminalRouter::new(config);
    router.start()?;

    Ok((router, graph_manager))
}