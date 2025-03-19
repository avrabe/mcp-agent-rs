use std::fmt;
use std::io;
use tokio::time::Duration;
use crate::utils::error::McpError as UtilsMcpError;

/// Represents errors that can occur during MCP protocol operations.
#[derive(Debug)]
pub enum McpError {
    /// An I/O error occurred during read/write operations.
    Io(io::Error),
    /// A protocol-level error occurred with a descriptive message.
    Protocol(String),
    /// A connection-related error occurred with a descriptive message.
    Connection(String),
    /// The operation timed out.
    Timeout,
    /// The retry mechanism was exhausted with a descriptive message.
    RetryExhausted(String),
    /// The circuit breaker is open, preventing further operations.
    CircuitBreakerOpen,
    /// The message was invalid with a descriptive message.
    InvalidMessage(String),
    /// A serialization error occurred with a descriptive message.
    Serialization(String),
    /// A deserialization error occurred with a descriptive message.
    Deserialization(String),
}

impl fmt::Display for McpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            McpError::Io(err) => write!(f, "IO error: {}", err),
            McpError::Protocol(msg) => write!(f, "Protocol error: {}", msg),
            McpError::Connection(msg) => write!(f, "Connection error: {}", msg),
            McpError::Timeout => write!(f, "Operation timed out"),
            McpError::RetryExhausted(msg) => write!(f, "Retry exhausted: {}", msg),
            McpError::CircuitBreakerOpen => write!(f, "Circuit breaker is open"),
            McpError::InvalidMessage(msg) => write!(f, "Invalid message: {}", msg),
            McpError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            McpError::Deserialization(msg) => write!(f, "Deserialization error: {}", msg),
        }
    }
}

impl std::error::Error for McpError {}

impl From<io::Error> for McpError {
    fn from(err: io::Error) -> Self {
        McpError::Io(err)
    }
}

impl From<UtilsMcpError> for McpError {
    fn from(err: UtilsMcpError) -> Self {
        match err {
            UtilsMcpError::InvalidMessage(msg) => McpError::InvalidMessage(msg),
            UtilsMcpError::Io(err) => McpError::Io(err),
            UtilsMcpError::Utf8(err) => McpError::Protocol(format!("UTF-8 error: {}", err)),
            UtilsMcpError::ConnectionFailed(msg) => McpError::Connection(msg),
            UtilsMcpError::NotConnected => McpError::Connection("Not connected".to_string()),
            UtilsMcpError::Timeout => McpError::Timeout,
            UtilsMcpError::NotImplemented => McpError::Protocol("Feature not implemented".to_string()),
        }
    }
}

/// Configuration for retry behavior.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// The maximum number of retry attempts.
    pub max_attempts: u32,
    /// The initial delay between retry attempts.
    pub initial_delay: Duration,
    /// The maximum delay between retry attempts.
    pub max_delay: Duration,
    /// The factor by which to increase the delay after each attempt.
    pub backoff_factor: f64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(10),
            backoff_factor: 2.0,
        }
    }
}

/// A circuit breaker that tracks failures and prevents operations when too many failures occur.
#[derive(Debug)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    reset_timeout: Duration,
    failures: u32,
    last_failure: Option<std::time::Instant>,
    state: CircuitBreakerState,
}

#[derive(Debug, PartialEq)]
enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

impl CircuitBreaker {
    /// Creates a new circuit breaker with the given failure threshold and reset timeout.
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            reset_timeout,
            failures: 0,
            last_failure: None,
            state: CircuitBreakerState::Closed,
        }
    }

    /// Records a successful operation, resetting the failure count.
    pub fn record_success(&mut self) {
        self.failures = 0;
        self.last_failure = None;
        self.state = CircuitBreakerState::Closed;
    }

    /// Records a failed operation, potentially opening the circuit breaker.
    pub fn record_failure(&mut self) {
        self.failures += 1;
        self.last_failure = Some(std::time::Instant::now());
        if self.failures >= self.failure_threshold {
            self.state = CircuitBreakerState::Open;
        }
    }

    /// Checks if an operation can be executed based on the circuit breaker state.
    pub fn can_execute(&mut self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if let Some(last_failure) = self.last_failure {
                    if last_failure.elapsed() >= self.reset_timeout {
                        self.state = CircuitBreakerState::HalfOpen;
                        true
                    } else {
                        false
                    }
                } else {
                    true
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }
}

/// Executes an operation with retry behavior based on the provided configuration.
///
/// # Type Parameters
///
/// * `F` - A function that returns a future with a result
/// * `T` - The success type of the operation
/// * `E` - The error type of the operation
///
/// # Arguments
///
/// * `config` - The retry configuration to use
/// * `operation` - The operation to execute with retry behavior
///
/// # Returns
///
/// Returns the result of the operation if successful, or a `McpError` if all retries are exhausted.
pub async fn with_retry<F, T, E>(
    config: &RetryConfig,
    mut operation: F,
) -> Result<T, McpError>
where
    F: FnMut() -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, E>> + Send>>,
    E: std::fmt::Display + Send + Sync + 'static,
{
    let mut attempts = 0;
    let mut delay = config.initial_delay;

    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(error) => {
                attempts += 1;
                if attempts >= config.max_attempts {
                    return Err(McpError::RetryExhausted(error.to_string()));
                }

                tokio::time::sleep(delay).await;
                delay = std::cmp::min(
                    Duration::from_secs_f64(delay.as_secs_f64() * config.backoff_factor),
                    config.max_delay,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_retry_success() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_factor: 2.0,
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<&str, McpError> = with_retry(&config, || {
            let attempts = attempts_clone.clone();
            Box::pin(async move {
                let current = attempts.fetch_add(1, Ordering::SeqCst);
                if current < 2 {
                    Err(McpError::Protocol("Temporary error".to_string()))
                } else {
                    Ok("success")
                }
            })
        })
        .await;

        assert_eq!(result.unwrap(), "success");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_retry_exhaustion() {
        let config = RetryConfig {
            max_attempts: 3,
            initial_delay: Duration::from_millis(10),
            max_delay: Duration::from_millis(100),
            backoff_factor: 2.0,
        };

        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_clone = attempts.clone();

        let result: Result<&str, McpError> = with_retry(&config, || {
            let attempts = attempts_clone.clone();
            Box::pin(async move {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(McpError::Protocol("Persistent error".to_string()))
            })
        })
        .await;

        assert!(matches!(result, Err(McpError::RetryExhausted(_))));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
} 