# MCP-Agent Telemetry Integration

This document provides an overview of the telemetry capabilities in MCP-Agent, which includes distributed tracing, metrics collection, and performance monitoring.

## Overview

MCP-Agent incorporates OpenTelemetry-based distributed tracing and metrics collection to provide visibility into the system's operation. This telemetry implementation supports:

- Distributed tracing with Jaeger or OTLP exporters
- Custom metrics collection and export
- Span instrumentation with detailed context
- Resource and attributes management
- Error tracking and performance measurement

## Getting Started

### Feature Flags

Telemetry is available through feature flags:

- `telemetry-jaeger`: Enables tracing with Jaeger
- `telemetry-otlp`: Enables tracing with OTLP/OpenTelemetry Protocol

When building or running, enable the appropriate feature:

```bash
# Build with Jaeger support
cargo build --features telemetry-jaeger

# Run with OTLP support
cargo run --features telemetry-otlp
```

### Running Jaeger Locally

To visualize traces with Jaeger, start a local Jaeger instance:

```bash
docker run -d --name jaeger \
  -p 6831:6831/udp \
  -p 16686:16686 \
  jaegertracing/all-in-one:latest
```

This starts Jaeger with the collector listening on port 6831 (UDP) and the UI available at http://localhost:16686.

## Configuration

Telemetry can be configured through the `TelemetryConfig` struct:

```rust
let mut config = TelemetryConfig::default();
config.service_name = "my-service".to_string();
config.jaeger_endpoint = Some("127.0.0.1:6831".to_string());
config.sampling_ratio = 0.25; // Sample 25% of traces

// Add custom attributes to all spans
let mut attributes = HashMap::new();
attributes.insert("environment".to_string(), "development".to_string());
config.attributes = attributes;

// Initialize telemetry
init_telemetry(config)?;
```

### Configuration Options

| Option            | Description                                    | Default          |
| ----------------- | ---------------------------------------------- | ---------------- |
| `service_name`    | Name of the service in traces                  | `"mcp-agent"`    |
| `otlp_endpoint`   | OTLP endpoint (e.g., "http://localhost:4317")  | `None`           |
| `jaeger_endpoint` | Jaeger agent endpoint (e.g., "127.0.0.1:6831") | `None`           |
| `enable_console`  | Enable console logging                         | `true`           |
| `enable_json`     | Enable JSON formatted logs                     | `false`          |
| `enable_tracing`  | Enable distributed tracing                     | `true`           |
| `enable_metrics`  | Enable metrics collection                      | `true`           |
| `sampling_ratio`  | Ratio of traces to sample (0.0 to 1.0)         | `1.0`            |
| `attributes`      | Custom attributes for all spans                | `HashMap::new()` |

## Usage Examples

### Basic Tracing

```rust
use mcp_agent::telemetry::{init_telemetry, shutdown_telemetry, TelemetryConfig};
use tracing::{info, info_span};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize telemetry
    let config = TelemetryConfig::default();
    init_telemetry(config)?;

    // Create a span for the operation
    let span = info_span!("my_operation", component = "example");
    let _guard = span.enter();

    // Log within the span
    info!("Starting operation");

    // Work happens here...

    info!("Operation completed");

    // Ensure all spans are exported
    shutdown_telemetry();

    Ok(())
}
```

### Creating Child Spans

```rust
use tracing::{info, info_span};

fn process_item(id: &str) {
    // Create child span
    let span = info_span!("process_item", item_id = %id);
    let _guard = span.enter();

    info!("Processing item {}", id);

    // Work happens here...

    info!("Item processed");
}
```

### Using Instrumentation Attribute Macro

```rust
use tracing::instrument;

#[instrument(fields(user_id = %user.id))]
async fn handle_request(user: User, request: Request) -> Result<Response, Error> {
    // Automatically creates a span with function name and arguments

    // Work happens here...

    Ok(Response::new())
}
```

### Measuring Operation Duration

```rust
use mcp_agent::telemetry::span_duration;

fn some_operation() {
    // Create duration measurement
    let _guard = span_duration("database_query");

    // Operation being measured...

    // Guard is dropped automatically at end of scope, which records the duration
}
```

### Recording Custom Metrics

```rust
use mcp_agent::telemetry::add_metrics;
use std::collections::HashMap;

fn process_batch(items: &[Item]) {
    // Process items...

    // Record metrics
    let mut metrics = HashMap::new();
    metrics.insert("items_processed", items.len() as f64);
    metrics.insert("processing_time_ms", 42.5);
    add_metrics(metrics);
}
```

### Handling Errors

```rust
use mcp_agent::telemetry::set_error_on_current_span;
use std::error::Error;

fn fallible_operation() -> Result<(), Box<dyn Error>> {
    if let Err(e) = some_function() {
        // Mark current span as failed
        set_error_on_current_span(&e);
        return Err(e.into());
    }

    Ok(())
}
```

## Advanced Topics

### Creating Custom Contexts

```rust
use mcp_agent::telemetry::create_trace_context;
use tracing::Level;

fn start_background_task() {
    let context = create_trace_context("background_task", Level::INFO);
    // Use context...
}
```

### Manual Batch Submission

For long-running processes, you may want to periodically flush traces:

```rust
// Force export of spans every 30 seconds
tokio::spawn(async {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        opentelemetry::global::force_flush_tracer_provider();
    }
});
```

## Example Application

The telemetry module includes an example application that demonstrates these features:

```bash
cargo run --bin telemetry_example --features telemetry-jaeger
```

This will generate spans and metrics that you can visualize in Jaeger UI at http://localhost:16686.

## Best Practices

1. **Use descriptive span names**: Choose names that describe the operation, not the function.
2. **Add context to spans**: Include relevant fields for debugging (IDs, sizes, etc.).
3. **Create spans at appropriate granularity**: Too many spans create overhead; too few provide little insight.
4. **Handle errors properly**: Mark spans as errors when operations fail.
5. **Use sampling in production**: For high-volume services, reduce sampling to avoid performance impacts.
6. **Shutdown properly**: Always call `shutdown_telemetry()` before application exit.

## Troubleshooting

### Common Issues

- **No spans appearing in Jaeger**: Check that Jaeger is running and the endpoint is correct.
- **Spans missing information**: Ensure fields are added to spans correctly.
- **High overhead**: Reduce sampling ratio or limit instrumentation points.

For additional help, submit an issue in the GitHub repository.
