use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::Result;
use axum::{Json, Router, extract::State, routing::post};
use serde_json::{Value, json};
use tempfile::tempdir;
use tokio::time::{Duration, sleep};

use crate::runtime_paths::{RuntimePaths, ensure_runtime_directories};

use super::{
    client::{call_broker_tool, ping_broker},
    installation::{
        ensure_broker_installation, read_broker_dpop_key_pair, read_broker_local_auth_token,
    },
    public_error::PublicBrokerError,
    remote_mcp::RemoteMcpClient,
    secret_store::SecretStore,
    secrets::write_broker_remote_session,
    server::LocalBrokerServer,
    session::{BrokerRemoteSession, write_broker_remote_session_snapshot},
};

const MCP_PROTOCOL_VERSION: &str = "2025-03-26";

#[derive(Default)]
struct TestSecretStore {
    secrets: Mutex<BTreeMap<String, String>>,
}

impl SecretStore for TestSecretStore {
    fn set_secret(&self, account: &str, secret: &str) -> Result<()> {
        self.secrets
            .lock()
            .unwrap_or_else(|_| panic!("secret lock poisoned"))
            .insert(account.to_string(), secret.to_string());
        Ok(())
    }

    fn get_secret(&self, account: &str) -> Result<Option<String>> {
        Ok(self
            .secrets
            .lock()
            .unwrap_or_else(|_| panic!("secret lock poisoned"))
            .get(account)
            .cloned())
    }

    fn delete_secret(&self, account: &str) -> Result<bool> {
        Ok(self
            .secrets
            .lock()
            .unwrap_or_else(|_| panic!("secret lock poisoned"))
            .remove(account)
            .is_some())
    }
}

#[derive(Clone)]
struct FakeRemoteState {
    call_tool_count: Arc<AtomicUsize>,
    initialize_count: Arc<AtomicUsize>,
    list_tools_count: Arc<AtomicUsize>,
}

#[tokio::test]
async fn local_broker_handles_parallel_forwarded_calls() -> Result<()> {
    let runtime_paths = temp_runtime_paths()?;
    ensure_runtime_directories(&runtime_paths)?;
    let secret_store: Arc<dyn SecretStore> = Arc::new(TestSecretStore::default());
    let metadata = ensure_broker_installation(&runtime_paths, secret_store.as_ref()).await?;

    let remote_state = FakeRemoteState {
        call_tool_count: Arc::new(AtomicUsize::new(0)),
        initialize_count: Arc::new(AtomicUsize::new(0)),
        list_tools_count: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/mcp", post(fake_remote_handler))
        .with_state(remote_state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let session = BrokerRemoteSession {
        schema_version: 1,
        access_token: "access-token".to_string(),
        access_token_expires_at: "2099-01-01T00:00:00Z".to_string(),
        authenticated_at: "2099-01-01T00:00:00Z".to_string(),
        client_id: "client-123".to_string(),
        issuer: format!("http://127.0.0.1:{}", address.port()),
        redirect_uri: "http://127.0.0.1/callback".to_string(),
        refresh_token: "refresh-token".to_string(),
        resource: format!("http://127.0.0.1:{}/mcp", address.port()),
        scope: "driggsby.default".to_string(),
        token_type: "DPoP".to_string(),
    };
    write_broker_remote_session(secret_store.as_ref(), &metadata.broker_id, &session)?;
    write_broker_remote_session_snapshot(&runtime_paths, &session)?;

    let Some(auth_token) =
        read_broker_local_auth_token(secret_store.as_ref(), &metadata.broker_id)?
    else {
        panic!("missing auth token");
    };
    let Some(dpop_keys) =
        read_broker_dpop_key_pair(&runtime_paths, secret_store.as_ref(), &metadata.broker_id)?
    else {
        panic!("missing dpop keys");
    };
    let server = LocalBrokerServer::bind(
        auth_token,
        RemoteMcpClient::new()?,
        runtime_paths.clone(),
        secret_store.clone(),
        metadata.broker_id.clone(),
    )
    .await?;
    let broker_task = tokio::spawn(async move {
        let _ = server.run(dpop_keys).await;
    });

    sleep(Duration::from_millis(100)).await;
    let Some(ping) = ping_broker(&runtime_paths, secret_store.as_ref()).await? else {
        panic!("missing broker ping");
    };
    assert_eq!(ping.cli_version.as_deref(), Some(env!("CARGO_PKG_VERSION")));

    let mut tasks = tokio::task::JoinSet::new();
    for index in 0..20 {
        let runtime_paths = runtime_paths.clone();
        let secret_store = secret_store.clone();
        tasks.spawn(async move {
            call_broker_tool(
                &runtime_paths,
                secret_store.as_ref(),
                "echo_balance",
                Some(json!({ "amount": format!("{index}") })),
            )
            .await
        });
    }

    let mut results = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let task_result = match result {
            Ok(value) => value,
            Err(error) => panic!("join failed: {error}"),
        }?;
        let Some(value) = task_result else {
            panic!("missing broker result");
        };
        results.push(value);
    }

    assert_eq!(results.len(), 20);
    assert_eq!(remote_state.initialize_count.load(Ordering::SeqCst), 1);
    assert_eq!(remote_state.list_tools_count.load(Ordering::SeqCst), 1);
    assert_eq!(remote_state.call_tool_count.load(Ordering::SeqCst), 20);

    let validation_error = call_broker_tool(
        &runtime_paths,
        secret_store.as_ref(),
        "fail_validation",
        Some(json!({ "start_date": "not-a-date" })),
    )
    .await;
    let Err(error) = validation_error else {
        panic!("expected validation error");
    };
    assert!(error.downcast_ref::<PublicBrokerError>().is_some());
    assert!(error.to_string().contains("invalid input"));

    broker_task.abort();
    Ok(())
}

fn temp_runtime_paths() -> Result<RuntimePaths> {
    let temp = tempdir()?;
    let base = temp.keep();
    Ok(RuntimePaths {
        config_dir: base.join("config"),
        state_dir: base.join("state"),
        metadata_path: base.join("config").join("cli-metadata.json"),
        session_snapshot_path: base.join("config").join("cli-session.json"),
        socket_path: base.join("state").join("cli.sock"),
        lock_path: base.join("state").join("cli.lock"),
    })
}

async fn fake_remote_handler(
    State(state): State<FakeRemoteState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> (axum::http::HeaderMap, Json<Value>) {
    let method = body
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match method {
        "initialize" => {
            state.initialize_count.fetch_add(1, Ordering::SeqCst);
            let mut response_headers = axum::http::HeaderMap::new();
            response_headers.insert(
                "Mcp-Session-Id",
                axum::http::HeaderValue::from_static("session-123"),
            );
            (
                response_headers,
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": body.get("id").cloned().unwrap_or(Value::Null),
                    "result": {
                        "protocolVersion": MCP_PROTOCOL_VERSION,
                        "capabilities": { "tools": {} },
                        "serverInfo": { "name": "fake-remote", "version": "0.1.0" }
                    }
                })),
            )
        }
        "notifications/initialized" => (axum::http::HeaderMap::new(), Json(Value::Null)),
        "tools/list" => {
            assert_eq!(
                headers
                    .get("Mcp-Session-Id")
                    .and_then(|value| value.to_str().ok()),
                Some("session-123")
            );
            state.list_tools_count.fetch_add(1, Ordering::SeqCst);
            sleep(Duration::from_millis(50)).await;
            (
                axum::http::HeaderMap::new(),
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": body.get("id").cloned().unwrap_or(Value::Null),
                    "result": {
                        "tools": [
                            {
                                "name": "echo_balance",
                                "description": "Echo a balance amount.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "amount": { "type": "string" }
                                    },
                                    "required": ["amount"],
                                    "additionalProperties": false
                                }
                            },
                            {
                                "name": "fail_validation",
                                "description": "Return a validation error.",
                                "inputSchema": {
                                    "type": "object",
                                    "properties": {
                                        "start_date": { "type": "string" }
                                    },
                                    "additionalProperties": false
                                }
                            }
                        ]
                    }
                })),
            )
        }
        "tools/call" => {
            assert_eq!(
                headers
                    .get("Mcp-Session-Id")
                    .and_then(|value| value.to_str().ok()),
                Some("session-123")
            );
            state.call_tool_count.fetch_add(1, Ordering::SeqCst);
            sleep(Duration::from_millis(50)).await;
            let tool_name = body
                .get("params")
                .and_then(|params| params.get("name"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if tool_name == "fail_validation" {
                return (
                    axum::http::HeaderMap::new(),
                    Json(json!({
                        "jsonrpc": "2.0",
                        "id": body.get("id").cloned().unwrap_or(Value::Null),
                        "error": {
                            "message": "invalid input: start_date must use YYYY-MM-DD"
                        }
                    })),
                );
            }
            let amount = body
                .get("params")
                .and_then(|params| params.get("arguments"))
                .and_then(|arguments| arguments.get("amount"))
                .cloned()
                .unwrap_or_else(|| json!("none"));
            (
                axum::http::HeaderMap::new(),
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": body.get("id").cloned().unwrap_or(Value::Null),
                    "result": {
                        "content": [
                            {
                                "type": "text",
                                "text": format!("echoed {}", amount.as_str().unwrap_or("none"))
                            }
                        ]
                    }
                })),
            )
        }
        _ => (
            axum::http::HeaderMap::new(),
            Json(json!({
                "jsonrpc": "2.0",
                "id": body.get("id").cloned().unwrap_or(Value::Null),
                "error": {
                    "message": "unexpected method"
                }
            })),
        ),
    }
}
