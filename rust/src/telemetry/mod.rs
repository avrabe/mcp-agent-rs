//! Telemetry module provides OpenTelemetry integration for tracing and metrics.

#[cfg(test)]
use std::time::Duration;

/// Configuration for the telemetry system
#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    /// The service name to use in traces and metrics
    pub service_name: String,
    /// The endpoint to send OTLP data to (e.g., "http://localhost:4317")
    pub otlp_endpoint: Option<String>,
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
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "mcp-agent".to_string(),
            otlp_endpoint: None,
            enable_console: true,
            enable_json: false,
            enable_tracing: true,
            enable_metrics: true,
            sampling_ratio: 1.0,
        }
    }
}

/// Initialize telemetry based on the configured features and options
pub fn init_telemetry(config: TelemetryConfig) -> Result<(), Box<dyn std::error::Error>> {
    // We'll add feature-gated implementations in later PRs
    // For now, just set up basic console logging
    
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

/// Shutdown the OpenTelemetry system
///
/// This ensures all pending spans are exported before the application exits.
pub fn shutdown_telemetry() {
    // Placeholder for future implementation
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

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_telemetry_config_default() {
        let config = TelemetryConfig::default();
        assert_eq!(config.service_name, "mcp-agent");
        assert_eq!(config.otlp_endpoint, None);
        assert!(config.enable_console);
        assert!(!config.enable_json);
        assert!(config.enable_tracing);
        assert!(config.enable_metrics);
        assert_eq!(config.sampling_ratio, 1.0);
    }
    
    #[tokio::test]
    async fn test_span_duration() {
        // This is mostly to test that span_duration doesn't panic
        let _guard = span_duration("test_operation");
        tokio::time::sleep(Duration::from_millis(10)).await;
        // Guard will be dropped here and should record the duration
    }
} 