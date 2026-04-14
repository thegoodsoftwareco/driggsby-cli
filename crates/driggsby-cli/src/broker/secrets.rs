use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    auth::dpop::Jwk, runtime_paths::RuntimePaths,
    user_guidance::build_reauthentication_required_message,
};

use super::{
    grants::BrokerClientGrant,
    public_error::PublicBrokerError,
    secret_store::SecretStore,
    session::{BrokerRemoteSession, read_broker_remote_session_snapshot},
    types::BrokerMetadata,
};

const BROKER_SECRETS_ACCOUNT_SUFFIX: &str = "broker-secrets";
const BROKER_SECRET_BUNDLE_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerSecretBundle {
    pub schema_version: u8,
    pub local_auth_token: String,
    pub dpop_private_jwk: Jwk,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_grants: Vec<BrokerClientGrant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_session_secrets: Option<BrokerRemoteSessionSecrets>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerRemoteSessionSecrets {
    pub schema_version: u8,
    pub access_token: String,
    pub public_binding_sha256: String,
    pub refresh_token: String,
}

#[derive(Debug, Clone)]
pub struct BrokerDpopKeyPair {
    pub private_jwk: Jwk,
    pub public_jwk: Jwk,
}

impl BrokerSecretBundle {
    pub fn new(local_auth_token: String, dpop_private_jwk: Jwk) -> Self {
        Self {
            schema_version: BROKER_SECRET_BUNDLE_SCHEMA_VERSION,
            local_auth_token,
            dpop_private_jwk,
            client_grants: Vec::new(),
            remote_session_secrets: None,
        }
    }

    fn is_supported(&self) -> bool {
        self.schema_version == BROKER_SECRET_BUNDLE_SCHEMA_VERSION
    }
}

pub fn read_broker_secret_bundle(
    secret_store: &dyn SecretStore,
    broker_id: &str,
) -> Result<Option<BrokerSecretBundle>> {
    let Some(raw) = secret_store.get_secret(&broker_secrets_account_name(broker_id))? else {
        return Ok(None);
    };
    let parsed: BrokerSecretBundle = serde_json::from_str(&raw).map_err(|_| {
        PublicBrokerError::new(build_reauthentication_required_message(
            "The local CLI auth state is incomplete",
        ))
    })?;
    Ok(parsed.is_supported().then_some(parsed))
}

pub fn write_broker_secret_bundle(
    secret_store: &dyn SecretStore,
    broker_id: &str,
    bundle: &BrokerSecretBundle,
) -> Result<()> {
    secret_store.set_secret(
        &broker_secrets_account_name(broker_id),
        &serde_json::to_string(bundle)?,
    )
}

pub fn delete_broker_secret_bundle(
    secret_store: &dyn SecretStore,
    broker_id: &str,
) -> Result<bool> {
    secret_store.delete_secret(&broker_secrets_account_name(broker_id))
}

pub fn read_broker_local_auth_token(
    secret_store: &dyn SecretStore,
    broker_id: &str,
) -> Result<Option<String>> {
    Ok(read_broker_secret_bundle(secret_store, broker_id)?.map(|bundle| bundle.local_auth_token))
}

pub fn read_broker_remote_session(
    runtime_paths: &RuntimePaths,
    secret_store: &dyn SecretStore,
    broker_id: &str,
) -> Result<Option<BrokerRemoteSession>> {
    let Some(bundle) = read_broker_secret_bundle(secret_store, broker_id)? else {
        return Ok(None);
    };
    let Some(secrets) = bundle.remote_session_secrets else {
        return Ok(None);
    };
    let Some(snapshot) = read_broker_remote_session_snapshot(runtime_paths)? else {
        return Ok(None);
    };
    let summary = snapshot.session;
    // Keep URL-bearing public metadata outside the secret bundle for CodeQL and
    // bind it here so local JSON edits cannot redirect token-bearing requests.
    if secrets.public_binding_sha256 != remote_session_public_binding_sha256(&summary) {
        return Err(
            PublicBrokerError::new(build_reauthentication_required_message(
                "The local CLI auth state is incomplete",
            ))
            .into(),
        );
    }
    Ok(Some(BrokerRemoteSession {
        schema_version: secrets.schema_version,
        access_token: secrets.access_token,
        access_token_expires_at: summary.access_token_expires_at,
        authenticated_at: summary.authenticated_at,
        client_id: summary.client_id,
        issuer: summary.issuer,
        redirect_uri: summary.redirect_uri,
        refresh_token: secrets.refresh_token,
        resource: summary.resource,
        scope: summary.scope,
        token_type: summary.token_type,
    }))
}

pub fn read_broker_remote_session_secrets(
    secret_store: &dyn SecretStore,
    broker_id: &str,
) -> Result<Option<BrokerRemoteSessionSecrets>> {
    Ok(read_broker_secret_bundle(secret_store, broker_id)?
        .and_then(|bundle| bundle.remote_session_secrets))
}

pub fn verify_broker_remote_session_binding(
    summary: &super::session::BrokerRemoteSessionSummary,
    secrets: &BrokerRemoteSessionSecrets,
) -> Result<()> {
    if secrets.public_binding_sha256 == remote_session_public_binding_sha256(summary) {
        return Ok(());
    }
    Err(
        PublicBrokerError::new(build_reauthentication_required_message(
            "The local CLI auth state is incomplete",
        ))
        .into(),
    )
}

pub fn write_broker_remote_session(
    secret_store: &dyn SecretStore,
    broker_id: &str,
    session: &BrokerRemoteSession,
) -> Result<()> {
    let mut bundle = read_broker_secret_bundle(secret_store, broker_id)?
        .ok_or_else(|| anyhow::anyhow!("The local CLI auth state is incomplete."))?;
    bundle.remote_session_secrets = Some(BrokerRemoteSessionSecrets {
        schema_version: session.schema_version,
        access_token: session.access_token.clone(),
        public_binding_sha256: remote_session_public_binding_sha256(
            &super::session::summarize_broker_remote_session(session),
        ),
        refresh_token: session.refresh_token.clone(),
    });
    write_broker_secret_bundle(secret_store, broker_id, &bundle)
}

pub fn remote_session_public_binding_sha256(
    summary: &super::session::BrokerRemoteSessionSummary,
) -> String {
    let mut hasher = Sha256::new();
    for part in [
        summary.access_token_expires_at.as_str(),
        summary.authenticated_at.as_str(),
        summary.client_id.as_str(),
        summary.issuer.as_str(),
        summary.redirect_uri.as_str(),
        summary.resource.as_str(),
        summary.scope.as_str(),
        summary.token_type.as_str(),
    ] {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }

    let digest = hasher.finalize();
    let mut rendered = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(rendered, "{byte:02x}");
    }
    rendered
}

pub fn dpop_key_pair_from_bundle(
    metadata: &BrokerMetadata,
    bundle: &BrokerSecretBundle,
) -> BrokerDpopKeyPair {
    BrokerDpopKeyPair {
        private_jwk: bundle.dpop_private_jwk.clone(),
        public_jwk: metadata.dpop.public_jwk.clone(),
    }
}

pub fn read_broker_dpop_key_pair(
    secret_store: &dyn SecretStore,
    metadata: &BrokerMetadata,
) -> Result<Option<BrokerDpopKeyPair>> {
    Ok(
        read_broker_secret_bundle(secret_store, &metadata.broker_id)?
            .map(|bundle| dpop_key_pair_from_bundle(metadata, &bundle)),
    )
}

pub fn broker_secrets_account_name(broker_id: &str) -> String {
    format!("driggsby__{broker_id}__{BROKER_SECRETS_ACCOUNT_SUFFIX}")
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Mutex};

    use anyhow::Result;

    use crate::{
        auth::dpop::Jwk,
        broker::{
            secret_store::SecretStore,
            session::{BrokerRemoteSession, write_broker_remote_session_snapshot},
        },
        runtime_paths::RuntimePaths,
    };

    use super::{read_broker_remote_session, write_broker_remote_session};

    #[derive(Default)]
    struct TestSecretStore {
        secrets: Mutex<BTreeMap<String, String>>,
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

    #[test]
    fn remote_session_rejects_tampered_public_snapshot() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let runtime_paths = test_runtime_paths(&temp_dir);
        let secret_store = TestSecretStore::default();
        let broker_id = "broker-id";
        let session = test_session("https://mcp.example.test/mcp");
        let mut bundle =
            super::BrokerSecretBundle::new("local-token".to_string(), test_private_jwk());
        super::write_broker_secret_bundle(&secret_store, broker_id, &bundle)?;
        write_broker_remote_session(&secret_store, broker_id, &session)?;

        let tampered = test_session("https://evil.example.test/mcp");
        write_broker_remote_session_snapshot(&runtime_paths, &tampered)?;

        let error = read_broker_remote_session(&runtime_paths, &secret_store, broker_id)
            .err()
            .map(|error| error.to_string());

        assert!(error.is_some_and(|message| {
            message.contains("local CLI auth state is incomplete")
                && message.contains("npx driggsby@latest mcp setup")
        }));

        // Keep this mutable variable live long enough to prove the initial write
        // path had no hidden dependency on the tampered snapshot.
        bundle.remote_session_secrets = None;
        Ok(())
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

    fn test_session(resource: &str) -> BrokerRemoteSession {
        BrokerRemoteSession {
            schema_version: 1,
            access_token: "access-token".to_string(),
            access_token_expires_at: "2099-01-01T00:00:00Z".to_string(),
            authenticated_at: "2099-01-01T00:00:00Z".to_string(),
            client_id: "client-id".to_string(),
            issuer: "https://auth.example.test".to_string(),
            redirect_uri: "http://127.0.0.1/callback".to_string(),
            refresh_token: "refresh-token".to_string(),
            resource: resource.to_string(),
            scope: "driggsby.default".to_string(),
            token_type: "DPoP".to_string(),
        }
    }

    fn test_private_jwk() -> Jwk {
        Jwk {
            kty: "EC".to_string(),
            crv: "P-256".to_string(),
            x: "x".to_string(),
            y: "y".to_string(),
            d: Some("d".to_string()),
        }
    }
}
