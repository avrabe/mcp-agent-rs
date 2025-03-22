//! Telemetry example demonstrates integration with Jaeger for distributed tracing
//!
//! This example shows how to:
//! 1. Configure and initialize telemetry with Jaeger
//! 2. Create spans and record operations
//! 3. Add metrics and context to spans
//! 4. Visualize the resulting traces in Jaeger UI

use mcp_agent::telemetry::{
    add_metrics, init_telemetry, set_error_on_current_span, shutdown_telemetry, span_duration,
    TelemetryConfig,
};
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::time::Duration;
use tracing::{info, info_span, instrument, warn};

// Custom error type that implements std::error::Error
#[derive(Debug)]
struct RequestError(String);

impl fmt::Display for RequestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for RequestError {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Configure telemetry for Jaeger
    let mut config = TelemetryConfig::default();
    config.service_name = "telemetry-example".to_string();
    config.jaeger_endpoint = Some("127.0.0.1:6831".to_string());

    // Add custom attributes for this process
    let mut attributes = HashMap::new();
    attributes.insert("environment".to_string(), "development".to_string());
    attributes.insert("version".to_string(), env!("CARGO_PKG_VERSION").to_string());
    config.attributes = attributes;

    // Initialize telemetry
    println!("Initializing telemetry with Jaeger...");
    println!("NOTE: This example requires Jaeger to be running locally.");
    println!("You can start Jaeger with Docker using:");
    println!(
        "  docker run -d --name jaeger -p 6831:6831/udp -p 16686:16686 jaegertracing/all-in-one:latest"
    );

    init_telemetry(config)?;

    // Create a top-level span for the entire operation
    let main_span = info_span!("process", name = "main_process");
    let _main_guard = main_span.enter();

    info!("Starting telemetry example");

    // Perform a sequence of operations to generate interesting spans
    process_request("request-1", 5).await?;

    // Add some delay to see timing in the trace
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Process a second request that will produce an error
    if let Err(e) = process_request("request-2", 15).await {
        warn!(error = %e, "Request processing failed");
        // Record error in the current span
        set_error_on_current_span(&e);
    }

    // Measure a custom operation with span_duration
    {
        let _guard = span_duration("final_operation");
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Add custom metrics
        let mut metrics = HashMap::new();
        metrics.insert("processing_time_ms", 42.5);
        metrics.insert("items_processed", 128.0);
        add_metrics(metrics);
    }

    info!("Telemetry example completed");

    // Ensure all traces are exported
    shutdown_telemetry();
    println!("Telemetry shutdown complete. Check Jaeger UI at http://localhost:16686");
    println!("You should see traces for the 'telemetry-example' service");

    Ok(())
}

/// Process a request with nested spans for different operations
#[instrument(fields(request_id = %id, priority = priority))]
async fn process_request(id: &str, priority: u8) -> Result<(), RequestError> {
    info!("Processing request");

    // Simulate some work
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Measure a specific operation
    let result = validate_request(id, priority).await;

    // Conditionally perform more work
    if result.is_ok() {
        let _span = info_span!("persist", database = "main").entered();
        info!("Persisting request data");

        // Simulate persistence
        tokio::time::sleep(Duration::from_millis(25)).await;

        // Record success metric
        let mut metrics = HashMap::new();
        metrics.insert("request_success", 1.0);
        add_metrics(metrics);
    }

    result
}

/// Validate a request with its own span
#[instrument]
async fn validate_request(id: &str, priority: u8) -> Result<(), RequestError> {
    info!("Validating request");

    // Simulate validation
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Higher priority requests are handled differently
    if priority > 10 {
        // Create a proper error that implements Error trai
        let err = RequestError(format!("Request {id} has too high priority: {priority}"));
        // Return error that will be displayed in span
        return Err(err);
    }

    info!("Request validated successfully");
    Ok(())
}
