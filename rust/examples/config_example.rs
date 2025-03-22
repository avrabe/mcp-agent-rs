// Only include necessary imports
#[cfg(feature = "terminal-web")]
use std::net::{IpAddr, Ipv4Addr};

#[cfg(feature = "terminal-web")]
use mcp_agent::{
    error::Result,
    terminal::{
        TerminalSystem,
        config::{AuthConfig, AuthMethod, TerminalConfig, WebTerminalConfig},
    },
};

#[cfg(feature = "terminal-web")]
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    println!("Terminal Configuration Example");
    println!("=============================");

    // Method 1: Configure using factory methods and then customize
    println!("\n1. Using factory methods:");
    let mut config1 = TerminalConfig::dual_terminal();

    // Configure web terminal settings
    config1.web_terminal_config.host = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
    config1.web_terminal_config.port = 8080;
    config1.web_terminal_config.enable_visualization = true;

    // Configure authentication
    config1.web_terminal_config.auth_config.auth_method = AuthMethod::None;
    config1.web_terminal_config.auth_config.allow_anonymous = true;

    println!(
        "  - Web terminal host: {:?}",
        config1.web_terminal_config.host
    );
    println!(
        "  - Web terminal port: {}",
        config1.web_terminal_config.port
    );
    println!(
        "  - Visualization enabled: {}",
        config1.web_terminal_config.enable_visualization
    );
    println!(
        "  - Auth method: {:?}",
        config1.web_terminal_config.auth_config.auth_method
    );
    println!(
        "  - Allow anonymous: {}",
        config1.web_terminal_config.auth_config.allow_anonymous
    );

    // Method 2: Explicit construction
    println!("\n2. Using explicit construction:");
    let config2 = TerminalConfig {
        console_terminal_enabled: true,
        web_terminal_enabled: true,
        web_terminal_config: WebTerminalConfig {
            host: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 9090,
            enable_visualization: true,
            auth_config: AuthConfig {
                auth_method: AuthMethod::Basic,
                jwt_secret: "example-secret".to_string(),
                token_expiration_secs: 3600,
                username: "admin".to_string(),
                password: "password".to_string(),
                allow_anonymous: false,
            },
        },
        input_timeout_secs: 0,      // No timeout
        max_message_size: 10485760, // 10MB
    };

    println!(
        "  - Web terminal host: {:?}",
        config2.web_terminal_config.host
    );
    println!(
        "  - Web terminal port: {}",
        config2.web_terminal_config.port
    );
    println!(
        "  - Visualization enabled: {}",
        config2.web_terminal_config.enable_visualization
    );
    println!(
        "  - Auth method: {:?}",
        config2.web_terminal_config.auth_config.auth_method
    );
    println!(
        "  - Allow anonymous: {}",
        config2.web_terminal_config.auth_config.allow_anonymous
    );

    println!("\nNote: This example doesn't actually start the terminal system.");
    println!("To start it, use:");
    println!("  let terminal = TerminalSystem::new(config);");
    println!("  terminal.start().await?;");

    Ok(())
}

// Add a non-terminal version so the example compiles without terminal-web feature
#[cfg(not(feature = "terminal-web"))]
fn main() {
    println!("This example requires the terminal-web feature to be enabled.");
    println!("Try running with: cargo run --example config_example --features=\"terminal-web\"");
}
