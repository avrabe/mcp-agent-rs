//! Connection module for handling MCP protocol connections

use std::fmt;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

/// Represents the different types of streams that can be used for MCP communication
#[derive(Clone)]
pub enum StreamType {
    /// A TCP stream connection
    Tcp(Arc<Mutex<TcpStream>>),
}

impl fmt::Debug for StreamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamType::Tcp(_) => write!(f, "StreamType::Tcp"),
        }
    }
} 