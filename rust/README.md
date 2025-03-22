# MCP Agent Rust Library

This is the Rust implementation of the MCP (Multi-agent Cooperative Protocol) Agent system.

## Testing

The codebase has several components that can be tested. However, there's a special consideration for examples that depend on the `terminal-web` feature:

### Standard Tests

To run the standard library, binary, and test suite tests:

```bash
cargo test --lib --bins --tests --benches
```

### Running All Tests Except for terminal-web Examples

There's a helper script `run_tests.sh` that runs all tests except for the examples that depend on the `terminal-web` feature (like `simple_visualization.rs`):

```bash
./run_tests.sh
```

### Terminal Web Feature

The `terminal-web` feature enables visualization capabilities but is currently under development and has some implementation issues. If you want to use or test the visualization functionality:

1. Use the feature flag when building or running examples:

```bash
cargo run --example simple_visualization --features terminal-web
```

2. Note that running tests for examples that require this feature might fail until the implementation issues are fixed:

```bash
cargo test --example simple_visualization --features terminal-web
```

When fixing the `create_sample_agent_graph` function or other parts of the visualization system, make sure they're enclosed in the `terminal_web_code` module or similarly protected by feature flags.
