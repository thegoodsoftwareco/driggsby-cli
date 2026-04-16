use crate::cli::supported_mcp_config::CliMcpClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KnownClient {
    ClaudeCode,
    Codex,
    Other,
}

impl KnownClient {
    pub(super) fn from_client_id(client_id: &str) -> Option<Self> {
        [Self::ClaudeCode, Self::Codex, Self::Other]
            .into_iter()
            .find(|client| client.integration_id() == client_id)
    }

    pub(super) fn integration_id(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::Other => "other",
        }
    }

    pub(super) fn display_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::Other => "Other MCP client",
        }
    }

    pub(super) fn cli_mcp_client(self) -> Option<CliMcpClient> {
        match self {
            Self::ClaudeCode => Some(CliMcpClient::ClaudeCode),
            Self::Codex => Some(CliMcpClient::Codex),
            Self::Other => None,
        }
    }
}
