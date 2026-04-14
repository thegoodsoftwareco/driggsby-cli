use crate::{
    broker::grants::{CLIENT_KEY_ENV, CreatedClientGrant},
    cli::McpScope,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CliMcpClient {
    ClaudeCode,
    Codex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct McpConfigCommand {
    pub program: String,
    pub args: Vec<String>,
}

pub(super) fn build_installer_command(
    client: CliMcpClient,
    created: &CreatedClientGrant,
    scope: Option<McpScope>,
) -> McpConfigCommand {
    let key = format!("{}={}", CLIENT_KEY_ENV, created.client_key);
    match client {
        CliMcpClient::ClaudeCode => {
            let mut args = vec!["mcp".to_string(), "add".to_string(), "-e".to_string(), key];
            let scope = scope.unwrap_or(McpScope::User);
            args.extend([
                "-s".to_string(),
                scope.as_cli_value().to_string(),
                "driggsby".to_string(),
                "--".to_string(),
                "npx".to_string(),
                "-y".to_string(),
                "driggsby@latest".to_string(),
                "mcp-server".to_string(),
            ]);
            McpConfigCommand {
                program: "claude".to_string(),
                args,
            }
        }
        CliMcpClient::Codex => McpConfigCommand {
            program: "codex".to_string(),
            args: vec![
                "mcp".to_string(),
                "add".to_string(),
                "--env".to_string(),
                key,
                "driggsby".to_string(),
                "--".to_string(),
                "npx".to_string(),
                "-y".to_string(),
                "driggsby@latest".to_string(),
                "mcp-server".to_string(),
            ],
        },
    }
}

pub(super) fn build_remover_command(client: CliMcpClient) -> McpConfigCommand {
    build_scoped_remover_command(client, None)
}

pub(super) fn build_scoped_remover_command(
    client: CliMcpClient,
    scope: Option<McpScope>,
) -> McpConfigCommand {
    match client {
        CliMcpClient::ClaudeCode => {
            let mut args = vec![
                "mcp".to_string(),
                "remove".to_string(),
                "driggsby".to_string(),
            ];
            if let Some(scope) = scope {
                args.extend(["-s".to_string(), scope.as_cli_value().to_string()]);
            }
            McpConfigCommand {
                program: "claude".to_string(),
                args,
            }
        }
        CliMcpClient::Codex => McpConfigCommand {
            program: "codex".to_string(),
            args: vec![
                "mcp".to_string(),
                "remove".to_string(),
                "driggsby".to_string(),
            ],
        },
    }
}

pub(super) fn render_shell_command(command: &McpConfigCommand) -> String {
    std::iter::once(command.program.as_str())
        .chain(command.args.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(value: &str) -> String {
    if value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/' | b'=')
    }) {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::{CliMcpClient, build_installer_command, render_shell_command};

    #[test]
    fn codex_installer_uses_single_client_key() {
        let created = test_grant();
        let command = build_installer_command(CliMcpClient::Codex, &created, None);

        assert_eq!(command.program, "codex");
        assert!(
            command
                .args
                .contains(&"DRIGGSBY_CLIENT_KEY=dby_client_v1_secret".to_string())
        );
        assert!(!render_shell_command(&command).contains("CLIENT_GRANT"));
    }

    #[test]
    fn claude_code_installer_defaults_to_user_scope() {
        let created = test_grant();
        let command = build_installer_command(CliMcpClient::ClaudeCode, &created, None);

        assert!(
            command
                .args
                .windows(2)
                .any(|values| values == ["-s", "user"])
        );
    }

    fn test_grant() -> crate::broker::grants::CreatedClientGrant {
        crate::broker::grants::CreatedClientGrant {
            grant: crate::broker::grants::BrokerClientGrant {
                schema_version: 1,
                grant_id: "lc_id".to_string(),
                display_name: "Codex".to_string(),
                integration_id: Some("codex".to_string()),
                client_key_sha256: "hash".to_string(),
                created_at: "2026-04-13T00:00:00Z".to_string(),
                last_used_at: None,
                disconnected_at: None,
            },
            client_key: "dby_client_v1_secret".to_string(),
        }
    }
}
