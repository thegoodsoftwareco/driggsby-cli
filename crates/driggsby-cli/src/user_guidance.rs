pub const DRIGGSBY_LOGIN_COMMAND: &str = "npx driggsby@latest login";
pub const DRIGGSBY_STATUS_COMMAND: &str = "npx driggsby@latest status";
pub const DRIGGSBY_LOGOUT_COMMAND: &str = "npx driggsby@latest logout";
pub const DRIGGSBY_MCP_SERVER_COMMAND: &str = "npx -y driggsby@latest mcp-server";

pub fn build_reauthentication_required_message(detail: &str) -> String {
    format!("{detail}. Reconnect Driggsby by running {DRIGGSBY_LOGIN_COMMAND}.")
}

pub fn build_broker_investigation_message(detail: &str) -> String {
    format!(
        "{detail}. Check CLI readiness by running {DRIGGSBY_STATUS_COMMAND}. If authentication expired, reconnect with {DRIGGSBY_LOGIN_COMMAND}."
    )
}
