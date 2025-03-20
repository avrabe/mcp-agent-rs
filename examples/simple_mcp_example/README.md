# Simple MCP Examples

This directory contains simple examples demonstrating the basic functionality of the MCP agent in Rust.

## Overview

The Model Context Protocol (MCP) allows AI assistants to interact with external services through a standardized interface. The MCP agent in this repository provides a Rust implementation for building MCP-compatible applications.

## Examples

### 1. Simple MCP Example (Markdown Guide)

- **File**: [simple_mcp_example.md](./simple_mcp_example.md)
- **Description**: A step-by-step guide showing how to start a mock MCP server and connect to it using the terminal client.
- **Features**:
  - Starting a mock server
  - Connecting with the terminal client
  - Sending messages
  - Executing functions
  - Understanding the MCP protocol

### 2. Simple Client Example (Rust Code)

- **File**: [simple_client.rs](./simple_client.rs)
- **Description**: A programmatic example in Rust showing how to use the MCP agent API to connect to a server and send messages.
- **Features**:
  - Creating an agent instance
  - Connecting to a server
  - Sending different types of messages (ping, JSON)
  - Executing functions with arguments
  - Handling connection lifecycle

### 3. Server Manager Example (Rust Code)

- **File**: [server_manager_example.rs](./server_manager_example.rs)
- **Description**: Demonstrates how to use the ServerManager component to manage MCP server lifecycle.
- **Features**:
  - Creating a ServerManager
  - Registering server configurations
  - Setting up initialization hooks
  - Starting and stopping servers programmatically
  - Managing connections to multiple servers
  - Sending messages to servers

#### New Features in `ServerManager`

The `ServerManager` has been enhanced with new features:

1. **Configuration File Loading:**

   ```rust
   // Load from a specific file
   manager.load_from_file("config.yaml").await?;

   // Load from standard config locations
   manager.load_from_config(None).await?;

   // Create a manager directly from config
   let manager = ServerManager::from_config(None).await?;
   ```

2. **Initialization Hooks:**

   ```rust
   // Register a hook that will run when a server is initialized
   manager.register_init_hook("server-name", |name, connection| {
       println!("Server {} initialized with connection to {}", name, connection.addr());
       Ok(())
   }).await?;
   ```

3. **Server Lifecycle Management:**

   ```rust
   // Start a server
   let connection = manager.start_server("server-name").await?;

   // Check if a server is connected
   let is_connected = manager.is_server_connected("server-name").await;

   // Stop a specific server
   manager.stop_server("server-name").await?;

   // Stop all servers
   manager.stop_all_servers().await?;
   ```

4. **Server Information:**

   ```rust
   // Get list of registered server names
   let servers = manager.get_server_names().await;

   // Get count of connected servers
   let count = manager.connected_server_count().await;
   ```

## Running the Examples

### Prerequisites

- Rust and Cargo installed
- The MCP agent codebase built

### Running the Mock Server

```bash
cd /path/to/mcp-agent
cargo run --bin mock_server
```

### Running the Terminal Client

```bash
cd /path/to/mcp-agent
cargo run --bin terminal
```

### Running the Simple Client Example

```bash
cd /path/to/mcp-agent
cargo run --example simple_client
```

### Running the Server Manager Example

```bash
cd /path/to/mcp-agent
cargo run --example server_manager_example
```

## Extending the Examples

These examples provide a foundation for understanding MCP in Rust. You can extend them by:

1. Implementing custom message handlers in the mock server
2. Adding more complex request/response patterns
3. Building your own MCP server to provide specific functionality
4. Creating multi-agent workflows where agents communicate via MCP

## Additional Resources

- [Model Context Protocol](https://modelcontextprotocol.io/introduction) - The official MCP documentation
- [Building Effective Agents](https://www.anthropic.com/research/building-effective-agents) - Anthropic's guide on agent patterns
