use std::{sync::Arc, time::Duration};

use anyhow::{Result, bail};
use reqwest::header::{ACCEPT, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, Notify, Semaphore};

use crate::auth::{dpop::create_dpop_proof, url_security::assert_broker_remote_url};

use super::{
    public_error::PublicBrokerError,
    secrets::{BrokerDpopKeyPair, BrokerRemoteSessionSecrets},
    session::BrokerRemoteSessionSummary,
};

const MCP_PROTOCOL_VERSION: &str = "2025-03-26";
const MAX_REMOTE_CONCURRENCY: usize = 32;

#[derive(Clone)]
pub struct RemoteMcpClient {
    http_client: reqwest::Client,
    state: Arc<Mutex<RemoteSessionState>>,
    initialize_lock: Arc<Mutex<()>>,
    tools_inflight: Arc<Mutex<Option<Arc<Notify>>>>,
    concurrency_limit: Arc<Semaphore>,
}

#[derive(Debug, Default)]
struct RemoteSessionState {
    initialized: bool,
    next_request_id: u64,
    session_key: Option<String>,
    session_id: Option<String>,
    tools_cache: Option<Vec<Value>>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: Option<String>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
    #[allow(dead_code)]
    id: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    message: String,
}

impl RemoteMcpClient {
    pub fn new() -> Result<Self> {
        Ok(Self {
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()?,
            state: Arc::new(Mutex::new(RemoteSessionState::default())),
            initialize_lock: Arc::new(Mutex::new(())),
            tools_inflight: Arc::new(Mutex::new(None)),
            concurrency_limit: Arc::new(Semaphore::new(MAX_REMOTE_CONCURRENCY)),
        })
    }

    pub async fn list_tools(
        &self,
        summary: &BrokerRemoteSessionSummary,
        secrets: &BrokerRemoteSessionSecrets,
        dpop_keys: &BrokerDpopKeyPair,
    ) -> Result<Vec<Value>> {
        self.load_tools(summary, secrets, dpop_keys, false).await
    }

    pub async fn call_tool(
        &self,
        summary: &BrokerRemoteSessionSummary,
        secrets: &BrokerRemoteSessionSecrets,
        dpop_keys: &BrokerDpopKeyPair,
        tool_name: &str,
        args: Option<Value>,
    ) -> Result<Value> {
        let tools = self.load_tools(summary, secrets, dpop_keys, false).await?;
        let tool_exists = tools.iter().any(|tool| {
            tool.get("name")
                .and_then(Value::as_str)
                .map(|name| name == tool_name)
                .unwrap_or(false)
        });
        if !tool_exists {
            let refreshed = self.load_tools(summary, secrets, dpop_keys, true).await?;
            if !refreshed.iter().any(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .map(|name| name == tool_name)
                    .unwrap_or(false)
            }) {
                return Err(PublicBrokerError::new(
                    "That Driggsby tool is not available in this session anymore. Start a fresh client session and try again.",
                )
                .into());
            }
        }

        let _permit = self.concurrency_limit.acquire().await?;
        let request_id = self.next_request_id().await;
        let payload = match args {
            Some(arguments) => json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": tool_name,
                    "arguments": arguments
                },
                "id": request_id
            }),
            None => json!({
                "jsonrpc": "2.0",
                "method": "tools/call",
                "params": {
                    "name": tool_name
                },
                "id": request_id
            }),
        };
        self.post_with_session_retry(summary, secrets, dpop_keys, payload)
            .await
    }

    async fn load_tools(
        &self,
        summary: &BrokerRemoteSessionSummary,
        secrets: &BrokerRemoteSessionSecrets,
        dpop_keys: &BrokerDpopKeyPair,
        refresh: bool,
    ) -> Result<Vec<Value>> {
        let session_key = remote_session_key(summary, secrets, dpop_keys)?;
        if !refresh && let Some(cached) = self.cached_tools_for_session(&session_key).await {
            return Ok(cached);
        }

        loop {
            let waiting_on = {
                let mut inflight = self.tools_inflight.lock().await;
                if let Some(existing) = inflight.clone() {
                    Some(existing)
                } else {
                    let notify = Arc::new(Notify::new());
                    *inflight = Some(notify.clone());
                    None
                }
            };
            if let Some(notify) = waiting_on {
                notify.notified().await;
                if let Some(cached) = self.cached_tools_for_session(&session_key).await {
                    return Ok(cached);
                }
                continue;
            }

            let result = self.fetch_tools(summary, secrets, dpop_keys).await;
            if let Ok(tools) = &result {
                let mut state = self.state.lock().await;
                if state.session_key.as_deref() == Some(session_key.as_str()) {
                    state.tools_cache = Some(tools.clone());
                }
            }
            let notify = self.tools_inflight.lock().await.take();
            if let Some(notify) = notify {
                notify.notify_waiters();
            }
            return result;
        }
    }

    async fn cached_tools_for_session(&self, session_key: &str) -> Option<Vec<Value>> {
        let state = self.state.lock().await;
        if state.session_key.as_deref() != Some(session_key) {
            return None;
        }
        state.tools_cache.clone()
    }

    async fn fetch_tools(
        &self,
        summary: &BrokerRemoteSessionSummary,
        secrets: &BrokerRemoteSessionSecrets,
        dpop_keys: &BrokerDpopKeyPair,
    ) -> Result<Vec<Value>> {
        let _permit = self.concurrency_limit.acquire().await?;
        let request_id = self.next_request_id().await;
        let payload = json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": {},
            "id": request_id
        });
        let result = self
            .post_with_session_retry(summary, secrets, dpop_keys, payload)
            .await?;
        Ok(result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    async fn post_with_session_retry(
        &self,
        summary: &BrokerRemoteSessionSummary,
        secrets: &BrokerRemoteSessionSecrets,
        dpop_keys: &BrokerDpopKeyPair,
        payload: Value,
    ) -> Result<Value> {
        let session_key = remote_session_key(summary, secrets, dpop_keys)?;
        let session_id = self.ensure_initialized(summary, secrets, dpop_keys).await?;
        match self
            .post_json_internal(
                summary,
                secrets,
                dpop_keys,
                payload.clone(),
                session_id.clone(),
            )
            .await
        {
            Ok((result, _)) => Ok(result),
            Err(error) if error.to_string().contains("session expired") => {
                self.clear_session_if_current(&session_key, session_id.as_deref())
                    .await;
                let session_id = self.ensure_initialized(summary, secrets, dpop_keys).await?;
                let (result, _) = self
                    .post_json_internal(summary, secrets, dpop_keys, payload, session_id)
                    .await?;
                Ok(result)
            }
            Err(error) => Err(error),
        }
    }

    async fn ensure_initialized(
        &self,
        summary: &BrokerRemoteSessionSummary,
        secrets: &BrokerRemoteSessionSecrets,
        dpop_keys: &BrokerDpopKeyPair,
    ) -> Result<Option<String>> {
        let session_key = remote_session_key(summary, secrets, dpop_keys)?;
        {
            let state = self.state.lock().await;
            if state.initialized && state.session_key.as_deref() == Some(session_key.as_str()) {
                return Ok(state.session_id.clone());
            }
        }

        let _guard = self.initialize_lock.lock().await;
        {
            let mut state = self.state.lock().await;
            if state.session_key.as_deref() != Some(session_key.as_str()) {
                state.initialized = false;
                state.session_id = None;
                state.session_key = Some(session_key.clone());
                state.tools_cache = None;
            } else if state.initialized {
                return Ok(state.session_id.clone());
            }
        };

        let request_id = self.next_request_id().await;
        let initialize_payload = json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {
                    "name": "driggsby-cli",
                    "version": "0.1.0"
                }
            },
            "id": request_id
        });
        let (result, session_id) = self
            .post_json_internal(summary, secrets, dpop_keys, initialize_payload, None)
            .await?;
        let _ = result;
        {
            let mut state = self.state.lock().await;
            state.session_key = Some(session_key);
            state.session_id = session_id.clone();
        }
        let initialized_payload = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        });
        let _ = self
            .post_json_internal(
                summary,
                secrets,
                dpop_keys,
                initialized_payload,
                session_id.clone(),
            )
            .await?;
        {
            let mut state = self.state.lock().await;
            state.initialized = true;
        }
        Ok(session_id)
    }

    async fn post_json_internal(
        &self,
        summary: &BrokerRemoteSessionSummary,
        secrets: &BrokerRemoteSessionSecrets,
        dpop_keys: &BrokerDpopKeyPair,
        payload: Value,
        session_id: Option<String>,
    ) -> Result<(Value, Option<String>)> {
        let resource_url = validated_remote_resource_url(&summary.resource)?;
        let dpop_proof = create_dpop_proof(
            &dpop_keys.private_jwk,
            &dpop_keys.public_jwk,
            "POST",
            resource_url.as_str(),
            Some(&secrets.access_token),
        )?;
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json, text/event-stream"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "MCP-Protocol-Version",
            HeaderValue::from_static(MCP_PROTOCOL_VERSION),
        );
        headers.insert(
            reqwest::header::AUTHORIZATION,
            HeaderValue::from_str(&format!("{} {}", summary.token_type, secrets.access_token))?,
        );
        headers.insert("DPoP", HeaderValue::from_str(&dpop_proof)?);
        if let Some(session_id) = &session_id {
            headers.insert("Mcp-Session-Id", HeaderValue::from_str(session_id)?);
        }

        let response = self
            .http_client
            .post(resource_url)
            .headers(headers)
            .json(&payload)
            .send()
            .await?;

        let new_session_id = response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|value| value.to_str().ok())
            .map(ToString::to_string);

        if response.status() == reqwest::StatusCode::NOT_FOUND && session_id.is_some() {
            bail!("session expired");
        }
        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(PublicBrokerError::new(
                "Authentication has expired or the saved CLI session is no longer valid.\n\nNext:\n  npx driggsby@latest mcp connect",
            )
            .into());
        }
        let status = response.status();
        let body = response.text().await?;
        if status == reqwest::StatusCode::ACCEPTED
            || status == reqwest::StatusCode::NO_CONTENT
            || body.trim().is_empty()
            || body.trim() == "null"
        {
            return Ok((Value::Null, new_session_id));
        }
        let parsed: JsonRpcResponse = serde_json::from_str(&body)?;
        if let Some(error) = parsed.error {
            return Err(PublicBrokerError::new(error.message).into());
        }
        Ok((parsed.result.unwrap_or(Value::Null), new_session_id))
    }

    async fn next_request_id(&self) -> u64 {
        let mut state = self.state.lock().await;
        state.next_request_id += 1;
        state.next_request_id
    }

    async fn clear_session_if_current(&self, session_key: &str, failed_session_id: Option<&str>) {
        let mut state = self.state.lock().await;
        if state.session_key.as_deref() != Some(session_key) {
            return;
        }
        if state.session_id.as_deref() != failed_session_id {
            return;
        }
        state.initialized = false;
        state.session_key = None;
        state.session_id = None;
        state.tools_cache = None;
    }
}

fn validated_remote_resource_url(resource: &str) -> Result<reqwest::Url> {
    assert_broker_remote_url(resource, "The Driggsby MCP resource URL")?;
    Ok(reqwest::Url::parse(resource)?)
}

fn remote_session_key(
    summary: &BrokerRemoteSessionSummary,
    secrets: &BrokerRemoteSessionSecrets,
    dpop_keys: &BrokerDpopKeyPair,
) -> Result<String> {
    let mut hasher = Sha256::new();
    for part in [
        secrets.schema_version.to_string(),
        summary.issuer.clone(),
        summary.resource.clone(),
        summary.client_id.clone(),
        summary.redirect_uri.clone(),
        summary.scope.clone(),
        summary.token_type.clone(),
        summary.authenticated_at.clone(),
        summary.access_token_expires_at.clone(),
        secrets.access_token.clone(),
        secrets.refresh_token.clone(),
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    hasher.update(serde_json::to_vec(&dpop_keys.public_jwk)?);

    let digest = hasher.finalize();
    let mut rendered = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(rendered, "{byte:02x}");
    }
    Ok(rendered)
}
