use anyhow::Result;
use std::collections::HashMap;

use mcp_agent::telemetry::{init_telemetry, TelemetryConfig};

// Import orchestrator code with public visibility
mod orchestrator {
    // Re-export the MockLlmClient and OrchestratorWorkflow with public visibility
    use anyhow::Result;
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Duration;

    use mcp_agent::llm::types::LlmClient;

    use mcp_agent::workflow::{WorkflowEngine, WorkflowState};
    use mcp_agent::{Completion, CompletionRequest, LlmConfig};

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub enum SubWorkflowType {
        DataPreparation,
        Analysis,
        Summarization,
    }

    pub struct SubWorkflow {
        pub workflow_type: SubWorkflowType,
        pub state: WorkflowState,
        pub result: Option<String>,
        pub dependencies: Vec<SubWorkflowType>,
    }

    impl SubWorkflow {
        pub fn new(workflow_type: SubWorkflowType, dependencies: Vec<SubWorkflowType>) -> Self {
            let mut metadata = HashMap::new();
            let type_str = format!("{:?}", workflow_type);
            metadata.insert("workflow_type".to_string(), json!(type_str));

            Self {
                workflow_type,
                state: WorkflowState::new(
                    Some(format!("SubWorkflow_{}", type_str)),
                    Some(metadata),
                ),
                result: None,
                dependencies,
            }
        }
    }

    pub struct OrchestratorWorkflow {
        state: WorkflowState,
        engine: WorkflowEngine,
        llm_client: Arc<MockLlmClient>,
        input_data: String,
        sub_workflows: HashMap<SubWorkflowType, SubWorkflow>,
        final_result: Option<String>,
    }

    impl OrchestratorWorkflow {
        pub fn new(
            engine: WorkflowEngine,
            llm_client: Arc<MockLlmClient>,
            input_data: String,
        ) -> Self {
            let mut metadata = HashMap::new();
            metadata.insert("input_size".to_string(), json!(input_data.len()));

            // Create the sub-workflows with their dependencies
            let mut sub_workflows = HashMap::new();

            sub_workflows.insert(
                SubWorkflowType::DataPreparation,
                SubWorkflow::new(SubWorkflowType::DataPreparation, vec![]),
            );

            sub_workflows.insert(
                SubWorkflowType::Analysis,
                SubWorkflow::new(
                    SubWorkflowType::Analysis,
                    vec![SubWorkflowType::DataPreparation],
                ),
            );

            sub_workflows.insert(
                SubWorkflowType::Summarization,
                SubWorkflow::new(
                    SubWorkflowType::Summarization,
                    vec![SubWorkflowType::Analysis],
                ),
            );

            Self {
                state: WorkflowState::new(Some("OrchestratorWorkflow".to_string()), Some(metadata)),
                engine,
                llm_client,
                input_data,
                sub_workflows,
                final_result: None,
            }
        }

        // Rest of implementation included via include_str!
    }

    // Skip including the file to avoid formatting issues
    // include!("orchestrator_workflow/main.rs");
    // We'll implement the necessary functions directly

    // Replace with public LLM client implementation with correct fields
    pub struct MockLlmClient {
        config: LlmConfig,
    }

    impl MockLlmClient {
        pub fn new() -> Self {
            Self {
                config: LlmConfig {
                    model: "llama2".to_string(),
                    api_url: "http://localhost:11434".to_string(),
                    api_key: None,
                    max_tokens: None,
                    temperature: Some(0.7),
                    top_p: Some(0.9),
                    parameters: HashMap::new(),
                },
            }
        }
    }

    #[async_trait]
    impl LlmClient for MockLlmClient {
        async fn complete(&self, _request: CompletionRequest) -> Result<Completion> {
            // Simulate some processing time
            tokio::time::sleep(Duration::from_millis(300)).await;

            // Return a mock response
            Ok(Completion {
                content: "This is a mock response from the LLM service.".to_string(),
                model: Some(self.config.model.clone()),
                prompt_tokens: Some(10),
                completion_tokens: Some(15),
                total_tokens: Some(25),
                metadata: Some(HashMap::new()),
            })
        }

        async fn is_available(&self) -> Result<bool> {
            Ok(true)
        }

        fn config(&self) -> &LlmConfig {
            &self.config
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize telemetry with correct config
    let telemetry_config = TelemetryConfig {
        service_name: "workflow-visualizer".to_string(),
        otlp_endpoint: None,
        jaeger_endpoint: None,
        enable_console: true,
        enable_json: false,
        enable_tracing: true,
        enable_metrics: true,
        sampling_ratio: 1.0,
        attributes: HashMap::new(),
        alerting_config: None,
    };
    if let Err(e) = init_telemetry(telemetry_config) {
        eprintln!("Failed to initialize telemetry: {}", e);
        return Ok(());
    }

    // Import terminal components
    #[cfg(feature = "terminal-web")]
    {
        use mcp_agent::terminal::config::AuthConfig;
        use mcp_agent::terminal::{initialize_visualization, TerminalConfig, TerminalSystem};

        // Create the terminal system with web visualization enabled
        let terminal_config = TerminalConfig {
            console_enabled: true,
            web_terminal_enabled: true,
            web_terminal_host: "127.0.0.1".to_string(),
            web_terminal_port: 8080,
            auth_config: AuthConfig::default(),
        };
        let terminal_system = TerminalSystem::new(terminal_config);

        // Start the terminal system
        terminal_system.start().await?;

        // Create the workflow engine
        let engine = WorkflowEngine::new();

        // Create the mock LLM client
        let llm_client = Arc::new(orchestrator::MockLlmClient::new());

        // Sample input data
        let input_data = "This is a sample document that needs to be processed through an orchestrated workflow.".to_string();

        // Create the workflow
        let workflow =
            orchestrator::OrchestratorWorkflow::new(engine.clone(), llm_client, input_data);

        // Initialize visualization
        let graph_manager =
            initialize_visualization(&terminal_system, Some(engine.clone()), vec![], vec![], None)
                .await;

        // Print instructions
        println!("\n{}", "Workflow Visualization Demo".bold().green());
        println!(
            "Open your browser at: {}",
            format!("http://{}", terminal_system.web_terminal_address().unwrap()).cyan()
        );
        println!(
            "{}",
            "Click on 'Show Visualization' to see the workflow graph".yellow()
        );
        println!("{}", "The workflow will start in 5 seconds...".cyan());

        // Give time to open the web interface
        tokio::time::sleep(Duration::from_secs(5)).await;

        // Execute the workflow
        match execute_workflow(workflow).await {
            Ok(result) => {
                println!("\n{}", "Workflow Result:".bold().green());
                println!("{:?}", result.output());
                println!(
                    "Workflow completed in {} ms",
                    result.execution_time().as_millis()
                );
            }
            Err(e) => {
                println!("\n{}", "Workflow Error:".bold().red());
                println!("{:?}", e);
            }
        }

        // Keep the terminal system running to allow viewing the visualization
        println!(
            "\n{}",
            "Visualization is available in the web terminal."
                .bold()
                .cyan()
        );
        println!("{}", "Press Ctrl+C to exit...".yellow());

        // Wait for Ctrl+C
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let tx_clone = tx.clone();

        ctrlc::set_handler(move || {
            let _ = tx_clone.send(());
        })?;

        let _ = rx.await;

        // Stop the terminal system
        terminal_system.stop().await?;
    }

    #[cfg(not(feature = "terminal-web"))]
    {
        println!("This example requires the 'terminal-web' feature to be enabled.");
        println!(
            "Please rebuild with: cargo build --example workflow_visualizer --features terminal-web"
        );
    }

    Ok(())
}
