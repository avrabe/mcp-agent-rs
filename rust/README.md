# MCP Agent Rust Library

This crate implements the core functionality for the MCP (Message Control Protocol) agent in Rust.

## Features

- Connection management for MCP servers
- Message formatting and processing
- Server management for local and remote MCP servers
- Configuration management with YAML support
- Task execution engine

## Feature Flags

The library uses feature flags to enable optional functionality:

- `server_registry` (default): Enables the `ServerRegistry` module for managing multiple server connections. Disable this flag if you only need the `ServerManager` functionality without the full registry.

## ServerManager Features

The `ServerManager` module provides the following capabilities:

- Loading server configurations from config files
- Registering initialization hooks for servers
- Starting and stopping server processes
- Managing server lifecycles
- Sending and receiving messages
- Configuration through standard settings files

## Usage

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
mcp-agent = "0.1.0"
```

Or with specific features:

```toml
[dependencies]
mcp-agent = { version = "0.1.0", default-features = false }
```

## Example

```rust
use mcp_agent::mcp::{ServerManager, ManagerServerSettings};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new server manager
    let manager = ServerManager::new();

    // Register a server
    let settings = ManagerServerSettings {
        transport: "stdio".to_string(),
        command: Some("echo".to_string()),
        args: Some(vec!["Hello MCP".to_string()]),
        url: None,
        env: Some(HashMap::from([
            ("MCP_SERVER_PORT".to_string(), "8765".to_string()),
            ("DEBUG".to_string(), "1".to_string()),
        ])),
        auth: None,
        read_timeout_seconds: Some(30),
        auto_reconnect: true,
        max_reconnect_attempts: 3,
    };

    manager.register_server("test-server", settings).await?;

    // Register an initialization hook
    manager.register_init_hook("test-server", |name, connection| {
        println!("Server '{}' has been initialized with address: {}", name, connection.addr());
        Ok(())
    }).await?;

    // Load configurations from a file
    manager.load_from_file("config/mcp_agent.config.yaml").await?;

    // Start a server
    let connection = manager.start_server("test-server").await?;

    // Send a message and receive a response
    // ...

    // Stop the server
    manager.stop_server("test-server").await?;

    Ok(())
}
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.
