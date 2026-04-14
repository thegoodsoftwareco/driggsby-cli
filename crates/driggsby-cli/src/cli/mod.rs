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
  npx driggsby@latest mcp setup
  npx driggsby@latest mcp setup claude-code
  npx driggsby@latest mcp setup claude-desktop
  npx driggsby@latest mcp setup codex
  npx driggsby@latest mcp setup claude-code --mcp-scope user
  npx driggsby@latest mcp setup codex --no-auto-add-mcp-config
  npx driggsby@latest mcp list
  npx driggsby@latest mcp revoke codex
  npx driggsby@latest mcp revoke-all
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
    long_about = "Connect AI clients to your Driggsby financial data over MCP.\n\n  npx driggsby@latest mcp setup       # set up a client\n  npx driggsby@latest status          # check readiness",
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
        name = "setup",
        alias = "connect",
        alias = "add",
        about = "Set up Driggsby for an AI client.",
        long_about = "Set up Driggsby for an AI client.\n\nRun once per client. Opens browser sign-in if needed.\n\nSupported clients: claude-code, claude-desktop, codex\nOther IDs: letters, numbers, and hyphens."
    )]
    Setup {
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
        name = "revoke",
        alias = "disconnect",
        alias = "remove",
        about = "Revoke a client key.",
        long_about = "Revoke a client key.\n\nRun this command to see connected client IDs:\n  npx driggsby@latest mcp list\n\nSupported client IDs: claude-code, claude-desktop, codex\nCustom IDs are the names shown by mcp list."
    )]
    Revoke {
        #[arg(help = "Client ID: claude-code, claude-desktop, codex, or custom.")]
        client: Option<String>,
    },
    #[command(
        name = "revoke-all",
        alias = "disconnect-all",
        alias = "remove-all",
        about = "Revoke Driggsby access on this device."
    )]
    RevokeAll,
}

#[derive(Debug, Clone)]
pub enum McpClientAction {
    List,
    Revoke { client: Option<String> },
    RevokeAll,
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

        assert!(help.contains("npx driggsby@latest mcp setup"));
        assert!(help.contains("npx driggsby@latest mcp setup claude-code"));
        assert!(help.contains("npx driggsby@latest mcp setup claude-desktop"));
        assert!(help.contains("npx driggsby@latest mcp list"));
        assert!(help.contains("--no-auto-add-mcp-config"));
        assert!(help.contains("--mcp-scope"));
        assert!(help.contains("npx driggsby@latest mcp revoke-all"));
        assert!(!help.contains("npx driggsby@latest mcp add"));
        assert!(!help.contains("npx driggsby@latest mcp remove"));
        assert!(!help.contains("npx driggsby@latest mcp remove-all"));
        assert!(!help.contains("npx driggsby@latest mcp connect"));
        assert!(!help.contains("npx driggsby@latest mcp disconnect"));
        assert!(!help.contains("npx driggsby@latest mcp disconnect-all"));
        assert!(!help.contains("npx driggsby@latest mcp clients"));
        assert!(!help.contains("npx driggsby@latest login"));
        assert!(!help.contains("npx driggsby@latest logout"));
        assert!(help.contains("npx -y driggsby@latest mcp-server"));
        assert!(help.contains("npx driggsby@latest status"));
        assert!(help.contains("Manage connected AI clients"));
    }
}
