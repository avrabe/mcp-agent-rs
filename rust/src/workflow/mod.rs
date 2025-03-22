//! Workflow engine for task orchestration and state management.
//!
//! This module provides a Rust implementation of the workflow engine
//! ported from the Python implementation. It supports:
//!
//! - Workflow state managemen
//! - Task execution with retry policies
//! - Signal handling for workflow pausing/resuming
//! - Integration with the MCP agent telemetry system

/// Workflow execution engine
pub mod engine;
/// Signal handling for workflows
pub mod signal;
/// State management for workflows
pub mod state;
/// Task definition and execution for workflows
pub mod task;

// Re-export key components
pub use engine::{execute_workflow, Workflow, WorkflowEngine, WorkflowEngineConfig};
pub use signal::{AsyncSignalHandler, SignalHandler, WorkflowSignal};
pub use state::{SharedWorkflowState, WorkflowResult, WorkflowState};
pub use task::{task, RetryConfig, TaskGroup, WorkflowTask};
