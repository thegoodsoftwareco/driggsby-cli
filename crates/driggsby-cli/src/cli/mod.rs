pub mod commands;
pub mod format;

use clap::{CommandFactory, Parser, Subcommand};

const EXAMPLES: &str = "\
Examples:
  npx driggsby@latest login
  npx driggsby@latest status
  npx -y driggsby@latest mcp-server
  codex mcp add driggsby -- npx -y driggsby@latest mcp-server";

#[derive(Debug, Parser)]
#[command(
    name = "driggsby",
    bin_name = "npx driggsby@latest",
    version,
    arg_required_else_help = true,
    disable_help_subcommand = true,
    about = "Local Driggsby CLI for connecting AI clients to Driggsby over MCP.",
    long_about = "Local Driggsby CLI for connecting AI clients to Driggsby over MCP.\n\nThe normal flow is:\n  1. Run npx driggsby@latest login once to connect the CLI.\n  2. Point your MCP client at npx -y driggsby@latest mcp-server.\n  3. Use npx driggsby@latest status any time to confirm readiness.",
    after_help = EXAMPLES,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Commands {
    #[command(about = "Open the browser sign-in flow and connect the CLI.")]
    Login,
    #[command(about = "Show a clear readiness summary for humans and agents.")]
    Status,
    #[command(about = "Run the local MCP server that AI clients should launch.")]
    McpServer,
    #[command(about = "Clear local CLI auth and session state.")]
    Logout,
    #[command(name = "cli-daemon", hide = true)]
    CliDaemon,
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

        assert!(help.contains("npx driggsby@latest login"));
        assert!(help.contains("npx -y driggsby@latest mcp-server"));
        assert!(help.contains("npx driggsby@latest status"));
        assert!(help.contains("connect the CLI"));
    }
}
