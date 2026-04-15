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
