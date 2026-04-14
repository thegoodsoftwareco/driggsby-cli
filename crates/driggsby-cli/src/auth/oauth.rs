use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::discovery::{AuthorizationServerMetadata, ProtectedResourceMetadata};

#[derive(Debug, Clone)]
pub struct OAuthClientRegistration {
    pub client_id: String,
    pub scope: String,
}

#[derive(Debug, Clone)]
pub struct OAuthTokenExchangeResult {
    pub access_token: String,
    pub access_token_expires_at: String,
    pub refresh_token: String,
    pub scope: String,
    pub token_type: String,
}

#[derive(Debug, Deserialize)]
struct RegistrationResponse {
    client_id: String,
    scope: String,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
    scope: String,
    token_type: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterBrokerClientRequest<'a> {
    pub client_name: &'a str,
    pub dpop_bound_access_tokens: bool,
    pub grant_types: [&'a str; 2],
    pub redirect_uris: [&'a str; 1],
    pub response_types: [&'a str; 1],
    pub scope: &'a str,
    pub token_endpoint_auth_method: &'a str,
}

pub async fn register_broker_client(
    metadata: &AuthorizationServerMetadata,
    client_name: &str,
    redirect_uri: &str,
    requested_scope: &str,
) -> Result<OAuthClientRegistration> {
    let response = reqwest::Client::new()
        .post(&metadata.registration_endpoint)
        .header(reqwest::header::ACCEPT, "application/json")
        .json(&RegisterBrokerClientRequest {
            client_name,
            dpop_bound_access_tokens: true,
            grant_types: ["authorization_code", "refresh_token"],
            redirect_uris: [redirect_uri],
            response_types: ["code"],
            scope: requested_scope,
            token_endpoint_auth_method: "none",
        })
        .send()
        .await?;
    if !response.status().is_success() {
        bail!("Driggsby sign-in could not register the local CLI with the remote service.");
    }
    let parsed: RegistrationResponse = response.json().await?;
    Ok(OAuthClientRegistration {
        client_id: parsed.client_id,
        scope: parsed.scope,
    })
}

pub fn build_authorization_url(
    client_id: &str,
    metadata: &AuthorizationServerMetadata,
    pkce_challenge: &str,
    redirect_uri: &str,
    resource: &ProtectedResourceMetadata,
    requested_scope: &str,
    state: &str,
) -> Result<String> {
    let mut url = url::Url::parse(&metadata.authorization_endpoint)?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", requested_scope)
        .append_pair("resource", &resource.resource)
        .append_pair("state", state)
        .append_pair("code_challenge", pkce_challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

pub async fn exchange_authorization_code(
    metadata: &AuthorizationServerMetadata,
    client_id: &str,
    code: &str,
    code_verifier: &str,
    dpop_proof: &str,
    redirect_uri: &str,
    resource: &ProtectedResourceMetadata,
) -> Result<OAuthTokenExchangeResult> {
    let response = reqwest::Client::new()
        .post(&metadata.token_endpoint)
        .header(reqwest::header::ACCEPT, "application/json")
        .header("DPoP", dpop_proof)
        .form(&[
            ("client_id", client_id),
            ("code", code),
            ("code_verifier", code_verifier),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
            ("resource", &resource.resource),
        ])
        .send()
        .await?;
    if !response.status().is_success() {
        bail!("Driggsby sign-in could not exchange the authorization code for tokens.");
    }
    parse_token_response(response.json().await?, true)
}

pub async fn refresh_access_token(
    metadata: &AuthorizationServerMetadata,
    client_id: &str,
    refresh_token: &str,
    resource: &str,
    dpop_proof: &str,
) -> Result<OAuthTokenExchangeResult> {
    let response = reqwest::Client::new()
        .post(&metadata.token_endpoint)
        .header(reqwest::header::ACCEPT, "application/json")
        .header("DPoP", dpop_proof)
        .form(&[
            ("client_id", client_id),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("resource", resource),
        ])
        .send()
        .await?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default().to_lowercase();
        if status == reqwest::StatusCode::UNAUTHORIZED
            || status == reqwest::StatusCode::FORBIDDEN
            || (status == reqwest::StatusCode::BAD_REQUEST
                && [
                    "invalid_grant",
                    "invalid_token",
                    "expired",
                    "revoked",
                    "unauthorized_client",
                ]
                .iter()
                .any(|needle| body.contains(needle)))
        {
            bail!(
                "Authentication has expired or the saved CLI session is no longer valid.\n\nNext:\n  npx driggsby@latest mcp connect"
            );
        }
        bail!(
            "Driggsby could not refresh the CLI session with the remote service. Wait a moment and try again."
        );
    }
    parse_token_response(response.json().await?, false)
}

pub fn create_oauth_state() -> String {
    Uuid::now_v7().to_string()
}

fn parse_token_response(
    payload: TokenResponse,
    require_refresh_token: bool,
) -> Result<OAuthTokenExchangeResult> {
    if require_refresh_token && payload.refresh_token.is_none() {
        bail!(
            "Driggsby could not continue because the remote service did not return a refresh token."
        );
    }
    let expires_at =
        time::OffsetDateTime::now_utc() + time::Duration::seconds(payload.expires_in as i64);
    Ok(OAuthTokenExchangeResult {
        access_token: payload.access_token,
        access_token_expires_at: expires_at
            .format(&time::format_description::well_known::Rfc3339)?,
        refresh_token: payload.refresh_token.unwrap_or_default(),
        scope: payload.scope,
        token_type: payload.token_type,
    })
}
