//! Performance tests for the terminal system
//!
//! These tests verify the performance characteristics of the terminal system
//! under various load conditions.

use futures::future::join_all;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::sleep;

use mcp_agent::error::Error;
use mcp_agent::terminal::{TerminalSystem, config::TerminalConfig};

/// Test throughput of the terminal system with console output
#[tokio::test]
async fn test_console_throughput() -> Result<(), Error> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO) // Use INFO to reduce logging overhead
        .try_init();

    // Create a console-only configuration for testing
    let config = TerminalConfig::console_only();

    // Create the terminal system
    let mut terminal = TerminalSystem::new(config)?;

    // Start the terminal system
    terminal.start().await?;

    // Number of messages to send
    const NUM_MESSAGES: usize = 1000;
    const MESSAGE_SIZE: usize = 100;

    // Create a test message
    let test_message = "X".repeat(MESSAGE_SIZE);

    // Measure time to send messages
    let start = Instant::now();

    for i in 0..NUM_MESSAGES {
        let message = format!("Message {}: {}\n", i, test_message);
        terminal.write(&message).await?;
    }

    let elapsed = start.elapsed();
    let throughput = NUM_MESSAGES as f64 / elapsed.as_secs_f64();

    println!("Console throughput: {:.2} messages/sec", throughput);
    println!("Total time: {:.2?} for {} messages", elapsed, NUM_MESSAGES);
    println!("Average message size: {} bytes", MESSAGE_SIZE);

    // Clean up
    terminal.stop().await?;

    Ok(())
}

/// Test concurrent write operations to the terminal
#[tokio::test]
async fn test_concurrent_writes() -> Result<(), Error> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO) // Use INFO to reduce logging overhead
        .try_init();

    // Create a console-only configuration for testing
    let config = TerminalConfig::console_only();

    // Create the terminal system
    let mut terminal = TerminalSystem::new(config)?;

    // Start the terminal system
    terminal.start().await?;

    // Number of concurrent tasks
    const NUM_TASKS: usize = 10;
    const MESSAGES_PER_TASK: usize = 100;

    // Create a shared terminal reference
    let terminal_ref = terminal.clone();

    // Measure time for concurrent writes
    let start = Instant::now();

    // Create multiple tasks that write to the terminal
    let mut tasks = Vec::new();
    for task_id in 0..NUM_TASKS {
        let terminal_clone = terminal_ref.clone();

        tasks.push(tokio::spawn(async move {
            for msg_id in 0..MESSAGES_PER_TASK {
                let message = format!("Task {} - Message {}\n", task_id, msg_id);
                if let Err(e) = terminal_clone.write(&message).await {
                    eprintln!("Error writing message: {}", e);
                }

                // Small delay to simulate processing
                sleep(Duration::from_micros(10)).await;
            }
        }));
    }

    // Wait for all tasks to complete
    join_all(tasks).await;

    let elapsed = start.elapsed();
    let total_messages = NUM_TASKS * MESSAGES_PER_TASK;
    let throughput = total_messages as f64 / elapsed.as_secs_f64();

    println!(
        "Concurrent write throughput: {:.2} messages/sec",
        throughput
    );
    println!(
        "Total time: {:.2?} for {} messages from {} tasks",
        elapsed, total_messages, NUM_TASKS
    );

    // Clean up
    terminal.stop().await?;

    Ok(())
}

/// Test write and read operations with simulated processing
#[tokio::test]
async fn test_write_read_cycle() -> Result<(), Error> {
    // Initialize logging
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    // Create a console-only configuration for testing
    let config = TerminalConfig::console_only();

    // Create the terminal system
    let mut terminal = TerminalSystem::new(config)?;

    // Start the terminal system
    terminal.start().await?;

    // Create a channel to simulate input
    let (input_tx, mut input_rx) = mpsc::channel::<String>(100);

    // Spawn a task to send simulated input
    tokio::spawn(async move {
        // Allow time for terminal system to start
        sleep(Duration::from_millis(100)).await;

        // Send a series of inputs
        for i in 1..=5 {
            let input = format!("Simulated input {}", i);
            if let Err(e) = input_tx.send(input).await {
                eprintln!("Error sending simulated input: {}", e);
                break;
            }

            // Wait between inputs
            sleep(Duration::from_millis(50)).await;
        }
    });

    // Main processing loop
    let start_time = Instant::now();
    let mut message_count = 0;

    // Run for a short time or until we receive no more input
    while let Some(input) = input_rx.recv().await {
        // Write the received input to the terminal
        terminal.write(&format!("Received: {}\n", input)).await?;
        message_count += 1;

        // Process the input
        let response = format!("Processed: {}\n", input);
        terminal.write(&response).await?;
        message_count += 1;

        // Check if we've been running long enough
        if start_time.elapsed() > Duration::from_secs(5) {
            break;
        }
    }

    let elapsed = start_time.elapsed();
    println!("Processed {} messages in {:.2?}", message_count, elapsed);
    println!(
        "Average processing time: {:.2?} per message",
        elapsed / message_count as u32
    );

    // Clean up
    terminal.stop().await?;

    Ok(())
}
