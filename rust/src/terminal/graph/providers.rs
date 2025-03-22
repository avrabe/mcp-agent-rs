//! Graph Data Providers
//!
//! Provides data providers for various components that can be visualized as graphs:
//! - Workflow Engine
//! - Agent System
//! - Human Input Points
//! - LLM Integration

use crate::error::{Error, Result};
use crate::terminal::graph::{Graph, GraphManager};
use crate::workflow::engine::WorkflowEngine;
use crate::workflow::signal::NullSignalHandler;

use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

use super::{GraphEdge, GraphNode};
use crate::mcp::agent::Agent;
// Comment out these imports for now - these would be implemented based on LLM and human input systems
// use crate::llm::LlmProvider;
// use crate::human_input::HumanInputProvider;

/// Trait for providers that can generate graph data in a trait object-safe way
pub trait GraphDataProvider: Send + Sync + Debug {
    /// Generate a graph representation (non-async wrapper)
    fn generate_graph_boxed(
        &self,
    ) -> Box<dyn std::future::Future<Output = Result<Graph, Error>> + Send + Unpin>;

    /// Set up tracking for graph updates (non-async wrapper)
    fn setup_tracking_boxed(
        &self,
        graph_manager: Arc<GraphManager>,
    ) -> Box<dyn std::future::Future<Output = Result<(), Error>> + Send + Unpin>;
}

/// Trait for providers that can generate graph data
#[async_trait]
pub trait AsyncGraphDataProvider: Send + Sync + Debug {
    /// Generate a graph representation
    async fn generate_graph(&self) -> Result<Graph, Error>;

    /// Set up tracking for graph updates
    async fn setup_tracking(&self, graph_manager: Arc<GraphManager>) -> Result<(), Error>;
}

/// Helper extension trait
pub trait GraphDataProviderExt: AsyncGraphDataProvider {
    fn as_graph_data_provider(&self) -> &dyn GraphDataProvider;
}

impl<T: AsyncGraphDataProvider + 'static> GraphDataProvider for T {
    fn generate_graph_boxed(
        &self,
    ) -> Box<dyn std::future::Future<Output = Result<Graph, Error>> + Send + Unpin> {
        Box::pin(self.generate_graph())
    }

    fn setup_tracking_boxed(
        &self,
        graph_manager: Arc<GraphManager>,
    ) -> Box<dyn std::future::Future<Output = Result<(), Error>> + Send + Unpin> {
        Box::pin(self.setup_tracking(graph_manager))
    }
}

impl<T: AsyncGraphDataProvider + 'static> GraphDataProviderExt for T {
    fn as_graph_data_provider(&self) -> &dyn GraphDataProvider {
        self
    }
}

/// Helper functions for working with trait objects
pub mod graph_provider_helpers {
    use super::*;

    /// Generate a graph using the given provider
    pub async fn generate_graph(
        provider: &dyn GraphDataProvider,
    ) -> Result<Graph, crate::error::Error> {
        provider.generate_graph_boxed().await
    }

    /// Set up tracking for the given provider
    pub async fn setup_tracking(
        provider: &dyn GraphDataProvider,
        graph_manager: Arc<GraphManager>,
    ) -> Result<(), crate::error::Error> {
        provider.setup_tracking_boxed(graph_manager).await
    }
}

/// Workflow graph provider
#[derive(Debug)]
pub struct WorkflowGraphProvider {
    /// Name of the provider
    name: String,
    /// Workflow engine to visualize
    workflow_engine: Arc<WorkflowEngine>,
    /// Current workflow state cache for tracking changes
    workflow_state_cache: RwLock<Option<WorkflowState>>,
    /// Graph representation
    graph: RwLock<Option<Arc<RwLock<Graph>>>>,
    /// Graph manager
    graph_manager: RwLock<Option<Arc<GraphManager>>>,
}

impl WorkflowGraphProvider {
    /// Create a new workflow graph provider with the given engine
    pub fn new(workflow_engine: Arc<WorkflowEngine>) -> Self {
        Self {
            name: "Workflow Graph Provider".to_string(),
            workflow_engine,
            workflow_state_cache: RwLock::new(None),
            graph: RwLock::new(None),
            graph_manager: RwLock::new(None),
        }
    }

    /// Create a graph from the current workflow state
    async fn create_graph_from_workflow(&self) -> Result<Graph, Error> {
        let workflow_state = self.workflow_engine.state().await?;
        let mut graph = Graph {
            id: "workflow-graph".to_string(),
            name: "Workflow Graph".to_string(),
            graph_type: "workflow".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            properties: HashMap::new(),
        };

        // Store workflow state in cache for change detection
        {
            let mut cache = self.workflow_state_cache.write().await;
            *cache = Some(workflow_state.clone());
        }

        // Create nodes for each task
        for task in &workflow_state.tasks {
            let mut properties = HashMap::new();
            properties.insert(
                "task_type".to_string(),
                serde_json::to_value(&task.task_type).unwrap_or_default(),
            );

            let node = GraphNode {
                id: task.id.clone(),
                name: task.name.clone(),
                node_type: "task".to_string(),
                status: task.status.to_string(),
                properties,
            };

            graph.nodes.push(node);
        }

        // Create edges for task dependencies
        for task in &workflow_state.tasks {
            for dep_id in &task.dependencies {
                let edge = GraphEdge {
                    id: format!("{}_{}", dep_id, task.id),
                    source: dep_id.clone(),
                    target: task.id.clone(),
                    edge_type: "dependency".to_string(),
                    properties: HashMap::new(),
                };

                graph.edges.push(edge);
            }
        }

        Ok(graph)
    }

    /// Update the graph with current workflow state
    pub async fn update_graph_from_workflow(&self, graph: &mut Graph) -> Result<(), Error> {
        // In a real implementation, we would get the workflow state and add nodes/edges
        // For now, just add some placeholder nodes

        // Add a workflow node
        let workflow_node = GraphNode {
            id: "workflow-main".to_string(),
            name: "Main Workflow".to_string(),
            node_type: "workflow".to_string(),
            status: "active".to_string(),
            properties: HashMap::new(),
        };

        // Add the workflow node
        graph.nodes.push(workflow_node);

        // Add some task nodes
        let task_nodes = vec![
            GraphNode {
                id: "task-1".to_string(),
                name: "Task 1".to_string(),
                node_type: "task".to_string(),
                status: "completed".to_string(),
                properties: HashMap::new(),
            },
            GraphNode {
                id: "task-2".to_string(),
                name: "Task 2".to_string(),
                node_type: "task".to_string(),
                status: "active".to_string(),
                properties: HashMap::new(),
            },
            GraphNode {
                id: "task-3".to_string(),
                name: "Task 3".to_string(),
                node_type: "task".to_string(),
                status: "pending".to_string(),
                properties: HashMap::new(),
            },
        ];

        // Add the task nodes
        for node in task_nodes {
            graph.nodes.push(node);
        }

        // Add edges between workflow and tasks
        let edges = vec![
            GraphEdge {
                id: "edge-1".to_string(),
                source: "workflow-main".to_string(),
                target: "task-1".to_string(),
                edge_type: "contains".to_string(),
                properties: HashMap::new(),
            },
            GraphEdge {
                id: "edge-2".to_string(),
                source: "workflow-main".to_string(),
                target: "task-2".to_string(),
                edge_type: "contains".to_string(),
                properties: HashMap::new(),
            },
            GraphEdge {
                id: "edge-3".to_string(),
                source: "workflow-main".to_string(),
                target: "task-3".to_string(),
                edge_type: "contains".to_string(),
                properties: HashMap::new(),
            },
            GraphEdge {
                id: "edge-4".to_string(),
                source: "task-1".to_string(),
                target: "task-2".to_string(),
                edge_type: "dependency".to_string(),
                properties: HashMap::new(),
            },
            GraphEdge {
                id: "edge-5".to_string(),
                source: "task-2".to_string(),
                target: "task-3".to_string(),
                edge_type: "dependency".to_string(),
                properties: HashMap::new(),
            },
        ];

        // Add the edges
        for edge in edges {
            graph.edges.push(edge);
        }

        Ok(())
    }
}

#[async_trait]
impl AsyncGraphDataProvider for WorkflowGraphProvider {
    async fn generate_graph(&self) -> Result<Graph, Error> {
        self.create_graph_from_workflow().await
    }

    async fn setup_tracking(&self, graph_manager: Arc<GraphManager>) -> Result<(), Error> {
        // Store graph manager reference
        *self.graph_manager.write().await = Some(graph_manager.clone());
        Ok(())
    }
}

impl Clone for WorkflowGraphProvider {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            workflow_engine: self.workflow_engine.clone(),
            workflow_state_cache: RwLock::new(None),
            graph: RwLock::new(None),
            graph_manager: RwLock::new(None),
        }
    }
}

/// Agent graph provider
#[derive(Debug)]
pub struct AgentGraphProvider {
    /// Provider name
    pub name: String,
    /// Agents to visualize
    agents: RwLock<Vec<Arc<Agent>>>,
    graph: RwLock<Option<Arc<RwLock<Graph>>>>,
    graph_manager: RwLock<Option<Arc<GraphManager>>>,
}

impl AgentGraphProvider {
    /// Create a new agent graph provider
    pub fn new() -> Self {
        Self {
            name: "Agent Graph Provider".to_string(),
            agents: RwLock::new(Vec::new()),
            graph: RwLock::new(None),
            graph_manager: RwLock::new(None),
        }
    }

    /// Add an agent to the provider
    pub async fn add_agent(&self, agent: Arc<Agent>) {
        let mut agents = self.agents.write().await;
        agents.push(agent);
    }

    /// Update the graph with current agent data
    pub async fn update_graph_from_agents(&self, graph: &mut Graph) -> Result<(), Error> {
        let agents = self.agents.read().await;

        // For each agent, create a node
        for (i, agent) in agents.iter().enumerate() {
            let id = format!("agent-{}", i);
            let name = format!("Agent {}", i);

            // Create a node for the agen
            let node = GraphNode {
                id: id.clone(),
                name,
                node_type: "agent".to_string(),
                status: "connected".to_string(), // Default status
                properties: HashMap::new(),
            };

            // Add the node to the graph
            graph.nodes.push(node);
        }

        Ok(())
    }

    /// Create a graph from the current agents
    async fn create_graph_from_agents(&self) -> Result<Graph, Error> {
        let agents = self.agents.read().await;
        let mut graph = Graph {
            id: "agent-graph".to_string(),
            name: "Agent System".to_string(),
            graph_type: "agent".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            properties: HashMap::new(),
        };

        // Create nodes for each agen
        for (i, agent) in agents.iter().enumerate() {
            let mut properties = HashMap::new();
            properties.insert(
                "agent_type".to_string(),
                serde_json::to_value("agent").unwrap_or_default(),
            );

            let node = GraphNode {
                id: format!("agent-{}", i),
                name: format!("Agent {}", i),
                node_type: "agent".to_string(),
                status: "connected".to_string(), // Default status
                properties,
            };

            graph.nodes.push(node);
        }

        // TODO: Add edges for agent connections when that data is available

        Ok(graph)
    }
}

#[async_trait]
impl AsyncGraphDataProvider for AgentGraphProvider {
    async fn generate_graph(&self) -> Result<Graph, Error> {
        self.create_graph_from_agents().await
    }

    async fn setup_tracking(&self, graph_manager: Arc<GraphManager>) -> Result<(), Error> {
        // Store graph manager reference
        *self.graph_manager.write().await = Some(graph_manager.clone());
        Ok(())
    }
}

impl Clone for AgentGraphProvider {
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            agents: RwLock::new(Vec::new()),
            graph: RwLock::new(None),
            graph_manager: RwLock::new(None),
        }
    }
}

/// Human input graph provider
#[derive(Debug)]
pub struct HumanInputGraphProvider {
    /// Graph representation
    graph: RwLock<Option<Arc<RwLock<Graph>>>>,
    /// Graph manager
    graph_manager: RwLock<Option<Arc<GraphManager>>>,
    /// Human input provider
    human_input_provider: RwLock<Option<Arc<dyn std::any::Any + Send + Sync>>>,
}

impl HumanInputGraphProvider {
    /// Create a new human input graph provider
    pub fn new() -> Self {
        Self {
            graph: RwLock::new(None),
            graph_manager: RwLock::new(None),
            human_input_provider: RwLock::new(None),
        }
    }

    /// Create a graph from the current human input state
    async fn create_graph_from_human_input(&self) -> Result<Graph, Error> {
        let mut graph = Graph {
            id: "human_input-graph".to_string(),
            name: "Human Input Points".to_string(),
            graph_type: "human_input".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            properties: HashMap::new(),
        };

        // If we have a human input provider, create a sample graph
        // if let Some(_provider) = &self.human_input_provider {
        // TODO: Get actual human input points when API is available
        // For now, create a sample visualization

        let app_node = GraphNode {
            id: "app".to_string(),
            name: "Application".to_string(),
            node_type: "application".to_string(),
            status: "active".to_string(),
            properties: HashMap::new(),
        };

        let input1 = GraphNode {
            id: "input1".to_string(),
            name: "Text Input".to_string(),
            node_type: "human_input".to_string(),
            status: "waiting".to_string(),
            properties: HashMap::new(),
        };

        let input2 = GraphNode {
            id: "input2".to_string(),
            name: "Confirmation".to_string(),
            node_type: "human_input".to_string(),
            status: "pending".to_string(),
            properties: HashMap::new(),
        };

        graph.nodes.push(app_node);
        graph.nodes.push(input1.clone());
        graph.nodes.push(input2.clone());

        graph.edges.push(GraphEdge {
            id: "edge1".to_string(),
            source: "app".to_string(),
            target: "input1".to_string(),
            edge_type: "request".to_string(),
            properties: HashMap::new(),
        });

        graph.edges.push(GraphEdge {
            id: "edge2".to_string(),
            source: "app".to_string(),
            target: "input2".to_string(),
            edge_type: "request".to_string(),
            properties: HashMap::new(),
        });
        // }

        Ok(graph)
    }
}

#[async_trait]
impl AsyncGraphDataProvider for HumanInputGraphProvider {
    async fn generate_graph(&self) -> Result<Graph, Error> {
        self.create_graph_from_human_input().await
    }

    async fn setup_tracking(&self, graph_manager: Arc<GraphManager>) -> Result<(), Error> {
        // Store graph manager reference
        *self.graph_manager.write().await = Some(graph_manager.clone());
        Ok(())
    }
}

impl Clone for HumanInputGraphProvider {
    fn clone(&self) -> Self {
        Self {
            graph: RwLock::new(None),
            graph_manager: RwLock::new(None),
            human_input_provider: RwLock::new(None),
        }
    }
}

/// LLM integration graph provider
#[derive(Debug)]
pub struct LlmIntegrationGraphProvider {
    /// Graph representation
    graph: RwLock<Option<Arc<RwLock<Graph>>>>,
    /// Graph manager
    graph_manager: RwLock<Option<Arc<GraphManager>>>,
    /// LLM providers
    llm_providers: RwLock<Vec<Arc<dyn std::any::Any + Send + Sync>>>,
}

impl LlmIntegrationGraphProvider {
    /// Create a new LLM integration graph provider
    pub fn new() -> Self {
        Self {
            graph: RwLock::new(None),
            graph_manager: RwLock::new(None),
            llm_providers: RwLock::new(Vec::new()),
        }
    }

    /// Add an LLM provider to track
    pub async fn add_llm_provider(&self, provider: Arc<dyn std::any::Any + Send + Sync>) {
        let mut providers = self.llm_providers.write().await;
        providers.push(provider);
    }

    /// Create a graph from the current LLM integration state
    async fn create_graph_from_llm_integration(&self) -> Result<Graph, Error> {
        let providers = self.llm_providers.read().await;
        let mut graph = Graph {
            id: "llm_integration-graph".to_string(),
            name: "LLM Integration".to_string(),
            graph_type: "llm_integration".to_string(),
            nodes: Vec::new(),
            edges: Vec::new(),
            properties: HashMap::new(),
        };

        let app_node = GraphNode {
            id: "app".to_string(),
            name: "Application".to_string(),
            node_type: "application".to_string(),
            status: "active".to_string(),
            properties: HashMap::new(),
        };

        graph.nodes.push(app_node);

        // Add nodes for each LLM provider
        let mut provider_idx = 1;
        for _provider in providers.iter() {
            // TODO: Get actual provider info
            // For now, create a sample visualization

            let provider_id = format!("provider{}", provider_idx);
            let provider_name = format!("LLM Provider {}", provider_idx);

            let node = GraphNode {
                id: provider_id.clone(),
                name: provider_name,
                node_type: "llm_provider".to_string(),
                status: "active".to_string(),
                properties: HashMap::new(),
            };

            graph.nodes.push(node);

            graph.edges.push(GraphEdge {
                id: format!("edge{}", provider_idx),
                source: "app".to_string(),
                target: provider_id,
                edge_type: "request".to_string(),
                properties: HashMap::new(),
            });

            provider_idx += 1;
        }

        Ok(graph)
    }
}

#[async_trait]
impl AsyncGraphDataProvider for LlmIntegrationGraphProvider {
    async fn generate_graph(&self) -> Result<Graph, Error> {
        self.create_graph_from_llm_integration().await
    }

    async fn setup_tracking(&self, graph_manager: Arc<GraphManager>) -> Result<(), Error> {
        // Store graph manager reference
        *self.graph_manager.write().await = Some(graph_manager.clone());
        Ok(())
    }
}

impl Clone for LlmIntegrationGraphProvider {
    fn clone(&self) -> Self {
        Self {
            graph: RwLock::new(None),
            graph_manager: RwLock::new(None),
            llm_providers: RwLock::new(Vec::new()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_workflow_graph() {
        let workflow_engine = Arc::new(WorkflowEngine::new(NullSignalHandler::new()));
        let provider = WorkflowGraphProvider::new(workflow_engine);
        let graph = provider.generate_graph().await.unwrap();
        assert_eq!(graph.graph_type, "workflow");
    }

    #[tokio::test]
    async fn test_create_agent_graph() {
        let provider = AgentGraphProvider::new();
        let graph = provider.generate_graph().await.unwrap();
        assert_eq!(graph.graph_type, "agent");
    }

    #[tokio::test]
    async fn test_create_human_input_graph() {
        let provider = HumanInputGraphProvider::new();
        let graph = provider.generate_graph().await.unwrap();
        assert_eq!(graph.graph_type, "human_input");
    }

    #[tokio::test]
    async fn test_create_llm_integration_graph() {
        let provider = LlmIntegrationGraphProvider::new();
        let graph = provider.generate_graph().await.unwrap();
        assert_eq!(graph.graph_type, "llm_integration");
    }
}

// Find and fix the String error conversion issue in register_graph:
// Add a method to convert string errors to proper Error values
async fn string_to_error<T>(result: Result<T, String>) -> Result<T, crate::error::Error> {
    match result {
        Ok(val) => Ok(val),
        Err(e) => Err(crate::error::Error::TerminalError(e)),
    }
}

// Helper to handle the Future returned by register_graph
async fn register_graph_with_manager(
    graph_manager: Arc<GraphManager>,
    graph: Graph,
) -> Result<(), Error> {
    graph_manager.register_graph(graph).await
}
