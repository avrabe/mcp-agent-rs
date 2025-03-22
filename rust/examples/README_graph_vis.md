# Graph Visualization Example

This directory contains the `graph_vis.rs` example which demonstrates the graph visualization capabilities of the MCP Agent. Currently, this is a placeholder implementation due to several pending fixes in the core visualization system.

## Current Status

The current implementation is a placeholder that simulates the functionality that will be available once the visualization system is fully implemented. It describes the intended functionality and lists the issues that need to be fixed.

## Running the Example

To run the example, you can use the provided script:

```bash
./run_graph_vis.sh
```

Alternatively, you can run it directly with Cargo:

```bash
cargo run --example graph_vis
```

## Complete Implementation (Future)

The complete implementation will include:

1. A terminal system with web terminal support
2. A graph manager for tracking workflow visualizations
3. Registration of workflow graphs with tasks and dependencies
4. Real-time updates of node statuses to simulate workflow execution
5. A web-based visualization interface

## Issues to Fix

For details on the specific issues that need to be fixed, please see the [visualization_issues.md](../docs/visualization_issues.md) document in the docs directory.

The primary issues include:

1. Type mismatches in SprottyStatus
2. WebTerminal implementation mismatches with the Terminal trait
3. WebSocket communication ownership issues
4. Missing GraphDataProvider trait functionality
5. Router implementation issues with graph manager registration

## Example Visualization (Future)

Once implemented, the visualization will show a workflow graph with nodes representing tasks and edges representing dependencies. The tasks will change status (idle, running, completed, failed) as the workflow progresses.

The visualization will be accessible through a web browser at a URL displayed in the console (e.g., http://localhost:9393/vis).

## Contributing

If you're interested in helping with implementing the visualization system, please refer to the [visualization_issues.md](../docs/visualization_issues.md) document for specific details on what needs to be fixed.
