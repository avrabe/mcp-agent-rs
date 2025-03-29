#[tokio::test]
async fn test_human_input_event_handler() {
    let graph_manager = Arc::new(GraphManager::new());
    let handler = HumanInputEventHandler::new(graph_manager.clone(), "test-input".to_string());

    let state = InputState {
        status: "waiting".to_string(),
        last_input: Some(Utc::now()),
        timeout: Some(Utc::now() + chrono::Duration::seconds(30)),
        description: Some("Test input".to_string()),
        required: true,
    };

    assert!(handler.handle_state_change(state).await.is_ok());
    assert!(handler.handle_timeout().await.is_ok());
} 