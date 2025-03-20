use mcp_agent::mcp::agent::{Agent, AgentConfig};
use mcp_agent::mcp::executor::{ExecutorConfig, Signal, TaskResult};
use mcp_agent::utils::error::McpError;
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn test_agent_config() {
    // Test default config
    let default_config = AgentConfig::default();
    assert_eq!(default_config.default_timeout_secs, 30);
    assert_eq!(default_config.max_reconnect_attempts, 5);
    assert_eq!(default_config.reconnect_base_delay_ms, 100);
    assert_eq!(default_config.reconnect_max_delay_ms, 5000);
    assert!(default_config.executor_config.is_none());
    assert!(default_config.mcp_settings.is_none());

    // Test custom config
    let executor_config = ExecutorConfig {
        max_concurrent_tasks: 5,
        default_timeout_secs: 30,
        max_timeout_secs: 1800,
        retry_attempts: 2,
        retry_base_delay_ms: 200,
        retry_max_delay_ms: 5000,
    };

    let custom_config = AgentConfig {
        executor_config: Some(executor_config),
        mcp_settings: None,
        default_timeout_secs: 60,
        max_reconnect_attempts: 10,
        reconnect_base_delay_ms: 200,
        reconnect_max_delay_ms: 10000,
    };

    let agent = Agent::new(Some(custom_config));
    assert_eq!(agent.connection_count().await, 0);
}

#[tokio::test]
async fn test_agent_task_execution() {
    let agent = Agent::new(None);
    let args = json!({ "value": 42 });
    
    let result = agent.execute_task("test_success", args.clone(), None).await.unwrap();
    assert!(result.is_success());
    assert_eq!(result.success_value(), Some(&args));
    
    // Test failure
    let result = agent.execute_task("test_failure", args.clone(), None).await;
    assert!(result.is_err());
    
    // Test timeout
    let result = agent.execute_task(
        "test_timeout", 
        args.clone(),
        Some(Duration::from_millis(100)),
    ).await;
    
    assert!(result.is_err());
    if let Err(e) = result {
        assert!(matches!(e, McpError::Timeout));
    }
}

#[tokio::test]
async fn test_agent_task_streaming() {
    let agent = Agent::new(None);
    let args = json!({ "value": 42 });
    
    let mut rx = agent.execute_task_stream("test_stream_success", args.clone(), None).await.unwrap();
    
    let mut count = 0;
    while let Some(result) = rx.recv().await {
        assert!(result.is_success());
        count += 1;
    }
    
    assert_eq!(count, 5);
}

#[tokio::test]
async fn test_agent_id_generation() {
    let agent = Agent::new(None);
    let id1 = agent.generate_id();
    let id2 = agent.generate_id();
    
    // IDs should be unique
    assert_ne!(id1, id2);
    
    // IDs should be valid UUID strings
    assert_eq!(id1.len(), 36);
    assert_eq!(id2.len(), 36);
} 