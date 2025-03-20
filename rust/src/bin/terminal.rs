use mcp_agent::mcp::agent::{Agent, AgentConfig};
use mcp_agent::mcp::types::{Message, MessageType, Priority};
use mcp_agent::utils::error::McpResult;
use rustyline::error::ReadlineError;
use rustyline::Editor;
use serde_json::json;
use std::collections::HashMap;
use std::str::FromStr;
use tokio::runtime::Runtime;

/// Interactive terminal for the MCP agent
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure logging
    env_logger::init();

    // Create async runtime
    let rt = Runtime::new()?;

    // Initialize agent
    let agent = Agent::new(None);
    
    // Create command handler with registered commands
    let mut command_handler = CommandHandler::new(agent);
    command_handler.register_command("help", Box::new(help_command));
    command_handler.register_command("connect", Box::new(connect_command));
    command_handler.register_command("disconnect", Box::new(disconnect_command));
    command_handler.register_command("list", Box::new(list_command));
    command_handler.register_command("send", Box::new(send_command));
    command_handler.register_command("exec", Box::new(exec_command));
    command_handler.register_command("stream", Box::new(stream_command));
    command_handler.register_command("exit", Box::new(exit_command));
    command_handler.register_command("quit", Box::new(exit_command));

    // Start REPL
    println!("MCP Agent Terminal");
    println!("Type 'help' for available commands");

    let mut rl = Editor::<()>::new();
    if let Err(err) = rl.load_history("history.txt") {
        println!("No previous history: {}", err);
    }

    // Main REPL loop
    loop {
        let readline = rl.readline("mcp> ");
        match readline {
            Ok(line) => {
                let trimmed_line = line.trim();
                if !trimmed_line.is_empty() {
                    rl.add_history_entry(trimmed_line);
                    
                    // Execute command in runtime
                    let result = rt.block_on(command_handler.execute_command(trimmed_line));
                    
                    // Check for exit command
                    if let Some(CommandResult::Exit) = result {
                        break;
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("CTRL-C received, press CTRL-D to exit");
            }
            Err(ReadlineError::Eof) => {
                println!("CTRL-D received, exiting");
                break;
            }
            Err(err) => {
                println!("Error: {:?}", err);
                break;
            }
        }
    }

    // Save command history
    if let Err(err) = rl.save_history("history.txt") {
        println!("Error saving history: {}", err);
    }

    Ok(())
}

/// Result of a command execution
#[derive(Debug)]
enum CommandResult {
    Success(String),
    Error(String),
    Exit,
}

/// Type definition for command functions
type CommandFn = Box<dyn Fn(&Agent, &[&str]) -> McpResult<CommandResult>>;

/// Handler for MCP terminal commands
struct CommandHandler {
    agent: Agent,
    commands: HashMap<String, CommandFn>,
}

impl CommandHandler {
    /// Create a new command handler
    fn new(agent: Agent) -> Self {
        Self {
            agent,
            commands: HashMap::new(),
        }
    }

    /// Register a command with the handler
    fn register_command(&mut self, name: &str, command: CommandFn) {
        self.commands.insert(name.to_string(), command);
    }

    /// Execute a command string
    async fn execute_command(&self, command_line: &str) -> Option<CommandResult> {
        let parts: Vec<&str> = command_line.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }

        let command = parts[0];
        let args = &parts[1..];

        match self.commands.get(command) {
            Some(cmd_fn) => {
                match cmd_fn(&self.agent, args) {
                    Ok(result) => {
                        match &result {
                            CommandResult::Success(message) => println!("Success: {}", message),
                            CommandResult::Error(error) => println!("Error: {}", error),
                            CommandResult::Exit => println!("Exiting..."),
                        }
                        Some(result)
                    }
                    Err(e) => {
                        println!("Command error: {}", e);
                        Some(CommandResult::Error(e.to_string()))
                    }
                }
            }
            None => {
                println!("Unknown command: {}", command);
                println!("Type 'help' for available commands");
                None
            }
        }
    }
}

/// Implementation of the help command
fn help_command(_agent: &Agent, _args: &[&str]) -> McpResult<CommandResult> {
    let help_text = r#"
Available commands:
  help                    - Show this help message
  connect <id> <address>  - Connect to a server with given ID and address
  disconnect <id>         - Disconnect from a server
  list                    - List connected servers
  send <id> <payload>     - Send a message to a server
  exec <function> <args>  - Execute a function with arguments
  stream <function> <args> - Execute a function and stream results
  exit, quit              - Exit the terminal
"#;
    
    Ok(CommandResult::Success(help_text.to_string()))
}

/// Implementation of the connect command
fn connect_command(agent: &Agent, args: &[&str]) -> McpResult<CommandResult> {
    if args.len() < 2 {
        return Ok(CommandResult::Error(
            "Usage: connect <id> <address>".to_string(),
        ));
    }

    let server_id = args[0];
    let address = args[1];

    tokio::task::block_in_place(|| {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            // Use the test server connection for simplicity
            agent.connect_to_test_server(server_id, address).await?;
            Ok(CommandResult::Success(format!("Connected to {}", server_id)))
        })
    })
}

/// Implementation of the disconnect command
fn disconnect_command(agent: &Agent, args: &[&str]) -> McpResult<CommandResult> {
    if args.is_empty() {
        return Ok(CommandResult::Error(
            "Usage: disconnect <id>".to_string(),
        ));
    }

    let server_id = args[0];

    tokio::task::block_in_place(|| {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            agent.disconnect(server_id).await?;
            Ok(CommandResult::Success(format!("Disconnected from {}", server_id)))
        })
    })
}

/// Implementation of the list command
fn list_command(agent: &Agent, _args: &[&str]) -> McpResult<CommandResult> {
    tokio::task::block_in_place(|| {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let count = agent.connection_count().await;
            let connections = agent.list_connections().await;
            
            if connections.is_empty() {
                Ok(CommandResult::Success("No active connections".to_string()))
            } else {
                let conn_list = connections.join(", ");
                Ok(CommandResult::Success(format!("Connected to {} servers: {}", count, conn_list)))
            }
        })
    })
}

/// Implementation of the send command
fn send_command(agent: &Agent, args: &[&str]) -> McpResult<CommandResult> {
    if args.len() < 2 {
        return Ok(CommandResult::Error(
            "Usage: send <id> <payload>".to_string(),
        ));
    }

    let server_id = args[0];
    let payload = args[1..].join(" ");

    tokio::task::block_in_place(|| {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let message = Message::new(
                MessageType::Request,
                Priority::Normal,
                payload.as_bytes().to_vec(),
                None,
                None,
            );
            
            agent.send_message(server_id, message).await?;
            Ok(CommandResult::Success(format!("Message sent to {}", server_id)))
        })
    })
}

/// Implementation of the exec command
fn exec_command(agent: &Agent, args: &[&str]) -> McpResult<CommandResult> {
    if args.len() < 2 {
        return Ok(CommandResult::Error(
            "Usage: exec <function> <args>".to_string(),
        ));
    }

    let function = args[0];
    let payload = args[1..].join(" ");
    
    // Try to parse the payload as JSON, or use it as a string value
    let json_args = match serde_json::from_str::<serde_json::Value>(&payload) {
        Ok(value) => value,
        Err(_) => json!({ "value": payload }),
    };

    tokio::task::block_in_place(|| {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let result = agent.execute_task(function, json_args, None).await?;
            if result.is_success() {
                let value = result.success_value().unwrap_or(&json!(null));
                Ok(CommandResult::Success(format!("Function executed. Result: {}", value)))
            } else {
                let error = result.error_message().unwrap_or("Unknown error");
                Ok(CommandResult::Error(format!("Function execution failed: {}", error)))
            }
        })
    })
}

/// Implementation of the stream command
fn stream_command(agent: &Agent, args: &[&str]) -> McpResult<CommandResult> {
    if args.len() < 2 {
        return Ok(CommandResult::Error(
            "Usage: stream <function> <args>".to_string(),
        ));
    }

    let function = args[0];
    let payload = args[1..].join(" ");
    
    // Try to parse the payload as JSON, or use it as a string value
    let json_args = match serde_json::from_str::<serde_json::Value>(&payload) {
        Ok(value) => value,
        Err(_) => json!({ "value": payload }),
    };

    tokio::task::block_in_place(|| {
        let rt = tokio::runtime::Handle::current();
        rt.block_on(async {
            let mut rx = agent.execute_task_stream(function, json_args, None).await?;
            
            println!("Streaming results:");
            let mut count = 0;
            while let Some(result) = rx.recv().await {
                count += 1;
                if result.is_success() {
                    let value = result.success_value().unwrap_or(&json!(null));
                    println!("[{}] {}", count, value);
                } else {
                    let error = result.error_message().unwrap_or("Unknown error");
                    println!("[{}] Error: {}", count, error);
                }
            }
            
            Ok(CommandResult::Success(format!("Received {} results", count)))
        })
    })
}

/// Implementation of the exit command
fn exit_command(_agent: &Agent, _args: &[&str]) -> McpResult<CommandResult> {
    Ok(CommandResult::Exit)
} 