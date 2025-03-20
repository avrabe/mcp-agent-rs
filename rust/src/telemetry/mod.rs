//! Telemetry module provides OpenTelemetry integration for tracing and metrics.
//!
//! This module offers comprehensive observability features:
//! - Distributed tracing with Jaeger or OTLP exporters
//! - Custom metrics collection and export
//! - Span instrumentation with detailed context
//! - Resource and attributes management
//! - Alerting system based on telemetry thresholds

use std::collections::HashMap;
use tracing::{span, Span, Level, field};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

// Re-export alerts module
pub mod alerts;
use alerts::{AlertingSystem, AlertingConfig, AlertDefinition};

// Global alerting system
use std::sync::OnceLock;
static ALERTING_SYSTEM: OnceLock<AlertingSystem> = OnceLock::new();

/// Get the global alerting system instance
pub fn alerting() -> &'static AlertingSystem {
    ALERTING_SYSTEM.get_or_init(|| {
        AlertingSystem::new(AlertingConfig::default())
    })
}

/// Configuration for the telemetry system
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// The service name to use in traces and metrics
    pub service_name: String,
    /// The endpoint to send OTLP data to (e.g., "http://localhost:4317")
    pub otlp_endpoint: Option<String>,
    /// The Jaeger endpoint to send traces to (e.g., "127.0.0.1:6831")
    pub jaeger_endpoint: Option<String>,
    /// Whether to enable console logging of traces
    pub enable_console: bool,
    /// Whether to enable JSON formatted logs
    pub enable_json: bool,
    /// Whether to enable tracing
    pub enable_tracing: bool,
    /// Whether to enable metrics
    pub enable_metrics: bool,
    /// The sampling ratio for traces (0.0 to 1.0)
    pub sampling_ratio: f64,
    /// Custom attributes to add to all spans
    pub attributes: HashMap<String, String>,
    /// Alerting system configuration
    pub alerting_config: Option<AlertingConfig>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "mcp-agent".to_string(),
            otlp_endpoint: None,
            jaeger_endpoint: None,
            enable_console: true,
            enable_json: false,
            enable_tracing: true,
            enable_metrics: true,
            sampling_ratio: 1.0,
            attributes: HashMap::new(),
            alerting_config: None,
        }
    }
}

/// Initialize telemetry based on the configured features and options
pub fn init_telemetry(config: TelemetryConfig) -> Result<(), Box<dyn std::error::Error>> {
    // Initialize alerting system
    if let Some(alerting_config) = &config.alerting_config {
        let _ = ALERTING_SYSTEM.get_or_init(|| {
            AlertingSystem::new(alerting_config.clone())
        });
    } else {
        // Initialize with default configuration
        let _ = ALERTING_SYSTEM.get_or_init(|| {
            AlertingSystem::new(AlertingConfig::default())
        });
    }
    
    #[cfg(any(feature = "telemetry-jaeger", feature = "telemetry-otlp"))]
    {
        if config.enable_tracing {
            return init_opentelemetry(config);
        }
    }
    
    // Fall back to simple tracing if no OpenTelemetry features are enabled
    // or if tracing is disabled
    if config.enable_console {
        if config.enable_json {
            tracing_subscriber::fmt()
                .json()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .init();
        } else {
            tracing_subscriber::fmt()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .init();
        }
    }
    
    Ok(())
}

/// Initialize OpenTelemetry with either Jaeger or OTLP exporter
#[cfg(any(feature = "telemetry-jaeger", feature = "telemetry-otlp"))]
fn init_opentelemetry(config: TelemetryConfig) -> Result<(), Box<dyn std::error::Error>> {
    use opentelemetry::KeyValue;
    
    // Build resource with service name and custom attributes
    let mut resource_attributes = Vec::new();
    resource_attributes.push(KeyValue::new("service.name", config.service_name.clone()));
    resource_attributes.push(KeyValue::new("service.version", env!("CARGO_PKG_VERSION")));
    
    // Add custom attributes
    for (key, value) in &config.attributes {
        resource_attributes.push(KeyValue::new(key.clone(), value.clone()));
    }

    // Create tracer based on available features
    
    #[cfg(feature = "telemetry-otlp")]
    if let Some(endpoint) = &config.otlp_endpoint {
        return setup_otlp_tracer(&config, endpoint, resource_attributes);
    }
    
    #[cfg(feature = "telemetry-jaeger")]
    if let Some(endpoint) = &config.jaeger_endpoint {
        return setup_jaeger_tracer(&config, endpoint, resource_attributes);
    }
    
    #[cfg(feature = "telemetry-jaeger")]
    return setup_jaeger_tracer(&config, "127.0.0.1:6831", resource_attributes);
    
    #[allow(unreachable_code)]
    {
        // Fallback to console-only tracing
        if config.enable_console {
            if config.enable_json {
                tracing_subscriber::fmt()
                    .json()
                    .with_env_filter(EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new("info")))
                    .init();
            } else {
                tracing_subscriber::fmt()
                    .with_env_filter(EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| EnvFilter::new("info")))
                    .init();
            }
        } else {
            tracing_subscriber::registry()
                .with(EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("info")))
                .init();
        }
        
        Ok(())
    }
}

/// Set up OTLP tracer
#[cfg(feature = "telemetry-otlp")]
fn setup_otlp_tracer(
    config: &TelemetryConfig,
    endpoint: &str, 
    resource_attributes: Vec<opentelemetry::KeyValue>
) -> Result<(), Box<dyn std::error::Error>> {
    use opentelemetry_otlp::WithExportConfig;
    use tracing_opentelemetry::OpenTelemetryLayer;
    
    // Set up the tracer
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .http()
                .with_endpoint(endpoint),
        )
        .with_trace_config(
            opentelemetry_sdk::trace::config()
                .with_sampler(opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(config.sampling_ratio))
                .with_resource(opentelemetry_sdk::Resource::new(resource_attributes))
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;
    
    // Create registry with OpenTelemetry layer
    let otel_layer = OpenTelemetryLayer::new(tracer);
    
    // Combine layers based on configuration
    if config.enable_console {
        if config.enable_json {
            tracing_subscriber::registry()
                .with(otel_layer)
                .with(tracing_subscriber::fmt::layer().json())
                .with(EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("info")))
                .init();
        } else {
            tracing_subscriber::registry()
                .with(otel_layer)
                .with(tracing_subscriber::fmt::layer())
                .with(EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("info")))
                .init();
        }
    } else {
        tracing_subscriber::registry()
            .with(otel_layer)
            .with(EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")))
            .init();
    }
    
    Ok(())
}

/// Set up Jaeger tracer
#[cfg(feature = "telemetry-jaeger")]
fn setup_jaeger_tracer(
    config: &TelemetryConfig,
    endpoint: &str,
    resource_attributes: Vec<opentelemetry::KeyValue>
) -> Result<(), Box<dyn std::error::Error>> {
    use tracing_opentelemetry::OpenTelemetryLayer;
    
    // Parse agent endpoint into host and port
    let parts: Vec<&str> = endpoint.split(':').collect();
    let (agent_host, agent_port) = match parts.as_slice() {
        [host, port] => (host.to_string(), port.parse::<u16>().unwrap_or(6831)),
        _ => ("127.0.0.1".to_string(), 6831),
    };
    
    // Configure trace config
    let trace_config = opentelemetry_sdk::trace::config()
        .with_sampler(opentelemetry_sdk::trace::Sampler::TraceIdRatioBased(config.sampling_ratio))
        .with_resource(opentelemetry_sdk::Resource::new(resource_attributes));
    
    // Configure and install Jaeger tracer
    let tracer = opentelemetry_jaeger::new_agent_pipeline()
        .with_service_name(config.service_name.clone())
        .with_endpoint(format!("{}:{}", agent_host, agent_port))
        .with_trace_config(trace_config)
        .install_simple()?;
    
    // Create registry with OpenTelemetry layer
    let otel_layer = OpenTelemetryLayer::new(tracer);
    
    // Combine layers based on configuration
    if config.enable_console {
        if config.enable_json {
            tracing_subscriber::registry()
                .with(otel_layer)
                .with(tracing_subscriber::fmt::layer().json())
                .with(EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("info")))
                .init();
        } else {
            tracing_subscriber::registry()
                .with(otel_layer)
                .with(tracing_subscriber::fmt::layer())
                .with(EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new("info")))
                .init();
        }
    } else {
        tracing_subscriber::registry()
            .with(otel_layer)
            .with(EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")))
            .init();
    }
    
    Ok(())
}

/// Shutdown the OpenTelemetry system
///
/// This ensures all pending spans are exported before the application exits.
pub fn shutdown_telemetry() {
    #[cfg(any(feature = "telemetry-jaeger", feature = "telemetry-otlp"))]
    opentelemetry::global::shutdown_tracer_provider();
}

/// Create a span duration tracker for measuring operation durations
///
/// This function creates a guard that measures the time between creation and drop,
/// then records that duration as a metric.
pub fn span_duration(name: &'static str) -> impl Drop {
    let start = std::time::Instant::now();
    
    // Create a tracing span
    let span = tracing::info_span!("operation", name = name);
    let _guard = span.enter();
    
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

/// Add metrics to the current span and process alerting
///
/// This function adds metrics to the current span for later collection
/// and processes them through the alerting system to trigger alerts
/// if any thresholds are exceeded.
pub fn add_metrics(metrics: HashMap<&'static str, f64>) {
    // Record metrics in tracing
    for (key, value) in metrics.iter() {
        tracing::info!(
            target: "metrics",
            metric = key,
            value = value,
        );
    }
    
    // Process alerts based on metrics
    alerting().process_metrics(&metrics);
}

/// Create a new trace context
pub fn create_trace_context(name: &'static str, level: Level) -> Span {
    match level {
        Level::TRACE => tracing::trace_span!("context", name = name),
        Level::DEBUG => tracing::debug_span!("context", name = name),
        Level::INFO => tracing::info_span!("context", name = name),
        Level::WARN => tracing::warn_span!("context", name = name),
        Level::ERROR => tracing::error_span!("context", name = name),
    }
}

/// Sets the error flag and message on the current span
pub fn set_error_on_current_span(error: &dyn std::error::Error) {
    span::Span::current().record("error", true);
    span::Span::current().record("error.message", &field::display(error));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert_eq!(config.service_name, "mcp-agent");
        assert_eq!(config.otlp_endpoint, None);
        assert_eq!(config.jaeger_endpoint, None);
        assert!(config.enable_console);
        assert!(!config.enable_json);
        assert!(config.enable_tracing);
        assert!(config.enable_metrics);
        assert_eq!(config.sampling_ratio, 1.0);
        assert!(config.attributes.is_empty());
    }
    
    #[tokio::test]
    async fn test_span_duration() {
        // This is mostly to test that span_duration doesn't panic
        let _guard = span_duration("test_operation");
        tokio::time::sleep(Duration::from_millis(10)).await;
        // Guard will be dropped here and should record the duration
    }
    
    #[test]
    fn test_add_metrics() {
        // Just ensure it doesn't panic
        let mut metrics = HashMap::new();
        metrics.insert("test_metric", 42.0);
        add_metrics(metrics);
    }
} 