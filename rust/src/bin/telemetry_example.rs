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
    // Configure telemetry
    let mut config = TelemetryConfig::default();
    config.service_name = "telemetry-example".to_string();
    config.enable_console = true;
    config.log_level = "debug".to_string();
    config.enable_opentelemetry = true;
    config.opentelemetry_endpoint = Some("http://localhost:4317".to_string());

    // Initialize telemetry
    println!("Initializing telemetry...");
    println!("This example will log information to the console.");
    println!("If OpenTelemetry is enabled via feature flags, it will send data to the endpoint.");

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
