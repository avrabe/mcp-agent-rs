// ... existing code ...
use self::models::SprottyStatus;
// ... existing code ...
    SprottyEdge, SprottyGraph, SprottyGraphLayout, SprottyNode, SprottyRoot, TaskNode,
// ... existing code ...

    pub async fn register_graph(&self, graph: Graph) -> Result<()> {
        // ... existing code ...
    }

    pub async fn update_node(&self, graph_id: &str, node: GraphNode) -> Result<()> {
        let graphs = self.graphs.read().await;
        if let Some(graph) = graphs.get(graph_id) {
            let mut nodes = graph.nodes.write().await;
            if let Some(existing_node) = nodes.get_mut(&node.id) {
                *existing_node = node;
                self.notify_graph_update(graph_id.to_string(), GraphUpdate::NodeUpdated(node.clone())).await;
                Ok(())
            } else {
                Err(Error::TerminalError(format!("Node {} not found in graph {}", node.id, graph_id)))
            }
        } else {
            Err(Error::TerminalError(format!("Graph {} not found", graph_id)))
        }
    }

    pub async fn initialize_visualization(workflow_engine: Option<Arc<WorkflowEngine>>, agents: Vec<Arc<Agent>>) -> Result<Arc<GraphManager>> {
        // ... existing code ...
    }
}