mod client_config_cleanup;
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
  npx driggsby@latest mcp list
  npx driggsby@latest mcp disconnect codex
  npx driggsby@latest mcp disconnect-all
  npx driggsby@latest status
  npx -y driggsby@latest mcp-server";

#[derive(Debug, Parser)]
#[command(
    name = "driggsby",
    bin_name = "npx driggsby@latest",
    version,
    arg_required_else_help = true,
    disable_help_subcommand = true,
    about = "Connect AI clients to your Driggsby financial data over MCP.",
    long_about = "Connect AI clients to your Driggsby financial data over MCP.\n\n  npx driggsby@latest mcp connect      # set up a client\n  npx driggsby@latest status            # check readiness",
    after_help = EXAMPLES,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    #[command(about = "Manage connected AI clients.")]
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    #[command(about = "Check if Driggsby is ready.")]
    Status,
    #[command(about = "Run the MCP server (launched by AI clients).")]
    McpServer,
    #[command(name = "cli-daemon", hide = true)]
    CliDaemon,
}

#[derive(Debug, Clone, Subcommand)]
pub enum McpCommand {
    #[command(
        about = "Connect an AI client to Driggsby.",
        long_about = "Connect an AI client to Driggsby.\n\nRun once per client. Opens browser sign-in if needed.\n\nSupported clients: claude-code, claude-desktop, codex\nOther IDs: letters, numbers, and hyphens."
    )]
    Connect {
        #[arg(help = "Client ID: claude-code, claude-desktop, codex, or custom.")]
        client: Option<String>,
        #[arg(long, help = "Print MCP config instead of auto-adding it.")]
        no_auto_add_mcp_config: bool,
        #[arg(
            long,
            value_enum,
            help = "Claude Code only. Values: local, user (default)."
        )]
        mcp_scope: Option<McpScope>,
    },
    #[command(about = "List connected clients.")]
    List,
    #[command(
        about = "Disconnect a client.",
        long_about = "Disconnect a client.\n\nRun this command to see connected client IDs:\n  npx driggsby@latest mcp list\n\nSupported client IDs: claude-code, claude-desktop, codex\nCustom IDs are the names shown by mcp list."
    )]
    Disconnect {
        #[arg(help = "Client ID: claude-code, claude-desktop, codex, or custom.")]
        client: Option<String>,
    },
    #[command(
        name = "disconnect-all",
        about = "Disconnect all clients and clear local state."
    )]
    DisconnectAll,
}

#[derive(Debug, Clone)]
pub enum McpClientAction {
    List,
    Disconnect { client: Option<String> },
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
        assert!(help.contains("npx driggsby@latest mcp list"));
        assert!(help.contains("--no-auto-add-mcp-config"));
        assert!(help.contains("--mcp-scope"));
        assert!(help.contains("npx driggsby@latest mcp disconnect-all"));
        assert!(!help.contains("npx driggsby@latest mcp clients"));
        assert!(!help.contains("npx driggsby@latest login"));
        assert!(!help.contains("npx driggsby@latest revoke-all"));
        assert!(!help.contains("npx driggsby@latest logout"));
        assert!(help.contains("npx -y driggsby@latest mcp-server"));
        assert!(help.contains("npx driggsby@latest status"));
        assert!(help.contains("Manage connected AI clients"));
    }
}
