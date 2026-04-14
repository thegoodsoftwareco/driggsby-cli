mod client_id;
pub mod commands;
pub mod connect;
mod connect_session;
mod desktop_mcp_config;
pub mod format;
mod known_client;
mod supported_mcp_config;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};

const EXAMPLES: &str = "\
Examples:
  npx driggsby@latest mcp connect
  npx driggsby@latest mcp connect claude-code
  npx driggsby@latest mcp connect claude-desktop
  npx driggsby@latest mcp connect codex
  npx driggsby@latest mcp connect claude-code --mcp-scope user
  npx driggsby@latest mcp connect codex --no-auto-add-mcp-config
  npx driggsby@latest mcp clients list
  npx driggsby@latest mcp clients disconnect codex
  npx driggsby@latest mcp clients disconnect-all
  npx driggsby@latest status
  npx -y driggsby@latest mcp-server";

#[derive(Debug, Parser)]
#[command(
    name = "driggsby",
    bin_name = "npx driggsby@latest",
    version,
    arg_required_else_help = true,
    disable_help_subcommand = true,
    about = "Local Driggsby CLI for connecting AI clients to Driggsby over MCP.",
    long_about = "Local Driggsby CLI for connecting AI clients to Driggsby over MCP.\n\nNormal flow:\n  1. Connect each MCP client once:\n     npx driggsby@latest mcp connect\n  2. Sign in to Driggsby CLI if prompted.\n  3. Confirm readiness any time:\n     npx driggsby@latest status",
    after_help = EXAMPLES,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    #[command(about = "Manage Driggsby MCP client connections.")]
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    #[command(about = "Show a clear readiness summary for humans and agents.")]
    Status,
    #[command(about = "Run the local MCP server that AI clients should launch.")]
    McpServer,
    #[command(name = "cli-daemon", hide = true)]
    CliDaemon,
}

#[derive(Debug, Clone, Subcommand)]
pub enum McpCommand {
    #[command(
        about = "Connect Driggsby to one MCP client.",
        long_about = "Connect Driggsby to one MCP client.\n\nRun this once per MCP client you want to use with Driggsby. If your saved Driggsby CLI session is missing or older than 8 hours, this opens browser sign-in first.\n\nSupported client IDs:\n  claude-code\n  claude-desktop\n  codex\n\nOther client IDs may use letters, numbers, and hyphens."
    )]
    Connect {
        #[arg(
            help = "Supported client ID: claude-code, claude-desktop, or codex. Other IDs may use letters, numbers, and hyphens."
        )]
        client: Option<String>,
        #[arg(
            long,
            help = "Print the MCP config instead of automatically adding it for supported clients."
        )]
        no_auto_add_mcp_config: bool,
        #[arg(
            long,
            value_enum,
            help = "Claude Code MCP config scope only. Supported values: local, user. Defaults to user."
        )]
        mcp_scope: Option<McpScope>,
    },
    #[command(about = "List or disconnect connected local MCP clients.")]
    Clients {
        #[command(subcommand)]
        command: ClientCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum ClientCommand {
    #[command(about = "List connected local MCP clients.")]
    List,
    #[command(about = "Disconnect a connected local MCP client.")]
    Disconnect {
        #[arg(
            help = "Client ID, or known client id such as claude-code, claude-desktop, or codex."
        )]
        client: String,
    },
    #[command(
        name = "disconnect-all",
        about = "Disconnect all local Driggsby MCP clients and clear local MCP state."
    )]
    DisconnectAll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum McpScope {
    Local,
    User,
}

impl McpScope {
    pub fn as_cli_value(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::User => "user",
        }
    }
}

pub fn parse_cli() -> Cli {
    Cli::parse()
}

pub fn render_help() -> String {
    let mut command = Cli::command();
    let mut output = Vec::new();
    let _ = command.write_long_help(&mut output);
    String::from_utf8_lossy(&output).into_owned()
}

#[cfg(test)]
mod tests {
    use super::render_help;

    #[test]
    fn help_mentions_happy_path_and_examples() {
        let help = render_help();

        assert!(help.contains("npx driggsby@latest mcp connect"));
        assert!(help.contains("npx driggsby@latest mcp connect claude-code"));
        assert!(help.contains("npx driggsby@latest mcp connect claude-desktop"));
        assert!(help.contains("--no-auto-add-mcp-config"));
        assert!(help.contains("--mcp-scope"));
        assert!(help.contains("npx driggsby@latest mcp clients list"));
        assert!(help.contains("npx driggsby@latest mcp clients disconnect-all"));
        assert!(!help.contains("npx driggsby@latest login"));
        assert!(!help.contains("npx driggsby@latest revoke-all"));
        assert!(!help.contains("npx driggsby@latest logout"));
        assert!(help.contains("npx -y driggsby@latest mcp-server"));
        assert!(help.contains("npx driggsby@latest status"));
        assert!(help.contains("Manage Driggsby MCP client connections"));
    }
}
