//! # MCP Resources System
//!
//! The resources system provides functionality for managing structured data or content
//! that provides additional context to language models.
//!
//! This module implements the resources primitive as specified in the Model Context Protocol.
//!
//! ## Features
//!
//! - Resource lifecycle management (creation, updating, deletion)
//! - Content chunking and embedding for large resources
//! - Efficient retrieval with metadata filtering
//! - Versioning and history tracking
//!
//! ## Usage
//!
//! ```rust,no_run
//! use mcp_agent::mcp::resources::{Resource, ResourceManager};
//!
//! // Create a resource
//! let resource = Resource::new(
//!     "readme",
//!     "README.md",
//!     "# Project Documentation\n\nThis is a sample README file.",
//! );
//!
//! // Register the resource with the manager
//! let mut manager = ResourceManager::new();
//! manager.register_resource(resource);
//!
//! // Retrieve the resource
//! let readme = manager.get_resource("readme");
//! ```

mod error;
mod handler;
mod models;
mod provider;
mod templates;
mod utils;

// Re-export the public API
pub use error::ResourceError;
pub use handler::{ResourcesCapabilities, ResourcesHandler};
pub use models::{BinaryContent, Resource, ResourceContents, TextContent};
pub use provider::{FileSystemProvider, ResourceProvider};
pub use templates::{ResourceTemplate, TemplateParameter};

use std::sync::Arc;

/// Resource manager for controlling access to resource providers
#[derive(Debug)]
pub struct ResourceManager {
    providers: Vec<Arc<dyn ResourceProvider>>,
    capabilities: ResourcesCapabilities,
}

impl ResourceManager {
    /// Creates a new resource manager with default settings
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            capabilities: ResourcesCapabilities {
                subscribe: true,
                list_changed: true,
            },
        }
    }

    /// Registers a resource provider with the manager
    pub fn register_provider<P: ResourceProvider + 'static>(&mut self, provider: P) {
        self.providers.push(Arc::new(provider));
    }

    /// Returns all registered resource providers
    pub fn providers(&self) -> &[Arc<dyn ResourceProvider>] {
        &self.providers
    }

    /// Returns the capabilities for the resources system
    pub fn capabilities(&self) -> &ResourcesCapabilities {
        &self.capabilities
    }

    /// Sets the capabilities for the resources system
    pub fn set_capabilities(&mut self, capabilities: ResourcesCapabilities) {
        self.capabilities = capabilities;
    }
}

impl Default for ResourceManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Initializes the resources system
pub fn initialize() -> std::result::Result<(), Box<dyn std::error::Error>> {
    Ok(())
}
