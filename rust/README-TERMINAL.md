# MCP-Agent Terminal System

The MCP-Agent Terminal System provides a flexible dual-terminal interface for Rust applications, allowing seamless interaction through both console and web-based terminals simultaneously.

## Features

- **Dual Terminal Support**: Use both console and web terminals concurrently with synchronized I/O
- **WebSocket Interface**: Web terminal implementation using Axum and WebSockets
- **Configurable Authentication**: Support for JWT and Basic authentication methods
- **Terminal Synchronization**: All terminal input and output are synchronized across interfaces
- **Extensible Design**: Easy to extend with additional terminal types

## Architecture

The terminal system is organized into several components:

1. **Terminal Router**: Central component that manages terminal I/O and routing
2. **Terminal Synchronizer**: Handles synchronization of I/O between multiple terminal interfaces
3. **Console Terminal**: Implementation for local console interaction
4. **Web Terminal Server**: Implementation for browser-based terminal interaction using WebSockets

## Usage

### Basic Example

```rust
use mcp_agent::terminal::config::{AuthConfig, AuthMethod, TerminalConfig};
use mcp_agent::terminal::TerminalSystem;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create configuration with both terminals enabled
    let config = TerminalConfig::dual_terminal(
        "127.0.0.1".to_string(),  // Web host
        9000,                     // Web port
        AuthConfig::default(),    // Default auth config (no auth)
    );

    // Create and start the terminal system
    let terminal = TerminalSystem::new(config);
    terminal.start().await?;

    // Display welcome message on all terminals
    terminal.write("Welcome to MCP-Agent Terminal System!\n").await?;

    // Read input from any terminal
    let input = terminal.read().await?;
    terminal.write(&format!("You entered: {}\n", input)).await?;

    // Stop the terminal system
    terminal.stop().await?;

    Ok(())
}
```

### Terminal Configuration

The terminal system is highly configurable:

```rust
// Console-only terminal
let config = TerminalConfig::console_only();

// Web-only terminal
let config = TerminalConfig::web_only(
    "127.0.0.1".to_string(),
    9000,
    AuthConfig::default(),
);

// Dual terminal with JWT authentication
let config = TerminalConfig::dual_terminal(
    "0.0.0.0".to_string(),  // Listen on all interfaces
    9000,
    AuthConfig {
        auth_method: AuthMethod::Jwt,
        require_authentication: true,
        jwt_secret: "your-secret-key".to_string(),
        token_expiration_secs: 3600,
        username: "admin".to_string(),
        password: "password".to_string(),
        allow_anonymous: false,
    },
);
```

## Web Terminal Interface

The web terminal provides a browser-based terminal interface using xterm.js. When enabled, the web terminal is accessible at `http://<host>:<port>/` by default.

### Authentication Options

The web terminal supports multiple authentication methods:

- **None**: No authentication required
- **Basic**: HTTP Basic Authentication
- **JWT**: JSON Web Token authentication

## Terminal Trait

Custom terminal implementations can be created by implementing the `Terminal` trait:

```rust
#[async_trait]
pub trait Terminal: Send + Sync {
    // Get the terminal identifier
    async fn id(&self) -> Result<String, Error>;

    // Start the terminal
    async fn start(&mut self) -> Result<(), Error>;

    // Stop the terminal
    async fn stop(&mut self) -> Result<(), Error>;

    // Display output on the terminal
    async fn display(&self, output: &str) -> Result<(), Error>;

    // Echo input received from another terminal
    async fn echo_input(&self, input: &str) -> Result<(), Error>;

    // Execute a command on the terminal
    async fn execute_command(&self, command: &str, tx: oneshot::Sender<String>) -> Result<(), Error>;
}
```

## Integration with MCP-Agent

The terminal system integrates with the MCP-Agent framework to provide interactive capabilities for agents. It allows:

1. **Human-in-the-loop workflows**: Agents can request human input through any available terminal
2. **Multi-interface interaction**: Users can interact with agents through console, web, or both
3. **Tool execution**: Allows agents to display tool execution process and results

## Building and Running

To use the terminal system, enable the appropriate feature flags in your Cargo.toml:

```toml
[dependencies]
mcp-agent = { version = "0.1.0", features = ["terminal-web"] }
```

Available features:

- `terminal-web`: Basic web terminal support
- `terminal-full`: Complete terminal system with all features including authentication

## Example: Dual Terminal Demo

Run the included dual terminal example:

```bash
cargo run --example dual_terminal --features="terminal-full"
```

This will start a demo that accepts input from both console and web terminals, with all I/O synchronized between them.

## Implementation Notes

### Web Terminal Implementation

The web terminal implementation is currently a work in progress. The basic structure has been set up with the following components:

- `WebTerminalConfig`: Configuration for the web terminal, including host, port, authentication settings, and visualization options.
- `WebTerminal`: Implementation of the `Terminal` trait for web-based interaction.
- `SprottyGraph` and related models: Data structures for graph visualization.

However, the full implementation requires additional work:

1. **Asynchronous vs Synchronous API**: The current Terminal trait has been updated to use synchronous methods, but the existing code was originally designed with async methods. A decision needs to be made whether to stick with synchronous methods or convert back to async.

2. **Terminal Synchronizer**: The Terminal Synchronizer needs to be fully implemented to properly handle input and output between multiple terminals.

3. **Web Server Integration**: The actual web server implementation for the WebTerminal is not yet complete. This would involve:

   - Setting up a web server with routes for terminal access and visualization
   - WebSocket handling for real-time terminal communication
   - Authentication middleware
   - UI for terminal interaction and visualization

4. **Graph Visualization**: The SprottyGraph models are set up, but integration with a frontend visualization library (like Sprotty) is needed.

### Current Status

- A basic structure for the terminal system exists
- The console terminal implementation works partially
- The web terminal is a placeholder with stubs for future implementation
- Graph models are defined but not fully integrated
- A minimal example (`minimal_vis`) demonstrates the basic concept

### Next Steps

1. Decide on synchronous vs asynchronous API
2. Implement the web server for WebTerminal
3. Develop the frontend UI for terminal interaction and visualization
4. Integrate with the rest of the MCP-Agent framework
5. Add comprehensive tests
