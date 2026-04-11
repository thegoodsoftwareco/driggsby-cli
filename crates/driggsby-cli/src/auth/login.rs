use anyhow::{Result, bail};

use crate::{
    broker::{
        installation::{ensure_broker_installation, read_broker_dpop_key_pair},
        secret_store::SecretStore,
        session::{
            BrokerRemoteSession, write_broker_remote_session, write_broker_remote_session_snapshot,
        },
    },
    runtime_paths::RuntimePaths,
    user_guidance::build_reauthentication_required_message,
};

use super::{
    browser::open_browser_url,
    config::resolve_broker_auth_config,
    discovery::{
        AuthorizationServerMetadata, ProtectedResourceMetadata,
        fetch_authorization_server_metadata, fetch_protected_resource_metadata,
    },
    dpop::create_dpop_proof,
    loopback::{LoopbackAuthorizationResult, start_loopback_auth_listener},
    oauth::{
        OAuthClientRegistration, build_authorization_url, create_oauth_state,
        exchange_authorization_code, register_broker_client,
    },
    pkce::generate_pkce_pair,
    url_security::assert_broker_remote_url,
};

#[derive(Debug, Clone)]
pub struct BrokerLoginResult {
    pub browser_opened: bool,
    pub broker_id: String,
    pub dpop_thumbprint: String,
    pub session: BrokerRemoteSession,
    pub sign_in_url: String,
}

pub async fn login_broker(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    on_manual_sign_in_url: impl FnOnce(&str) -> Result<()>,
) -> Result<BrokerLoginResult> {
    let config = resolve_broker_auth_config()?;
    let metadata = ensure_broker_installation(runtime_paths, secret_store).await?;
    let protected_resource_metadata =
        fetch_protected_resource_metadata(&config.protected_resource_metadata_url).await?;
    let authorization_server_metadata =
        fetch_authorization_server_metadata(&protected_resource_metadata.authorization_server)
            .await?;
    validate_remote_metadata(&authorization_server_metadata, &protected_resource_metadata)?;

    let listener = start_loopback_auth_listener(config.login_timeout_ms).await?;
    let registration = register_broker_client(
        &authorization_server_metadata,
        &config.client_name,
        &listener.redirect_uri,
        &config.requested_scope,
    )
    .await?;
    let pkce = generate_pkce_pair();
    let state = create_oauth_state();
    let sign_in_url = build_authorization_url(
        &registration.client_id,
        &authorization_server_metadata,
        &pkce.challenge,
        &listener.redirect_uri,
        &protected_resource_metadata,
        &config.requested_scope,
        &state,
    )?;
    let browser_opened = open_browser_url(&sign_in_url).await?;
    if !browser_opened {
        on_manual_sign_in_url(&sign_in_url)?;
    }
    let callback = listener.wait_for_result().await?;
    listener.close().await?;

    let LoopbackAuthorizationResult::Success(callback) = callback else {
        let LoopbackAuthorizationResult::Error(error) = callback else {
            bail!("Driggsby sign-in was not completed.");
        };
        bail!(
            "{}",
            error
                .error_description
                .unwrap_or_else(|| "Driggsby sign-in was not completed.".to_string())
        );
    };

    if callback.state != state {
        bail!("Driggsby sign-in returned an invalid state value. Try again.");
    }

    let session = exchange_and_store_session(
        runtime_paths,
        secret_store,
        &metadata.broker_id,
        ExchangeSessionInput {
            authorization_code: &callback.code,
            authorization_server_metadata: &authorization_server_metadata,
            code_verifier: &pkce.verifier,
            protected_resource_metadata: &protected_resource_metadata,
            redirect_uri: &listener.redirect_uri,
            registration: &registration,
        },
    )
    .await?;

    Ok(BrokerLoginResult {
        browser_opened,
        broker_id: metadata.broker_id,
        dpop_thumbprint: metadata.dpop.thumbprint,
        session,
        sign_in_url,
    })
}

struct ExchangeSessionInput<'a> {
    authorization_code: &'a str,
    authorization_server_metadata: &'a AuthorizationServerMetadata,
    code_verifier: &'a str,
    protected_resource_metadata: &'a ProtectedResourceMetadata,
    redirect_uri: &'a str,
    registration: &'a OAuthClientRegistration,
}

async fn exchange_and_store_session(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    broker_id: &str,
    input: ExchangeSessionInput<'_>,
) -> Result<BrokerRemoteSession> {
    let dpop_key_pair = read_broker_dpop_key_pair(runtime_paths, secret_store, broker_id)?
        .ok_or_else(|| {
            anyhow::anyhow!(build_reauthentication_required_message(
                "The local CLI DPoP key is missing"
            ))
        })?;
    let dpop_proof = create_dpop_proof(
        &dpop_key_pair.private_jwk,
        &dpop_key_pair.public_jwk,
        "POST",
        &input.authorization_server_metadata.token_endpoint,
        None,
    )?;
    let tokens = exchange_authorization_code(
        input.authorization_server_metadata,
        &input.registration.client_id,
        input.authorization_code,
        input.code_verifier,
        &dpop_proof,
        input.redirect_uri,
        input.protected_resource_metadata,
    )
    .await?;
    let session = BrokerRemoteSession {
        schema_version: 1,
        access_token: tokens.access_token,
        access_token_expires_at: tokens.access_token_expires_at,
        authenticated_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?,
        client_id: input.registration.client_id.clone(),
        issuer: input.authorization_server_metadata.issuer.clone(),
        redirect_uri: input.redirect_uri.to_string(),
        refresh_token: tokens.refresh_token,
        resource: input.protected_resource_metadata.resource.clone(),
        scope: tokens.scope,
        token_type: tokens.token_type,
    };
    write_broker_remote_session(secret_store, broker_id, &session)?;
    write_broker_remote_session_snapshot(runtime_paths, &session)?;
    Ok(session)
}

fn validate_remote_metadata(
    authorization_server_metadata: &AuthorizationServerMetadata,
    protected_resource_metadata: &ProtectedResourceMetadata,
) -> Result<()> {
    assert_broker_remote_url(
        &protected_resource_metadata.authorization_server,
        "The Driggsby authorization server URL",
    )?;
    assert_broker_remote_url(
        &protected_resource_metadata.resource,
        "The Driggsby MCP resource URL",
    )?;
    assert_broker_remote_url(
        &authorization_server_metadata.issuer,
        "The Driggsby issuer URL",
    )?;
    assert_broker_remote_url(
        &authorization_server_metadata.authorization_endpoint,
        "The Driggsby authorization endpoint",
    )?;
    assert_broker_remote_url(
        &authorization_server_metadata.registration_endpoint,
        "The Driggsby registration endpoint",
    )?;
    assert_broker_remote_url(
        &authorization_server_metadata.token_endpoint,
        "The Driggsby token endpoint",
    )?;
    require_contains(
        &authorization_server_metadata.code_challenge_methods_supported,
        "S256",
        "Driggsby sign-in requires S256 PKCE support.",
    )?;
    require_contains(
        &authorization_server_metadata.dpop_signing_alg_values_supported,
        "ES256",
        "Driggsby sign-in requires ES256 DPoP support.",
    )?;
    require_contains(
        &authorization_server_metadata.grant_types_supported,
        "authorization_code",
        "Driggsby sign-in requires authorization code grant support.",
    )?;
    require_contains(
        &authorization_server_metadata.response_types_supported,
        "code",
        "Driggsby sign-in requires code response support.",
    )?;
    require_contains(
        &authorization_server_metadata.token_endpoint_auth_methods_supported,
        "none",
        "Driggsby sign-in requires public client token exchange support.",
    )?;
    require_contains(
        &authorization_server_metadata.scopes_supported,
        "driggsby.default",
        "Driggsby sign-in requires the driggsby.default scope to be available.",
    )?;
    require_contains(
        &protected_resource_metadata.scopes_supported,
        "driggsby.default",
        "Driggsby sign-in requires the driggsby.default scope to be available.",
    )?;
    Ok(())
}

fn require_contains(values: &[String], needle: &str, message: &str) -> Result<()> {
    if values.iter().any(|value| value == needle) {
        return Ok(());
    }
    bail!("{message}")
}
