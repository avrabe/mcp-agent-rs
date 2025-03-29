/// Fix the stop method to match the trait
pub async fn stop(&self) -> Result<()> {
    if self.is_running.load(Ordering::SeqCst) {
        log::debug!("Stopping web terminal server");
        
        // Mark as not running first to prevent any new connections
        self.is_running.store(false, Ordering::SeqCst);

        // Close all active client connections first - with timeout
        let close_clients_result = tokio::time::timeout(Duration::from_millis(500), async {
            let mut state = match self.state.try_lock() {
                Ok(guard) => guard,
                Err(_) => {
                    log::warn!("Could not acquire state lock when stopping web terminal - continuing anyway");
                    return;
                }
            };
            
            // Get all client IDs to avoid borrowing issues
            let client_ids: Vec<String> = state.clients.keys().cloned().collect();
            log::debug!("Closing {} active client connections", client_ids.len());
            
            // Close each connection
            for client_id in client_ids {
                if let Some(client) = state.clients.get_mut(&client_id) {
                    if let Some(sender) = client.sender.take() {
                        // Use a simple send without a timeout since it's synchronous
                        match sender.send(Message::Close(None)) {
                            Ok(_) => log::debug!("Sent close message to client {}", client_id),
                            Err(e) => log::error!("Failed to close websocket connection for client {}: {}", client_id, e),
                        }
                    }
                }
                
                // Remove client from the map
                state.clients.remove(&client_id);
            }
            
            // Clear all clients
            state.clients.clear();
        }).await;
        
        if close_clients_result.is_err() {
            log::warn!("Timed out closing client connections - continuing with shutdown");
        }
        
        // Wait a short time for connections to close gracefully
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        // Send shutdown signal if available - with timeout
        let shutdown_result = tokio::time::timeout(Duration::from_millis(500), async {
            let shutdown_sender = match self.shutdown_tx.try_lock() {
                Ok(mut guard) => guard.take(),
                Err(_) => {
                    log::warn!("Could not acquire shutdown_tx lock - continuing anyway");
                    None
                }
            };

            if let Some(tx) = shutdown_sender {
                // Use a simple send without a timeout since it's synchronous
                match tx.send(()) {
                    Ok(_) => log::debug!("Sent shutdown signal to web server"),
                    Err(e) => log::error!("Failed to send shutdown signal: {}", e),
                }
            } else {
                log::debug!("No shutdown sender available");
            }
        }).await;
        
        if shutdown_result.is_err() {
            log::warn!("Timed out sending shutdown signal - continuing anyway");
        }
        
        // Give server time to shutdown
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        
        log::debug!("Web terminal server stopped");
    } else {
        log::debug!("Web terminal server already stopped");
    }

    Ok(())
} 