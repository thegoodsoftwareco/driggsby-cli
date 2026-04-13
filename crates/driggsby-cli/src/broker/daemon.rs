use std::sync::Arc;

use anyhow::Result;

use crate::runtime_paths::RuntimePaths;

use super::{
    installation::{dpop_key_pair_for_installation, read_broker_installation_with_secrets},
    public_error::PublicBrokerError,
    remote_mcp::RemoteMcpClient,
    resolve_secret_store::resolve_secret_store,
    server::LocalBrokerServer,
};

pub async fn run_broker_daemon(runtime_paths: &RuntimePaths) -> Result<()> {
    let resolved_secret_store = resolve_secret_store(runtime_paths)?;
    let secret_store = resolved_secret_store.store;
    let installed = read_broker_installation_with_secrets(runtime_paths, secret_store.as_ref())?
        .ok_or_else(|| {
            PublicBrokerError::new(
                crate::user_guidance::build_reauthentication_required_message(
                    "The Driggsby CLI is not connected",
                ),
            )
        })?;
    let auth_token = installed.secrets.local_auth_token.clone();
    let dpop_keys = dpop_key_pair_for_installation(&installed);
    let server = LocalBrokerServer::bind(
        auth_token,
        RemoteMcpClient::new()?,
        runtime_paths.clone(),
        Arc::from(secret_store),
        installed.metadata.broker_id,
    )
    .await?;
    server.run(dpop_keys).await
}
