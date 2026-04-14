use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    time::timeout,
};

#[cfg(unix)]
use std::os::unix::fs::FileTypeExt;

use crate::runtime_paths::RuntimePaths;

use super::{
    grants::ClientGrantCredentials,
    installation::{read_broker_local_auth_token, read_broker_metadata},
    public_error::PublicBrokerError,
    secret_store::SecretStore,
    types::{BrokerRequest, BrokerResponse, BrokerStatus, PingResult},
};

const BROKER_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const BROKER_CONTROL_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const BROKER_STATUS_RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
const BROKER_RESPONSE_TIMEOUT: Duration = Duration::from_secs(120);
const BROKER_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_SERVER_DETECTION_TIMEOUT: Duration = Duration::from_millis(250);

pub async fn ping_broker(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
) -> Result<Option<PingResult>> {
    let Some(response) =
        send_broker_request(runtime_paths, secret_store, "ping", None, None, None).await?
    else {
        return Ok(None);
    };
    Ok(Some(serde_json::from_value(response)?))
}

pub async fn get_broker_status(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
) -> Result<Option<BrokerStatus>> {
    let Some(response) =
        send_broker_request(runtime_paths, secret_store, "get_status", None, None, None).await?
    else {
        return Ok(None);
    };
    Ok(Some(serde_json::from_value(
        response
            .get("status")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
    )?))
}

pub async fn shutdown_broker(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
) -> Result<bool> {
    let Some(response) =
        send_broker_request(runtime_paths, secret_store, "shutdown", None, None, None).await?
    else {
        return Ok(false);
    };
    Ok(response
        .get("stopped")
        .and_then(Value::as_bool)
        .unwrap_or(false))
}

pub async fn list_broker_tools(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    client_credentials: &ClientGrantCredentials,
) -> Result<Option<Value>> {
    send_broker_request(
        runtime_paths,
        secret_store,
        "list_tools",
        None,
        None,
        Some(client_credentials),
    )
    .await
}

pub async fn call_broker_tool(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    client_credentials: &ClientGrantCredentials,
    tool_name: &str,
    args: Option<Value>,
) -> Result<Option<Value>> {
    send_broker_request(
        runtime_paths,
        secret_store,
        "call_tool",
        Some(tool_name.to_string()),
        args,
        Some(client_credentials),
    )
    .await
}

pub async fn local_server_is_running(runtime_paths: &RuntimePaths) -> bool {
    if !socket_appears_available(runtime_paths) {
        return false;
    }

    matches!(
        timeout(
            LOCAL_SERVER_DETECTION_TIMEOUT,
            UnixStream::connect(&runtime_paths.socket_path),
        )
        .await,
        Ok(Ok(_))
    )
}

async fn send_broker_request(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    method: &str,
    tool_name: Option<String>,
    args: Option<Value>,
    client_credentials: Option<&ClientGrantCredentials>,
) -> Result<Option<Value>> {
    if !socket_appears_available(runtime_paths) {
        return Ok(None);
    }

    let Some(metadata) = read_broker_metadata(runtime_paths)? else {
        return Ok(None);
    };
    let Some(auth_token) = read_broker_local_auth_token(secret_store, &metadata.broker_id)? else {
        return Ok(None);
    };
    let request = BrokerRequest {
        auth_token,
        challenge: uuid::Uuid::now_v7().to_string(),
        client_key: client_credentials.map(|value| value.client_key.clone()),
        id: uuid::Uuid::now_v7().to_string(),
        method: method.to_string(),
        tool_name,
        args,
    };

    let stream = match timeout(
        BROKER_CONNECT_TIMEOUT,
        UnixStream::connect(&runtime_paths.socket_path),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        Ok(Err(_)) | Err(_) => return Ok(None),
    };
    let (reader, mut writer) = stream.into_split();
    match timeout(
        BROKER_WRITE_TIMEOUT,
        writer.write_all(format!("{}\n", serde_json::to_string(&request)?).as_bytes()),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(_)) | Err(_) => return Ok(None),
    }
    let mut line = String::new();
    let mut reader = BufReader::new(reader);
    match timeout(broker_response_timeout(method), reader.read_line(&mut line)).await {
        Ok(Ok(0)) | Ok(Err(_)) | Err(_) => return Ok(None),
        Ok(Ok(_)) => {}
    }
    let response: BrokerResponse = serde_json::from_str(line.trim_end())?;
    if !response.ok {
        return Err(PublicBrokerError::new(
            response
                .error
                .unwrap_or_else(|| "Request failed.".to_string()),
        )
        .into());
    }
    Ok(response.result)
}

fn broker_response_timeout(method: &str) -> Duration {
    match method {
        "ping" | "shutdown" => BROKER_CONTROL_RESPONSE_TIMEOUT,
        "get_status" => BROKER_STATUS_RESPONSE_TIMEOUT,
        "list_tools" | "call_tool" => BROKER_RESPONSE_TIMEOUT,
        _ => BROKER_CONTROL_RESPONSE_TIMEOUT,
    }
}

fn socket_appears_available(runtime_paths: &RuntimePaths) -> bool {
    #[cfg(unix)]
    {
        if let Ok(metadata) = std::fs::metadata(&runtime_paths.socket_path) {
            return metadata.file_type().is_socket();
        }
        false
    }

    #[cfg(not(unix))]
    {
        runtime_paths.socket_path.exists()
    }
}
