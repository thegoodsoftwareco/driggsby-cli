use std::io::{self, Write as _};

use anyhow::Result;

use crate::{
    auth::login::login_broker,
    broker::{
        client::{local_server_is_running, shutdown_broker},
        daemon::run_broker_daemon,
        installation::{clear_broker_installation, resolve_broker_status_for_display},
        resolve_secret_store::{resolve_secret_store, resolve_secret_store_for_logout},
    },
    cli::format::format_status_text,
    runtime_paths::{RuntimePaths, ensure_runtime_directories},
    user_guidance::DRIGGSBY_MCP_SERVER_COMMAND,
};

pub async fn run_login_command(runtime_paths: &RuntimePaths) -> Result<()> {
    ensure_runtime_directories(runtime_paths)?;
    println!("Preparing Driggsby sign-in...");
    flush_stdout()?;

    let resolved_secret_store = resolve_secret_store(runtime_paths)?;
    if let Some(notice) = &resolved_secret_store.notice {
        println!("{notice}");
    }

    login_broker(
        runtime_paths,
        resolved_secret_store.store.as_ref(),
        print_manual_sign_in_url,
    )
    .await?;

    println!("Connected successfully.");
    println!();
    println!("Configure your MCP client with:");
    println!("  {DRIGGSBY_MCP_SERVER_COMMAND}");
    Ok(())
}

fn print_manual_sign_in_url(sign_in_url: &str) -> Result<()> {
    println!("Your browser did not open automatically.");
    println!("Open this URL to finish connecting Driggsby:");
    println!("{sign_in_url}");
    println!();
    flush_stdout()
}

fn flush_stdout() -> Result<()> {
    io::stdout().flush()?;
    Ok(())
}

pub async fn run_logout_command(runtime_paths: &RuntimePaths) -> Result<()> {
    let resolved_secret_store = resolve_secret_store_for_logout(runtime_paths)?;
    let _ = shutdown_broker(runtime_paths, resolved_secret_store.store.as_ref()).await;
    clear_broker_installation(runtime_paths, resolved_secret_store.store.as_ref())?;
    if let Some(notice) = resolved_secret_store.notice {
        println!("{notice}");
    }
    println!("Disconnected.");
    println!();
    println!("Local CLI session data has been cleared.");
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
