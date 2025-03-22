//! Connection module for handling MCP protocol connections

use std::collections::HashMap;
use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;
use std::time::Instant;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tracing::{debug, error, info, instrument, trace_span, warn};

use crate::telemetry;
use crate::utils::error::McpResult;

/// Type of stream used for MCP communication
#[derive(Clone)]
pub enum StreamType {
    /// TCP stream connection
    Tcp(Arc<Mutex<TcpStream>>),
}

impl Debug for StreamType {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            StreamType::Tcp(_) => write!(f, "StreamType::Tcp"),
        }
    }
}

/// MCP connection manager
#[derive(Debug)]
pub struct ConnectionManager {
    /// Active connections
    connections: HashMap<String, StreamType>,
}

impl ConnectionManager {
    /// Create a new connection manager
    #[instrument]
    pub fn new() -> Self {
        debug!("Creating new connection manager");
        Self {
            connections: HashMap::new(),
        }
    }

    /// Add a connection with an endpoint identifier
    #[instrument(skip(self), fields(endpoint_id = %endpoint_id, endpoint_address = %endpoint_address))]
    pub async fn add_connection(
        &mut self,
        endpoint_id: String,
        endpoint_address: String,
    ) -> McpResult<()> {
        debug!(
            "Adding endpoint {} with address {}",
            endpoint_id, endpoint_address
        );

        // Try to connect to the endpoint
        match TcpStream::connect(&endpoint_address).await {
            Ok(stream) => {
                self.connections
                    .insert(endpoint_id, StreamType::Tcp(Arc::new(Mutex::new(stream))));
                Ok(())
            }
            Err(e) => {
                error!("Failed to connect to {}: {}", endpoint_address, e);
                Err(e.into())
            }
        }
    }

    /// Check if a connection with the given ID exists
    #[instrument(skip(self), fields(endpoint_id = %endpoint_id))]
    pub fn contains_key(&self, endpoint_id: &str) -> bool {
        self.connections.contains_key(endpoint_id)
    }

    /// Get the number of active connections
    #[instrument(skip(self))]
    pub fn len(&self) -> usize {
        self.connections.len()
    }

    /// Get a list of all connection IDs
    #[instrument(skip(self))]
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.connections.keys()
    }

    /// Remove a connection by ID
    #[instrument(skip(self), fields(endpoint_id = %endpoint_id))]
    pub fn remove(&mut self, endpoint_id: &str) -> Option<StreamType> {
        debug!("Removing connection for endpoint {}", endpoint_id);
        self.connections.remove(endpoint_id)
    }

    /// Connect to a TCP endpoint
    #[instrument(skip(self), fields(endpoint = %endpoint))]
    pub async fn connect_tcp(&mut self, endpoint: &str) -> McpResult<()> {
        let _span_guard = telemetry::span_duration("connect_tcp");
        let start = Instant::now();

        info!("Connecting to TCP endpoint: {}", endpoint);

        let connect_span = trace_span!("tcp_connect", endpoint = %endpoint);
        let _connect_guard = connect_span.enter();

        let stream = match TcpStream::connect(endpoint).await {
            Ok(stream) => {
                info!("Successfully connected to {}", endpoint);
                stream
            }
            Err(e) => {
                error!("Failed to connect to {}: {}", endpoint, e);

                // Record connection failure metrics
                let mut metrics = HashMap::new();
                metrics.insert("connection_failures", 1.0);
                telemetry::add_metrics(metrics);

                return Err(e.into());
            }
        };

        // Store the connection
        self.connections.insert(
            endpoint.to_string(),
            StreamType::Tcp(Arc::new(Mutex::new(stream))),
        );

        let duration = start.elapsed();
        debug!("Connection established in {:?}", duration);

        // Record connection metrics
        let mut metrics = HashMap::new();
        metrics.insert("connection_duration_ms", duration.as_millis() as f64);
        metrics.insert("active_connections", self.connections.len() as f64);
        telemetry::add_metrics(metrics);

        Ok(())
    }

    /// Disconnect from an endpoint
    #[instrument(skip(self), fields(endpoint = %endpoint))]
    pub async fn disconnect(&mut self, endpoint: &str) -> McpResult<()> {
        let _span_guard = telemetry::span_duration("disconnect");

        if self.connections.remove(endpoint).is_some() {
            info!("Disconnected from {}", endpoint);

            // Record connection metrics
            let mut metrics = HashMap::new();
            metrics.insert("active_connections", self.connections.len() as f64);
            telemetry::add_metrics(metrics);

            Ok(())
        } else {
            warn!("No active connection to {}", endpoint);
            Ok(())
        }
    }

    /// Get connection status
    #[instrument(skip(self))]
    pub fn get_connection_status(&self) -> HashMap<String, bool> {
        let _span_guard = telemetry::span_duration("get_connection_status");

        let mut status = HashMap::new();
        for endpoint in self.connections.keys() {
            status.insert(endpoint.clone(), true);
        }

        debug!("Connection status: {} active connections", status.len());
        status
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}
