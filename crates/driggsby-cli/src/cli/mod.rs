mod client_id;
pub mod connect;
mod known_client;
mod supported_mcp_config;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};

const EXAMPLES: &str = "\
Examples:
  npx driggsby@latest mcp setup
  npx driggsby@latest mcp setup claude-code
  npx driggsby@latest mcp setup codex";

#[derive(Debug, Parser)]
#[command(
    name = "driggsby",
    bin_name = "npx driggsby@latest",
    version,
    arg_required_else_help = true,
    disable_help_subcommand = true,
    about = "Set up Driggsby for AI clients.",
    long_about = "Set up Driggsby for AI clients.\n\n  npx driggsby@latest mcp setup       # set up a client",
    after_help = EXAMPLES,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    #[command(about = "Set up Driggsby MCP clients.")]
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum McpCommand {
    #[command(
        name = "setup",
        about = "Set up Driggsby for an AI client.",
        long_about = "Set up Driggsby for an AI client.\n\nRun once per client. This adds Driggsby's MCP URL to the client config. Follow the printed next step to authenticate.\n\nSupported clients: claude-code, codex."
    )]
    Setup {
        #[arg(help = "Client ID: claude-code or codex.")]
        client: Option<String>,
        #[arg(long, help = "Print the setup command instead of running it.")]
        print: bool,
        #[arg(
            short = 's',
            value_enum,
            help = "Claude Code only. Values: local, user (default)."
        )]
        mcp_scope: Option<McpScope>,
    },
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
    fn help_matches_launch_surface() {
        let help = render_help();

        assert!(help.contains("npx driggsby@latest mcp setup"));
        assert!(help.contains("npx driggsby@latest mcp setup claude-code"));
        assert!(help.contains("npx driggsby@latest mcp setup codex"));
        assert!(!help.contains("npx driggsby@latest mcp setup claude-code -s user"));
        assert!(!help.contains("npx driggsby@latest mcp setup codex --print"));
        assert!(help.contains("Set up Driggsby MCP clients"));
    }
}
