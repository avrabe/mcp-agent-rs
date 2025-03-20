//! Agent module for handling agent lifecycle and connections to remote endpoints.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{debug, info, warn, error, instrument, trace_span, Span};
use uuid::Uuid;

use crate::mcp::types::Message;
use crate::utils::error::{McpError, McpResult};
use crate::telemetry;

/// Configuration for an MCP agent.
#[derive(Debug, Clone, Default)]
pub struct AgentConfig {
    /// Default connection timeout in seconds
    pub connection_timeout_secs: u64,
    /// Default request timeout in seconds
    pub request_timeout_secs: u64,
    /// Whether to enable automatic reconnection
    pub enable_auto_reconnect: bool,
    /// Interval for health checks in seconds
    pub health_check_interval_secs: u64,
}

/// An agent that can connect to multiple servers and manage message passing.
#[derive(Debug)]
pub struct Agent {
    /// Active connections
    connections: Arc<Mutex<HashMap<String, String>>>,
    /// Agent configuration
    config: AgentConfig,
    /// Message statistics
    stats: Arc<Mutex<AgentStats>>,
}

/// Message and connection statistics for the agent
#[derive(Debug, Default)]
struct AgentStats {
    /// Total number of messages sent
    messages_sent: u64,
    /// Total number of messages received
    messages_received: u64,
    /// Total number of failed operations
    operation_failures: u64,
    /// Total number of connection attempts
    connection_attempts: u64,
    /// Total number of successful connections
    connection_successes: u64,
    /// Total number of connection failures
    connection_failures: u64,
    /// Map of server IDs to operation counts
    server_operations: HashMap<String, u64>,
}

impl Agent {
    /// Creates a new agent with optional configuration
    #[instrument(skip(_config))]
    pub fn new(_config: Option<serde_json::Value>) -> Self {
        debug!("Creating new agent");
        
        // In a real implementation, this would parse the config JSON
        let config = AgentConfig::default();
        
        let agent = Self {
            connections: Arc::new(Mutex::new(HashMap::new())),
            config,
            stats: Arc::new(Mutex::new(AgentStats::default())),
        };
        
        info!("Agent created with default configuration");
        agent
    }

    /// Connect to a test server
    #[instrument(skip(self), fields(server_id = %server_id, server_address = %server_address))]
    pub async fn connect_to_test_server(&self, server_id: &str, server_address: &str) -> McpResult<()> {
        let _guard = telemetry::span_duration("connect_to_server");
        let start = Instant::now();
        
        info!("Connecting to server {} at {}", server_id, server_address);
        
        // Update statistics
        {
            let mut stats = self.stats.lock().await;
            stats.connection_attempts += 1;
        }
        
        // Simulate connection process
        let mut connections = self.connections.lock().await;
        connections.insert(server_id.to_string(), server_address.to_string());
        
        let duration = start.elapsed();
        debug!("Connected to server {} in {:?}", server_id, duration);
        
        // Update statistics
        {
            let mut stats = self.stats.lock().await;
            stats.connection_successes += 1;
            
            // Record metrics
            let mut metrics = HashMap::new();
            metrics.insert("connection_duration_ms", duration.as_millis() as f64);
            metrics.insert("active_connections", connections.len() as f64);
            metrics.insert("connection_successes", stats.connection_successes as f64);
            telemetry::add_metrics(metrics);
        }
        
        Ok(())
    }

    /// Returns the number of active connections
    #[instrument(skip(self))]
    pub async fn connection_count(&self) -> usize {
        let connections = self.connections.lock().await;
        let count = connections.len();
        
        debug!("Current connection count: {}", count);
        count
    }

    /// Lists all active connection IDs
    #[instrument(skip(self))]
    pub async fn list_connections(&self) -> Vec<String> {
        let _guard = telemetry::span_duration("list_connections");
        
        let connections = self.connections.lock().await;
        let connection_list = connections.keys().cloned().collect::<Vec<_>>();
        
        debug!("Listed {} active connections", connection_list.len());
        connection_list
    }

    /// Sends a message to a specific server
    #[instrument(skip(self, message), fields(server_id = %server_id, message_type = ?message.message_type, message_id = %message.id))]
    pub async fn send_message(&self, server_id: &str, message: Message) -> McpResult<()> {
        let _guard = telemetry::span_duration("send_message");
        let start = Instant::now();
        
        info!("Sending message to server {}: type={:?}, id={}", 
             server_id, message.message_type, message.id);
        
        // Check if we're connected to this server
        let is_connected = {
            let connections = self.connections.lock().await;
            connections.contains_key(server_id)
        };
        
        if !is_connected {
            warn!("Cannot send message: not connected to server {}", server_id);
            
            // Track failure
            {
                let mut stats = self.stats.lock().await;
                stats.operation_failures += 1;
            }
            
            // Record failure metrics
            let mut metrics = HashMap::new();
            metrics.insert("message_send_failures", 1.0);
            metrics.insert("operation_failures", 1.0);
            telemetry::add_metrics(metrics);
            
            return Err(McpError::NotConnected);
        }
        
        // In a real implementation, this would use the connection to send the message
        // Here we just simulate success
        
        // Update message count
        {
            let mut stats = self.stats.lock().await;
            stats.messages_sent += 1;
            
            let server_ops = stats.server_operations.entry(server_id.to_string()).or_insert(0);
            *server_ops += 1;
        }
        
        // Record metrics for this message
        let duration = start.elapsed();
        debug!("Message sent to server {} in {:?}", server_id, duration);
        
        let mut metrics = HashMap::new();
        metrics.insert("message_send_duration_ms", duration.as_millis() as f64);
        metrics.insert("message_size_bytes", message.payload.len() as f64);
        telemetry::add_metrics(metrics);
        
        Ok(())
    }

    /// Executes a task on a server with arguments and wait for result
    #[instrument(skip(self, args), fields(task_name = %task_name))]
    pub async fn execute_task(
        &self, 
        task_name: &str, 
        args: serde_json::Value,
        timeout: Option<Duration>
    ) -> McpResult<serde_json::Value> {
        let _guard = telemetry::span_duration("execute_task");
        let start = Instant::now();
        
        // Generate a request ID for tracking
        let request_id = Uuid::new_v4().to_string();
        
        info!("Executing task '{}' with request ID {}", task_name, request_id);
        debug!("Task arguments: {}", args);
        
        // In a real implementation, this would send a task request and await response
        // Here we just simulate execution and response
        
        // Simulate task execution time
        let execution_time = rand::random::<u64>() % 100;
        tokio::time::sleep(Duration::from_millis(execution_time)).await;
        
        let result = serde_json::json!({
            "status": "success", 
            "result": "dummy-value",
            "request_id": request_id,
            "execution_time_ms": execution_time
        });
        
        let duration = start.elapsed();
        debug!("Task '{}' completed in {:?}", task_name, duration);
        
        // Record metrics
        let mut metrics = HashMap::new();
        metrics.insert("task_execution_duration_ms", duration.as_millis() as f64);
        metrics.insert("task_execution_time_ms", execution_time as f64);
        telemetry::add_metrics(metrics);
        
        Ok(result)
    }

    /// Disconnect from a specific server
    #[instrument(skip(self), fields(server_id = %server_id))]
    pub async fn disconnect(&self, server_id: &str) -> McpResult<()> {
        let _guard = telemetry::span_duration("disconnect");
        let start = Instant::now();
        
        info!("Disconnecting from server {}", server_id);
        
        let mut connections = self.connections.lock().await;
        let was_connected = connections.remove(server_id).is_some();
        
        if was_connected {
            let duration = start.elapsed();
            info!("Disconnected from server {} in {:?}", server_id, duration);
            
            // Record metrics
            let mut metrics = HashMap::new();
            metrics.insert("disconnection_duration_ms", duration.as_millis() as f64);
            metrics.insert("active_connections", connections.len() as f64);
            telemetry::add_metrics(metrics);
            
            Ok(())
        } else {
            warn!("Cannot disconnect: not connected to server {}", server_id);
            Ok(())
        }
    }
    
    /// Get agent statistics
    #[instrument(skip(self))]
    pub async fn get_stats(&self) -> AgentMetrics {
        let _guard = telemetry::span_duration("get_stats");
        
        let stats = self.stats.lock().await;
        let connections = self.connections.lock().await;
        
        let metrics = AgentMetrics {
            connections_count: connections.len(),
            messages_sent: stats.messages_sent,
            messages_received: stats.messages_received,
            operation_failures: stats.operation_failures,
            connection_attempts: stats.connection_attempts,
            connection_successes: stats.connection_successes,
            connection_failures: stats.connection_failures,
        };
        
        debug!("Retrieved agent metrics: {:?}", metrics);
        metrics
    }
}

/// Public metrics for the agent
#[derive(Debug, Clone)]
pub struct AgentMetrics {
    /// Number of active connections
    pub connections_count: usize,
    /// Total number of messages sent
    pub messages_sent: u64,
    /// Total number of messages received
    pub messages_received: u64,
    /// Total number of operation failures
    pub operation_failures: u64,
    /// Total number of connection attempts
    pub connection_attempts: u64,
    /// Total number of successful connections
    pub connection_successes: u64,
    /// Total number of connection failures
    pub connection_failures: u64,
} 