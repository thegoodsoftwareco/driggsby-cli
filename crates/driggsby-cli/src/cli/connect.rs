use std::{
    io::{self, IsTerminal, Write as _},
    process::Command as StdCommand,
    time::Duration,
};

use anyhow::{Result, bail};
use tokio::process::Command as TokioCommand;

use crate::{
    broker::{
        grants::{
            CLIENT_KEY_ENV, CreatedClientGrant, create_client_grant, disconnect_client_grant,
            disconnect_other_grants_for_integration, list_client_grants,
        },
        installation::read_broker_metadata,
        local_lock::LocalStateLock,
        secret_store::SecretStore,
    },
    cli::McpScope,
    cli::client_id,
    cli::connect_session::ensure_recent_cli_session,
    cli::desktop_mcp_config::{install_desktop_mcp_config, remove_desktop_mcp_config},
    cli::known_client::KnownClient,
    cli::supported_mcp_config::{
        build_installer_command, build_remover_command, render_shell_command,
    },
    runtime_paths::{RuntimePaths, ensure_runtime_directories},
};

type BrokerClientGrant = crate::broker::grants::BrokerClientGrant;
const CLIENT_CONFIG_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConnectTarget {
    Known(KnownClient),
    Other(String),
}

impl ConnectTarget {
    fn client_id(&self) -> &str {
        match self {
            Self::Known(client) => client.integration_id(),
            Self::Other(client_id) => client_id.as_str(),
        }
    }

    fn display_name(&self) -> String {
        match self {
            Self::Known(client) => client.display_name().to_string(),
            Self::Other(client_id) => client_id.to_string(),
        }
    }

    fn integration_id(&self) -> Option<&str> {
        match self {
            Self::Known(client) => Some(client.integration_id()),
            Self::Other(client_id) => Some(client_id.as_str()),
        }
    }
}

pub async fn run_connect_command(
    runtime_paths: &RuntimePaths,
    requested_client: Option<String>,
    no_auto_add_mcp_config: bool,
    mcp_scope: Option<McpScope>,
) -> Result<()> {
    let target = resolve_connect_target(requested_client)?;
    validate_connect_target(&target)?;
    validate_mcp_scope(&target, mcp_scope)?;
    ensure_runtime_directories(runtime_paths)?;
    let display_name = target.display_name();
    println!("Connecting Driggsby to {}...", display_name);
    flush_stdout()?;

    let resolved_store = crate::broker::resolve_secret_store::resolve_secret_store(runtime_paths)?;

    if let ConnectTarget::Known(client) = &target
        && !no_auto_add_mcp_config
    {
        let _connect_lock = LocalStateLock::acquire(runtime_paths)?;
        let broker_id =
            ensure_recent_cli_session(runtime_paths, resolved_store.store.as_ref()).await?;
        let created = create_client_grant(
            resolved_store.store.as_ref(),
            &broker_id,
            &display_name,
            target.integration_id(),
        )?;
        println!();
        if install_known_client(*client, &created, mcp_scope).await?
            && let Some(integration_id) = target.integration_id()
        {
            disconnect_other_grants_for_integration(
                resolved_store.store.as_ref(),
                &broker_id,
                integration_id,
                &created.grant.grant_id,
            )?;
        }
        return Ok(());
    }

    let created = {
        let _connect_lock = LocalStateLock::acquire(runtime_paths)?;
        let broker_id =
            ensure_recent_cli_session(runtime_paths, resolved_store.store.as_ref()).await?;
        create_client_grant(
            resolved_store.store.as_ref(),
            &broker_id,
            &display_name,
            target.integration_id(),
        )?
    };

    println!();
    print_one_time_mcp_config_with_secret(&created);
    Ok(())
}

pub async fn run_clients_command(
    runtime_paths: &RuntimePaths,
    command: super::ClientCommand,
) -> Result<()> {
    if matches!(command, super::ClientCommand::DisconnectAll) {
        return crate::cli::commands::run_disconnect_all_command(runtime_paths).await;
    }
    let disconnect_client = match &command {
        super::ClientCommand::Disconnect { client } => Some(parse_client_selector(client)?),
        super::ClientCommand::List | super::ClientCommand::DisconnectAll => None,
    };

    let Some(metadata) = read_broker_metadata(runtime_paths)? else {
        println!("No connected MCP clients.");
        return Ok(());
    };
    let resolved_store = crate::broker::resolve_secret_store::resolve_secret_store(runtime_paths)?;

    match command {
        super::ClientCommand::Disconnect { .. } => {
            let client =
                disconnect_client.ok_or_else(|| anyhow::anyhow!("Client ID is required."))?;
            let disconnected = {
                let _client_mutation_lock = LocalStateLock::acquire(runtime_paths)?;
                disconnect_client_grant(
                    resolved_store.store.as_ref(),
                    &metadata.broker_id,
                    &client,
                )?
            };
            if disconnected.is_empty() {
                println!("No matching connected MCP client found for {client}.");
            } else {
                println!("Disconnected {client}.");
                remove_known_client_configs(&disconnected);
                if disconnected.iter().any(|grant| {
                    !grant
                        .integration_id
                        .as_deref()
                        .is_some_and(is_known_client_id)
                }) {
                    println!();
                    println!("Driggsby cannot remove MCP config for this client automatically.");
                    println!("Remove Driggsby manually from that client's MCP settings.");
                }
            }
        }
        super::ClientCommand::List => {
            print_client_grants(resolved_store.store.as_ref(), &metadata.broker_id)?;
        }
        super::ClientCommand::DisconnectAll => {}
    }
    Ok(())
}

fn resolve_connect_target(requested_client: Option<String>) -> Result<ConnectTarget> {
    match requested_client {
        Some(value) => Ok(parse_connect_target(&value)),
        None => prompt_for_connect_target(),
    }
}

fn validate_connect_target(target: &ConnectTarget) -> Result<()> {
    if target.client_id().trim().is_empty() {
        bail!("Client ID is required.");
    }
    if let ConnectTarget::Other(client_id) = target
        && !client_id::is_valid(client_id)
    {
        bail!(
            "Client ID may use only letters, numbers, and hyphens.\n\nExamples:\n  raycast\n  my-mcp-client"
        );
    }
    if matches!(target, ConnectTarget::Known(KnownClient::ClaudeDesktop))
        && !cfg!(target_os = "macos")
    {
        bail!("Claude Desktop automatic setup is supported only on macOS in this release.");
    }
    Ok(())
}

fn validate_mcp_scope(target: &ConnectTarget, mcp_scope: Option<McpScope>) -> Result<()> {
    if mcp_scope.is_none() {
        return Ok(());
    }
    if matches!(target, ConnectTarget::Known(KnownClient::ClaudeCode)) {
        return Ok(());
    }
    bail!(
        "--mcp-scope is currently supported only for Claude Code. Codex does not expose an MCP scope flag, and Claude Desktop uses its app-level config file."
    );
}

fn parse_connect_target(value: &str) -> ConnectTarget {
    let canonical = client_id::canonicalize(value);
    if let Some(client) = KnownClient::from_client_id(&canonical) {
        return ConnectTarget::Known(client);
    }
    ConnectTarget::Other(canonical)
}

fn parse_client_selector(value: &str) -> Result<String> {
    let canonical = client_id::canonicalize(value);
    if canonical.trim().is_empty() {
        bail!("Client ID is required.");
    }
    if !client_id::is_valid(&canonical) {
        bail!(
            "Client ID may use only letters, numbers, and hyphens.\n\nExamples:\n  raycast\n  my-mcp-client"
        );
    }
    Ok(canonical)
}

fn prompt_for_connect_target() -> Result<ConnectTarget> {
    if !io::stdin().is_terminal() {
        bail!("Pass a client name.\n\nExample:\n  npx driggsby@latest mcp connect claude-code");
    }

    println!("Which client are you setting up?");
    println!();
    println!("  1. Claude Code");
    println!("  2. Claude Desktop");
    println!("  3. Codex");
    println!("  4. Other MCP client");
    println!();
    print!("Choose 1-4: ");
    flush_stdout()?;

    let choice = read_trimmed_line()?;
    match choice.as_str() {
        "1" => Ok(ConnectTarget::Known(KnownClient::ClaudeCode)),
        "2" => Ok(ConnectTarget::Known(KnownClient::ClaudeDesktop)),
        "3" => Ok(ConnectTarget::Known(KnownClient::Codex)),
        "4" => prompt_for_other_client_name(),
        _ => bail!("Choose 1, 2, 3, or 4."),
    }
}

fn prompt_for_other_client_name() -> Result<ConnectTarget> {
    println!("Use letters, numbers, and hyphens.");
    print!("Client ID: ");
    flush_stdout()?;
    let name = read_trimmed_line()?;
    if name.is_empty() {
        bail!("Client ID is required.");
    }
    Ok(parse_connect_target(&name))
}

fn read_trimmed_line() -> Result<String> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

async fn install_known_client(
    client: KnownClient,
    created: &CreatedClientGrant,
    mcp_scope: Option<McpScope>,
) -> Result<bool> {
    let Some(cli_client) = client.cli_mcp_client() else {
        return install_desktop_client(client, created);
    };
    let installer = build_installer_command(cli_client, created, mcp_scope);
    println!("Adding Driggsby to {}...", client.display_name());
    flush_stdout()?;

    let mut command = TokioCommand::new(&installer.program);
    command.args(&installer.args).kill_on_drop(true);
    let output = tokio::time::timeout(CLIENT_CONFIG_COMMAND_TIMEOUT, command.output()).await;
    match output {
        Ok(Ok(output)) if output.status.success() => {
            println!("{} is connected.", client.display_name());
            println!();
            println!("Connected MCP client:");
            println!("  {}", client.display_name());
            Ok(true)
        }
        Ok(Ok(_)) => {
            print_auto_setup_failure(
                client,
                &format!("{} mcp add returned an error.", installer.program),
            );
            print_one_time_mcp_config_with_secret(created);
            Ok(false)
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            print_auto_setup_failure(client, &format!("{} was not found.", installer.program));
            print_one_time_mcp_config_with_secret(created);
            Ok(false)
        }
        Ok(Err(_)) => {
            print_auto_setup_failure(client, &format!("Could not run {}.", installer.program));
            print_one_time_mcp_config_with_secret(created);
            Ok(false)
        }
        Err(_) => {
            print_auto_setup_failure(client, &format!("{} mcp add timed out.", installer.program));
            print_one_time_mcp_config_with_secret(created);
            Ok(false)
        }
    }
}

fn install_desktop_client(client: KnownClient, created: &CreatedClientGrant) -> Result<bool> {
    let Some(desktop_client) = client.desktop_mcp_client() else {
        return Ok(false);
    };
    println!("Adding Driggsby to {}...", client.display_name());
    flush_stdout()?;

    match install_desktop_mcp_config(desktop_client, created) {
        Ok(()) => {
            println!("{} is connected.", client.display_name());
            println!();
            println!("Connected MCP client:");
            println!("  {}", client.display_name());
            Ok(true)
        }
        Err(error) => {
            print_auto_setup_failure(client, &first_error_line(&error));
            print_one_time_mcp_config_with_secret(created);
            Ok(false)
        }
    }
}

fn print_auto_setup_failure(client: KnownClient, reason: &str) {
    println!("Automatic MCP config update failed.");
    println!(
        "Could not add Driggsby to {} automatically.",
        client.display_name()
    );
    println!("Reason:");
    println!("  {reason}");
    println!();
    println!("Driggsby created this client's local key, but the MCP client still needs config.");
    println!();
    println!("Next:");
    println!("  Add the MCP config below manually.");
    println!("  Or fix the client install and run:");
    println!(
        "    npx driggsby@latest mcp connect {}",
        client.integration_id()
    );
    println!();
}

fn first_error_line(error: &anyhow::Error) -> String {
    error
        .to_string()
        .lines()
        .next()
        .unwrap_or("Automatic setup failed.")
        .to_string()
}

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
    if let Some(desktop_client) = client.desktop_mcp_client() {
        match remove_desktop_mcp_config(desktop_client) {
            Ok(true) => println!("Removed Driggsby from {}.", client.display_name()),
            Ok(false) => println!("No Driggsby MCP config found in {}.", client.display_name()),
            Err(_) => {
                println!(
                    "Could not remove Driggsby from {} automatically.",
                    client.display_name()
                );
                println!("Remove Driggsby from that client's MCP settings.");
            }
        }
        return;
    }

    let Some(cli_client) = client.cli_mcp_client() else {
        return;
    };
    let remover = build_remover_command(cli_client);
    let output = StdCommand::new(&remover.program)
        .args(&remover.args)
        .output();
    match output {
        Ok(output) if output.status.success() => {
            println!("Removed Driggsby from {}.", client.display_name());
        }
        Ok(_) | Err(_) => {
            println!(
                "Could not remove Driggsby from {} automatically.",
                client.display_name()
            );
            println!("Run this command to remove the MCP config:");
            println!("  {}", render_shell_command(&remover));
        }
    }
}

fn print_one_time_mcp_config_with_secret(created: &CreatedClientGrant) {
    println!("Add this MCP server to your client:");
    println!();
    println!("Command:");
    println!("  npx -y driggsby@latest mcp-server");
    println!();
    println!("Environment:");
    println!("  {}={}", CLIENT_KEY_ENV, created.client_key);
    println!();
    println!("Treat DRIGGSBY_CLIENT_KEY like an API key.");
    println!("It is shown once and cannot be viewed again.");
    println!("This client key is active until you disconnect it.");
    println!();
    println!("Disconnect this client with:");
    println!(
        "  npx driggsby@latest mcp clients disconnect {}",
        client_id_for_grant(&created.grant)
    );
}

fn print_client_grants(secret_store: &dyn SecretStore, broker_id: &str) -> Result<()> {
    let grants = list_client_grants(secret_store, broker_id)?;
    if grants.is_empty() {
        println!("No connected MCP clients.");
        return Ok(());
    }
    println!("Connected MCP clients:");
    for grant in grants {
        println!("  {}", client_id_for_grant(&grant));
    }
    Ok(())
}

fn client_id_for_grant(grant: &BrokerClientGrant) -> &str {
    grant
        .integration_id
        .as_deref()
        .unwrap_or(grant.display_name.as_str())
}

fn is_known_client_id(client_id: &str) -> bool {
    KnownClient::from_client_id(client_id).is_some()
}

fn flush_stdout() -> Result<()> {
    io::stdout().flush()?;
    Ok(())
}

#[cfg(test)]
mod tests;
