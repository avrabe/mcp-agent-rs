use mcp_agent::mcp::executor::{AsyncioExecutor, ExecutorConfig, Signal, TaskResult};
use mcp_agent::utils::error::McpError;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn test_executor_config() {
    let default_config = ExecutorConfig::default();
    assert_eq!(default_config.max_concurrent_tasks, 10);
    assert_eq!(default_config.default_timeout_secs, 60);
    assert_eq!(default_config.max_timeout_secs, 3600);
    assert_eq!(default_config.retry_attempts, 3);
    assert_eq!(default_config.retry_base_delay_ms, 100);
    assert_eq!(default_config.retry_max_delay_ms, 10000);

    let custom_config = ExecutorConfig {
        max_concurrent_tasks: 5,
        default_timeout_secs: 30,
        max_timeout_secs: 1800,
        retry_attempts: 2,
        retry_base_delay_ms: 200,
        retry_max_delay_ms: 5000,
    };

    let executor = AsyncioExecutor::new(Some(custom_config));
    assert_eq!(executor.running_task_count(), 0);
}

#[tokio::test]
async fn test_task_result_methods() {
    // Test success result
    let value = json!({ "result": 42 });
    let success = TaskResult::Success(value.clone());
    assert!(success.is_success());
    assert_eq!(success.success_value(), Some(&value));
    assert_eq!(success.error_message(), None);
    
    // Test failure result
    let error_msg = "Something went wrong";
    let failure = TaskResult::Failure(error_msg.to_string());
    assert!(!failure.is_success());
    assert_eq!(failure.success_value(), None);
    assert_eq!(failure.error_message(), Some(error_msg));
}

#[tokio::test]
async fn test_signal_construction() {
    // Test basic signal creation
    let signal = Signal::new("test_signal", Some(json!({ "data": "value" })));
    assert_eq!(signal.name, "test_signal");
    assert_eq!(signal.workflow_id, None);
    assert!(signal.payload.is_some());
    
    // Test signal for workflow
    let workflow_id = "test_workflow";
    let signal = Signal::for_workflow("test_signal", workflow_id, None);
    assert_eq!(signal.name, "test_signal");
    assert_eq!(signal.workflow_id, Some(workflow_id.to_string()));
    assert!(signal.payload.is_none());
} 