//! Telemetry and metrics system for the agent
//!
//! This module provides the telemetry and metrics system for the agent, including:
//! - Custom metrics collection for agent operations
//! - Span instrumentation for tracing
//! - Resource monitoring and management
//! - Alerting system for notifying when certain thresholds are reached

use std::collections::HashMap;
use tracing::span;
use tracing_subscriber::EnvFilter;

#[cfg(any(feature = "telemetry-jaeger", feature = "telemetry-otlp"))]
use opentelemetry::sdk::Resource;

#[cfg(any(feature = "telemetry-jaeger", feature = "telemetry-otlp"))]
use tracing_opentelemetry::OpenTelemetryLayer;

/// Configuration for the telemetry system
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// Name of the service
    pub service_name: String,
    /// Enable console output
    pub enable_console: bool,
    /// Log level
    pub log_level: String,
    /// Enable OpenTelemetry
    pub enable_opentelemetry: bool,
    /// OpenTelemetry endpoint
    pub opentelemetry_endpoint: Option<String>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "mcp-agent".to_string(),
            enable_console: true,
            log_level: "info".to_string(),
            enable_opentelemetry: false,
            opentelemetry_endpoint: None,
        }
    }
}

/// Initialize telemetry for the specified service with configuration options
pub fn init_telemetry(config: TelemetryConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Create a builder for the telemetry system
    let builder = tracing_subscriber::fmt().with_env_filter(
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(config.log_level)),
    );

    // Configure based on settings
    if config.enable_console {
        // Initialize with console output
        builder.with_target(true).with_ansi(true).init();
    } else {
        // Initialize without console output
        builder.init();
    }

    Ok(())
}

/// Initialize telemetry with default configuration (for backwards compatibility)
pub fn init_default_telemetry(service_name: &str, enable_console: bool, log_level: Option<&str>) {
    let config = TelemetryConfig {
        service_name: service_name.to_string(),
        enable_console,
        log_level: log_level.unwrap_or("info").to_string(),
        enable_opentelemetry: false,
        opentelemetry_endpoint: None,
    };

    if let Err(e) = init_telemetry(config) {
        eprintln!("Failed to initialize telemetry: {}", e);
    }
}

/// Set an error flag and message on the current span
pub fn set_error_on_current_span(err: &dyn std::error::Error) {
    span::Span::current().record("error", true);
    span::Span::current().record("error.msg", err.to_string().as_str());
}

/// Add a single metric with tags to the telemetry system
pub fn add_metric(name: &str, value: f64, tags: &[(&str, String)]) {
    // Create a hashmap for the metric
    let mut metrics = HashMap::new();
    metrics.insert(name, value);

    // Format tags for tracing
    let tags_str = tags
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join(",");

    // Log the metric for tracing
    tracing::info!(
        target: "metrics",
        metric_name = %name,
        metric_value = %value,
        metric_tags = %tags_str,
        "Recorded metric"
    );
}

/// Add multiple metrics at once
pub fn add_metrics(metrics: HashMap<&'static str, f64>) {
    for (key, value) in metrics.iter() {
        add_metric(key, *value, &[]);
    }
}

/// A span duration tracker for measuring operation durations
pub fn span_duration(name: &'static str) -> impl Drop {
    let start = std::time::Instant::now();
    struct Guard {
        name: &'static str,
        start: std::time::Instant,
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            let duration = self.start.elapsed();
            tracing::info!(
                target: "metrics",
                duration_ms = duration.as_millis() as f64,
                operation = self.name,
                "Operation completed"
            );
        }
    }

    Guard { name, start }
}

/// Shutdown telemetry (placeholder for when OpenTelemetry is enabled)
pub fn shutdown_telemetry() {
    #[cfg(any(feature = "telemetry-jaeger", feature = "telemetry-otlp"))]
    opentelemetry::global::shutdown_tracer_provider();
}
