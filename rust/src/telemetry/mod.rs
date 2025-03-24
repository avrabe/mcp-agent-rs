//! Telemetry and metrics system for the agent
//!
//! This module provides the telemetry and metrics system for the agent, including:
//! - Custom metrics collection for agent operations
//! - Span instrumentation for tracing
//! - Resource monitoring and management
//! - Alerting system for notifying when certain thresholds are reached

// Standard library
use std::collections::HashMap;
// use std::sync::Arc;
// use std::sync::Mutex;
use std::time::Duration;

// Crates
use async_trait::async_trait;
// use chrono::Utc; // Unused import
use log::warn;
// use std::sync::Arc; // Unused import
// use std::sync::Mutex; // Unused import
use tracing::span;

// Internal
use crate::error::Result;

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
pub fn init_telemetry(
    config: TelemetryConfig,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    // Create a builder for the telemetry system
    let builder = tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(config.log_level)),
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
    // #[cfg(any(feature = "telemetry-jaeger", feature = "telemetry-otlp"))]
    // opentelemetry::global::shutdown_tracer_provider();
}

/// Telemetry trait for sending metrics and traces
#[async_trait]
pub trait Telemetry: Send + Sync {
    /// Send a metric
    async fn send_metric(
        &self,
        name: &str,
        value: f64,
        tags: Option<Vec<(&str, String)>>,
    ) -> Result<()>;

    /// Send a trace
    async fn send_trace(
        &self,
        name: &str,
        duration: Duration,
        status: bool,
        tags: Option<Vec<(&str, String)>>,
    ) -> Result<()>;
}

/// A null implementation of Telemetry that does nothing when telemetry is sent
#[derive(Debug)]
pub struct NullTelemetry;

impl Default for NullTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl NullTelemetry {
    /// Creates a new NullTelemetry instance
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Telemetry for NullTelemetry {
    async fn send_metric(
        &self,
        _name: &str,
        _value: f64,
        _tags: Option<Vec<(&str, String)>>,
    ) -> Result<()> {
        Ok(())
    }

    async fn send_trace(
        &self,
        _name: &str,
        _duration: Duration,
        _status: bool,
        _tags: Option<Vec<(&str, String)>>,
    ) -> Result<()> {
        Ok(())
    }
}

/// Default telemetry implementation which will be used when OpenTelemetry is not available
#[derive(Debug)]
pub struct DefaultTelemetry {
    // pub metrics: Arc<Mutex<Option<metrics::Meter>>>,
    // pub tracer: Arc<Mutex<Option<trace::Tracer>>>,
}

impl Default for DefaultTelemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultTelemetry {
    /// Creates a new DefaultTelemetry instance
    pub fn new() -> Self {
        Self {
            // metrics: Arc::new(Mutex::new(None)),
            // tracer: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl Telemetry for DefaultTelemetry {
    async fn send_metric(
        &self,
        name: &str,
        _value: f64,
        _tags: Option<Vec<(&str, String)>>,
    ) -> Result<()> {
        warn!(
            "OpenTelemetry metrics not implemented yet. Metric: {}",
            name
        );
        Ok(())
    }

    async fn send_trace(
        &self,
        name: &str,
        duration: Duration,
        status: bool,
        _tags: Option<Vec<(&str, String)>>,
    ) -> Result<()> {
        warn!(
            "OpenTelemetry tracing not implemented yet. Trace: {}, duration: {:?}, status: {}",
            name, duration, status
        );
        Ok(())
    }
}
