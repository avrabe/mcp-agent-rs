/// State shared between transport server components
#[derive(Debug)]
pub struct TransportServerState {
    /// Message handler
    message_handler: Arc<dyn MessageHandler>,
}

/// Server implementation that can handle multiple transport types
#[derive(Debug)]
pub struct TransportServer {
    /// Server state
    state: Arc<TransportServerState>,
    /// Server configuration
    config: TransportServerConfig,
} 