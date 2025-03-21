//! Alerting system for telemetry thresholds
//!
//! This module provides functionality to create alerts based on telemetry metrics
//! and report them to various outputs, including terminal.

use colored::Colorize;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tracing::debug;

/// Alert severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AlertSeverity {
    /// Informational alert
    Info,
    /// Warning alert
    Warning,
    /// Error alert
    Error,
    /// Critical alert
    Critical,
}

impl fmt::Display for AlertSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AlertSeverity::Info => write!(f, "INFO"),
            AlertSeverity::Warning => write!(f, "WARNING"),
            AlertSeverity::Error => write!(f, "ERROR"),
            AlertSeverity::Critical => write!(f, "CRITICAL"),
        }
    }
}

/// Alert definition for a specific metric
#[derive(Debug, Clone)]
pub struct AlertDefinition {
    /// Unique identifier for the alert
    pub id: String,
    /// Human-readable name for the alert
    pub name: String,
    /// Description of the alert
    pub description: String,
    /// Metric name to monitor
    pub metric_name: String,
    /// Threshold value that triggers the alert
    pub threshold: f64,
    /// Comparison operator (>, <, >=, <=, ==)
    pub operator: AlertOperator,
    /// Alert severity
    pub severity: AlertSeverity,
    /// Cool-down period after alert is triggered (to avoid alert storms)
    pub cooldown: Duration,
    /// Whether the alert is enabled
    pub enabled: bool,
}

/// Alert comparison operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertOperator {
    /// Greater than
    GreaterThan,
    /// Less than
    LessThan,
    /// Greater than or equal to
    GreaterThanOrEqual,
    /// Less than or equal to
    LessThanOrEqual,
    /// Equal to
    Equal,
}

impl fmt::Display for AlertOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AlertOperator::GreaterThan => write!(f, ">"),
            AlertOperator::LessThan => write!(f, "<"),
            AlertOperator::GreaterThanOrEqual => write!(f, ">="),
            AlertOperator::LessThanOrEqual => write!(f, "<="),
            AlertOperator::Equal => write!(f, "=="),
        }
    }
}

/// Triggered alert instance
#[derive(Debug, Clone)]
pub struct Alert {
    /// Alert definition that triggered this alert
    pub definition: AlertDefinition,
    /// Actual value that triggered the alert
    pub value: f64,
    /// Time when the alert was triggered
    pub triggered_at: Instant,
    /// Whether the alert has been acknowledged
    pub acknowledged: bool,
    /// Time when the alert was acknowledged (if any)
    pub acknowledged_at: Option<Instant>,
}

impl Alert {
    /// Create a new alert instance
    pub fn new(definition: AlertDefinition, value: f64) -> Self {
        Self {
            definition,
            value,
            triggered_at: Instant::now(),
            acknowledged: false,
            acknowledged_at: None,
        }
    }

    /// Check if alert is expired based on cooldown
    pub fn is_expired(&self) -> bool {
        let elapsed = self.triggered_at.elapsed();
        let cooldown = self.definition.cooldown;
        let is_expired = elapsed > cooldown;

        // Debug output to help with test troubleshooting
        if is_expired {
            debug!(
                "Alert '{}' has expired. Elapsed: {:?}, Cooldown: {:?}",
                self.definition.id, elapsed, cooldown
            );
        }

        is_expired
    }

    /// Acknowledge the alert
    pub fn acknowledge(&mut self) {
        self.acknowledged = true;
        self.acknowledged_at = Some(Instant::now());
    }

    /// Format alert for display
    pub fn format(&self) -> String {
        let severity = match self.definition.severity {
            AlertSeverity::Info => self.definition.severity.to_string().blue(),
            AlertSeverity::Warning => self.definition.severity.to_string().yellow(),
            AlertSeverity::Error => self.definition.severity.to_string().red(),
            AlertSeverity::Critical => self.definition.severity.to_string().bright_red().bold(),
        };

        format!(
            "[{}] {}: {} {} {} (current: {})",
            severity,
            self.definition.name,
            self.definition.metric_name,
            self.definition.operator,
            self.definition.threshold,
            self.value
        )
    }
}

/// Terminal display options for alerts
#[derive(Debug, Clone)]
pub struct TerminalAlertOptions {
    /// Whether to show timestamps
    pub show_timestamps: bool,
    /// Whether to use colors in terminal output
    pub use_colors: bool,
    /// Whether to show acknowledged alerts
    pub show_acknowledged: bool,
    /// Maximum severity level to display (None for all)
    pub max_severity: Option<AlertSeverity>,
}

impl Default for TerminalAlertOptions {
    fn default() -> Self {
        Self {
            show_timestamps: true,
            use_colors: true,
            show_acknowledged: false,
            max_severity: None,
        }
    }
}

/// Alerting system configuration
#[derive(Debug, Clone)]
pub struct AlertingConfig {
    /// Whether alerting is enabled
    pub enabled: bool,
    /// Terminal reporting configuration
    pub terminal_reporting: bool,
    /// Terminal alert options
    pub terminal_options: TerminalAlertOptions,
    /// Default cooldown period for alerts
    pub default_cooldown: Duration,
}

impl Default for AlertingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            terminal_reporting: true,
            terminal_options: TerminalAlertOptions::default(),
            default_cooldown: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Session-wide alert suppression
#[derive(Debug)]
pub struct SessionSuppression {
    /// Alerts suppressed for the entire session
    pub session_suppressed: HashSet<String>,
    /// Alerts suppressed temporarily with expiration time
    pub temp_suppressed: HashMap<String, Instant>,
}

impl Default for SessionSuppression {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionSuppression {
    /// Create a new session suppression tracker
    pub fn new() -> Self {
        Self {
            session_suppressed: HashSet::new(),
            temp_suppressed: HashMap::new(),
        }
    }

    /// Suppress an alert for the entire session
    pub fn suppress_for_session(&mut self, alert_id: &str) {
        self.session_suppressed.insert(alert_id.to_string());
        // Remove from temporary suppression if it was there
        self.temp_suppressed.remove(alert_id);
    }

    /// Suppress an alert temporarily
    pub fn suppress_temporarily(&mut self, alert_id: &str, duration: Duration) {
        let expiration = Instant::now() + duration;
        self.temp_suppressed
            .insert(alert_id.to_string(), expiration);
    }

    /// Check if an alert is suppressed
    pub fn is_suppressed(&mut self, alert_id: &str) -> bool {
        // Clean up expired temporary suppressions
        self.clean_expired_suppressions();

        // Check if suppressed for session or temporarily
        self.session_suppressed.contains(alert_id) || self.temp_suppressed.contains_key(alert_id)
    }

    /// Clean up expired temporary suppressions
    fn clean_expired_suppressions(&mut self) {
        let now = Instant::now();
        self.temp_suppressed
            .retain(|_, expiration| *expiration > now);
    }

    /// Reset all suppressions
    pub fn reset(&mut self) {
        self.session_suppressed.clear();
        self.temp_suppressed.clear();
    }
}

/// Main alerting system implementation
#[derive(Debug)]
pub struct AlertingSystem {
    /// Alert definitions
    definitions: Arc<RwLock<Vec<AlertDefinition>>>,
    /// Active alerts
    active_alerts: Arc<Mutex<Vec<Alert>>>,
    /// Alert history
    alert_history: Arc<Mutex<Vec<Alert>>>,
    /// Alert suppression
    suppression: Arc<Mutex<SessionSuppression>>,
    /// Configuration
    config: AlertingConfig,
}

impl AlertingSystem {
    /// Create a new alerting system
    pub fn new(config: AlertingConfig) -> Self {
        Self {
            definitions: Arc::new(RwLock::new(Vec::new())),
            active_alerts: Arc::new(Mutex::new(Vec::new())),
            alert_history: Arc::new(Mutex::new(Vec::new())),
            suppression: Arc::new(Mutex::new(SessionSuppression::new())),
            config,
        }
    }

    /// Add a new alert definition
    pub fn add_definition(&self, definition: AlertDefinition) {
        let mut definitions = self.definitions.write().unwrap();

        // Check if definition with this ID already exists
        if let Some(index) = definitions.iter().position(|d| d.id == definition.id) {
            // Save the ID for logging
            let def_id = definition.id.clone();

            // Replace existing definition
            definitions[index] = definition;
            debug!("Replaced alert definition: {}", def_id);
        } else {
            // Add new definition
            definitions.push(definition);
            debug!("Added alert definition: {}", definitions.last().unwrap().id);
        }
    }

    /// Remove alert definition by ID
    pub fn remove_definition(&self, id: &str) -> bool {
        let mut definitions = self.definitions.write().unwrap();

        if let Some(index) = definitions.iter().position(|d| d.id == id) {
            definitions.remove(index);
            debug!("Removed alert definition: {}", id);
            true
        } else {
            false
        }
    }

    /// Process metrics and trigger alerts
    pub fn process_metrics(&self, metrics: &HashMap<&'static str, f64>) {
        if !self.config.enabled {
            return;
        }

        // Clean up expired alerts first
        self.cleanup_expired_alerts();

        let definitions = self.definitions.read().unwrap();

        // Debug metrics
        debug!("Processing metrics: {:?}", metrics);

        for definition in definitions.iter() {
            if !definition.enabled {
                continue;
            }

            // Skip if alert is suppressed
            let mut suppression = self.suppression.lock().unwrap();
            if suppression.is_suppressed(&definition.id) {
                continue;
            }
            drop(suppression);

            // Check if metric exists and compare against threshold
            if let Some(&value) = metrics.get(definition.metric_name.as_str()) {
                let should_trigger = match definition.operator {
                    AlertOperator::GreaterThan => value > definition.threshold,
                    AlertOperator::LessThan => {
                        // Special case: don't trigger LessThan for exact matches
                        let equals_epsilon = 1e-9;
                        value < definition.threshold
                            && (definition.threshold - value).abs() > equals_epsilon
                    }
                    AlertOperator::GreaterThanOrEqual => value >= definition.threshold,
                    AlertOperator::LessThanOrEqual => value <= definition.threshold,
                    AlertOperator::Equal => {
                        let equals_epsilon = 1e-9;
                        (value - definition.threshold).abs() < equals_epsilon
                    }
                };

                debug!(
                    "Evaluating metric {} (value: {}) against threshold {} with operator {}: should_trigger = {}",
                    definition.metric_name,
                    value,
                    definition.threshold,
                    definition.operator,
                    should_trigger
                );

                if should_trigger {
                    // Check if alert is already active
                    let active_alerts = self.active_alerts.lock().unwrap();
                    let is_active = active_alerts
                        .iter()
                        .any(|a| a.definition.id == definition.id);
                    drop(active_alerts);

                    if !is_active {
                        // Trigger new alert
                        self.trigger_alert(definition.clone(), value);
                    }
                }
            } else {
                debug!("Metric not found: {}", definition.metric_name);
            }
        }
    }

    /// Trigger a new alert
    fn trigger_alert(&self, definition: AlertDefinition, value: f64) {
        let alert = Alert::new(definition, value);

        // Add to active alerts
        {
            let mut active_alerts = self.active_alerts.lock().unwrap();
            debug!(
                "Triggering alert: {} ({})",
                alert.definition.name, alert.definition.id
            );
            active_alerts.push(alert.clone());
        }

        // Add to history
        {
            let mut history = self.alert_history.lock().unwrap();
            history.push(alert.clone());

            // Limit history size
            if history.len() > 1000 {
                history.remove(0);
            }
        }

        // Report to terminal if enabled
        if self.config.terminal_reporting {
            self.report_to_terminal(&alert);
        }
    }

    /// Clean up expired alerts
    fn cleanup_expired_alerts(&self) {
        let mut active_alerts = self.active_alerts.lock().unwrap();
        let before_count = active_alerts.len();

        // Remove expired alerts
        active_alerts.retain(|alert| !alert.is_expired());

        let after_count = active_alerts.len();
        if before_count != after_count {
            debug!("Cleaned up {} expired alerts", before_count - after_count);
        }
    }

    /// Report alert to terminal
    fn report_to_terminal(&self, alert: &Alert) {
        let options = &self.config.terminal_options;

        // Skip if alert is acknowledged and we don't show acknowledged
        if alert.acknowledged && !options.show_acknowledged {
            return;
        }

        // Skip if severity filter is applied
        if let Some(max_severity) = options.max_severity {
            if alert.definition.severity > max_severity {
                return;
            }
        }

        // Format and print alert
        let mut alert_text = alert.format();

        // Add timestamp if enabled
        if options.show_timestamps {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            alert_text = format!("[{}] {}", now, alert_text);
        }

        // Print to terminal
        println!("{}", alert_text);
    }

    /// Acknowledge an alert by ID
    pub fn acknowledge_alert(&self, alert_id: &str) -> bool {
        let mut acknowledged = false;

        // Acknowledge in active alerts
        {
            let mut active_alerts = self.active_alerts.lock().unwrap();

            for alert in active_alerts.iter_mut() {
                if alert.definition.id == alert_id && !alert.acknowledged {
                    alert.acknowledge();
                    acknowledged = true;
                    debug!("Acknowledged alert: {}", alert_id);
                }
            }
        }

        // Acknowledge in history too
        if acknowledged {
            let mut history = self.alert_history.lock().unwrap();

            for alert in history.iter_mut() {
                if alert.definition.id == alert_id && !alert.acknowledged {
                    alert.acknowledge();
                }
            }
        }

        acknowledged
    }

    /// Suppress an alert for the entire session
    pub fn suppress_alert_for_session(&self, alert_id: &str) -> bool {
        let mut suppression = self.suppression.lock().unwrap();
        suppression.suppress_for_session(alert_id);
        debug!("Suppressed alert for session: {}", alert_id);
        true
    }

    /// Suppress an alert temporarily
    pub fn suppress_alert_temporarily(&self, alert_id: &str, duration: Duration) -> bool {
        let mut suppression = self.suppression.lock().unwrap();
        suppression.suppress_temporarily(alert_id, duration);
        debug!(
            "Suppressed alert temporarily: {} (for {:?})",
            alert_id, duration
        );
        true
    }

    /// Get active alerts
    pub fn get_active_alerts(&self) -> Vec<Alert> {
        let mut active_alerts = self.active_alerts.lock().unwrap();

        // Clean up expired alerts
        active_alerts.retain(|alert| !alert.is_expired());

        active_alerts.clone()
    }

    /// Get alert history
    pub fn get_alert_history(&self) -> Vec<Alert> {
        let history = self.alert_history.lock().unwrap();
        history.clone()
    }

    /// Reset all suppressions
    pub fn reset_suppressions(&self) {
        let mut suppression = self.suppression.lock().unwrap();
        suppression.reset();
        debug!("Reset all alert suppressions");
    }
}

// Convenience functions for creating alert definitions
impl AlertDefinition {
    /// Create a new alert definition
    pub fn new(
        id: &str,
        name: &str,
        description: &str,
        metric_name: &str,
        threshold: f64,
        operator: AlertOperator,
        severity: AlertSeverity,
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            metric_name: metric_name.to_string(),
            threshold,
            operator,
            severity,
            cooldown: Duration::from_secs(300), // Default 5 minutes
            enabled: true,
        }
    }

    /// Set a custom cooldown period
    pub fn with_cooldown(mut self, cooldown: Duration) -> Self {
        self.cooldown = cooldown;
        self
    }

    /// Set whether the alert is enabled
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_alert_definition() {
        let alert_def = AlertDefinition::new(
            "high_cpu",
            "High CPU Usage",
            "CPU usage is above threshold",
            "cpu_usage",
            90.0,
            AlertOperator::GreaterThan,
            AlertSeverity::Warning,
        );

        assert_eq!(alert_def.id, "high_cpu");
        assert_eq!(alert_def.threshold, 90.0);
        assert_eq!(alert_def.severity, AlertSeverity::Warning);
    }

    #[test]
    fn test_alert_trigger() {
        let config = AlertingConfig {
            enabled: true,
            terminal_reporting: false, // Don't print to terminal in tests
            ..Default::default()
        };

        let alerting = AlertingSystem::new(config);

        // Add a test definition
        let alert_def = AlertDefinition::new(
            "test_alert",
            "Test Alert",
            "Test alert for unit testing",
            "test_metric",
            50.0,
            AlertOperator::GreaterThan,
            AlertSeverity::Info,
        )
        .with_cooldown(Duration::from_millis(50));

        alerting.add_definition(alert_def);

        // Test with metric below threshold
        let mut metrics = HashMap::new();
        metrics.insert("test_metric", 40.0);
        alerting.process_metrics(&metrics);

        // Should not trigger
        assert_eq!(alerting.get_active_alerts().len(), 0);

        // Test with metric above threshold
        metrics.insert("test_metric", 60.0);
        alerting.process_metrics(&metrics);

        // Should trigger
        assert_eq!(alerting.get_active_alerts().len(), 1);

        // Test cooldown - shouldn't trigger again immediately
        alerting.process_metrics(&metrics);
        assert_eq!(alerting.get_active_alerts().len(), 1);

        // Wait for cooldown to expire
        thread::sleep(Duration::from_millis(60));

        // Active alerts should be empty now (expired)
        assert_eq!(alerting.get_active_alerts().len(), 0);

        // But history should have the alert
        assert_eq!(alerting.get_alert_history().len(), 1);
    }

    #[test]
    fn test_suppression() {
        let alerting = AlertingSystem::new(AlertingConfig::default());

        // Add a test definition
        let alert_def = AlertDefinition::new(
            "test_suppress",
            "Test Suppression",
            "Test alert for suppression",
            "test_metric",
            50.0,
            AlertOperator::GreaterThan,
            AlertSeverity::Info,
        );

        alerting.add_definition(alert_def);

        // Suppress the alert
        assert!(alerting.suppress_alert_for_session("test_suppress"));

        // Test - should not trigger due to suppression
        let mut metrics = HashMap::new();
        metrics.insert("test_metric", 60.0);
        alerting.process_metrics(&metrics);

        // Should not trigger
        assert_eq!(alerting.get_active_alerts().len(), 0);

        // Reset suppressions
        alerting.reset_suppressions();

        // Now it should trigger
        alerting.process_metrics(&metrics);
        assert_eq!(alerting.get_active_alerts().len(), 1);
    }
}
