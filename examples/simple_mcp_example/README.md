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

## Extending the Examples

These examples provide a foundation for understanding MCP in Rust. You can extend them by:

1. Implementing custom message handlers in the mock server
2. Adding more complex request/response patterns
3. Building your own MCP server to provide specific functionality
4. Creating multi-agent workflows where agents communicate via MCP

## Additional Resources

- [Model Context Protocol](https://modelcontextprotocol.io/introduction) - The official MCP documentation
- [Building Effective Agents](https://www.anthropic.com/research/building-effective-agents) - Anthropic's guide on agent patterns
