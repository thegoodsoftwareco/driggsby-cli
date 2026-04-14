use std::io::{self, Write as _};

use anyhow::Result;

use crate::{
    auth::login::login_broker,
    broker::{
        installation::read_broker_metadata,
        remote_session::{ensure_fresh_remote_session, session_needs_refresh},
        secret_store::SecretStore,
        secrets::read_broker_remote_session,
    },
    runtime_paths::RuntimePaths,
    user_guidance::DRIGGSBY_CONNECT_COMMAND,
};

const CLI_SESSION_FRESH_SECONDS: i64 = 8 * 60 * 60;

pub(super) async fn ensure_recent_cli_session(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
) -> Result<String> {
    if let Some(broker_id) = try_recent_cli_session(runtime_paths, secret_store).await? {
        return Ok(broker_id);
    }

    println!("Opening Driggsby sign-in...");
    flush_stdout()?;
    let login = login_broker(runtime_paths, secret_store, print_manual_sign_in_url).await?;
    Ok(login.broker_id)
}

async fn try_recent_cli_session(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
) -> Result<Option<String>> {
    let Some(metadata) = read_broker_metadata(runtime_paths)? else {
        return Ok(None);
    };
    match read_broker_remote_session(runtime_paths, secret_store, &metadata.broker_id) {
        Ok(Some(session))
            if session_is_recent(&session.authenticated_at) && !session_needs_refresh(&session) =>
        {
            println!("Using saved Driggsby session.");
            return Ok(Some(metadata.broker_id));
        }
        Ok(Some(session)) if session_is_recent(&session.authenticated_at) => {}
        Ok(_) | Err(_) => return Ok(None),
    }
    match ensure_fresh_remote_session(runtime_paths, secret_store, &metadata.broker_id).await {
        Ok(_) => {
            println!("Using saved Driggsby session.");
            Ok(Some(metadata.broker_id))
        }
        Err(error) if error.to_string().contains(DRIGGSBY_CONNECT_COMMAND) => Ok(None),
        Err(error) => Err(error),
    }
}

fn session_is_recent(authenticated_at: &str) -> bool {
    let Ok(authenticated_at) = time::OffsetDateTime::parse(
        authenticated_at,
        &time::format_description::well_known::Rfc3339,
    ) else {
        return false;
    };
    let age_seconds = (time::OffsetDateTime::now_utc() - authenticated_at).whole_seconds();
    (0..=CLI_SESSION_FRESH_SECONDS).contains(&age_seconds)
}

fn print_manual_sign_in_url(sign_in_url: &str) -> Result<()> {
    println!("Browser didn't open. Sign in here:");
    println!("{sign_in_url}");
    println!();
    flush_stdout()
}

fn flush_stdout() -> Result<()> {
    io::stdout().flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn session_recency_uses_eight_hour_window() -> anyhow::Result<()> {
        let recent = (time::OffsetDateTime::now_utc() - time::Duration::hours(1))
            .format(&time::format_description::well_known::Rfc3339)?;
        let stale = (time::OffsetDateTime::now_utc() - time::Duration::hours(9))
            .format(&time::format_description::well_known::Rfc3339)?;

        assert!(super::session_is_recent(&recent));
        assert!(!super::session_is_recent(&stale));
        assert!(!super::session_is_recent("not a timestamp"));
        Ok(())
    }
}
