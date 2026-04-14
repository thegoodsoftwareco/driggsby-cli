use std::sync::Arc;

use anyhow::{Result, bail};
use serde_json::json;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

use crate::runtime_paths::RuntimePaths;

use super::{
    grants::{ClientGrantCredentials, missing_client_grant_error, verify_client_grant},
    installation::build_broker_status,
    public_error::PublicBrokerError,
    remote_mcp::RemoteMcpClient,
    remote_session::ensure_fresh_remote_session,
    secret_store::SecretStore,
    secrets::{
        BrokerDpopKeyPair, read_broker_remote_session_secrets, verify_broker_remote_session_binding,
    },
    session::read_broker_remote_session_snapshot,
    types::{BrokerRequest, BrokerResponse},
};

pub struct LocalBrokerServer {
    auth_token: String,
    listener: UnixListener,
    remote_client: RemoteMcpClient,
    runtime_paths: RuntimePaths,
    secret_store: Arc<dyn SecretStore>,
    broker_id: String,
}

impl LocalBrokerServer {
    pub async fn bind(
        auth_token: String,
        remote_client: RemoteMcpClient,
        runtime_paths: RuntimePaths,
        secret_store: Arc<dyn SecretStore>,
        broker_id: String,
    ) -> Result<Self> {
        #[cfg(not(windows))]
        {
            let _ = std::fs::remove_file(&runtime_paths.socket_path);
        }
        let listener = UnixListener::bind(&runtime_paths.socket_path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                &runtime_paths.socket_path,
                std::fs::Permissions::from_mode(0o600),
            )?;
        }
        Ok(Self {
            auth_token,
            listener,
            remote_client,
            runtime_paths,
            secret_store,
            broker_id,
        })
    }

    pub async fn run(self, dpop_keys: BrokerDpopKeyPair) -> Result<()> {
        let shared = Arc::new(self);
        loop {
            let (stream, _) = shared.listener.accept().await?;
            let shared = shared.clone();
            let dpop_keys = dpop_keys.clone();
            tokio::spawn(async move {
                let _ = shared.handle_stream(stream, dpop_keys).await;
            });
        }
    }

    async fn handle_stream(&self, stream: UnixStream, dpop_keys: BrokerDpopKeyPair) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Ok(());
        }
        let request: BrokerRequest = serde_json::from_str(line.trim_end())?;
        let request_id = request.id.clone();
        let response = match self.dispatch_request(request, dpop_keys).await {
            Ok(response) => response,
            Err(error) => BrokerResponse {
                broker_proof: String::new(),
                id: request_id,
                ok: false,
                result: None,
                error: Some(public_broker_error_message(&error)),
            },
        };
        writer
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        Ok(())
    }

    async fn dispatch_request(
        &self,
        request: BrokerRequest,
        dpop_keys: BrokerDpopKeyPair,
    ) -> Result<BrokerResponse> {
        if request.auth_token != self.auth_token {
            return Ok(BrokerResponse {
                broker_proof: String::new(),
                id: request.id,
                ok: false,
                result: None,
                error: Some("CLI authentication failed.".to_string()),
            });
        }

        let result = match request.method.as_str() {
            "ping" => json!({
                "ok": true,
                "broker_id": self.broker_id,
                "cli_version": env!("CARGO_PKG_VERSION")
            }),
            "get_status" => json!({
                "status": build_broker_status(
                    &self.runtime_paths,
                    self.secret_store.as_ref(),
                    true
                ).await?
            }),
            "shutdown" => {
                std::process::exit(0);
            }
            "list_tools" => {
                self.verify_client_grant(&request)?;
                ensure_fresh_remote_session(
                    &self.runtime_paths,
                    self.secret_store.as_ref(),
                    &self.broker_id,
                )
                .await?;
                let summary = self.remote_session_summary()?;
                let secrets = self.remote_session_secrets(&summary)?;
                json!({
                    "tools": self.remote_client.list_tools(&summary, &secrets, &dpop_keys).await?
                })
            }
            "call_tool" => {
                self.verify_client_grant(&request)?;
                ensure_fresh_remote_session(
                    &self.runtime_paths,
                    self.secret_store.as_ref(),
                    &self.broker_id,
                )
                .await?;
                let summary = self.remote_session_summary()?;
                let secrets = self.remote_session_secrets(&summary)?;
                let tool_name = request
                    .tool_name
                    .ok_or_else(|| anyhow::anyhow!("Missing tool name."))?;
                self.remote_client
                    .call_tool(&summary, &secrets, &dpop_keys, &tool_name, request.args)
                    .await?
            }
            _ => bail!("CLI request failed."),
        };

        Ok(BrokerResponse {
            broker_proof: String::new(),
            id: request.id,
            ok: true,
            result: Some(result),
            error: None,
        })
    }

    fn verify_client_grant(&self, request: &BrokerRequest) -> Result<()> {
        let credentials = ClientGrantCredentials {
            client_key: request
                .client_key
                .clone()
                .ok_or_else(missing_client_grant_error)?,
        };
        verify_client_grant(self.secret_store.as_ref(), &self.broker_id, &credentials)
    }

    fn remote_session_summary(&self) -> Result<super::session::BrokerRemoteSessionSummary> {
        Ok(read_broker_remote_session_snapshot(&self.runtime_paths)?
            .ok_or_else(|| PublicBrokerError::new("The Driggsby CLI is not connected."))?
            .session)
    }

    fn remote_session_secrets(
        &self,
        summary: &super::session::BrokerRemoteSessionSummary,
    ) -> Result<super::secrets::BrokerRemoteSessionSecrets> {
        let secrets =
            read_broker_remote_session_secrets(self.secret_store.as_ref(), &self.broker_id)?
                .ok_or_else(|| PublicBrokerError::new("The Driggsby CLI is not connected."))?;
        verify_broker_remote_session_binding(summary, &secrets)?;
        Ok(secrets)
    }
}

fn public_broker_error_message(error: &anyhow::Error) -> String {
    if let Some(public_error) = error.downcast_ref::<PublicBrokerError>() {
        return public_error.message().to_string();
    }

    "Driggsby could not complete that request. Check the input and try again.\n\nNext:\n  npx driggsby@latest status".to_string()
}
