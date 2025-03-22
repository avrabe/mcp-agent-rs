use anyhow::Result;
use async_trait::async_trait;
use colored::Colorize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::info;

use mcp_agent::workflow::{Workflow, WorkflowResult, WorkflowState, execute_workflow, task};

// Task status for visualization
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl TaskStatus {
    fn to_string(&self) -> String {
        match self {
            TaskStatus::Pending => "pending".to_string(),
            TaskStatus::Running => "running".to_string(),
            TaskStatus::Completed => "completed".to_string(),
            TaskStatus::Failed => "failed".to_string(),
        }
    }
}

// Task definition
#[derive(Debug, Clone)]
struct Task {
    id: String,
    name: String,
    dependencies: Vec<String>,
    status: TaskStatus,
    duration_ms: u64,
    result: Option<String>,
}

// Workflow with dependency graph
struct DependencyWorkflow {
    state: WorkflowState,
    tasks: HashMap<String, Task>,
    task_order: Vec<String>,
    execution_status: Arc<Mutex<HashMap<String, TaskStatus>>>,
}

impl DependencyWorkflow {
    // Create a new workflow with predefined tasks
    fn new() -> Self {
        let mut tasks = HashMap::new();

        // Define tasks with dependencies
        tasks.insert(
            "task1".to_string(),
            Task {
                id: "task1".to_string(),
                name: "Data Collection".to_string(),
                dependencies: vec![],
                status: TaskStatus::Pending,
                duration_ms: 500,
                result: None,
            },
        );

        tasks.insert(
            "task2".to_string(),
            Task {
                id: "task2".to_string(),
                name: "Data Validation".to_string(),
                dependencies: vec!["task1".to_string()],
                status: TaskStatus::Pending,
                duration_ms: 300,
                result: None,
            },
        );

        tasks.insert(
            "task3".to_string(),
            Task {
                id: "task3".to_string(),
                name: "Data Transformation".to_string(),
                dependencies: vec!["task1".to_string()],
                status: TaskStatus::Pending,
                duration_ms: 700,
                result: None,
            },
        );

        tasks.insert(
            "task4".to_string(),
            Task {
                id: "task4".to_string(),
                name: "Data Analysis".to_string(),
                dependencies: vec!["task2".to_string(), "task3".to_string()],
                status: TaskStatus::Pending,
                duration_ms: 800,
                result: None,
            },
        );

        tasks.insert(
            "task5".to_string(),
            Task {
                id: "task5".to_string(),
                name: "Report Generation".to_string(),
                dependencies: vec!["task4".to_string()],
                status: TaskStatus::Pending,
                duration_ms: 600,
                result: None,
            },
        );

        // Compute task execution order (topological sort)
        let task_order = Self::compute_execution_order(&tasks);

        // Create execution status map
        let mut execution_status = HashMap::new();
        for (id, _) in &tasks {
            execution_status.insert(id.clone(), TaskStatus::Pending);
        }

        Self {
            state: WorkflowState::new(Some("DependencyWorkflow".to_string()), None),
            tasks,
            task_order,
            execution_status: Arc::new(Mutex::new(execution_status)),
        }
    }

    // Compute task execution order using topological sort
    fn compute_execution_order(tasks: &HashMap<String, Task>) -> Vec<String> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut temp_visited = HashSet::new();

        // Recursive depth-first search
        fn visit(
            task_id: &str,
            tasks: &HashMap<String, Task>,
            visited: &mut HashSet<String>,
            temp_visited: &mut HashSet<String>,
            result: &mut Vec<String>,
        ) {
            // Skip if already visited
            if visited.contains(task_id) {
                return;
            }

            // Check for cycles
            if temp_visited.contains(task_id) {
                panic!("Cycle detected in task dependencies");
            }

            // Mark as temporarily visited
            temp_visited.insert(task_id.to_string());

            // Visit dependencies
            if let Some(task) = tasks.get(task_id) {
                for dep in &task.dependencies {
                    visit(dep, tasks, visited, temp_visited, result);
                }
            }

            // Mark as visited and add to result
            temp_visited.remove(task_id);
            visited.insert(task_id.to_string());
            result.push(task_id.to_string());
        }

        // Visit all tasks
        for task_id in tasks.keys() {
            if !visited.contains(task_id) {
                visit(task_id, tasks, &mut visited, &mut temp_visited, &mut result);
            }
        }

        result
    }

    // Create a task execution function
    fn create_task_function(&self, task_id: String) -> task::WorkflowTask<String> {
        let task = self.tasks.get(&task_id).unwrap().clone();
        let status_map = self.execution_status.clone();
        let task_name = task.name.clone();

        task::task(&task_name, move || async move {
            // Update status to running
            {
                let mut status = status_map.lock().await;
                status.insert(task_id.clone(), TaskStatus::Running);
            }

            info!("Executing task: {} ({})", task.name, task_id);

            // Simulate task execution
            tokio::time::sleep(Duration::from_millis(task.duration_ms)).await;

            // Generate result
            let result = format!("Result of task {}: {}", task_id, task.name);

            // Update status to completed
            {
                let mut status = status_map.lock().await;
                status.insert(task_id.clone(), TaskStatus::Completed);
            }

            info!("Task completed: {} ({})", task.name, task_id);
            Ok(result)
        })
    }

    // Print current workflow state in a visual way
    async fn print_status(&self) {
        let status = self.execution_status.lock().await;

        println!("\n{}", "Current Workflow Status".bold().cyan());
        println!("{}", "=====================".cyan());

        for task_id in &self.task_order {
            let task = &self.tasks[task_id];
            let task_status = status.get(task_id).unwrap_or(&TaskStatus::Pending);

            let status_str = match task_status {
                TaskStatus::Pending => "⏳ PENDING".yellow(),
                TaskStatus::Running => "🔄 RUNNING".blue(),
                TaskStatus::Completed => "✅ COMPLETED".green(),
                TaskStatus::Failed => "❌ FAILED".red(),
            };

            let deps_str = if task.dependencies.is_empty() {
                "None".to_string()
            } else {
                task.dependencies
                    .iter()
                    .map(|id| self.tasks[id].name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            };

            println!(
                "{:<20} {:<15} Dependencies: {}",
                task.name.white().bold(),
                status_str,
                deps_str.cyan()
            );
        }

        println!("{}", "=====================".cyan());
    }
}

#[async_trait]
impl Workflow for DependencyWorkflow {
    async fn run(&mut self) -> Result<WorkflowResult> {
        // Initialize workflow
        self.state.set_status("starting");
        info!("Starting DependencyWorkflow");

        // Print initial state
        self.print_status().await;

        // Process all tasks in order
        let mut results = HashMap::new();

        for task_id in &self.task_order {
            // Check if dependencies are completed
            let mut can_execute = true;

            {
                let status = self.execution_status.lock().await;

                for dep_id in &self.tasks[task_id].dependencies {
                    if status.get(dep_id) != Some(&TaskStatus::Completed) {
                        can_execute = false;
                        break;
                    }
                }
            }

            if can_execute {
                let task_function = self.create_task_function(task_id.clone());
                match task_function.execute().await {
                    Ok(result) => {
                        results.insert(task_id.clone(), result);

                        // Print updated state after each task
                        self.print_status().await;

                        // Small delay to see the status
                        tokio::time::sleep(Duration::from_millis(300)).await;
                    }
                    Err(e) => {
                        // Update status to failed
                        {
                            let mut status = self.execution_status.lock().await;
                            status.insert(task_id.clone(), TaskStatus::Failed);
                        }

                        // Print failed state
                        self.print_status().await;

                        return Err(e);
                    }
                }
            } else {
                // Skip tasks with incomplete dependencies
                info!("Skipping task {} due to incomplete dependencies", task_id);
            }
        }

        // Complete workflow
        self.state.set_status("completed");

        // Create final report
        let report = format!(
            "Workflow completed with {} tasks. Final result: {}",
            results.len(),
            results
                .get("task5")
                .unwrap_or(&"No final result".to_string())
        );

        Ok(WorkflowResult::success(report))
    }

    fn state(&self) -> &WorkflowState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut WorkflowState {
        &mut self.state
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set up console logging
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("\n{}", "Dependency Workflow Example".bold().green());
    println!(
        "{}",
        "This example demonstrates a workflow with task dependencies.".yellow()
    );
    println!(
        "{}",
        "Tasks will be executed in order, respecting dependencies.".yellow()
    );

    // Create and execute the workflow
    let workflow = DependencyWorkflow::new();

    println!("\n{}", "Starting workflow execution...".cyan());

    // Execute the workflow
    match execute_workflow(workflow).await {
        Ok(result) => {
            println!("\n{}", "Workflow Result:".bold().green());
            println!("{}", result.output());
        }
        Err(e) => {
            println!("\n{}", "Workflow Error:".bold().red());
            println!("{:?}", e);
        }
    }

    Ok(())
}
