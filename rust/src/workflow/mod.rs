//! Workflow engine for task orchestration and state management.
//! 
//! This module provides a Rust implementation of the workflow engine
//! ported from the Python implementation. It supports:
//! 
//! - Workflow state management
//! - Task execution with retry policies
//! - Signal handling for workflow pausing/resuming
//! - Integration with the MCP agent telemetry system

/// State management for workflows
pub mod state;
/// Signal handling for workflows
pub mod signal;
/// Task definition and execution for workflows
pub mod task;
/// Workflow execution engine
pub mod engine;

// Re-export key components
pub use state::{WorkflowState, WorkflowResult, SharedWorkflowState};
pub use signal::{WorkflowSignal, SignalHandler, AsyncSignalHandler};
pub use task::{WorkflowTask, TaskGroup, RetryConfig, task};
pub use engine::{WorkflowEngine, WorkflowEngineConfig, Workflow, execute_workflow};
