use std::collections::HashMap;
use std::thread;
use std::time::Duration;

use mcp_agent::telemetry::alerts::{
    AlertDefinition, AlertOperator, AlertSeverity, AlertingConfig, AlertingSystem,
    TerminalAlertOptions,
};

// Define static string constants for test metrics
static TEST_VALUE: &str = "test_value";
static CPU_USAGE: &str = "cpu_usage";
static MEMORY_USAGE: &str = "memory_usage";

#[test]
fn test_alert_definition_creation() {
    let alert_def = AlertDefinition::new(
        "test_alert",
        "Test Alert",
        "This is a test alert",
        "test_metric",
        75.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Warning,
    );

    assert_eq!(alert_def.id, "test_alert");
    assert_eq!(alert_def.name, "Test Alert");
    assert_eq!(alert_def.description, "This is a test alert");
    assert_eq!(alert_def.metric_name, "test_metric");
    assert_eq!(alert_def.threshold, 75.0);
    assert!(matches!(alert_def.operator, AlertOperator::GreaterThan));
    assert!(matches!(alert_def.severity, AlertSeverity::Warning));
    assert_eq!(alert_def.cooldown, Duration::from_secs(300)); // Default
    assert!(alert_def.enabled);
}

#[test]
fn test_alert_definition_builder() {
    let custom_cooldown = Duration::from_secs(60);
    let alert_def = AlertDefinition::new(
        "test_alert",
        "Test Alert",
        "This is a test alert",
        "test_metric",
        75.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Warning,
    )
    .with_cooldown(custom_cooldown)
    .with_enabled(false);

    assert_eq!(alert_def.cooldown, custom_cooldown);
    assert!(!alert_def.enabled);
}

#[test]
fn test_alert_activation() {
    // Create an alerting system with a short cooldown
    let config = AlertingConfig {
        enabled: true,
        terminal_reporting: false, // Disable terminal output for tests
        terminal_options: TerminalAlertOptions::default(),
        default_cooldown: Duration::from_millis(50), // Short for testing
    };

    let alerting = AlertingSystem::new(config);

    // Add a test alert definition
    let alert_def = AlertDefinition::new(
        "high_value",
        "High Test Value",
        "Test value is above threshold",
        "test_value",
        50.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Warning,
    )
    .with_cooldown(Duration::from_millis(50)); // Use a short cooldown for testing

    alerting.add_definition(alert_def);

    // Check that no alerts are active initially
    assert_eq!(alerting.get_active_alerts().len(), 0);

    // Test value under threshold - should not trigger
    let mut metrics = HashMap::new();
    metrics.insert("test_value", 40.0);
    alerting.process_metrics(&metrics);

    assert_eq!(alerting.get_active_alerts().len(), 0);

    // Test value over threshold - should trigger
    metrics.insert("test_value", 60.0);
    alerting.process_metrics(&metrics);

    assert_eq!(alerting.get_active_alerts().len(), 1);

    // Check the triggered alert properties
    let active_alerts = alerting.get_active_alerts();
    assert_eq!(active_alerts[0].definition.id, "high_value");
    assert_eq!(active_alerts[0].value, 60.0);
    assert!(!active_alerts[0].acknowledged);

    // Trigger again immediately - should not create a second alert (cooldown)
    alerting.process_metrics(&metrics);
    assert_eq!(alerting.get_active_alerts().len(), 1);

    // Wait for cooldown to expire
    thread::sleep(Duration::from_millis(60));

    // Check that alert expired
    assert_eq!(alerting.get_active_alerts().len(), 0);

    // History should still have the alert
    assert_eq!(alerting.get_alert_history().len(), 1);

    // Trigger again - should create a new alert
    alerting.process_metrics(&metrics);
    assert_eq!(alerting.get_active_alerts().len(), 1);
    assert_eq!(alerting.get_alert_history().len(), 2);
}

#[test]
fn test_alert_suppression() {
    let config = AlertingConfig {
        enabled: true,
        terminal_reporting: false,
        terminal_options: TerminalAlertOptions::default(),
        default_cooldown: Duration::from_millis(50),
    };

    let alerting = AlertingSystem::new(config);

    // Add a test alert definition with short cooldown
    let mut alert_def = AlertDefinition::new(
        "test_suppress",
        "Test Suppression",
        "This alert tests suppression",
        TEST_VALUE,
        50.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Warning,
    );

    // Override the default cooldown with a short one for tests
    alert_def.cooldown = Duration::from_millis(50);

    alerting.add_definition(alert_def);

    // Trigger once to ensure everything works
    let mut metrics = HashMap::new();
    metrics.insert(TEST_VALUE, 60.0);
    alerting.process_metrics(&metrics);

    // Print active alerts for debugging
    println!(
        "Active alerts after first trigger: {:?}",
        alerting.get_active_alerts()
    );

    // There should be 1 alert active
    assert!(!alerting.get_active_alerts().is_empty());

    // Wait for cooldown
    thread::sleep(Duration::from_millis(100));

    // Clean up for the suppression test
    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts after cooldown: {:?}", active_alerts);
    assert_eq!(
        active_alerts.len(),
        0,
        "Alerts did not expire after cooldown"
    );

    // Suppress this alert for the session
    assert!(alerting.suppress_alert_for_session("test_suppress"));

    // Should not trigger when suppressed
    metrics.insert(TEST_VALUE, 60.0);
    alerting.process_metrics(&metrics);

    // Verify no alerts are active when suppressed
    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts when suppressed: {:?}", active_alerts);
    assert_eq!(
        active_alerts.len(),
        0,
        "Alert triggered despite being suppressed"
    );

    // Reset suppressions
    alerting.reset_suppressions();

    // Should trigger now
    alerting.process_metrics(&metrics);

    // Verify alert was triggered after reset
    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts after reset: {:?}", active_alerts);
    assert!(!active_alerts.is_empty(), "Alert not triggered after reset");
    assert_eq!(active_alerts[0].definition.id, "test_suppress");

    // Wait for cooldown
    thread::sleep(Duration::from_millis(100));

    // Clean up for next test
    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts after second cooldown: {:?}", active_alerts);
    assert_eq!(
        active_alerts.len(),
        0,
        "Alerts did not expire after second cooldown"
    );

    // Temporarily suppress
    alerting.suppress_alert_temporarily("test_suppress", Duration::from_millis(100));

    // Should not trigger when temporarily suppressed
    alerting.process_metrics(&metrics);
    let active_alerts = alerting.get_active_alerts();
    println!(
        "Active alerts during temporary suppression: {:?}",
        active_alerts
    );
    assert_eq!(
        active_alerts.len(),
        0,
        "Alert triggered despite temporary suppression"
    );

    // Wait for temporary suppression to expire
    thread::sleep(Duration::from_millis(150));

    // Should trigger now
    alerting.process_metrics(&metrics);
    let active_alerts = alerting.get_active_alerts();
    println!(
        "Active alerts after temp suppression expired: {:?}",
        active_alerts
    );
    assert!(
        !active_alerts.is_empty(),
        "Alert not triggered after temporary suppression expired"
    );
    assert_eq!(active_alerts[0].definition.id, "test_suppress");
}

#[test]
fn test_alert_acknowledgement() {
    let config = AlertingConfig {
        enabled: true,
        terminal_reporting: false,
        terminal_options: TerminalAlertOptions::default(),
        default_cooldown: Duration::from_millis(50),
    };

    let alerting = AlertingSystem::new(config);

    // Add a test alert definition with explicit short cooldown
    let mut alert_def = AlertDefinition::new(
        "test_ack",
        "Test Acknowledgement",
        "This alert tests acknowledgement",
        TEST_VALUE,
        50.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Warning,
    );

    // Override the default cooldown with a short one for tests
    alert_def.cooldown = Duration::from_millis(50);

    alerting.add_definition(alert_def);

    // Trigger the alert
    let mut metrics = HashMap::new();
    metrics.insert(TEST_VALUE, 60.0);
    alerting.process_metrics(&metrics);

    // Verify alert is triggered
    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts after triggering: {:?}", active_alerts);
    assert!(!active_alerts.is_empty(), "Alert was not triggered");
    assert_eq!(active_alerts[0].definition.id, "test_ack");
    assert!(!active_alerts[0].acknowledged);

    // Acknowledge the alert
    assert!(alerting.acknowledge_alert("test_ack"));

    // Alert should still be active, but acknowledged
    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts after acknowledgement: {:?}", active_alerts);
    assert!(
        !active_alerts.is_empty(),
        "Alert disappeared after acknowledgement"
    );
    assert!(
        active_alerts[0].acknowledged,
        "Alert was not marked as acknowledged"
    );

    // History should reflect acknowledgement
    let history = alerting.get_alert_history();
    println!("Alert history: {:?}", history);
    assert!(!history.is_empty(), "Alert history is empty");
    assert!(
        history[0].acknowledged,
        "Acknowledgement not recorded in history"
    );

    // Wait for cooldown
    thread::sleep(Duration::from_millis(100));

    // Active alerts should be empty (expired)
    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts after cooldown: {:?}", active_alerts);
    assert_eq!(
        active_alerts.len(),
        0,
        "Alert did not expire after cooldown"
    );

    // Trigger again, should create a new unacknowledged alert
    alerting.process_metrics(&metrics);

    // Verify a new unacknowledged alert
    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts after re-triggering: {:?}", active_alerts);
    assert!(!active_alerts.is_empty(), "Alert was not re-triggered");
    assert_eq!(active_alerts[0].definition.id, "test_ack");
    assert!(
        !active_alerts[0].acknowledged,
        "New alert should not be acknowledged"
    );
}

#[test]
fn test_multiple_alert_definitions() {
    let config = AlertingConfig {
        enabled: true,
        terminal_reporting: false,
        terminal_options: TerminalAlertOptions::default(),
        default_cooldown: Duration::from_millis(50),
    };

    let alerting = AlertingSystem::new(config);

    // Add multiple alert definitions with explicit short cooldowns
    let mut cpu_warning = AlertDefinition::new(
        "cpu_warning",
        "CPU Warning",
        "CPU usage above warning threshold",
        CPU_USAGE,
        80.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Warning,
    );
    cpu_warning.cooldown = Duration::from_millis(50);

    let mut cpu_critical = AlertDefinition::new(
        "cpu_critical",
        "CPU Critical",
        "CPU usage above critical threshold",
        CPU_USAGE,
        95.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Critical,
    );
    cpu_critical.cooldown = Duration::from_millis(50);

    let mut memory_warning = AlertDefinition::new(
        "memory_warning",
        "Memory Warning",
        "Memory usage above warning threshold",
        MEMORY_USAGE,
        70.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Warning,
    );
    memory_warning.cooldown = Duration::from_millis(50);

    let definitions = [cpu_warning, cpu_critical, memory_warning];

    for def in definitions.iter() {
        alerting.add_definition(def.clone());
    }

    // Verify alert definitions were added
    assert_eq!(alerting.get_active_alerts().len(), 0);

    // Test with CPU at warning level but not critical
    let mut metrics = HashMap::new();
    metrics.insert(CPU_USAGE, 85.0);
    metrics.insert(MEMORY_USAGE, 50.0);
    alerting.process_metrics(&metrics);

    // Only CPU warning should trigger
    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts with CPU warning: {:?}", active_alerts);
    assert_eq!(
        active_alerts.len(),
        1,
        "Expected only CPU warning to trigger"
    );
    assert_eq!(active_alerts[0].definition.id, "cpu_warning");

    // Wait for cooldown to expire
    thread::sleep(Duration::from_millis(100));

    // Remove existing alerts from being active
    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts after cooldown: {:?}", active_alerts);
    assert_eq!(
        active_alerts.len(),
        0,
        "Alerts did not expire after cooldown"
    );

    // Test with CPU at critical and memory at warning
    metrics.insert(CPU_USAGE, 98.0);
    metrics.insert(MEMORY_USAGE, 75.0);
    alerting.process_metrics(&metrics);

    // CPU warning, CPU critical and memory warning should all trigger
    let active_alerts = alerting.get_active_alerts();
    println!(
        "Active alerts with CPU critical and memory warning: {:?}",
        active_alerts
    );
    assert_eq!(
        active_alerts.len(),
        3,
        "Expected CPU warning, CPU critical, and memory warning to trigger"
    );

    // Verify each type of alert is present
    let has_cpu_warning = active_alerts
        .iter()
        .any(|a| a.definition.id == "cpu_warning");
    let has_cpu_critical = active_alerts
        .iter()
        .any(|a| a.definition.id == "cpu_critical");
    let has_memory_warning = active_alerts
        .iter()
        .any(|a| a.definition.id == "memory_warning");

    assert!(has_cpu_warning, "CPU warning alert should be triggered");
    assert!(has_cpu_critical, "CPU critical alert should be triggered");
    assert!(
        has_memory_warning,
        "Memory warning alert should be triggered"
    );
}

#[test]
fn test_alert_operator_comparisons() {
    let config = AlertingConfig {
        enabled: true,
        terminal_reporting: false,
        terminal_options: TerminalAlertOptions::default(),
        default_cooldown: Duration::from_millis(50),
    };

    let alerting = AlertingSystem::new(config);

    // Add alerts with different operators, all with short cooldowns
    let mut equal_def = AlertDefinition::new(
        "equal",
        "Equal Test",
        "Value exactly equal to threshold",
        TEST_VALUE,
        50.0,
        AlertOperator::Equal,
        AlertSeverity::Warning,
    );
    equal_def.cooldown = Duration::from_millis(50);

    let mut greater_than_def = AlertDefinition::new(
        "greater_than",
        "Greater Than Test",
        "Value greater than threshold",
        TEST_VALUE,
        50.0,
        AlertOperator::GreaterThan,
        AlertSeverity::Warning,
    );
    greater_than_def.cooldown = Duration::from_millis(50);

    let mut less_than_def = AlertDefinition::new(
        "less_than",
        "Less Than Test",
        "Value less than threshold",
        TEST_VALUE,
        50.0,
        AlertOperator::LessThan,
        AlertSeverity::Warning,
    );
    less_than_def.cooldown = Duration::from_millis(50);

    let mut greater_equal_def = AlertDefinition::new(
        "greater_equal",
        "Greater Than or Equal Test",
        "Value greater than or equal to threshold",
        TEST_VALUE,
        50.0,
        AlertOperator::GreaterThanOrEqual,
        AlertSeverity::Warning,
    );
    greater_equal_def.cooldown = Duration::from_millis(50);

    let mut less_equal_def = AlertDefinition::new(
        "less_equal",
        "Less Than or Equal Test",
        "Value less than or equal to threshold",
        TEST_VALUE,
        50.0,
        AlertOperator::LessThanOrEqual,
        AlertSeverity::Warning,
    );
    less_equal_def.cooldown = Duration::from_millis(50);

    let definitions = [
        equal_def,
        greater_than_def,
        less_than_def,
        greater_equal_def,
        less_equal_def,
    ];

    for def in definitions.iter() {
        alerting.add_definition(def.clone());
    }

    // Test Equal operator
    let mut metrics = HashMap::new();
    metrics.insert(TEST_VALUE, 50.0);
    alerting.process_metrics(&metrics);

    // Equal, GTE, and LTE should trigger
    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts for equal value (50.0): {:?}", active_alerts);
    assert_eq!(
        active_alerts.len(),
        3,
        "Expected Equal, GTE, and LTE to trigger"
    );

    let has_equal = active_alerts.iter().any(|a| a.definition.id == "equal");
    let has_greater_equal = active_alerts
        .iter()
        .any(|a| a.definition.id == "greater_equal");
    let has_less_equal = active_alerts
        .iter()
        .any(|a| a.definition.id == "less_equal");

    assert!(has_equal, "Equal alert should be triggered");
    assert!(
        has_greater_equal,
        "GreaterThanOrEqual alert should be triggered"
    );
    assert!(has_less_equal, "LessThanOrEqual alert should be triggered");

    // Wait for cooldown to expire
    thread::sleep(Duration::from_millis(100));

    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts after cooldown: {:?}", active_alerts);
    assert_eq!(
        active_alerts.len(),
        0,
        "Alerts did not expire after cooldown"
    );

    // Test Greater Than
    metrics.insert(TEST_VALUE, 51.0);
    alerting.process_metrics(&metrics);

    // GreaterThan and GreaterThanOrEqual should trigger
    let active_alerts = alerting.get_active_alerts();
    println!(
        "Active alerts for greater value (51.0): {:?}",
        active_alerts
    );
    assert_eq!(active_alerts.len(), 2, "Expected GT and GTE to trigger");

    let has_greater_than = active_alerts
        .iter()
        .any(|a| a.definition.id == "greater_than");
    let has_greater_equal = active_alerts
        .iter()
        .any(|a| a.definition.id == "greater_equal");

    assert!(has_greater_than, "GreaterThan alert should be triggered");
    assert!(
        has_greater_equal,
        "GreaterThanOrEqual alert should be triggered"
    );

    // Wait for cooldown to expire
    thread::sleep(Duration::from_millis(100));

    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts after cooldown: {:?}", active_alerts);
    assert_eq!(
        active_alerts.len(),
        0,
        "Alerts did not expire after cooldown"
    );

    // Test Less Than
    metrics.insert(TEST_VALUE, 49.0);
    alerting.process_metrics(&metrics);

    // LessThan and LessThanOrEqual should trigger
    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts for less value (49.0): {:?}", active_alerts);
    assert_eq!(active_alerts.len(), 2, "Expected LT and LTE to trigger");

    let has_less_than = active_alerts.iter().any(|a| a.definition.id == "less_than");
    let has_less_equal = active_alerts
        .iter()
        .any(|a| a.definition.id == "less_equal");

    assert!(has_less_than, "LessThan alert should be triggered");
    assert!(has_less_equal, "LessThanOrEqual alert should be triggered");

    // Special case: LessThan shouldn't trigger for exact match
    thread::sleep(Duration::from_millis(100));

    let active_alerts = alerting.get_active_alerts();
    println!("Active alerts after cooldown: {:?}", active_alerts);
    assert_eq!(
        active_alerts.len(),
        0,
        "Alerts did not expire after cooldown"
    );

    metrics.insert(TEST_VALUE, 50.0);
    alerting.process_metrics(&metrics);

    // Equal, GTE, and LTE should trigger, but not LessThan
    let active_alerts = alerting.get_active_alerts();
    println!(
        "Active alerts for equal value (50.0) - second check: {:?}",
        active_alerts
    );
    assert_eq!(
        active_alerts.len(),
        3,
        "Expected Equal, GTE, and LTE to trigger"
    );

    let has_equal = active_alerts.iter().any(|a| a.definition.id == "equal");
    let has_greater_equal = active_alerts
        .iter()
        .any(|a| a.definition.id == "greater_equal");
    let has_less_equal = active_alerts
        .iter()
        .any(|a| a.definition.id == "less_equal");
    let has_less_than = active_alerts.iter().any(|a| a.definition.id == "less_than");

    assert!(has_equal, "Equal alert should be triggered");
    assert!(
        has_greater_equal,
        "GreaterThanOrEqual alert should be triggered"
    );
    assert!(has_less_equal, "LessThanOrEqual alert should be triggered");
    assert!(
        !has_less_than,
        "LessThan alert should NOT be triggered for exact match"
    );
}
