use anyhow::{Result, bail};

use crate::runtime_paths::RuntimePaths;

use super::{
    file_secret_store::FileSecretStore,
    installation::read_broker_metadata,
    keyring_secret_store::{KeyringAvailability, KeyringSecretStore},
    secret_store::SecretStore,
};

const KEYRING_UNAVAILABLE_WITH_EXISTING_INSTALL_MESSAGE: &str = "This Driggsby CLI install already depends on platform secure storage, but that storage is unavailable in this shell. Reopen the original desktop session or restore keyring access before using this install.";
const LOGOUT_FALLBACK_NOTICE: &str = "Platform secure storage is unavailable here, so logout will clear local CLI files only. Any unreachable platform-keyring entries will remain until you return to the original session.";

pub struct ResolvedSecretStore {
    pub backend: &'static str,
    pub notice: Option<String>,
    pub store: Box<dyn SecretStore>,
}

pub fn resolve_secret_store(runtime_paths: &RuntimePaths) -> Result<ResolvedSecretStore> {
    let file_store = FileSecretStore::new(runtime_paths);
    if file_store.has_stored_secrets()? {
        return Ok(ResolvedSecretStore {
            backend: "file",
            notice: None,
            store: Box::new(file_store),
        });
    }

    let keyring_store = KeyringSecretStore::default();
    let keyring_availability = keyring_store.availability();
    let prefer_keyring_for_fresh_install = keyring_store.is_preferred_for_fresh_install();
    resolve_secret_store_without_file_secrets(
        runtime_paths,
        file_store,
        keyring_availability,
        prefer_keyring_for_fresh_install,
    )
}

#[cfg(test)]
fn resolve_secret_store_with_keyring_policy(
    runtime_paths: &RuntimePaths,
    keyring_availability: KeyringAvailability,
    prefer_keyring_for_fresh_install: bool,
) -> Result<ResolvedSecretStore> {
    let file_store = FileSecretStore::new(runtime_paths);
    if file_store.has_stored_secrets()? {
        return Ok(ResolvedSecretStore {
            backend: "file",
            notice: None,
            store: Box::new(file_store),
        });
    }

    resolve_secret_store_without_file_secrets(
        runtime_paths,
        file_store,
        keyring_availability,
        prefer_keyring_for_fresh_install,
    )
}

fn resolve_secret_store_without_file_secrets(
    runtime_paths: &RuntimePaths,
    file_store: FileSecretStore,
    keyring_availability: KeyringAvailability,
    prefer_keyring_for_fresh_install: bool,
) -> Result<ResolvedSecretStore> {
    let metadata_present = read_broker_metadata(runtime_paths)?.is_some();
    if metadata_present && !matches!(keyring_availability, KeyringAvailability::Available) {
        bail!(KEYRING_UNAVAILABLE_WITH_EXISTING_INSTALL_MESSAGE);
    }

    if matches!(keyring_availability, KeyringAvailability::Available)
        && (metadata_present || prefer_keyring_for_fresh_install)
    {
        return Ok(ResolvedSecretStore {
            backend: "keyring",
            notice: None,
            store: Box::new(KeyringSecretStore::default()),
        });
    }

    Ok(ResolvedSecretStore {
        backend: "file",
        notice: Some(fallback_notice(runtime_paths)),
        store: Box::new(file_store),
    })
}

fn fallback_notice(runtime_paths: &RuntimePaths) -> String {
    format!(
        "Platform secure storage is unavailable here. Driggsby will use an owner-only file-backed secret store under {}.",
        runtime_paths.config_dir.display()
    )
}

pub fn resolve_secret_store_for_logout(
    runtime_paths: &RuntimePaths,
) -> Result<ResolvedSecretStore> {
    match resolve_secret_store(runtime_paths) {
        Ok(store) => Ok(store),
        Err(_) => Ok(ResolvedSecretStore {
            backend: "file",
            notice: Some(LOGOUT_FALLBACK_NOTICE.to_string()),
            store: Box::new(FileSecretStore::new(runtime_paths)),
        }),
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::{
        auth::dpop::Jwk,
        broker::{
            file_secret_store::FileSecretStore,
            keyring_secret_store::KeyringAvailability,
            secret_store::SecretStore,
            types::{BrokerDpopMetadata, BrokerMetadata},
        },
        json_file::write_json_file,
        runtime_paths::RuntimePaths,
    };

    use super::resolve_secret_store_with_keyring_policy;

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

    fn write_metadata(runtime_paths: &RuntimePaths) -> Result<()> {
        write_json_file(
            &runtime_paths.metadata_path,
            &BrokerMetadata {
                schema_version: 1,
                broker_id: "019d754f-2ca2-73b0-bf51-3c689d49c469".to_string(),
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
                    thumbprint: "thumbprint".to_string(),
                },
            },
        )
    }

    #[test]
    fn fresh_install_falls_back_to_file_store_when_keyring_is_unavailable() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let runtime_paths = test_runtime_paths(&temp_dir);

        let resolved = resolve_secret_store_with_keyring_policy(
            &runtime_paths,
            KeyringAvailability::Unavailable,
            false,
        )?;

        assert_eq!(resolved.backend, "file");
        assert!(
            resolved
                .notice
                .as_deref()
                .is_some_and(|notice| notice.contains("Platform secure storage is unavailable"))
        );
        Ok(())
    }

    #[test]
    fn existing_install_does_not_fork_state_when_keyring_is_unavailable() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let runtime_paths = test_runtime_paths(&temp_dir);
        write_metadata(&runtime_paths)?;

        let error = resolve_secret_store_with_keyring_policy(
            &runtime_paths,
            KeyringAvailability::Unavailable,
            false,
        )
        .err()
        .map(|error| error.to_string());

        assert!(error.is_some_and(|message| {
            message.contains("already depends on platform secure storage")
                && message.contains("unavailable in this shell")
        }));
        Ok(())
    }

    #[test]
    fn existing_file_store_wins_over_keyring_availability() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let runtime_paths = test_runtime_paths(&temp_dir);
        let file_store = FileSecretStore::new(&runtime_paths);
        file_store.set_secret("account", "secret")?;

        let resolved = resolve_secret_store_with_keyring_policy(
            &runtime_paths,
            KeyringAvailability::Available,
            true,
        )?;

        assert_eq!(resolved.backend, "file");
        assert!(resolved.notice.is_none());
        Ok(())
    }

    #[test]
    fn fresh_install_uses_file_store_when_keyring_is_not_preferred() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let runtime_paths = test_runtime_paths(&temp_dir);

        let resolved = resolve_secret_store_with_keyring_policy(
            &runtime_paths,
            KeyringAvailability::Available,
            false,
        )?;

        assert_eq!(resolved.backend, "file");
        Ok(())
    }

    #[test]
    fn existing_install_uses_keyring_when_available_even_if_not_fresh_preferred() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let runtime_paths = test_runtime_paths(&temp_dir);
        write_metadata(&runtime_paths)?;

        let resolved = resolve_secret_store_with_keyring_policy(
            &runtime_paths,
            KeyringAvailability::Available,
            false,
        )?;

        assert_eq!(resolved.backend, "keyring");
        Ok(())
    }
}
