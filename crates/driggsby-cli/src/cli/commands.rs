use std::io::{self, Write as _};

use anyhow::{Result, bail};

use crate::{
    broker::{
        client::{local_server_is_running, shutdown_broker},
        daemon::run_broker_daemon,
        installation::{clear_broker_installation, resolve_broker_status_for_display},
        local_lock::LocalStateLock,
        resolve_secret_store::resolve_secret_store_for_disconnect_all,
    },
    cli::{client_config_cleanup::remove_all_known_client_configs, format::format_status_text},
    runtime_paths::{RuntimePaths, ensure_runtime_directories},
};

fn flush_stdout() -> Result<()> {
    io::stdout().flush()?;
    Ok(())
}

pub async fn run_disconnect_all_command(runtime_paths: &RuntimePaths) -> Result<()> {
    ensure_runtime_directories(runtime_paths)?;
    println!("Disconnecting Driggsby...");
    flush_stdout()?;

    let clear_result = {
        let _disconnect_lock = LocalStateLock::acquire(runtime_paths)?;
        match resolve_secret_store_for_disconnect_all(runtime_paths) {
            Ok(resolved_store) => {
                let _ = shutdown_broker(runtime_paths, resolved_store.store.as_ref()).await;
                clear_broker_installation(runtime_paths, resolved_store.store.as_ref())
            }
            Err(error) => Err(error),
        }
    };
    println!();
    println!("MCP config cleanup:");
    remove_all_known_client_configs();

    if let Err(error) = clear_result {
        println!();
        bail!("{error}");
    }

    println!();
    println!("Driggsby disconnected.");
    println!();
    println!("Other clients may still have Driggsby configured. Remove manually.");
    println!();
    println!("Reconnect:");
    println!("  npx driggsby@latest mcp connect");
    Ok(())
}

pub async fn run_status_command(runtime_paths: &RuntimePaths) -> Result<()> {
    let local_server_running = local_server_is_running(runtime_paths).await;
    let status = resolve_broker_status_for_display(runtime_paths, None, local_server_running)?;
    print!("{}", format_status_text(&status));
    Ok(())
}

pub async fn run_cli_daemon_command(runtime_paths: &RuntimePaths) -> Result<()> {
    run_broker_daemon(runtime_paths).await
}
