/// Factory for creating HTTP transport instances.
/// This factory creates configured HTTP transport clients for connecting to MCP servers.
#[derive(Debug)]
pub struct HttpTransportFactory {
    _config: TransportConfig,
}

impl HttpTransportFactory {
    /// Creates a new HTTP transport factory with the provided configuration.
    ///
    /// # Arguments
    /// * `config` - The transport configuration to use for created clients.
    pub fn new(config: TransportConfig) -> Self {
        Self { _config: config }
    }
} 