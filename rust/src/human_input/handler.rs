use anyhow::{Result, anyhow};
use async_trait::async_trait;
use colored::Colorize;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, error, info};

use std::io::{self, Write};
use std::sync::Arc;

use crate::human_input::types::{HumanInputHandler, HumanInputRequest, HumanInputResponse};

/// A console-based human input handler that displays prompts and collects input via the terminal
#[derive(Debug, Clone)]
pub struct ConsoleInputHandler {
    /// Whether to use colored outpu
    colored_output: bool,

    /// Access mutex to coordinate multiple requests
    access_mutex: Arc<Mutex<()>>,
}

impl Default for ConsoleInputHandler {
    fn default() -> Self {
        Self {
            colored_output: true,
            access_mutex: Arc::new(Mutex::new(())),
        }
    }
}

impl ConsoleInputHandler {
    /// Create a new console input handler
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new console input handler with colored output disabled
    pub fn without_color() -> Self {
        Self {
            colored_output: false,
            access_mutex: Arc::new(Mutex::new(())),
        }
    }

    /// Format a prompt with optional description for display
    fn format_prompt(&self, request: &HumanInputRequest) -> String {
        let mut prompt_text = String::new();

        // Add header
        if self.colored_output {
            prompt_text.push_str(&format!("{}\n", "HUMAN INPUT NEEDED".bold().blue()));
        } else {
            prompt_text.push_str("HUMAN INPUT NEEDED\n");
        }

        // Add separator line
        prompt_text.push_str(&format!("{}\n", "─".repeat(50)));

        // Add description if presen
        if let Some(description) = &request.description {
            if self.colored_output {
                prompt_text.push_str(&format!("{}\n\n", description.bold()));
            } else {
                prompt_text.push_str(&format!("{}:\n\n", description));
            }
        }

        // Add the promp
        prompt_text.push_str(&request.prompt);

        // Add timeout information if presen
        if let Some(timeout_seconds) = request.timeout_seconds {
            if self.colored_output {
                prompt_text.push_str(&format!(
                    "\n\n{} {}",
                    "Timeout:".yellow(),
                    format!("{} seconds", timeout_seconds).yellow()
                ));
            } else {
                prompt_text.push_str(&format!("\n\nTimeout: {} seconds", timeout_seconds));
            }
        }

        prompt_text
    }

    /// Collect input from the user via stdin
    async fn collect_input(&self, request: &HumanInputRequest) -> Result<String> {
        // Use a mutex to ensure only one input request is active at a time
        let _lock = self.access_mutex.lock().await;

        // Print the formatted promp
        let prompt = self.format_prompt(request);
        println!("\n{}\n", prompt);

        // Print input indicator
        if self.colored_output {
            print!("{} ", ">".green().bold());
        } else {
            print!("> ");
        }
        io::stdout().flush()?;

        // Apply timeout if specified
        if let Some(timeout_duration) = request.timeout() {
            debug!("Input request with timeout of {:?}", timeout_duration);

            match timeout(timeout_duration, self.read_line()).await {
                Ok(result) => result,
                Err(_) => {
                    if self.colored_output {
                        println!("\n{}", "Timeout waiting for input".red().bold());
                    } else {
                        println!("\nTimeout waiting for input");
                    }
                    Err(anyhow!("No response received within timeout period"))
                }
            }
        } else {
            self.read_line().await
        }
    }

    /// Read a line from stdin
    async fn read_line(&self) -> Result<String> {
        // Use tokio's blocking API to read input without blocking the runtime
        let mut input = String::new();

        tokio::task::spawn_blocking(move || match io::stdin().read_line(&mut input) {
            Ok(_) => Ok(input),
            Err(e) => Err(anyhow!("Failed to read input: {}", e)),
        })
        .await?
        .map(|s| s.trim().to_string())
    }
}

#[async_trait]
impl HumanInputHandler for ConsoleInputHandler {
    async fn handle_request(&self, request: HumanInputRequest) -> Result<HumanInputResponse> {
        info!(
            "Processing human input request: id={}, prompt={}",
            request.request_id, request.prompt
        );

        match self.collect_input(&request).await {
            Ok(response) => {
                debug!("Received human input response: {}", response);
                Ok(HumanInputResponse::new(request.request_id, response))
            }
            Err(e) => {
                error!("Error collecting human input: {}", e);
                Err(e)
            }
        }
    }
}
