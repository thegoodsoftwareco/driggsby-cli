use std::env;

use anyhow::Result;
use driggsby::{
    cli::{
        Commands, McpCommand,
        commands::{run_cli_daemon_command, run_status_command},
        connect::{run_clients_command, run_connect_command},
        parse_cli,
    },
    runtime_paths::resolve_runtime_paths,
    shim::run_mcp_server_command,
};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = parse_cli();
    let runtime_paths = resolve_runtime_paths(false)?;
    let current_exe = env::current_exe()?;

    match cli.command {
        Commands::Mcp { command } => match command {
            McpCommand::Setup {
                client,
                no_auto_add_mcp_config,
                mcp_scope,
            } => {
                run_connect_command(&runtime_paths, client, no_auto_add_mcp_config, mcp_scope).await
            }
            McpCommand::List => {
                run_clients_command(&runtime_paths, driggsby::cli::McpClientAction::List).await
            }
            McpCommand::Revoke { client } => {
                run_clients_command(
                    &runtime_paths,
                    driggsby::cli::McpClientAction::Revoke { client },
                )
                .await
            }
            McpCommand::RevokeAll => {
                run_clients_command(&runtime_paths, driggsby::cli::McpClientAction::RevokeAll).await
            }
        },
        Commands::Status => run_status_command(&runtime_paths).await,
        Commands::McpServer => run_mcp_server_command(&runtime_paths, &current_exe).await,
        Commands::CliDaemon => run_cli_daemon_command(&runtime_paths).await,
    }
}
