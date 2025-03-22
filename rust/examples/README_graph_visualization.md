# Graph Visualization Example

This example demonstrates how to visualize graph data using a web-based interface with real-time updates via WebSockets.

## Features

- Interactive graph visualization using D3.js
- Real-time updates via WebSockets
- Built-in web terminal for adding nodes and edges
- Drag-and-drop manipulation of the graph

## Running the Example

The example is provided in two formats:

### 1. Standalone Example (Recommended)

For the standalone version with no dependencies on the MCP Agent codebase:

```bash
cd examples/graph_vis_example
cargo run
```

This will start a web server at http://localhost:3000 where you can access the graph visualization interface.

### 2. MCP Agent Integration

To run the example integrated with the MCP Agent features:

```bash
# From the rust directory
cargo run --example graph_visualization --features terminal-web
```

## Using the Interface

The interface consists of two main parts:

- Left side: Graph visualization panel
- Right side: Web terminal for entering commands

### Available Commands

In the web terminal, you can use the following commands:

- `help` - Show available commands
- `add-node <id> <name> <type> [status]` - Add a new node to the graph
  - Example: `add-node n1 Task1 task active`
- `add-edge <id> <source> <target> [type]` - Add an edge between nodes
  - Example: `add-edge e1 n1 n2 flow`

### Example Session

1. Start the server
2. Open http://localhost:3000 in your browser
3. Use the terminal to add nodes:
   ```
   add-node n1 Task1 task active
   add-node n2 Task2 task pending
   ```
4. Connect the nodes:
   ```
   add-edge e1 n1 n2 dependency
   ```
5. Drag nodes to adjust the layout

## Implementation Details

The example uses:

- Axum web framework for the server
- WebSockets for real-time communication
- D3.js for graph visualization
- Serde for JSON serialization/deserialization

The graph visualization is based on a force-directed layout that automatically positions nodes and edges based on their connections.
