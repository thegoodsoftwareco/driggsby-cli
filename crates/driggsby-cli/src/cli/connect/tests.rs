use super::{ConnectTarget, KnownClient, parse_connect_target, validate_connect_target};
use crate::cli::McpScope;

#[test]
fn parses_known_and_other_connect_targets() {
    let cases = [
        ("claude-code", ConnectTarget::Known(KnownClient::ClaudeCode)),
        (
            "claude-desktop",
            ConnectTarget::Known(KnownClient::ClaudeDesktop),
        ),
        ("codex", ConnectTarget::Known(KnownClient::Codex)),
    ];
    for (input, expected) in cases {
        assert_eq!(parse_connect_target(input), expected);
    }
    assert_eq!(
        parse_connect_target("Raycast"),
        ConnectTarget::Other("raycast".to_string())
    );
}

#[test]
fn rejects_invalid_client_ids() {
    for value in ["   ", "raycast client", "raycast_client"] {
        let target = parse_connect_target(value);
        assert!(validate_connect_target(&target).is_err());
    }
}

#[test]
fn parses_disconnect_client_selector_like_connect_client_id() -> anyhow::Result<()> {
    assert_eq!(super::parse_client_selector("Raycast")?, "raycast");
    assert_eq!(super::parse_client_selector("codex")?, "codex");
    assert_eq!(
        super::parse_client_selector("claude-desktop")?,
        "claude-desktop"
    );
    assert!(super::parse_client_selector("raycast client").is_err());
    Ok(())
}

#[test]
fn known_client_labels_are_human_and_other_labels_are_canonical_ids() {
    assert_eq!(
        parse_connect_target("claude-code").display_name(),
        "Claude Code"
    );
    assert_eq!(parse_connect_target("Raycast").display_name(), "raycast");
}

#[test]
fn mcp_scope_is_only_supported_for_claude_code() {
    let scope = Some(McpScope::User);
    assert!(
        super::validate_mcp_scope(&ConnectTarget::Known(KnownClient::ClaudeCode), scope).is_ok()
    );
    assert!(super::validate_mcp_scope(&ConnectTarget::Known(KnownClient::Codex), scope).is_err());
}
