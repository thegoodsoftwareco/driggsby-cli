pub const DRIGGSBY_CONNECT_COMMAND: &str = "npx driggsby@latest mcp connect";
pub const DRIGGSBY_STATUS_COMMAND: &str = "npx driggsby@latest status";
pub const DRIGGSBY_DISCONNECT_ALL_COMMAND: &str = "npx driggsby@latest mcp disconnect-all";
pub const DRIGGSBY_MCP_SERVER_COMMAND: &str = "npx -y driggsby@latest mcp-server";

pub fn build_reauthentication_required_message(detail: &str) -> String {
    format!("{detail}.\n\nNext:\n  {DRIGGSBY_CONNECT_COMMAND}")
}

pub fn build_broker_investigation_message(detail: &str) -> String {
    format!(
        "{detail}.\n\nCheck status:\n  {DRIGGSBY_STATUS_COMMAND}\n\nReconnect:\n  {DRIGGSBY_CONNECT_COMMAND}"
    )
}
