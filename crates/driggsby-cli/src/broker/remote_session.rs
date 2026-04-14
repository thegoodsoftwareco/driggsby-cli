use anyhow::Result;

use crate::{
    auth::{
        discovery::fetch_authorization_server_metadata, dpop::create_dpop_proof,
        oauth::refresh_access_token,
    },
    runtime_paths::RuntimePaths,
    user_guidance::{
        DRIGGSBY_CONNECT_COMMAND, DRIGGSBY_STATUS_COMMAND, build_reauthentication_required_message,
    },
};

use super::{
    installation::read_broker_dpop_key_pair,
    public_error::PublicBrokerError,
    secret_store::SecretStore,
    secrets::{read_broker_remote_session, write_broker_remote_session},
    session::{
        BrokerRemoteSession, BrokerRemoteSessionSummary, summarize_broker_remote_session,
        write_broker_remote_session_snapshot,
    },
    types::BrokerRemoteAccessState,
};

const ACCESS_TOKEN_REFRESH_SKEW_MS: i128 = 60_000;

#[derive(Debug, Clone)]
pub struct BrokerRemoteSessionReadiness {
    pub connected: bool,
    pub ready: bool,
    pub reauthentication_required: bool,
    pub state: BrokerRemoteAccessState,
    pub detail: String,
    pub next_step_command: Option<String>,
    pub session: Option<BrokerRemoteSessionSummary>,
}

pub fn session_needs_refresh(session: &BrokerRemoteSession) -> bool {
    let expires_at = time::OffsetDateTime::parse(
        &session.access_token_expires_at,
        &time::format_description::well_known::Rfc3339,
    )
    .ok();
    let Some(expires_at) = expires_at else {
        return true;
    };
    let remaining_ms = (expires_at - time::OffsetDateTime::now_utc()).whole_milliseconds();
    remaining_ms <= ACCESS_TOKEN_REFRESH_SKEW_MS
}

pub async fn ensure_fresh_remote_session(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    broker_id: &str,
) -> Result<BrokerRemoteSession> {
    let Some(session) = read_broker_remote_session(runtime_paths, secret_store, broker_id)? else {
        return Err(
            PublicBrokerError::new(build_reauthentication_required_message(
                "The Driggsby CLI is not connected",
            ))
            .into(),
        );
    };
    if !session_needs_refresh(&session) {
        return Ok(session);
    }

    let metadata = fetch_authorization_server_metadata(&session.issuer)
        .await
        .map_err(|_| {
            PublicBrokerError::new("Can't reach Driggsby right now. Try again in a moment.")
        })?;
    let dpop_key_pair = read_broker_dpop_key_pair(runtime_paths, secret_store, broker_id)?
        .ok_or_else(|| {
            PublicBrokerError::new(build_reauthentication_required_message(
                "The local CLI key is missing",
            ))
        })?;
    let dpop_proof = create_dpop_proof(
        &dpop_key_pair.private_jwk,
        &dpop_key_pair.public_jwk,
        "POST",
        &metadata.token_endpoint,
        None,
    )?;
    let refreshed = refresh_access_token(
        &metadata,
        &session.client_id,
        &session.refresh_token,
        &session.resource,
        &dpop_proof,
    )
    .await?;
    let updated = BrokerRemoteSession {
        access_token: refreshed.access_token,
        access_token_expires_at: refreshed.access_token_expires_at,
        refresh_token: refreshed.refresh_token,
        scope: refreshed.scope,
        token_type: refreshed.token_type,
        ..session
    };
    write_broker_remote_session(secret_store, broker_id, &updated)?;
    write_broker_remote_session_snapshot(runtime_paths, &updated)?;
    Ok(updated)
}

pub async fn inspect_remote_session_readiness(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    broker_id: &str,
    refresh_if_needed: bool,
) -> Result<BrokerRemoteSessionReadiness> {
    let Some(session) = read_broker_remote_session(runtime_paths, secret_store, broker_id)? else {
        return Ok(BrokerRemoteSessionReadiness {
            connected: false,
            detail: "Not signed in yet.".to_string(),
            next_step_command: Some(DRIGGSBY_CONNECT_COMMAND.to_string()),
            ready: false,
            reauthentication_required: false,
            session: None,
            state: BrokerRemoteAccessState::NotConnected,
        });
    };

    if refresh_if_needed && session_needs_refresh(&session) {
        match ensure_fresh_remote_session(runtime_paths, secret_store, broker_id).await {
            Ok(refreshed) => {
                return Ok(BrokerRemoteSessionReadiness {
                    connected: true,
                    detail: "Driggsby is ready.".to_string(),
                    next_step_command: None,
                    ready: true,
                    reauthentication_required: false,
                    session: Some(summarize_broker_remote_session(&refreshed)),
                    state: BrokerRemoteAccessState::Ready,
                });
            }
            Err(error) => {
                let message = error.to_string();
                if message.contains(DRIGGSBY_CONNECT_COMMAND) {
                    return Ok(BrokerRemoteSessionReadiness {
                        connected: true,
                        detail: "Driggsby session expired. Reconnect to restore access."
                            .to_string(),
                        next_step_command: Some(DRIGGSBY_CONNECT_COMMAND.to_string()),
                        ready: false,
                        reauthentication_required: true,
                        session: Some(summarize_broker_remote_session(&session)),
                        state: BrokerRemoteAccessState::ReauthRequired,
                    });
                }
                return Ok(BrokerRemoteSessionReadiness {
                    connected: true,
                    detail: "Driggsby can't refresh right now. Will retry automatically."
                        .to_string(),
                    next_step_command: Some(DRIGGSBY_STATUS_COMMAND.to_string()),
                    ready: false,
                    reauthentication_required: false,
                    session: Some(summarize_broker_remote_session(&session)),
                    state: BrokerRemoteAccessState::TemporarilyUnavailable,
                });
            }
        }
    }

    if session_needs_refresh(&session) {
        return Ok(BrokerRemoteSessionReadiness {
            connected: true,
            detail: "Driggsby will reconnect automatically on next use.".to_string(),
            next_step_command: Some(DRIGGSBY_STATUS_COMMAND.to_string()),
            ready: false,
            reauthentication_required: false,
            session: Some(summarize_broker_remote_session(&session)),
            state: BrokerRemoteAccessState::TemporarilyUnavailable,
        });
    }

    Ok(BrokerRemoteSessionReadiness {
        connected: true,
        detail: "Driggsby is ready.".to_string(),
        next_step_command: None,
        ready: true,
        reauthentication_required: false,
        session: Some(summarize_broker_remote_session(&session)),
        state: BrokerRemoteAccessState::Ready,
    })
}
