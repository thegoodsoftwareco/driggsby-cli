use std::{path::Path, process::Stdio, time::Duration};

use anyhow::{Result, bail};
use tokio::{process::Command, time::sleep};

use crate::{
    runtime_paths::RuntimePaths,
    user_guidance::{build_broker_investigation_message, build_reauthentication_required_message},
};

use super::{
    client::{ping_broker, shutdown_broker},
    installation::read_broker_installation_with_secrets,
    local_lock::LocalStateLock,
    public_error::PublicBrokerError,
    secret_store::SecretStore,
};

const CURRENT_CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

pub async fn ensure_broker_running(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    current_exe: &Path,
) -> Result<()> {
    let _startup_lock = LocalStateLock::acquire(runtime_paths)?;

    if read_broker_installation_with_secrets(runtime_paths, secret_store)?.is_none() {
        return Err(
            PublicBrokerError::new(build_reauthentication_required_message(
                "The Driggsby CLI is not connected",
            ))
            .into(),
        );
    }

    if running_broker_matches_current_version(runtime_paths, secret_store).await? {
        return Ok(());
    }
    if ping_broker(runtime_paths, secret_store).await?.is_some() {
        let _ = shutdown_broker(runtime_paths, secret_store).await;
        wait_for_broker_shutdown(runtime_paths, secret_store, Duration::from_secs(2)).await?;
    }
    Command::new(current_exe)
        .arg("cli-daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if wait_for_broker(runtime_paths, secret_store, Duration::from_secs(4)).await? {
        return Ok(());
    }
    bail!(build_broker_investigation_message(
        "The local Driggsby CLI service did not start cleanly"
    ))
}

async fn wait_for_broker(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    timeout_window: Duration,
) -> Result<bool> {
    let deadline = tokio::time::Instant::now() + timeout_window;
    while tokio::time::Instant::now() < deadline {
        if running_broker_matches_current_version(runtime_paths, secret_store).await? {
            return Ok(true);
        }
        sleep(Duration::from_millis(100)).await;
    }
    Ok(false)
}

async fn wait_for_broker_shutdown(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    timeout_window: Duration,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + timeout_window;
    while tokio::time::Instant::now() < deadline {
        if ping_broker(runtime_paths, secret_store).await?.is_none() {
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }
    Ok(())
}

async fn running_broker_matches_current_version(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
) -> Result<bool> {
    let Some(ping) = ping_broker(runtime_paths, secret_store).await? else {
        return Ok(false);
    };
    Ok(ping.cli_version.as_deref() == Some(CURRENT_CLI_VERSION))
}
