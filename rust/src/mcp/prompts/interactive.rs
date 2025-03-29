use serde::{Deserialize, Serialize};
use std::fmt;

use super::types::{PromptId, PromptParameters};

/// A slash command that can be used to trigger a prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlashCommand {
    /// The unique identifier of the command
    pub id: String,
    /// The slash command text (without the /)
    pub command: String,
    /// A brief description of what the command does
    pub description: String,
    /// The prompt ID to execute
    pub prompt_id: PromptId,
    /// Optional default parameters for the prompt
    pub default_parameters: Option<PromptParameters>,
}

impl SlashCommand {
    /// Create a new slash command
    pub fn new(id: &str, command: &str, description: &str, prompt_id: &str) -> Self {
        Self {
            id: id.to_string(),
            command: command.to_string(),
            description: description.to_string(),
            prompt_id: PromptId::new(prompt_id),
            default_parameters: None,
        }
    }

    /// Set default parameters for the slash command
    pub fn with_default_parameters(mut self, parameters: PromptParameters) -> Self {
        self.default_parameters = Some(parameters);
        self
    }

    /// Check if a text matches this slash command
    pub fn matches(&self, text: &str) -> bool {
        let text = text.trim();

        // Check if starts with slash and matches command
        if text.starts_with('/') && text[1..].starts_with(&self.command) {
            // Check if it's an exact match or followed by space/end
            let command_end = 1 + self.command.len();
            return text.len() == command_end || text.as_bytes().get(command_end) == Some(&b' ');
        }

        false
    }

    /// Extract arguments from slash command text
    pub fn extract_args(&self, text: &str) -> Option<String> {
        let text = text.trim();

        if !self.matches(text) {
            return None;
        }

        let command_part = format!("/{}", self.command);
        let args_part = text[command_part.len()..].trim();

        if args_part.is_empty() {
            None
        } else {
            Some(args_part.to_string())
        }
    }
}

impl fmt::Display for SlashCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "/{} - {}", self.command, self.description)
    }
}

/// A menu option that can be used to trigger a prompt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuOption {
    /// The unique identifier of the option
    pub id: String,
    /// The display text for the menu option
    pub text: String,
    /// A brief description of what the option does
    pub description: String,
    /// The prompt ID to execute
    pub prompt_id: PromptId,
    /// Optional default parameters for the prompt
    pub default_parameters: Option<PromptParameters>,
    /// Icon or emoji for the menu option
    pub icon: Option<String>,
    /// Group or category for the menu option
    pub group: Option<String>,
}

impl MenuOption {
    /// Create a new menu option
    pub fn new(id: &str, text: &str, description: &str, prompt_id: &str) -> Self {
        Self {
            id: id.to_string(),
            text: text.to_string(),
            description: description.to_string(),
            prompt_id: PromptId::new(prompt_id),
            default_parameters: None,
            icon: None,
            group: None,
        }
    }

    /// Set default parameters for the menu option
    pub fn with_default_parameters(mut self, parameters: PromptParameters) -> Self {
        self.default_parameters = Some(parameters);
        self
    }

    /// Set an icon for the menu option
    pub fn with_icon(mut self, icon: &str) -> Self {
        self.icon = Some(icon.to_string());
        self
    }

    /// Set a group for the menu option
    pub fn with_group(mut self, group: &str) -> Self {
        self.group = Some(group.to_string());
        self
    }
}

/// A menu group that contains multiple menu options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuGroup {
    /// The unique identifier of the group
    pub id: String,
    /// The display text for the menu group
    pub text: String,
    /// Menu options in this group
    pub options: Vec<MenuOption>,
}

impl MenuGroup {
    /// Create a new menu group
    pub fn new(id: &str, text: &str) -> Self {
        Self {
            id: id.to_string(),
            text: text.to_string(),
            options: Vec::new(),
        }
    }

    /// Add an option to the menu group
    pub fn add_option(&mut self, option: MenuOption) -> &mut Self {
        self.options.push(option);
        self
    }
}
