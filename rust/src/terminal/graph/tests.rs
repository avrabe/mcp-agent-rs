use super::*;
use crate::workflow::signal::NullSignalHandler;
use chrono::Utc;

#[tokio::test]
async fn test_graph_manager_initialization() {
    let manager = GraphManager::new();
    assert!(manager.providers.read().await.is_empty());
    assert!(manager.router.read().await.is_some());
}

#[tokio::test]
async fn test_graph_manager_provider_registration() {
    let manager = GraphManager::new();
    let provider = WorkflowGraphProvider::new();
    
    assert!(manager.register_provider("workflow", provider).await.is_ok());
    assert_eq!(manager.providers.read().await.len(), 1);
}

#[tokio::test]
async fn test_graph_manager_duplicate_registration() {
    let manager = GraphManager::new();
    let provider = WorkflowGraphProvider::new();
    
    assert!(manager.register_provider("workflow", provider.clone()).await.is_ok());
    assert!(manager.register_provider("workflow", provider).await.is_err());
}

#[tokio::test]
async fn test_graph_manager_provider_removal() {
    let manager = GraphManager::new();
    let provider = WorkflowGraphProvider::new();
    
    assert!(manager.register_provider("workflow", provider).await.is_ok());
    assert!(manager.remove_provider("workflow").await.is_ok());
    assert!(manager.providers.read().await.is_empty());
}

#[tokio::test]
async fn test_graph_manager_get_provider() {
    let manager = GraphManager::new();
    let provider = WorkflowGraphProvider::new();
    
    assert!(manager.register_provider("workflow", provider.clone()).await.is_ok());
    let retrieved = manager.get_provider("workflow").await;
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name(), "workflow");
}

#[tokio::test]
async fn test_graph_manager_update_graph() {
    let manager = GraphManager::new();
    let provider = WorkflowGraphProvider::new();
    
    assert!(manager.register_provider("workflow", provider).await.is_ok());
    assert!(manager.update_graph("workflow").await.is_ok());
}

#[tokio::test]
async fn test_graph_manager_update_nonexistent_graph() {
    let manager = GraphManager::new();
    assert!(manager.update_graph("nonexistent").await.is_err());
}

#[tokio::test]
async fn test_graph_manager_get_all_graphs() {
    let manager = GraphManager::new();
    let workflow_provider = WorkflowGraphProvider::new();
    let agent_provider = AgentGraphProvider::new();
    
    assert!(manager.register_provider("workflow", workflow_provider).await.is_ok());
    assert!(manager.register_provider("agent", agent_provider).await.is_ok());
    
    let graphs = manager.get_all_graphs().await;
    assert_eq!(graphs.len(), 2);
    assert!(graphs.contains_key("workflow"));
    assert!(graphs.contains_key("agent"));
}

#[tokio::test]
async fn test_graph_manager_notification_system() {
    let manager = GraphManager::new();
    let provider = WorkflowGraphProvider::new();
    
    assert!(manager.register_provider("workflow", provider).await.is_ok());
    
    // Simulate a graph update
    let graph = Graph {
        id: "workflow".to_string(),
        graph_type: "workflow".to_string(),
        nodes: vec![],
        edges: vec![],
        metadata: HashMap::new(),
    };
    
    assert!(manager.notify_graph_update("workflow", graph).await.is_ok());
}

#[tokio::test]
async fn test_graph_manager_error_handling() {
    let manager = GraphManager::new();
    
    // Test removing non-existent provider
    assert!(manager.remove_provider("nonexistent").await.is_err());
    
    // Test getting non-existent provider
    assert!(manager.get_provider("nonexistent").await.is_none());
    
    // Test updating non-existent graph
    assert!(manager.update_graph("nonexistent").await.is_err());
}

#[tokio::test]
async fn test_graph_manager_concurrent_access() {
    let manager = Arc::new(GraphManager::new());
    let mut handles = vec![];
    
    // Spawn multiple tasks that register providers concurrently
    for i in 0..5 {
        let manager_clone = manager.clone();
        handles.push(tokio::spawn(async move {
            let provider = WorkflowGraphProvider::new();
            manager_clone.register_provider(&format!("provider_{}", i), provider).await
        }));
    }
    
    // Wait for all tasks to complete
    for handle in handles {
        assert!(handle.await.unwrap().is_ok());
    }
    
    // Verify all providers were registered
    assert_eq!(manager.providers.read().await.len(), 5);
}

#[tokio::test]
async fn test_graph_manager_cleanup() {
    let manager = GraphManager::new();
    let provider = WorkflowGraphProvider::new();
    
    assert!(manager.register_provider("workflow", provider).await.is_ok());
    assert!(manager.remove_provider("workflow").await.is_ok());
    
    // Verify cleanup
    assert!(manager.providers.read().await.is_empty());
    assert!(manager.get_provider("workflow").await.is_none());
}

#[tokio::test]
async fn test_graph_manager_provider_lifecycle() {
    let manager = GraphManager::new();
    let provider = WorkflowGraphProvider::new();
    
    // Register provider
    assert!(manager.register_provider("workflow", provider).await.is_ok());
    
    // Update graph
    assert!(manager.update_graph("workflow").await.is_ok());
    
    // Get graph
    let graphs = manager.get_all_graphs().await;
    assert!(graphs.contains_key("workflow"));
    
    // Remove provider
    assert!(manager.remove_provider("workflow").await.is_ok());
    
    // Verify removal
    assert!(manager.providers.read().await.is_empty());
    assert!(manager.get_provider("workflow").await.is_none());
} 