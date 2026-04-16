use crate::cli::{McpScope, known_client::KnownClient};

#[test]
fn parses_supported_clients() -> anyhow::Result<()> {
    let cases = [
        ("claude-code", KnownClient::ClaudeCode),
        ("codex", KnownClient::Codex),
    ];
    for (input, expected) in cases {
        assert_eq!(super::parse_client(input)?, expected);
    }
    Ok(())
}

#[test]
fn rejects_unsupported_clients() {
    for value in ["   ", "raycast", "claude-desktop"] {
        assert!(super::parse_client(value).is_err());
    }
}

#[test]
fn mcp_scope_is_only_supported_for_claude_code() {
    let scope = Some(McpScope::User);
    assert!(super::validate_mcp_scope(KnownClient::ClaudeCode, scope).is_ok());
    assert!(super::validate_mcp_scope(KnownClient::Codex, scope).is_err());
}

#[test]
fn next_steps_are_client_specific() {
    assert_eq!(
        super::next_step_lines(KnownClient::ClaudeCode),
        ["  Open Claude Code, run /mcp, and authenticate Driggsby to get started."]
    );
    assert_eq!(
        super::next_step_lines(KnownClient::Codex),
        [
            "  Complete the Driggsby sign-in in the browser window opened by Codex.",
            "  If no browser window opened, run:",
            "    codex mcp login driggsby",
        ]
    );
}

#[test]
fn only_codex_streams_client_setup_output() {
    assert!(!super::stream_config_output(KnownClient::ClaudeCode));
    assert!(super::stream_config_output(KnownClient::Codex));
}

#[cfg(unix)]
#[tokio::test]
async fn streaming_config_command_still_captures_output() -> anyhow::Result<()> {
    let command = super::McpConfigCommand {
        program: "sh".to_string(),
        args: vec![
            "-c".to_string(),
            "printf 'already exists'; printf 'No MCP server found' >&2".to_string(),
        ],
    };

    let output = super::run_config_command_inner(&command, true).await?;

    assert!(output.status.success());
    assert!(super::command_reports_existing_config(&output));
    assert!(super::command_reports_missing_config(&output));
    Ok(())
}
