# Graph Visualization System Issues

This document outlines the current issues with the MCP Agent's graph visualization system and proposes solutions.

## Current Issues

1. **✅ Type Mismatches in SprottyStatus** (FIXED):

   - `SprottyStatus` vs `String` status field mismatches in multiple locations
   - Example error: `expected SprottyStatus, found String`
   - Fixed by implementing `From<String>` and `From<&str>` for SprottyStatus

2. **✅ Terminal Trait Implementation** (FIXED):

   - We've consolidated to a single async version of the `Terminal` trait in `mod.rs`
   - Updated `ConsoleTerminal` and `WebTerminal` implementations to match the async trait
   - Fixed Box<T> issues with `register_terminal` in `TerminalRouter`
   - Resolved configuration mismatches between the two `WebTerminalConfig` types
   - Multiple `Result<T, Error>` type mismatches in the code

3. **✅ WebSocket Communication** (FIXED):

   - Fixed sender/receiver ownership issues in the WebSocket communication
   - Resolved error: `use of moved value: sender` in `src/terminal/graph/api.rs`

4. **✅ GraphDataProvider Trait** (FIXED):

   - Implemented proper Debug trait bound for GraphDataProvider
   - Fixed "method not found in Arc<AgentGraphProvider>" error for setup_tracking
   - Added proper error handling when calling setup_tracking

5. **✅ Router Implementation** (FIXED):

   - The router now properly registers the graph manager
   - Added proper error handling in register_graph_manager and initialize_visualization
   - Fixed message-based approach for setting graph managers in the web terminal

6. **Duplicate Definitions**:

   - Duplicate implementation of `update_graph` function in `src/terminal/web.rs`
   - Error: `duplicate definitions with name update_graph`

7. **Struct Field Mismatches**:
   - Field name mismatches like `web_terminal_port` vs `web_terminal_config`
   - Type mismatches between similar structs (`WebTerminalConfig` from different modules)

## Implemented Solutions

### 1. ✅ Fix SprottyStatus Type (FIXED)

Added proper implementations of `From<String>` and `From<&str>` for `SprottyStatus`:

```rust
impl From<String> for SprottyStatus {
    fn from(s: String) -> Self {
        match s.as_str() {
            "active" => SprottyStatus::Active,
            "completed" => SprottyStatus::Completed,
            "error" => SprottyStatus::Error,
            "paused" => SprottyStatus::Paused,
            "waiting" => SprottyStatus::Waiting,
            _ => SprottyStatus::Idle,
        }
    }
}

impl From<&str> for SprottyStatus {
    fn from(s: &str) -> Self {
        match s {
            "active" => SprottyStatus::Active,
            "completed" => SprottyStatus::Completed,
            "error" => SprottyStatus::Error,
            "paused" => SprottyStatus::Paused,
            "waiting" => SprottyStatus::Waiting,
            _ => SprottyStatus::Idle,
        }
    }
}
```

### 2. ✅ Terminal Trait Implementation (FIXED)

We've consolidated the Terminal trait to a single async version:

```rust
#[async_trait]
pub trait Terminal: Send + Sync {
    /// Return the terminal ID
    async fn id(&self) -> Result<String>;

    /// Start the terminal
    async fn start(&mut self) -> Result<()>;

    /// Stop the terminal
    async fn stop(&mut self) -> Result<()>;

    /// Display output on the terminal
    async fn display(&self, output: &str) -> Result<()>;

    /// Echo input back to the terminal
    async fn echo_input(&self, input: &str) -> Result<()>;

    /// Execute a command on the terminal
    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<()>;
}
```

And updated the ConsoleTerminal and WebTerminal implementations to use the async trait. We also updated the TerminalRouter to use TokioMutex consistently and made it work with `Arc<dyn Terminal>` instead of boxed terminal types.

### 3. ✅ WebTerminalConfig Consolidation (FIXED)

We eliminated the duplicate WebTerminalConfig struct by:

1. Removing the WebTerminalConfig struct from `web.rs`
2. Using only the WebTerminalConfig from `config.rs`
3. Updating all usage to work with the IpAddr type instead of String
4. Fixed socket address creation to use SocketAddr::new() directly instead of string parsing

```rust
// Old code in web.rs
let addr = format!("{}:{}", self.config.host, self.config.port)
    .parse::<SocketAddr>()
    .map_err(|e| Error::TerminalError(format!("Invalid address: {}", e)))?;

// New code using proper types
let addr = SocketAddr::new(self.config.host, self.config.port);
```

### 4. ✅ WebSocket Communication Fixes (FIXED)

Fixed the sender/receiver ownership issues in WebSocket handlers:

1. Created a separate sender for each task to avoid ownership conflicts
2. Used cloning for the sender to ensure each async task has its own instance
3. Fixed the `handle_graph_socket` function in `terminal/graph/api.rs`:

```rust
// Create a separate sender for the receive task
let mut sender_for_recv = sender.clone();

// Then use sender_for_recv in the receive task
if let Err(e) = sender_for_recv.send(Message::Text(json)).await {
    error!("Error sending initial graph over WebSocket: {}", e);
    break;
}
```

### 5. ✅ GraphDataProvider Trait (FIXED)

Implemented proper Debug trait bound for GraphDataProvider:

```rust
impl Debug for GraphDataProvider {
    // Implementation
}
```

Fixed "method not found in Arc<AgentGraphProvider>" error for setup_tracking:

```rust
impl GraphDataProvider for WorkflowGraphProvider {
    async fn setup_tracking(&self, graph_manager: Arc<GraphManager>) -> Result<()> {
        // Implementation
        Ok(())
    }
}
```

Added proper error handling when calling setup_tracking:

```rust
impl GraphDataProvider for WorkflowGraphProvider {
    async fn setup_tracking(&self, graph_manager: Arc<GraphManager>) -> Result<()> {
        // Implementation
        Ok(())
    }
}
```

### 6. ✅ Router Implementation (FIXED)

Fixed the router implementation to properly handle the graph manager:

1. Implemented a message-based approach for setting the graph manager in WebTerminal:

   ```rust
   // In router.rs
   pub async fn register_graph_manager(&self, graph_manager: Arc<GraphManager>) -> Result<()> {
       // Store the graph manager for later use
       *self.graph_manager.lock().await = Some(graph_manager.clone());

       // Get the graph manager ID for logging
       let graph_id = match graph_manager.id().await {
           Ok(id) => id,
           Err(e) => {
               error!("Failed to get graph manager ID: {}", e);
               return Err(Error::TerminalError(format!("Failed to get graph manager ID: {}", e)));
           }
       };

       info!("Registering graph manager with ID: {}", graph_id);

       // Set the graph manager for the web terminal
       if let Some(web_terminal) = self.web_terminal.lock().await.as_ref() {
           // Send a command to set the graph manager
           let (tx, rx) = oneshot::channel();
           let cmd = format!("{}:{}", SET_GRAPH_MANAGER, serde_json::to_string(&graph_id)?);

           if let Err(e) = web_terminal.execute_command(&cmd, tx).await {
               error!("Failed to set graph manager: {}", e);
               return Err(e);
           }

           // Wait for response
           match rx.await {
               Ok(_) => {
                   info!("Graph manager set successfully");
                   // Log the visualization URL if available
                   if let Ok(addr) = self.web_terminal_address().await {
                       info!("Visualization available at {}/vis", addr);
                   }
               }
               Err(e) => {
                   error!("Failed to set graph manager: {}", e);
                   return Err(Error::TerminalError(format!("Failed to set graph manager: {}", e)));
               }
           }
       } else {
           warn!("Web terminal not active, skipping graph manager registration");
       }

       Ok(())
   }
   ```

2. Updated the WebTerminal to handle the SET_GRAPH_MANAGER command:

   ```rust
   // In web.rs
   async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<()> {
       if let Some((cmd, arg)) = command.split_once(':') {
           match cmd {
               "SET_GRAPH_MANAGER" => {
                   debug!("Setting graph manager: {}", arg);
                   let graph_id: String = serde_json::from_str(arg)?;

                   // Store the graph manager ID for later use
                   *self.graph_manager_id.lock().await = Some(graph_id);

                   // Send acknowledgment
                   if tx.send("ok".to_string()).is_err() {
                       warn!("Failed to send acknowledgment for SET_GRAPH_MANAGER command");
                   }
               }
               // Other commands...
           }
       }

       Ok(())
   }
   ```

3. Improved error handling in the initialize_visualization function:

   ```rust
   pub async fn initialize_visualization(
       config: &TerminalConfig,
       workflow_engine: Option<Arc<WorkflowEngine>>,
       agents: Vec<Arc<Agent>>,
   ) -> Result<Arc<GraphManager>> {
       // Create the graph manager with proper error handling
       let graph_manager = match super::graph::initialize_visualization(workflow_engine, agents).await {
           Ok(gm) => {
               info!("Graph manager successfully created");
               gm
           },
           Err(e) => {
               error!("Failed to initialize graph visualization: {}", e);
               return Err(e);
           }
       };

       // Register with proper error handling and fallbacks
       if config.web_terminal_enabled {
           if let Some(router) = TERMINAL_ROUTER.get() {
               match router.register_graph_manager(graph_manager.clone()).await {
                   Ok(_) => {
                       info!("Graph manager successfully registered with terminal router");
                   },
                   Err(e) => {
                       // Log the error but continue - we still return the graph manager
                       error!("Failed to register graph manager with terminal router: {}", e);
                       warn!("Visualization may not be fully functional");
                   }
               }
           } else {
               warn!("Terminal router not initialized; visualization registration skipped");
           }
       } else if config.web_terminal_config.enable_visualization {
           warn!("Visualization is enabled in config, but web terminal is disabled; visualization will not be available");
       }

       Ok(graph_manager)
   }
   ```

4. ✅ Resolve duplicate definitions of the `update_graph` function

   - Status: **RESOLVED**
   - The duplicate implementation in `graph/mod.rs` has been removed
   - Updates are now correctly handled through the proper notification system

5. Create visualization examples:

   - Status: **IN PROGRESS**
   - Created `visualize_workflow.rs` example to demonstrate graph visualization
   - Created `visualization_demo.html` for a browser-based visualization preview
   - Added `visualization_fixes.md` with detailed steps to resolve compilation issues

6. Remaining tasks:
   - Make `WebTerminalConfig` and related structs properly public
   - Implement the missing `get_terminal_router()` function
   - Fix module visibility in the terminal module
   - Ensure consistent struct field naming
   - Properly integrate the graph manager with the terminal system

## Next Steps

To complete the implementation, we need to:

1. Implement the missing `GraphDataProvider` functionality:

   ```rust
   impl GraphDataProvider for WorkflowGraphProvider {
       async fn setup_tracking(&self, graph_manager: Arc<GraphManager>) -> Result<()> {
           // Implementation
           Ok(())
       }
   }
   ```

2. Fix the router implementation to properly handle the graph manager:

   ```rust
   pub struct TerminalRouter {
       // Existing fields...
       graph_manager: Option<Arc<GraphManager>>,
   }
   ```

3. Resolve duplicate definitions of the `update_graph` function

4. Fix miscellaneous issues:
   - ✅ Add `#[derive(Serialize)]` to the `SprottyGraph` type
     - **Fixed**: Added the missing `Serialize` derive to `SprottyGraph` struct in `src/terminal/graph/models.rs`
   - ✅ Replace all remaining synchronous mutex usage with async versions
     - **Fixed**: Replaced std::sync::Mutex with tokio::sync::Mutex (as TokioMutex) in workflow/state.rs, mcp/connection.rs, and terminal/web.rs
   - ✅ Update struct field mismatches for consistent naming
     - **Fixed**: Consolidated field naming by moving all terminal configuration to the `WebTerminalConfig` struct in `terminal/config.rs`. Examples need to be updated to use:
       - `web_terminal_config.host` instead of `web_terminal_host`
       - `web_terminal_config.port` instead of `web_terminal_port`
       - `web_terminal_config.auth_config.allow_anonymous = true` instead of `require_authentication = false`
       - `web_terminal_config.enable_visualization` for visualization settings
