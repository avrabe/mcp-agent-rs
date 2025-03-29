use mcp_agent::mcp::prompts::{PromptManager, PromptParameter, PromptParameters, PromptTemplate};
use mcp_agent::utils::error::McpResult;

#[tokio::main]
async fn main() -> McpResult<()> {
    println!("Initializing MCP Prompts Example...");

    // Initialize the prompt manager
    let prompt_manager = PromptManager::new();

    println!("Creating templates...");

    // Create a code analysis template
    let code_analysis = PromptTemplate::with_details(
        "code_analysis",
        "Code Analysis",
        "Analyze code for errors, optimizations, and best practices",
        "Analyze the following {{language}} code:\n\n```{{language}}\n{{code}}\n```\n\nFocus on: {{focus}}",
        vec![
            PromptParameter::required("language", "Programming language"),
            PromptParameter::required("code", "Code to analyze"),
            PromptParameter::optional("focus", "Analysis focus", "errors,performance,security"),
        ],
        "1.0",
        "code",
    );

    println!("Registering code analysis template...");

    // Register the template
    prompt_manager.register_template(code_analysis).await?;

    println!("Creating parameters...");

    // Create parameters for the template
    let mut params = PromptParameters::new();
    params.add("language", "rust");
    params.add("code", "fn main() {\n    println!(\"Hello, world!\");\n}");
    params.add("focus", "performance");

    println!("Rendering prompt...");

    // Render the prompt
    let rendered = prompt_manager
        .render_prompt("code_analysis", &params)
        .await?;

    println!("\nRendered prompt:");
    println!("{}", rendered.text);
    println!("\nTemplate ID: {}", rendered.id.0);
    println!("Version: {}", rendered.version.0);

    println!("\nCreating commit message template...");

    // Create a commit message template
    let commit_message = PromptTemplate::with_details(
        "commit_message",
        "Generate Commit Message",
        "Generate a descriptive commit message for code changes",
        "Generate a commit message for the following changes:\n\n```diff\n{{diff}}\n```",
        vec![PromptParameter::required("diff", "Git diff of changes")],
        "1.0",
        "git",
    );

    println!("Registering commit message template...");

    // Register the template
    prompt_manager.register_template(commit_message).await?;

    println!("Listing templates...");

    // List all templates
    let templates = prompt_manager.list_templates().await;
    println!("\nAvailable templates:");
    for template in templates {
        println!("- {}", template);
    }

    println!("\nPrompts example completed successfully!");

    Ok(())
}
