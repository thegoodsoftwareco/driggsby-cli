use crate::{
    broker::types::{BrokerRemoteAccessState, BrokerStatus},
    user_guidance::DRIGGSBY_CONNECT_COMMAND,
};

pub fn format_status_text(status: &BrokerStatus) -> String {
    let remote_access_state = resolve_remote_access_state(status);
    let heading = resolve_heading(status, &remote_access_state);
    let explanation = resolve_explanation(status, &remote_access_state);
    let configuration_command = resolve_configuration_command(status, &remote_access_state);
    let recovery_command = resolve_recovery_command(status, &remote_access_state);

    let mut lines = vec![heading.to_string()];

    if let Some(explanation) = explanation {
        lines.push(String::new());
        lines.push(explanation);
    }

    if let Some(command) = configuration_command {
        lines.push(String::new());
        lines.push("Set up an MCP client:".to_string());
        lines.push(format!("  {command}"));
    }

    if let Some(command) = recovery_command {
        lines.push(String::new());
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

fn resolve_explanation(
    _status: &BrokerStatus,
    remote_access_state: &BrokerRemoteAccessState,
) -> Option<String> {
    match remote_access_state {
        BrokerRemoteAccessState::Ready => None,
        BrokerRemoteAccessState::NotConnected => Some("Sign in to connect Driggsby.".to_string()),
        BrokerRemoteAccessState::ReauthRequired => Some("Driggsby session expired.".to_string()),
        BrokerRemoteAccessState::TemporarilyUnavailable => {
            Some("Driggsby will reconnect automatically on next use.".to_string())
        }
    }
}

fn resolve_configuration_command(
    status: &BrokerStatus,
    remote_access_state: &BrokerRemoteAccessState,
) -> Option<String> {
    match remote_access_state {
        BrokerRemoteAccessState::Ready => Some(DRIGGSBY_CONNECT_COMMAND.to_string()),
        BrokerRemoteAccessState::TemporarilyUnavailable if status.remote_session.is_some() => {
            Some(DRIGGSBY_CONNECT_COMMAND.to_string())
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
            Some(DRIGGSBY_CONNECT_COMMAND.to_string())
        }
        BrokerRemoteAccessState::Ready | BrokerRemoteAccessState::TemporarilyUnavailable => None,
    }
}

fn resolve_remote_access_state(status: &BrokerStatus) -> BrokerRemoteAccessState {
    status
        .remote_access_state
        .clone()
        .unwrap_or(BrokerRemoteAccessState::NotConnected)
}

#[cfg(test)]
mod tests {
    use super::format_status_text;
    use crate::{
        broker::{
            session::BrokerRemoteSessionSummary,
            types::{BrokerRemoteAccessState, BrokerStatus},
        },
        user_guidance::DRIGGSBY_CONNECT_COMMAND,
    };

    fn ready_session() -> BrokerRemoteSessionSummary {
        BrokerRemoteSessionSummary {
            access_token_expires_at: "2026-04-10T03:15:54Z".to_string(),
            authenticated_at: "2026-04-10T02:15:54Z".to_string(),
            client_id: "client-123".to_string(),
            issuer: "https://app.driggsby.com".to_string(),
            redirect_uri: "http://127.0.0.1/callback".to_string(),
            resource: "https://app.driggsby.com/mcp".to_string(),
            scope: "driggsby.default".to_string(),
            token_type: "DPoP".to_string(),
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
        assert!(text.contains("Set up an MCP client:\n  npx driggsby@latest mcp setup"));
        assert!(!text.contains("Session:"));
        assert!(!text.contains("Local auth broker:"));
        assert!(!text.contains("Driggsby CLI"));
        assert!(!text.contains('`'));
    }

    #[test]
    fn disconnected_status_points_to_connect() {
        let text = format_status_text(&BrokerStatus {
            installed: false,
            broker_running: false,
            broker_id: None,
            dpop_thumbprint: None,
            remote_mcp_ready: false,
            remote_access_detail: None,
            remote_access_state: Some(BrokerRemoteAccessState::NotConnected),
            next_step_command: Some(DRIGGSBY_CONNECT_COMMAND.to_string()),
            remote_session: None,
            socket_path: "/tmp/cli.sock".to_string(),
        });

        assert!(text.starts_with("Not connected\n"));
        assert!(text.contains("Sign in to connect Driggsby."));
        assert!(text.contains("Next:\n  npx driggsby@latest mcp setup"));
        assert!(!text.contains('`'));
        assert!(!text.contains("Session:"));
        assert!(!text.contains("Local auth broker:"));
    }

    #[test]
    fn running_status_is_clean() {
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

        assert!(text.starts_with("Ready\n"));
        assert!(!text.contains("Session:"));
        assert!(!text.contains("Local auth broker:"));
        assert!(!text.contains("Next:"));
    }
}
