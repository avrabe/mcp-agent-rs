#![cfg(feature = "ollama")]

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use tokio::time::Duration;
use tracing::{debug, error, instrument};

use super::types::{Completion, CompletionRequest, LlmClient, LlmConfig, Message};
use crate::add_metric;

/// Client for the Ollama API
#[derive(Debug, Clone)]
pub struct OllamaClient {
    /// Base URL for the Ollama API
    base_url: String,
    /// HTTP client
    client: Client,
    /// Model to use
    model: String,
    /// System prompt
    system_prompt: Option<String>,
    /// Request timeout
    timeout: Duration,
}

/// Request to the Ollama API
#[derive(Debug, Serialize)]
struct OllamaRequest {
    /// Model to use
    model: String,

    /// Messages in the conversation
    messages: Vec<OllamaMessage>,

    /// Parameters for the request
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
}

/// Message format for Ollama
#[derive(Debug, Serialize)]
struct OllamaMessage {
    /// Role of the message sender
    role: String,

    /// Content of the message
    content: String,
}

/// Options for the Ollama request
#[derive(Debug, Serialize)]
struct OllamaOptions {
    /// Temperature for sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,

    /// Top-p sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,

    /// Number of tokens to predict
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
}

/// Response from Ollama API
#[derive(Debug, Clone, Deserialize)]
struct OllamaResponse {
    /// The generated text
    response: String,
    /// Token context
    context: Vec<i32>,
    /// The model used
    model: String,
    /// Created timestamp
    created_at: String,
    /// Whether the response is finished
    #[allow(dead_code)]
    done: bool,
}

/// Message in Ollama API request/response
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaResponseMessage {
    /// Role of the message sender
    #[allow(dead_code)]
    role: String,
    /// Content of the message
    content: String,
}

impl OllamaClient {
    /// Create a new Ollama client
    ///
    /// # Panics
    ///
    /// Panics if the HTTP client cannot be created
    pub fn new(config: LlmConfig) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            base_url: config.api_url.clone(),
            client,
            model: config.model.clone(),
            system_prompt: config.api_key.clone(), // Using api_key as system_prompt
            timeout: Duration::from_secs(120),
        }
    }

    /// Create a new Ollama client with default configuration
    pub fn new_with_defaults() -> Self {
        Self::new(LlmConfig::default())
    }

    /// Convert messages to Ollama format
    fn convert_messages(&self, messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .map(|msg| OllamaMessage {
                role: msg.role.to_string(),
                content: msg.content.clone(),
            })
            .collect()
    }

    /// Create Ollama options from a completion request
    fn create_options(&self, request: &CompletionRequest) -> Option<OllamaOptions> {
        if request.temperature.is_none() && request.top_p.is_none() && request.max_tokens.is_none()
        {
            return None;
        }

        Some(OllamaOptions {
            temperature: request.temperature,
            top_p: request.top_p,
            num_predict: request.max_tokens,
        })
    }
}

#[async_trait]
impl LlmClient for OllamaClient {
    #[instrument(skip(self, request), fields(model = %request.model))]
    async fn complete(&self, request: CompletionRequest) -> Result<Completion> {
        let start = std::time::Instant::now();

        let api_url = format!("{}/api/chat", self.base_url);
        debug!("Sending request to Ollama API: {}", api_url);

        let ollama_request = OllamaRequest {
            model: request.model.clone(),
            messages: self.convert_messages(&request.messages),
            options: self.create_options(&request),
        };

        let response = self
            .client
            .post(&api_url)
            .json(&ollama_request)
            .send()
            .await?;

        let status = response.status();
        if status != StatusCode::OK {
            let error_text = response.text().await?;
            error!("Ollama API error ({}): {}", status, error_text);
            return Err(anyhow!("Ollama API error ({}): {}", status, error_text));
        }

        let ollama_response: OllamaResponse = response.json().await?;
        let duration = start.elapsed();

        // Calculate token metrics
        let prompt_tokens = ollama_response.context.len() as u32;
        let completion_tokens = ollama_response.context.len() as u32;
        let total_tokens = prompt_tokens + completion_tokens;

        // Add metrics
        add_metric(
            "llm_request_duration_ms",
            duration.as_millis() as f64,
            &[
                ("model", request.model.clone()),
                ("provider", "ollama".to_string()),
            ],
        );

        add_metric(
            "llm_tokens_total",
            total_tokens as f64,
            &[
                ("model", request.model.clone()),
                ("provider", "ollama".to_string()),
            ],
        );

        // Build completion response
        let completion = Completion {
            content: ollama_response.response,
            model: Some(ollama_response.model),
            prompt_tokens: Some(prompt_tokens),
            completion_tokens: Some(completion_tokens),
            total_tokens: Some(total_tokens),
            metadata: Some(HashMap::from([(
                "total_duration_ns".to_string(),
                json!(duration.as_nanos()),
            )])),
        };

        Ok(completion)
    }

    async fn is_available(&self) -> Result<bool> {
        let api_url = format!("{}/api/tags", self.base_url);
        debug!("Checking Ollama availability: {}", api_url);

        match self.client.get(&api_url).send().await {
            Ok(response) => Ok(response.status().is_success()),
            Err(e) => {
                debug!("Ollama not available: {}", e);
                Ok(false)
            }
        }
    }

    fn config(&self) -> &LlmConfig {
        // Create a static config to return a reference to
        static mut CONFIG: Option<LlmConfig> = None;

        unsafe {
            if CONFIG.is_none() {
                CONFIG = Some(LlmConfig {
                    model: self.model.clone(),
                    api_url: self.base_url.clone(),
                    api_key: Some(self.system_prompt.clone().unwrap_or_default()),
                    max_tokens: None,
                    temperature: None,
                    top_p: None,
                    parameters: HashMap::new(),
                });
            }
            CONFIG.as_ref().unwrap()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Only run when Ollama is available
    async fn test_ollama_basic_completion() {
        let client = OllamaClient::new_with_defaults();

        // Skip test if Ollama is not available
        if !client.is_available().await.unwrap_or(false) {
            return;
        }

        let request = CompletionRequest {
            model: "mistral".to_string(),
            messages: vec![
                Message::system("You are a helpful assistant."),
                Message::user("Hello, how are you?"),
            ],
            max_tokens: Some(100),
            temperature: Some(0.7),
            top_p: None,
            parameters: HashMap::new(),
        };

        let completion = client.complete(request).await.unwrap();

        assert!(!completion.content.is_empty());
        assert_eq!(completion.model, Some("mistral".to_string()));
        assert!(completion.prompt_tokens.unwrap() > 0);
        assert!(completion.completion_tokens.unwrap() > 0);
        assert!(completion.total_tokens.unwrap() > 0);
    }
}
