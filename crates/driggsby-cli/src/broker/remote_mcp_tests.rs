use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{Json, Router, extract::State, routing::post};
use serde_json::{Value, json};
use tokio::time::{Duration, sleep};

use crate::auth::dpop::generate_dpop_key_material;

use super::{
    installation::BrokerDpopKeyPair, remote_mcp::RemoteMcpClient, session::BrokerRemoteSession,
};

const MCP_PROTOCOL_VERSION: &str = "2025-03-26";

#[derive(Clone)]
struct TestState {
    call_tool_count: Arc<AtomicUsize>,
    initialize_count: Arc<AtomicUsize>,
    issue_session_id: bool,
    list_tools_count: Arc<AtomicUsize>,
}

#[tokio::test]
async fn coalesces_tool_discovery_and_handles_parallel_calls() -> anyhow::Result<()> {
    let state = TestState {
        call_tool_count: Arc::new(AtomicUsize::new(0)),
        initialize_count: Arc::new(AtomicUsize::new(0)),
        issue_session_id: true,
        list_tools_count: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/mcp", post(test_mcp_handler))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let dpop = generate_dpop_key_material()?;
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
    let dpop_keys = BrokerDpopKeyPair {
        private_jwk: dpop.private_jwk,
        public_jwk: dpop.public_jwk,
    };
    let client = RemoteMcpClient::new()?;

    let mut tasks = tokio::task::JoinSet::new();
    for index in 0..20 {
        let client = client.clone();
        let session = session.clone();
        let dpop_keys = dpop_keys.clone();
        tasks.spawn(async move {
            client
                .call_tool(
                    &session,
                    &dpop_keys,
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
        results.push(task_result);
    }

    assert_eq!(results.len(), 20);
    assert_eq!(state.initialize_count.load(Ordering::SeqCst), 1);
    assert_eq!(state.list_tools_count.load(Ordering::SeqCst), 1);
    assert_eq!(state.call_tool_count.load(Ordering::SeqCst), 20);
    Ok(())
}

#[tokio::test]
async fn supports_stateless_remote_mcp_without_session_header() -> anyhow::Result<()> {
    let state = TestState {
        call_tool_count: Arc::new(AtomicUsize::new(0)),
        initialize_count: Arc::new(AtomicUsize::new(0)),
        issue_session_id: false,
        list_tools_count: Arc::new(AtomicUsize::new(0)),
    };
    let app = Router::new()
        .route("/mcp", post(test_mcp_handler))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let dpop = generate_dpop_key_material()?;
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
    let dpop_keys = BrokerDpopKeyPair {
        private_jwk: dpop.private_jwk,
        public_jwk: dpop.public_jwk,
    };
    let client = RemoteMcpClient::new()?;

    let result = client
        .call_tool(
            &session,
            &dpop_keys,
            "echo_balance",
            Some(json!({ "amount": "stateless" })),
        )
        .await?;

    assert_eq!(result["content"][0]["text"], "echoed stateless");
    assert_eq!(state.initialize_count.load(Ordering::SeqCst), 1);
    assert_eq!(state.list_tools_count.load(Ordering::SeqCst), 1);
    assert_eq!(state.call_tool_count.load(Ordering::SeqCst), 1);
    Ok(())
}

async fn test_mcp_handler(
    State(state): State<TestState>,
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
            if state.issue_session_id {
                response_headers.insert(
                    "Mcp-Session-Id",
                    axum::http::HeaderValue::from_static("session-123"),
                );
            }
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
            let expected_session_id = state.issue_session_id.then_some("session-123");
            assert_eq!(
                headers
                    .get("Mcp-Session-Id")
                    .and_then(|value| value.to_str().ok()),
                expected_session_id
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
                            }
                        ]
                    }
                })),
            )
        }
        "tools/call" => {
            let expected_session_id = state.issue_session_id.then_some("session-123");
            assert_eq!(
                headers
                    .get("Mcp-Session-Id")
                    .and_then(|value| value.to_str().ok()),
                expected_session_id
            );
            state.call_tool_count.fetch_add(1, Ordering::SeqCst);
            sleep(Duration::from_millis(50)).await;
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
