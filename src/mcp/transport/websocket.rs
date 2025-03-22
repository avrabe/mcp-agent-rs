/// Factory for creating WebSocket transport instances.
/// This factory creates configured WebSocket transport clients for connecting to MCP servers.
#[derive(Debug)]
pub struct WebSocketTransportFactory {
    _config: TransportConfig,
}

impl WebSocketTransportFactory {
    /// Creates a new WebSocket transport factory with the provided configuration.
    ///
    /// # Arguments
    /// * `config` - The transport configuration to use for created clients.
    pub fn new(config: TransportConfig) -> Self {
        Self { _config: config }
    }
} 