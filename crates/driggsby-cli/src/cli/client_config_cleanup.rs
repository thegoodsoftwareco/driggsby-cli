use std::process::Command;

use crate::cli::{
    desktop_mcp_config::remove_desktop_mcp_config,
    known_client::KnownClient,
    supported_mcp_config::{build_remover_command, render_shell_command},
};

pub(super) fn remove_known_client_configs(grants: &[crate::broker::grants::BrokerClientGrant]) {
    let mut removed_claude = false;
    let mut removed_claude_desktop = false;
    let mut removed_codex = false;
    for grant in grants {
        match grant.integration_id.as_deref() {
            Some("claude-code") if !removed_claude => {
                remove_known_client_config(KnownClient::ClaudeCode);
                removed_claude = true;
            }
            Some("claude-desktop") if !removed_claude_desktop => {
                remove_known_client_config(KnownClient::ClaudeDesktop);
                removed_claude_desktop = true;
            }
            Some("codex") if !removed_codex => {
                remove_known_client_config(KnownClient::Codex);
                removed_codex = true;
            }
            _ => {}
        }
    }
}

pub(super) fn remove_all_known_client_configs() {
    remove_known_client_config(KnownClient::ClaudeCode);
    remove_known_client_config(KnownClient::ClaudeDesktop);
    remove_known_client_config(KnownClient::Codex);
}

fn remove_known_client_config(client: KnownClient) {
    if client == KnownClient::ClaudeCode {
        remove_claude_code_configs();
        return;
    }
    if let Some(desktop_client) = client.desktop_mcp_client() {
        match remove_desktop_mcp_config(desktop_client) {
            Ok(true) => print_config_cleanup_row(client, "removed"),
            Ok(false) => print_config_cleanup_row(client, "already clear"),
            Err(_) => print_config_cleanup_row(client, "remove manually"),
        }
        return;
    }

    let Some(cli_client) = client.cli_mcp_client() else {
        return;
    };
    let remover = build_remover_command(cli_client);
    match Command::new(&remover.program).args(&remover.args).output() {
        Ok(output) if output.status.success() => print_config_cleanup_row(client, "removed"),
        Ok(output) if command_reports_missing_config(&output) => {
            print_config_cleanup_row(client, "already clear");
        }
        Ok(_) | Err(_) => {
            print_config_cleanup_row(client, "remove manually");
            println!("    {}", render_shell_command(&remover));
        }
    }
}

fn remove_claude_code_configs() {
    let mut removed_any = false;
    let mut failed_any = false;
    for scope in ["local", "user"] {
        match Command::new("claude")
            .args(["mcp", "remove", "driggsby", "-s", scope])
            .output()
        {
            Ok(output) if output.status.success() => removed_any = true,
            Ok(output) if command_reports_missing_config(&output) => {}
            Ok(_) | Err(_) => failed_any = true,
        }
    }
    match (removed_any, failed_any) {
        (_, true) => {
            print_config_cleanup_row(KnownClient::ClaudeCode, "remove manually");
            println!("    claude mcp remove driggsby -s local");
            println!("    claude mcp remove driggsby -s user");
        }
        (true, false) => print_config_cleanup_row(KnownClient::ClaudeCode, "removed"),
        (false, false) => print_config_cleanup_row(KnownClient::ClaudeCode, "already clear"),
    }
}

fn print_config_cleanup_row(client: KnownClient, status: &str) {
    println!("  {:<16} {}", client.display_name(), status);
}

fn command_reports_missing_config(output: &std::process::Output) -> bool {
    command_output_contains(output, "No MCP server found")
}

fn command_output_contains(output: &std::process::Output, needle: &str) -> bool {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    stdout.contains(needle) || stderr.contains(needle)
}
