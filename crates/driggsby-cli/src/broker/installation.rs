use std::fs;

use crate::{
    auth::dpop::generate_dpop_key_material,
    json_file::{read_json_file, remove_file_if_present, write_json_file},
    runtime_paths::RuntimePaths,
    user_guidance::DRIGGSBY_CONNECT_COMMAND,
};
use anyhow::Result;
use rand::Rng;

use super::{
    file_secret_store::FileSecretStore,
    remote_session::inspect_remote_session_readiness,
    secret_store::SecretStore,
    secrets::{
        BrokerSecretBundle, delete_broker_secret_bundle, dpop_key_pair_from_bundle,
        read_broker_dpop_key_pair as read_dpop_key_pair_from_bundle,
        read_broker_local_auth_token as read_local_auth_token_from_bundle,
        read_broker_secret_bundle, write_broker_secret_bundle,
    },
    session::{clear_broker_remote_session_snapshot, read_broker_remote_session_snapshot},
    types::{
        BrokerDpopMetadata, BrokerMetadata, BrokerReadiness, BrokerRemoteAccessState, BrokerStatus,
    },
};

pub use super::secrets::BrokerDpopKeyPair;
const BROKER_METADATA_SCHEMA_VERSION: u8 = 2;

pub struct BrokerInstallationSecrets {
    pub metadata: BrokerMetadata,
    pub secrets: BrokerSecretBundle,
}

pub async fn ensure_broker_installation(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
) -> Result<BrokerMetadata> {
    Ok(
        ensure_broker_installation_with_secrets(runtime_paths, secret_store)
            .await?
            .metadata,
    )
}

pub async fn ensure_broker_installation_with_secrets(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
) -> Result<BrokerInstallationSecrets> {
    if let Some(installed) = read_broker_installation_with_secrets(runtime_paths, secret_store)? {
        return Ok(installed);
    }

    let broker_id = uuid::Uuid::now_v7().to_string();
    let dpop = generate_dpop_key_material()?;
    let metadata = BrokerMetadata {
        schema_version: BROKER_METADATA_SCHEMA_VERSION,
        broker_id: broker_id.clone(),
        created_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)?,
        dpop: BrokerDpopMetadata {
            algorithm: dpop.algorithm.clone(),
            public_jwk: dpop.public_jwk.clone(),
            thumbprint: dpop.thumbprint.clone(),
        },
    };
    let secrets = BrokerSecretBundle::new(generate_local_auth_token(), dpop.private_jwk);

    clear_broker_remote_session_snapshot(runtime_paths)?;
    write_broker_secret_bundle(secret_store, &broker_id, &secrets)?;
    write_json_file(&runtime_paths.metadata_path, &metadata)?;

    Ok(BrokerInstallationSecrets { metadata, secrets })
}

pub fn read_broker_installation_with_secrets(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
) -> Result<Option<BrokerInstallationSecrets>> {
    let Some(metadata) = read_broker_metadata(runtime_paths)? else {
        return Ok(None);
    };
    let Some(secrets) = read_broker_secret_bundle(secret_store, &metadata.broker_id)? else {
        return Ok(None);
    };
    Ok(Some(BrokerInstallationSecrets { metadata, secrets }))
}

pub fn inspect_broker_readiness(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
) -> Result<BrokerReadiness> {
    let Some(metadata) = read_broker_metadata(runtime_paths)? else {
        return Ok(BrokerReadiness {
            installed: false,
            broker_id: None,
            dpop_thumbprint: None,
            local_auth_token_present: false,
            private_key_present: false,
            remote_session_present: false,
        });
    };

    let bundle = read_broker_secret_bundle(secret_store, &metadata.broker_id)?;
    let local_auth_token_present = bundle
        .as_ref()
        .is_some_and(|secrets| !secrets.local_auth_token.is_empty());
    let private_key_present = bundle
        .as_ref()
        .is_some_and(|secrets| secrets.dpop_private_jwk.d.is_some());
    let remote_session_present = bundle
        .as_ref()
        .and_then(|secrets| secrets.remote_session_secrets.as_ref())
        .is_some();

    Ok(BrokerReadiness {
        installed: local_auth_token_present && private_key_present,
        broker_id: Some(metadata.broker_id.clone()),
        dpop_thumbprint: Some(metadata.dpop.thumbprint.clone()),
        local_auth_token_present,
        private_key_present,
        remote_session_present,
    })
}

pub async fn build_broker_status(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    broker_running: bool,
) -> Result<BrokerStatus> {
    let readiness = inspect_broker_readiness(runtime_paths, secret_store)?;
    let remote = match &readiness.broker_id {
        Some(broker_id) => {
            inspect_remote_session_readiness(runtime_paths, secret_store, broker_id, true).await?
        }
        None => inspect_remote_session_readiness(runtime_paths, secret_store, "", false).await?,
    };

    Ok(BrokerStatus {
        installed: readiness.installed && readiness.private_key_present,
        broker_running,
        broker_id: readiness.broker_id,
        dpop_thumbprint: readiness.dpop_thumbprint,
        remote_mcp_ready: remote.ready,
        remote_access_detail: Some(remote.detail),
        remote_access_state: Some(remote.state),
        next_step_command: remote.next_step_command,
        remote_session: remote.session,
        socket_path: runtime_paths.socket_path.display().to_string(),
    })
}

pub fn resolve_broker_status_for_display(
    runtime_paths: &RuntimePaths,
    live_status: Option<BrokerStatus>,
    local_server_running: bool,
) -> Result<BrokerStatus> {
    if let Some(ref status) = live_status
        && status.remote_access_detail.is_some()
        && status.remote_access_state.is_some()
    {
        return Ok(status.clone());
    }
    build_display_status_from_local_state(runtime_paths, local_server_running)
}

pub fn clear_broker_installation(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
) -> Result<()> {
    if let Some(metadata) = read_broker_metadata(runtime_paths).ok().flatten() {
        let _ = delete_broker_secret_bundle(secret_store, &metadata.broker_id)?;
    }
    clear_broker_remote_session_snapshot(runtime_paths)?;
    FileSecretStore::new(runtime_paths).clear_all_files()?;
    remove_file_if_present(&runtime_paths.metadata_path)?;
    #[cfg(not(windows))]
    remove_file_if_present(&runtime_paths.socket_path)?;
    remove_empty_directory(&runtime_paths.config_dir)?;
    remove_empty_directory(&runtime_paths.state_dir)?;
    Ok(())
}

pub fn read_broker_metadata(runtime_paths: &RuntimePaths) -> Result<Option<BrokerMetadata>> {
    let metadata: Option<BrokerMetadata> = read_json_file(&runtime_paths.metadata_path)?;
    Ok(metadata.filter(|value| value.schema_version == BROKER_METADATA_SCHEMA_VERSION))
}

pub fn read_broker_local_auth_token(
    secret_store: &dyn SecretStore,
    broker_id: &str,
) -> Result<Option<String>> {
    read_local_auth_token_from_bundle(secret_store, broker_id)
}

pub fn read_broker_dpop_key_pair(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    broker_id: &str,
) -> Result<Option<BrokerDpopKeyPair>> {
    let Some(metadata) = read_broker_metadata(runtime_paths)? else {
        return Ok(None);
    };
    if metadata.broker_id != broker_id {
        return Ok(None);
    }
    read_dpop_key_pair_from_bundle(secret_store, &metadata)
}

pub fn dpop_key_pair_for_installation(installed: &BrokerInstallationSecrets) -> BrokerDpopKeyPair {
    dpop_key_pair_from_bundle(&installed.metadata, &installed.secrets)
}

fn generate_local_auth_token() -> String {
    let mut bytes = [0_u8; 16];
    rand::rng().fill_bytes(&mut bytes);
    let mut rendered = String::with_capacity(bytes.len() * 2);
    for byte in &bytes {
        use std::fmt::Write as _;
        let _ = write!(rendered, "{byte:02x}");
    }
    rendered
}

fn build_display_status_from_local_state(
    runtime_paths: &RuntimePaths,
    local_server_running: bool,
) -> Result<BrokerStatus> {
    let metadata = read_broker_metadata(runtime_paths)?;
    let snapshot = if metadata.is_some() {
        read_broker_remote_session_snapshot(runtime_paths)?
    } else {
        None
    };
    let remote_session = snapshot.map(|stored| stored.session);
    let (remote_mcp_ready, remote_access_state, remote_access_detail, next_step_command) =
        match remote_session.as_ref() {
            None => (
                false,
                BrokerRemoteAccessState::NotConnected,
                "Not signed in yet.".to_string(),
                Some(DRIGGSBY_CONNECT_COMMAND.to_string()),
            ),
            Some(session)
                if session_has_comfortable_headroom(session.access_token_expires_at.as_str()) =>
            {
                (
                    true,
                    BrokerRemoteAccessState::Ready,
                    "Driggsby is ready.".to_string(),
                    None,
                )
            }
            Some(_) => (
                false,
                BrokerRemoteAccessState::TemporarilyUnavailable,
                "Driggsby will reconnect automatically on next use.".to_string(),
                Some(DRIGGSBY_CONNECT_COMMAND.to_string()),
            ),
        };

    Ok(BrokerStatus {
        installed: metadata.is_some(),
        broker_running: local_server_running,
        broker_id: metadata.as_ref().map(|value| value.broker_id.clone()),
        dpop_thumbprint: metadata.as_ref().map(|value| value.dpop.thumbprint.clone()),
        remote_mcp_ready,
        remote_access_detail: Some(remote_access_detail),
        remote_access_state: Some(remote_access_state),
        next_step_command,
        remote_session,
        socket_path: runtime_paths.socket_path.display().to_string(),
    })
}

fn session_has_comfortable_headroom(expires_at: &str) -> bool {
    let Ok(expires_at) =
        time::OffsetDateTime::parse(expires_at, &time::format_description::well_known::Rfc3339)
    else {
        return false;
    };

    let remaining_seconds = (expires_at - time::OffsetDateTime::now_utc()).whole_seconds();
    remaining_seconds > 60
}

fn remove_empty_directory(path: &std::path::Path) -> Result<()> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound
                || error.kind() == std::io::ErrorKind::DirectoryNotEmpty =>
        {
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Mutex};

    use anyhow::Result;

    use crate::{
        auth::dpop::Jwk,
        broker::{
            secret_store::SecretStore,
            secrets::{broker_secrets_account_name, read_broker_secret_bundle},
            types::{BrokerDpopMetadata, BrokerMetadata, BrokerRemoteAccessState},
        },
        json_file::write_json_file,
        runtime_paths::RuntimePaths,
    };

    use super::{
        ensure_broker_installation_with_secrets, read_broker_installation_with_secrets,
        resolve_broker_status_for_display,
    };

    #[derive(Default)]
    struct TestSecretStore {
        secrets: Mutex<BTreeMap<String, String>>,
    }

    impl TestSecretStore {
        fn accounts(&self) -> Result<Vec<String>> {
            Ok(self
                .secrets
                .lock()
                .map_err(|_| anyhow::anyhow!("test secret lock failed"))?
                .keys()
                .cloned()
                .collect())
        }
    }

    impl SecretStore for TestSecretStore {
        fn set_secret(&self, account: &str, secret: &str) -> Result<()> {
            self.secrets
                .lock()
                .map_err(|_| anyhow::anyhow!("test secret lock failed"))?
                .insert(account.to_string(), secret.to_string());
            Ok(())
        }

        fn get_secret(&self, account: &str) -> Result<Option<String>> {
            Ok(self
                .secrets
                .lock()
                .map_err(|_| anyhow::anyhow!("test secret lock failed"))?
                .get(account)
                .cloned())
        }

        fn delete_secret(&self, account: &str) -> Result<bool> {
            Ok(self
                .secrets
                .lock()
                .map_err(|_| anyhow::anyhow!("test secret lock failed"))?
                .remove(account)
                .is_some())
        }
    }

    #[tokio::test]
    async fn fresh_install_writes_one_secret_bundle() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let runtime_paths = test_runtime_paths(&temp_dir);
        let secret_store = TestSecretStore::default();

        let installed =
            ensure_broker_installation_with_secrets(&runtime_paths, &secret_store).await?;
        let accounts = secret_store.accounts()?;

        assert_eq!(
            accounts,
            vec![broker_secrets_account_name(&installed.metadata.broker_id)]
        );
        assert_eq!(installed.metadata.schema_version, 2);
        assert_eq!(installed.secrets.schema_version, 1);
        assert!(installed.secrets.remote_session_secrets.is_none());
        assert!(read_broker_secret_bundle(&secret_store, &installed.metadata.broker_id)?.is_some());
        Ok(())
    }

    #[test]
    fn unsupported_metadata_does_not_reuse_old_session_snapshot() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let runtime_paths = test_runtime_paths(&temp_dir);
        write_test_metadata(&runtime_paths, 1, "old-broker-id")?;
        write_json_file(
            &runtime_paths.session_snapshot_path,
            &serde_json::json!({
                "schema_version": 1,
                "session": {
                    "access_token_expires_at": "2099-01-01T00:00:00Z",
                    "authenticated_at": "2026-04-10T02:15:54Z",
                    "client_id": "old-client",
                    "issuer": "https://auth.example.test",
                    "redirect_uri": "http://127.0.0.1/callback",
                    "resource": "https://mcp.example.test",
                    "scope": "driggsby.default",
                    "token_type": "DPoP"
                }
            }),
        )?;

        let status = resolve_broker_status_for_display(&runtime_paths, None, false)?;

        assert!(!status.installed);
        assert!(!status.remote_mcp_ready);
        assert_eq!(
            status.remote_access_state,
            Some(BrokerRemoteAccessState::NotConnected)
        );
        assert_eq!(
            status.next_step_command.as_deref(),
            Some("npx driggsby@latest mcp setup")
        );
        assert!(status.remote_session.is_none());
        Ok(())
    }

    #[test]
    fn read_installation_does_not_create_bundle_for_unsupported_metadata() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let runtime_paths = test_runtime_paths(&temp_dir);
        let secret_store = TestSecretStore::default();
        write_test_metadata(&runtime_paths, 1, "old-broker-id")?;

        let installed = read_broker_installation_with_secrets(&runtime_paths, &secret_store)?;

        assert!(installed.is_none());
        assert!(secret_store.accounts()?.is_empty());
        Ok(())
    }

    #[test]
    fn malformed_secret_bundle_returns_safe_reconnect_guidance() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let runtime_paths = test_runtime_paths(&temp_dir);
        let secret_store = TestSecretStore::default();
        let broker_id = "019d754f-2ca2-73b0-bf51-3c689d49c469";
        write_test_metadata(&runtime_paths, 2, broker_id)?;
        secret_store.set_secret(&broker_secrets_account_name(broker_id), "{not-json")?;

        let message = read_broker_installation_with_secrets(&runtime_paths, &secret_store)
            .err()
            .map(|error| error.to_string());

        assert!(message.is_some_and(|value| {
            value.contains("local CLI auth state is incomplete")
                && value.contains("npx driggsby@latest mcp setup")
                && !value.contains("expected")
        }));
        Ok(())
    }

    fn write_test_metadata(
        runtime_paths: &RuntimePaths,
        schema_version: u8,
        broker_id: &str,
    ) -> Result<()> {
        write_json_file(
            &runtime_paths.metadata_path,
            &BrokerMetadata {
                schema_version,
                broker_id: broker_id.to_string(),
                created_at: "2026-04-10T02:15:54Z".to_string(),
                dpop: BrokerDpopMetadata {
                    algorithm: "ES256".to_string(),
                    public_jwk: Jwk {
                        kty: "EC".to_string(),
                        crv: "P-256".to_string(),
                        x: "x".to_string(),
                        y: "y".to_string(),
                        d: None,
                    },
                    thumbprint: "test-thumbprint".to_string(),
                },
            },
        )
    }

    fn test_runtime_paths(temp_dir: &tempfile::TempDir) -> RuntimePaths {
        let config_dir = temp_dir.path().join("config");
        let state_dir = temp_dir.path().join("state");
        RuntimePaths {
            metadata_path: config_dir.join("cli-metadata.json"),
            session_snapshot_path: config_dir.join("cli-session.json"),
            socket_path: state_dir.join("cli.sock"),
            lock_path: state_dir.join("cli.lock"),
            config_dir,
            state_dir,
        }
    }
}
