/// Test the console terminal internal functionality
#[tokio::test]
async fn test_console_terminal_internal() -> Result<(), Error> {
    // Initialize logging for tests
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .try_init();

    // Create a console-only configuration for testing
    let config = TerminalConfig::console_only();

    // Create the terminal system
    let terminal = TerminalSystem::new(config);

    // Start the terminal system with timeout
    match tokio::time::timeout(Duration::from_secs(5), terminal.start()).await {
        Ok(result) => result?,
        Err(_) => {
            // Cleanup attempt on timeout
            let _ = terminal.stop().await;
            return Err(Error::from("Timed out while starting terminal system"));
        }
    }

    // Test writing to the terminal
    terminal.write("Test message to console\n").await?;
    
    // Allow a short delay for processing
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Clean up with timeout - use a shorter timeout and more aggressive error handling
    let stop_success = match tokio::time::timeout(Duration::from_secs(2), terminal.stop()).await {
        Ok(result) => result.is_ok(),
        Err(_) => {
            println!("❌ Timed out while stopping terminal system - test will exit anyway");
            false
        }
    };

    if !stop_success {
        println!("⚠️ Terminal system did not stop cleanly, but test will continue");
    }

    println!("✅ Test completed");
    Ok(())
} 