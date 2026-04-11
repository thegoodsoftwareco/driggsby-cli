use crate::{
    broker::types::{BrokerRemoteAccessState, BrokerStatus},
    user_guidance::{DRIGGSBY_LOGIN_COMMAND, DRIGGSBY_MCP_SERVER_COMMAND},
};

pub fn format_status_text(status: &BrokerStatus) -> String {
    let remote_access_state = resolve_remote_access_state(status);
    let heading = resolve_heading(status, &remote_access_state);
    let explanation = resolve_explanation(status, &remote_access_state);
    let configuration_command = resolve_configuration_command(status, &remote_access_state);
    let recovery_command = resolve_recovery_command(status, &remote_access_state);
    let details = resolve_details(status, &remote_access_state);

    let mut lines = vec![heading.to_string(), String::new()];

    for detail in &details {
        lines.push(detail.clone());
    }

    if !details.is_empty() && explanation.is_some() {
        lines.push(String::new());
    }

    let mut has_body = false;

    if let Some(explanation) = explanation {
        lines.push(explanation);
        has_body = true;
    }

    if let Some(command) = configuration_command {
        if has_body || !details.is_empty() {
            lines.push(String::new());
        }
        lines.push("Configure your MCP client with:".to_string());
        lines.push(format!("  {command}"));
        has_body = true;
    }

    if let Some(command) = recovery_command {
        if has_body || !details.is_empty() {
            lines.push(String::new());
        }
        lines.push("Next:".to_string());
        lines.push(format!("  {command}"));
    }

    let mut rendered = lines.join("\n");
    rendered.push('\n');
    rendered
}

fn resolve_heading(
    status: &BrokerStatus,
    remote_access_state: &BrokerRemoteAccessState,
) -> &'static str {
    match remote_access_state {
        BrokerRemoteAccessState::Ready => "Ready",
        BrokerRemoteAccessState::NotConnected => "Not connected",
        BrokerRemoteAccessState::ReauthRequired => "Reconnect required",
        BrokerRemoteAccessState::TemporarilyUnavailable if status.remote_session.is_some() => {
            "Almost ready"
        }
        BrokerRemoteAccessState::TemporarilyUnavailable => "Unavailable",
    }
}

fn resolve_details(
    status: &BrokerStatus,
    remote_access_state: &BrokerRemoteAccessState,
) -> Vec<String> {
    let mut lines = Vec::new();

    if !matches!(remote_access_state, BrokerRemoteAccessState::NotConnected) {
        lines.push(format!(
            "Session: {}",
            match remote_access_state {
                BrokerRemoteAccessState::Ready => "connected",
                BrokerRemoteAccessState::NotConnected => "not connected",
                BrokerRemoteAccessState::ReauthRequired => "reconnect required",
                BrokerRemoteAccessState::TemporarilyUnavailable => "refresh needed",
            }
        ));
    }

    if status.installed {
        lines.push(format!(
            "Local auth broker: {}",
            match remote_access_state {
                BrokerRemoteAccessState::Ready if status.broker_running => "running",
                BrokerRemoteAccessState::Ready => "waiting for client launch",
                _ if status.broker_running => "running",
                _ => "not running",
            }
        ));
    }

    lines
}

fn resolve_explanation(
    status: &BrokerStatus,
    remote_access_state: &BrokerRemoteAccessState,
) -> Option<String> {
    match remote_access_state {
        BrokerRemoteAccessState::Ready if status.broker_running => {
            Some("This CLI is ready to serve MCP requests.".to_string())
        }
        BrokerRemoteAccessState::Ready => None,
        BrokerRemoteAccessState::NotConnected => Some(
            "Sign in is required before this CLI can serve MCP requests.".to_string(),
        ),
        BrokerRemoteAccessState::ReauthRequired => Some(
            "The saved session is no longer valid, so this CLI cannot serve MCP requests yet."
                .to_string(),
        ),
        BrokerRemoteAccessState::TemporarilyUnavailable if status.remote_session.is_some() => {
            Some(
                "The saved session will be refreshed automatically the next time the MCP server starts."
                    .to_string(),
            )
        }
        BrokerRemoteAccessState::TemporarilyUnavailable => {
            Some("This CLI is not ready to serve MCP requests yet.".to_string())
        }
    }
}

fn resolve_configuration_command(
    status: &BrokerStatus,
    remote_access_state: &BrokerRemoteAccessState,
) -> Option<String> {
    match remote_access_state {
        BrokerRemoteAccessState::Ready => Some(DRIGGSBY_MCP_SERVER_COMMAND.to_string()),
        BrokerRemoteAccessState::TemporarilyUnavailable if status.remote_session.is_some() => {
            Some(DRIGGSBY_MCP_SERVER_COMMAND.to_string())
        }
        BrokerRemoteAccessState::NotConnected
        | BrokerRemoteAccessState::ReauthRequired
        | BrokerRemoteAccessState::TemporarilyUnavailable => None,
    }
}

fn resolve_recovery_command(
    _status: &BrokerStatus,
    remote_access_state: &BrokerRemoteAccessState,
) -> Option<String> {
    match remote_access_state {
        BrokerRemoteAccessState::NotConnected | BrokerRemoteAccessState::ReauthRequired => {
            Some(DRIGGSBY_LOGIN_COMMAND.to_string())
        }
        BrokerRemoteAccessState::Ready | BrokerRemoteAccessState::TemporarilyUnavailable => None,
    }
}

fn resolve_remote_access_state(status: &BrokerStatus) -> BrokerRemoteAccessState {
    status
        .remote_access_state
        .clone()
        .unwrap_or_else(|| infer_legacy_remote_access_state(status))
}

fn infer_legacy_remote_access_state(status: &BrokerStatus) -> BrokerRemoteAccessState {
    if status.remote_mcp_ready {
        return BrokerRemoteAccessState::Ready;
    }
    if status.remote_session.is_some() {
        return BrokerRemoteAccessState::TemporarilyUnavailable;
    }
    BrokerRemoteAccessState::NotConnected
}

#[cfg(test)]
mod tests {
    use super::format_status_text;
    use crate::{
        broker::{
            session::BrokerRemoteSessionSummary,
            types::{BrokerRemoteAccessState, BrokerStatus},
        },
        user_guidance::DRIGGSBY_LOGIN_COMMAND,
    };

    fn ready_session() -> BrokerRemoteSessionSummary {
        BrokerRemoteSessionSummary {
            access_token_expires_at: "2026-04-10T03:15:54Z".to_string(),
            authenticated_at: "2026-04-10T02:15:54Z".to_string(),
            client_id: "client-123".to_string(),
            issuer: "https://app.driggsby.com".to_string(),
            resource: "https://app.driggsby.com/mcp".to_string(),
            scope: "driggsby.default".to_string(),
        }
    }

    #[test]
    fn ready_but_waiting_for_client_launch_reads_as_expected() {
        let text = format_status_text(&BrokerStatus {
            installed: true,
            broker_running: false,
            broker_id: None,
            dpop_thumbprint: None,
            remote_mcp_ready: true,
            remote_access_detail: None,
            remote_access_state: Some(BrokerRemoteAccessState::Ready),
            next_step_command: None,
            remote_session: Some(ready_session()),
            socket_path: "/tmp/cli.sock".to_string(),
        });

        assert!(text.starts_with("Ready\n"));
        assert!(text.contains("Session: connected"));
        assert!(text.contains("Local auth broker: waiting for client launch"));
        assert!(
            text.contains("Configure your MCP client with:\n  npx -y driggsby@latest mcp-server")
        );
        assert!(!text.contains("Driggsby CLI"));
        assert!(!text.contains("Access token expires"));
        assert!(!text.contains("This is normal."));
    }

    #[test]
    fn disconnected_status_points_to_login() {
        let text = format_status_text(&BrokerStatus {
            installed: false,
            broker_running: false,
            broker_id: None,
            dpop_thumbprint: None,
            remote_mcp_ready: false,
            remote_access_detail: None,
            remote_access_state: Some(BrokerRemoteAccessState::NotConnected),
            next_step_command: Some(DRIGGSBY_LOGIN_COMMAND.to_string()),
            remote_session: None,
            socket_path: "/tmp/cli.sock".to_string(),
        });

        assert!(text.starts_with("Not connected\n"));
        assert!(text.contains("Sign in is required before this CLI can serve MCP requests."));
        assert!(text.contains("Next:\n  npx driggsby@latest login"));
        assert!(!text.contains('`'));
        assert!(!text.contains("Session:"));
        assert!(!text.contains("Local auth broker:"));
    }

    #[test]
    fn running_status_drops_next_step() {
        let text = format_status_text(&BrokerStatus {
            installed: true,
            broker_running: true,
            broker_id: None,
            dpop_thumbprint: None,
            remote_mcp_ready: true,
            remote_access_detail: None,
            remote_access_state: Some(BrokerRemoteAccessState::Ready),
            next_step_command: None,
            remote_session: Some(ready_session()),
            socket_path: "/tmp/cli.sock".to_string(),
        });

        assert!(text.contains("Local auth broker: running"));
        assert!(text.contains("This CLI is ready to serve MCP requests."));
        assert!(!text.contains("Next:"));
    }
}
