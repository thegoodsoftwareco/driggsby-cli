use std::{
    io::{self, IsTerminal, Write as _},
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
    cli::client_config_cleanup::remove_known_client_configs,
    cli::client_id,
    cli::connect_session::ensure_recent_cli_session,
    cli::desktop_mcp_config::install_desktop_mcp_config,
    cli::known_client::KnownClient,
    cli::supported_mcp_config::{
        McpConfigCommand, build_installer_command, build_scoped_remover_command,
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
    command: super::McpClientAction,
) -> Result<()> {
    if matches!(command, super::McpClientAction::DisconnectAll) {
        return crate::cli::commands::run_disconnect_all_command(runtime_paths).await;
    }
    let disconnect_client = match &command {
        super::McpClientAction::Disconnect { client } => {
            Some(parse_client_selector(client.as_deref())?)
        }
        super::McpClientAction::List | super::McpClientAction::DisconnectAll => None,
    };

    let Some(metadata) = read_broker_metadata(runtime_paths)? else {
        println!("No connected MCP clients.");
        return Ok(());
    };
    let resolved_store = crate::broker::resolve_secret_store::resolve_secret_store(runtime_paths)?;

    match command {
        super::McpClientAction::Disconnect { .. } => {
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
                    println!("Remove Driggsby from this client's MCP settings manually.");
                }
            }
        }
        super::McpClientAction::List => {
            print_client_grants(resolved_store.store.as_ref(), &metadata.broker_id)?;
        }
        super::McpClientAction::DisconnectAll => {}
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

fn parse_client_selector(value: Option<&str>) -> Result<String> {
    let Some(value) = value else {
        bail!(
            "Client ID is required.\n\nRun this command to see connected clients:\n  npx driggsby@latest mcp list"
        );
    };
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

    let output = run_config_command(&installer).await;
    match output {
        Ok(Ok(output)) if output.status.success() => {
            println!("{} is connected.", client.display_name());
            Ok(true)
        }
        Ok(Ok(output)) if command_reports_existing_config(&output) => {
            reinstall_existing_known_client(client, created, mcp_scope).await
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

async fn reinstall_existing_known_client(
    client: KnownClient,
    created: &CreatedClientGrant,
    mcp_scope: Option<McpScope>,
) -> Result<bool> {
    let Some(cli_client) = client.cli_mcp_client() else {
        return Ok(false);
    };
    let remover = build_scoped_remover_command(cli_client, mcp_scope.or(Some(McpScope::User)));
    match run_config_command(&remover).await {
        Ok(Ok(output)) if output.status.success() || command_reports_missing_config(&output) => {
            let installer = build_installer_command(cli_client, created, mcp_scope);
            match run_config_command(&installer).await {
                Ok(Ok(output)) if output.status.success() => {
                    println!("{} is connected.", client.display_name());
                    Ok(true)
                }
                _ => {
                    print_auto_setup_failure(client, "could not update existing MCP config.");
                    print_one_time_mcp_config_with_secret(created);
                    Ok(false)
                }
            }
        }
        _ => {
            print_auto_setup_failure(client, "could not replace existing MCP config.");
            print_one_time_mcp_config_with_secret(created);
            Ok(false)
        }
    }
}

async fn run_config_command(
    command: &McpConfigCommand,
) -> Result<std::io::Result<std::process::Output>, tokio::time::error::Elapsed> {
    let mut process = TokioCommand::new(&command.program);
    process.args(&command.args).kill_on_drop(true);
    tokio::time::timeout(CLIENT_CONFIG_COMMAND_TIMEOUT, process.output()).await
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
    println!("Auto-setup failed for {}: {reason}", client.display_name());
    println!();
    println!("Add the MCP config below manually, or fix the install and rerun:");
    println!(
        "  npx driggsby@latest mcp connect {}",
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

fn command_reports_existing_config(output: &std::process::Output) -> bool {
    command_output_contains(output, "already exists")
}

fn command_reports_missing_config(output: &std::process::Output) -> bool {
    command_output_contains(output, "No MCP server found")
}

fn command_output_contains(output: &std::process::Output, needle: &str) -> bool {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    stdout.contains(needle) || stderr.contains(needle)
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
    println!("This key is shown once - treat it like an API key.");
    println!();
    println!("Disconnect:");
    println!(
        "  npx driggsby@latest mcp disconnect {}",
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
    Ok(io::stdout().flush()?)
}

#[cfg(test)]
mod tests;
