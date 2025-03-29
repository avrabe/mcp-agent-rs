use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;

use super::template::PromptTemplate;
use super::types::{PromptParameters, RenderedPrompt};
use crate::utils::error::{McpError, McpResult};

/// Manager for prompt templates
#[derive(Debug, Clone)]
pub struct PromptManager {
    /// Templates indexed by ID
    templates: Arc<RwLock<HashMap<String, PromptTemplate>>>,
}

impl PromptManager {
    /// Create a new prompt manager
    pub fn new() -> Self {
        Self {
            templates: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a template with the manager
    pub async fn register_template(&self, template: PromptTemplate) -> McpResult<()> {
        let id = template.id.0.clone();
        let mut templates = self.templates.write().await;
        templates.insert(id, template);
        Ok(())
    }

    /// Get a template by ID
    pub async fn get_template(&self, id: &str) -> McpResult<PromptTemplate> {
        let templates = self.templates.read().await;
        templates
            .get(id)
            .cloned()
            .ok_or_else(|| McpError::NotFound(format!("Template not found: {}", id)))
    }

    /// Remove a template by ID
    pub async fn remove_template(&self, id: &str) -> McpResult<()> {
        let mut templates = self.templates.write().await;
        if templates.remove(id).is_none() {
            return Err(McpError::NotFound(format!("Template not found: {}", id)));
        }
        Ok(())
    }

    /// List all template IDs
    pub async fn list_templates(&self) -> Vec<String> {
        let templates = self.templates.read().await;
        templates.keys().cloned().collect()
    }

    /// Render a prompt using a template
    pub async fn render_prompt(
        &self,
        id: &str,
        parameters: &PromptParameters,
    ) -> McpResult<RenderedPrompt> {
        let template = self.get_template(id).await?;
        template.render(parameters)
    }

    /// Load templates from a directory
    pub async fn load_templates_from_directory(&self, dir_path: &str) -> McpResult<usize> {
        let path = Path::new(dir_path);
        if !path.exists() {
            return Err(McpError::IoError(format!(
                "Templates directory does not exist: {}",
                dir_path
            )));
        }

        let mut count = 0;
        let mut entries = fs::read_dir(path)
            .await
            .map_err(|e| McpError::IoError(format!("Failed to read templates directory: {}", e)))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| McpError::IoError(format!("Failed to read directory entry: {}", e)))?
        {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let content = fs::read_to_string(&path).await.map_err(|e| {
                    McpError::IoError(format!("Failed to read template file: {}", e))
                })?;

                let template: PromptTemplate = serde_json::from_str(&content).map_err(|e| {
                    McpError::DeserializationError(format!("Failed to parse template: {}", e))
                })?;

                self.register_template(template).await?;
                count += 1;
            }
        }

        Ok(count)
    }

    /// Save templates to a directory
    pub async fn save_templates_to_directory(&self, dir_path: &str) -> McpResult<usize> {
        let path = Path::new(dir_path);
        if !path.exists() {
            fs::create_dir_all(path).await.map_err(|e| {
                McpError::IoError(format!("Failed to create templates directory: {}", e))
            })?;
        }

        let templates = self.templates.read().await;
        let mut count = 0;

        for (id, template) in templates.iter() {
            let file_path = path.join(format!("{}.json", id));
            let content = serde_json::to_string_pretty(template).map_err(|e| {
                McpError::SerializationError(format!("Failed to serialize template: {}", e))
            })?;

            fs::write(&file_path, content)
                .await
                .map_err(|e| McpError::IoError(format!("Failed to write template file: {}", e)))?;

            count += 1;
        }

        Ok(count)
    }

    /// Register default templates
    pub async fn register_defaults(&self) -> McpResult<()> {
        // This would normally load defaults from embedded files or a config
        // For now, just create a few basic templates

        let code_analysis_template = PromptTemplate::with_details(
            "code_analysis",
            "Code Analysis",
            "Analyze code for errors, optimizations, and best practices",
            "Analyze the following {{language}} code:\n\n```{{language}}\n{{code}}\n```\n\nFocus on: {{focus}}",
            vec![
                super::types::PromptParameter::required("language", "Programming language"),
                super::types::PromptParameter::required("code", "Code to analyze"),
                super::types::PromptParameter::optional("focus", "Analysis focus", "errors,performance,security"),
            ],
            "1.0",
            "code",
        );

        let commit_message_template = PromptTemplate::with_details(
            "commit_message",
            "Generate Commit Message",
            "Generate a descriptive commit message for code changes",
            "Generate a commit message for the following changes:\n\n```diff\n{{diff}}\n```",
            vec![super::types::PromptParameter::required(
                "diff",
                "Git diff of changes",
            )],
            "1.0",
            "git",
        );

        // Register templates directly without the tokio::task::block_in_place
        self.register_template(code_analysis_template).await?;
        self.register_template(commit_message_template).await?;

        Ok(())
    }
}

impl Default for PromptManager {
    fn default() -> Self {
        Self::new()
    }
}
