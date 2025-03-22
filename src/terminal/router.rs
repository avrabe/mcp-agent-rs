use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, error, info};

use crate::workflow::WorkflowEngine;
use crate::error::{Error, Result};

use super::Terminal;
use super::config::TerminalConfig;
// Comment out the missing import
// use super::web::WebTerminalServer;
use super::web::WebTerminal;
use super::console::ConsoleTerminal;
use super::graph::{GraphManager, GraphDataProvider}; 