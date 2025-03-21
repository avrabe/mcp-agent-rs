//! Alerting example demonstrates the alerting system based on telemetry thresholds
//!
//! This example shows how to:
//! 1. Configure and initialize the telemetry with alerting
//! 2. Define alert thresholds for various metrics
//! 3. Trigger alerts by exceeding thresholds
//! 4. Handle alert suppression and acknowledgement

use std::collections::HashMap;
use std::error::Error;
use std::io::{self, Write};
use std::thread;
use std::time::Duration;

use mcp_agent::telemetry::{
    TelemetryConfig, add_metrics, alerting, init_telemetry, shutdown_telemetry,
};
use mcp_agent::{
    AlertDefinition, AlertOperator, AlertSeverity, AlertingConfig, TerminalAlertOptions,
};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Configure telemetry with alerting
    let mut config = TelemetryConfig::default();
    config.service_name = "alerting-example".to_string();

    // Configure terminal alerting
    let terminal_options = TerminalAlertOptions {
        show_timestamps: true,
        use_colors: true,
        show_acknowledged: false,
        max_severity: None,
    };

    let alerting_config = AlertingConfig {
        enabled: true,
        terminal_reporting: true,
        terminal_options,
        default_cooldown: Duration::from_secs(5), // Short cooldown for the example
    };

    config.alerting_config = Some(alerting_config);

    // Initialize telemetry
    println!("Initializing telemetry with alerting system...");
    init_telemetry(config)?;

    // Define alert thresholds
    define_alerts();

    // Main event loop to simulate metrics and handle alerts
    println!("\nStarting metric simulation. Press Ctrl+C to exit.");
    println!("Commands: 's <alert_id>' to suppress an alert for the session");
    println!("          't <alert_id> <seconds>' to temporarily suppress an alert");
    println!("          'a <alert_id>' to acknowledge an active alert");
    println!("          'r' to reset all suppressions");
    println!("          'h' to show active alerts");
    println!("          'q' to quit\n");

    let mut iteration = 0;

    // Start command input processing in a separate thread
    std::thread::spawn(|| {
        process_commands();
    });

    // Main simulation loop
    loop {
        iteration += 1;

        // Simulate increasing CPU usage that exceeds threshold
        let cpu_usage = 70.0 + (iteration as f64 * 2.0) % 40.0; // Oscillates between 70% and 110%

        // Simulate network latency that occasionally spikes
        let network_latency = 50.0 + (if iteration % 5 == 0 { 500.0 } else { 0.0 });

        // Simulate memory usage that grows then drops
        let memory_usage = (iteration as f64 * 3.0) % 100.0;

        // Simulate error rate that spikes every 10 iterations
        let error_rate = if iteration % 10 == 0 { 15.0 } else { 1.0 };

        // Collect metrics
        let mut metrics = HashMap::new();
        metrics.insert("cpu_usage", cpu_usage);
        metrics.insert("network_latency_ms", network_latency);
        metrics.insert("memory_usage_percent", memory_usage);
        metrics.insert("error_rate", error_rate);

        // Record metrics and trigger alerts if thresholds are exceeded
        add_metrics(metrics);

        info!(
            "Recorded metrics - CPU: {}%, Network: {}ms, Memory: {}%, Errors: {}/sec",
            cpu_usage, network_latency, memory_usage, error_rate
        );

        // Get active alerts and display count
        let active_alerts = alerting().get_active_alerts();
        if !active_alerts.is_empty() {
            println!("\nActive alerts: {}", active_alerts.len());
        }

        thread::sleep(Duration::from_secs(1));
    }
}

/// Define alert thresholds for various metrics
fn define_alerts() {
    let alert_definitions = [
        // High CPU usage alert
        AlertDefinition::new(
            "high_cpu",
            "High CPU Usage",
            "CPU usage is above threshold",
            "cpu_usage",
            90.0,
            AlertOperator::GreaterThan,
            AlertSeverity::Warning,
        ),
        // Critical CPU usage alert
        AlertDefinition::new(
            "critical_cpu",
            "Critical CPU Usage",
            "CPU usage is critically high",
            "cpu_usage",
            95.0,
            AlertOperator::GreaterThan,
            AlertSeverity::Critical,
        ),
        // High network latency alert
        AlertDefinition::new(
            "high_latency",
            "High Network Latency",
            "Network latency is above threshold",
            "network_latency_ms",
            200.0,
            AlertOperator::GreaterThan,
            AlertSeverity::Warning,
        )
        .with_cooldown(Duration::from_secs(30)),
        // High memory usage alert
        AlertDefinition::new(
            "high_memory",
            "High Memory Usage",
            "Memory usage is above threshold",
            "memory_usage_percent",
            80.0,
            AlertOperator::GreaterThan,
            AlertSeverity::Warning,
        ),
        // High error rate alert
        AlertDefinition::new(
            "high_error_rate",
            "High Error Rate",
            "Error rate is above threshold",
            "error_rate",
            10.0,
            AlertOperator::GreaterThan,
            AlertSeverity::Error,
        ),
    ];

    // Register all alert definitions
    for definition in alert_definitions.iter() {
        println!(
            "Registering alert definition: {} ({})",
            definition.name, definition.id
        );
        alerting().add_definition(definition.clone());
    }
}

/// Process user commands for alert management
fn process_commands() {
    let stdin = io::stdin();
    let mut buffer = String::new();

    loop {
        buffer.clear();
        print!("> ");
        let _ = io::stdout().flush();

        if stdin.read_line(&mut buffer).is_err() {
            warn!("Failed to read command");
            continue;
        }

        let command = buffer.trim();
        if command.is_empty() {
            continue;
        }

        let parts: Vec<&str> = command.split_whitespace().collect();

        match parts[0] {
            "q" | "quit" | "exit" => {
                println!("Shutting down...");
                shutdown_telemetry();
                std::process::exit(0);
            }
            "s" => {
                if parts.len() < 2 {
                    println!("Usage: s <alert_id>");
                    continue;
                }
                let alert_id = parts[1];
                if alerting().suppress_alert_for_session(alert_id) {
                    println!("Alert {} suppressed for the session", alert_id);
                } else {
                    println!("Failed to suppress alert {}", alert_id);
                }
            }
            "t" => {
                if parts.len() < 3 {
                    println!("Usage: t <alert_id> <seconds>");
                    continue;
                }
                let alert_id = parts[1];
                let seconds = parts[2].parse::<u64>().unwrap_or(60);
                let duration = Duration::from_secs(seconds);

                if alerting().suppress_alert_temporarily(alert_id, duration) {
                    println!("Alert {} suppressed for {} seconds", alert_id, seconds);
                } else {
                    println!("Failed to suppress alert {}", alert_id);
                }
            }
            "a" => {
                if parts.len() < 2 {
                    println!("Usage: a <alert_id>");
                    continue;
                }
                let alert_id = parts[1];
                if alerting().acknowledge_alert(alert_id) {
                    println!("Alert {} acknowledged", alert_id);
                } else {
                    println!("No active alert with ID {}", alert_id);
                }
            }
            "r" => {
                alerting().reset_suppressions();
                println!("All suppressions reset");
            }
            "h" => {
                let active_alerts = alerting().get_active_alerts();
                if active_alerts.is_empty() {
                    println!("No active alerts");
                } else {
                    println!("Active alerts:");
                    for (i, alert) in active_alerts.iter().enumerate() {
                        println!("{}. {}", i + 1, alert.format());
                    }
                }
            }
            _ => {
                println!("Unknown command: {}", command);
                println!("Commands: 's <alert_id>' to suppress an alert for the session");
                println!("          't <alert_id> <seconds>' to temporarily suppress an alert");
                println!("          'a <alert_id>' to acknowledge an active alert");
                println!("          'r' to reset all suppressions");
                println!("          'h' to show active alerts");
                println!("          'q' to quit");
            }
        }
    }
}
