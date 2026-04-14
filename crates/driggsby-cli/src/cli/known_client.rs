use crate::cli::{desktop_mcp_config::DesktopMcpConfigClient, supported_mcp_config::CliMcpClient};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KnownClient {
    ClaudeCode,
    ClaudeDesktop,
    Codex,
}

impl KnownClient {
    pub(super) fn from_client_id(client_id: &str) -> Option<Self> {
        [Self::ClaudeCode, Self::ClaudeDesktop, Self::Codex]
            .into_iter()
            .find(|client| client.integration_id() == client_id)
    }

    pub(super) fn integration_id(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::ClaudeDesktop => "claude-desktop",
            Self::Codex => "codex",
        }
    }

    pub(super) fn display_name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::ClaudeDesktop => "Claude Desktop",
            Self::Codex => "Codex",
        }
    }

    pub(super) fn cli_mcp_client(self) -> Option<CliMcpClient> {
        match self {
            Self::ClaudeCode => Some(CliMcpClient::ClaudeCode),
            Self::Codex => Some(CliMcpClient::Codex),
            Self::ClaudeDesktop => None,
        }
    }

    pub(super) fn desktop_mcp_client(self) -> Option<DesktopMcpConfigClient> {
        match self {
            Self::ClaudeDesktop => Some(DesktopMcpConfigClient::ClaudeDesktop),
            Self::ClaudeCode | Self::Codex => None,
        }
    }
}
