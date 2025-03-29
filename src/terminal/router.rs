use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::workflow::WorkflowEngine;
use crate::error::{Error, Result};

use super::Terminal;
use super::config::TerminalConfig;
// Comment out the missing import
// use super::web::WebTerminalServer;
use super::web::WebTerminal;
use super::console::ConsoleTerminal;
use super::graph::{GraphManager, GraphDataProvider}; 

/// Stop all terminals
pub async fn stop(&self) -> Result<()> {
    info!("Stopping terminal router");

    // Stop console terminal if it exists
    {
        let mut console_lock = match tokio::time::timeout(Duration::from_millis(500), self.console_terminal.lock()).await {
            Ok(lock) => lock,
            Err(_) => {
                warn!("Timed out acquiring console_terminal lock during shutdown");
                return Ok(());  // Continue with shutdown
            }
        };
        
        if let Some(console) = console_lock.take() {
            debug!("Stopping console terminal");
            
            // Get ID with timeout
            let id = match tokio::time::timeout(Duration::from_millis(200), console.id_sync()).await {
                Ok(result) => match result {
                    Ok(id) => id,
                    Err(e) => {
                        warn!("Error getting console terminal ID: {:?}", e);
                        "console".to_string()  // Fallback ID
                    }
                },
                Err(_) => {
                    warn!("Timed out getting console terminal ID");
                    "console".to_string()  // Fallback ID
                }
            };

            // Stop with timeout
            match tokio::time::timeout(
                Duration::from_secs(2),
                console.stop_sync()
            ).await {
                Ok(result) => {
                    if let Err(e) = result {
                        warn!("Error stopping console terminal: {:?}", e);
                    }
                },
                Err(_) => {
                    warn!("Timed out stopping console terminal");
                }
            }

            // Unregister from synchronizer with timeout
            let synchronizer_result = tokio::time::timeout(
                Duration::from_millis(500),
                self.synchronizer.lock()
            ).await;
            
            match synchronizer_result {
                Ok(synchronizer) => {
                    if let Err(e) = tokio::time::timeout(
                        Duration::from_millis(500),
                        synchronizer.unregister_terminal(&id)
                    ).await {
                        warn!("Timed out unregistering console terminal");
                    } else {
                        debug!("Console terminal unregistered");
                    }
                },
                Err(_) => {
                    warn!("Timed out acquiring synchronizer lock");
                }
            }
        }
    }

    // Stop web terminal if it exists
    {
        let mut web_lock = match tokio::time::timeout(Duration::from_millis(500), self.web_terminal.lock()).await {
            Ok(lock) => lock,
            Err(_) => {
                warn!("Timed out acquiring web_terminal lock during shutdown");
                return Ok(());  // Continue with shutdown
            }
        };
        
        if let Some(web) = web_lock.take() {
            debug!("Stopping web terminal");
            
            // Get ID with timeout
            let id = match tokio::time::timeout(Duration::from_millis(200), web.id_sync()).await {
                Ok(result) => match result {
                    Ok(id) => id,
                    Err(e) => {
                        warn!("Error getting web terminal ID: {:?}", e);
                        "web".to_string()  // Fallback ID
                    }
                },
                Err(_) => {
                    warn!("Timed out getting web terminal ID");
                    "web".to_string()  // Fallback ID
                }
            };

            // Manually call stop on the terminal with timeout
            match tokio::time::timeout(
                Duration::from_secs(2),
                terminal_helpers::stop(&*web)
            ).await {
                Ok(result) => {
                    if let Err(e) = result {
                        warn!("Error stopping web terminal: {:?}", e);
                    }
                },
                Err(_) => {
                    warn!("Timed out stopping web terminal");
                }
            }

            // Unregister from synchronizer with timeout
            let synchronizer_result = tokio::time::timeout(
                Duration::from_millis(500),
                self.synchronizer.lock()
            ).await;
            
            match synchronizer_result {
                Ok(synchronizer) => {
                    if let Err(e) = tokio::time::timeout(
                        Duration::from_millis(500),
                        synchronizer.unregister_terminal(&id)
                    ).await {
                        warn!("Timed out unregistering web terminal");
                    } else {
                        debug!("Web terminal unregistered");
                    }
                },
                Err(_) => {
                    warn!("Timed out acquiring synchronizer lock");
                }
            }
        }
    }

    // Wait a short time for cleanup
    tokio::time::sleep(Duration::from_millis(50)).await;

    info!("Terminal router stopped");
    Ok(())
} 