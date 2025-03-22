# Graph Visualization Issue Resolution Guide

This document outlines the steps needed to fix the compilation issues with the graph visualization example.

## Identified Issues

From our testing, we've identified several issues that need to be resolved:

1. The `terminal` module is gated behind the `terminal-web` feature flag
2. Private struct imports for `WebTerminalConfig`
3. Missing `get_terminal_router()` function
4. Duplicate imports and definitions in example files

## Step-by-Step Fixes

### 1. Fix the `visualize_workflow.rs` Example

The example code structure is correct, but needs some adjustments to work with the current codebase structure:

```rust
// Ensure proper imports with terminal-web feature
#[cfg(feature = "terminal-web")]
use mcp_agent::{
    error::Result,
    terminal::{
        config::{AuthConfig, AuthMethod, TerminalConfig},
        graph::{Graph, GraphNode, GraphEdge, GraphManager},
        TerminalSystem,
    },
};

// Make sure to use the public API only
#[cfg(feature = "terminal-web")]
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    // Create terminal configuration with web visualization enabled
    let mut config = TerminalConfig::web_only();
    config.web_terminal_config.port = 8080;
    config.web_terminal_config.enable_visualization = true;
    config.web_terminal_config.auth_config.allow_anonymous = true;

    // Create and start the terminal system
    let terminal = TerminalSystem::new(config);
    terminal.start().await?;

    // Create and register graph...
    // Rest of the example remains the same
}
```

### 2. Make Terminal Components Public

Several structs and functions need to be made public in the terminal module:

```rust
// In src/terminal/mod.rs
pub use router::get_terminal_router;
pub use web::config::WebTerminalConfig;

// Make sure the following are public
pub mod router;
pub mod web;
pub mod graph;
```

### 3. Fix the Router Implementation

```rust
// In src/terminal/router.rs
pub fn get_terminal_router() -> Router {
    // Implementation
}
```

### 4. Fix Feature Flag Handling

Make sure the terminal module is properly gated:

```rust
// In src/lib.rs or src/terminal/mod.rs
#[cfg(feature = "terminal-web")]
pub mod terminal;
```

### 5. Fix Graph Manager Integration

Ensure the graph manager is properly integrated with the terminal system:

```rust
// In src/terminal/mod.rs (within terminal-web feature flag)
impl TerminalSystem {
    // Add method to access graph manager
    pub async fn graph_manager(&self) -> Option<Arc<GraphManager>> {
        if let Some(web_terminal) = &self.web_terminal {
            web_terminal.graph_manager().await
        } else {
            None
        }
    }
}
```

### 6. Update Field Names for Consistency

Make sure all struct field names are consistent:

```rust
// In src/terminal/graph/models.rs
#[derive(Serialize)]
pub struct SprottyGraph {
    pub id: String,
    pub type_field: String, // Not "type" which is a reserved keyword
    pub children: Vec<SprottyNode>,
}
```

## Testing Your Changes

After implementing these fixes, you should be able to run the visualization example with:

```bash
RUST_LOG=debug cargo run --example visualize_workflow --features terminal-web
```

## Understanding the Flow

1. The `TerminalSystem` creates a web server (when configured)
2. The web server includes routes for the graph visualization
3. The `GraphManager` maintains graph state and notifies subscribers of changes
4. When you update a node, it propagates through the `notify_update` method
5. The web client visualizes these updates in real-time using Sprotty

## Troubleshooting

- If you get "module not found" errors, check that feature flags are set correctly
- If you get "private struct" errors, make sure to make the struct public or use a public re-export
- If the visualization doesn't update, check that the `notify_update` method is being called correctly
