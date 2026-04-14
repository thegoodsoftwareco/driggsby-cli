use anyhow::Result;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    public_error::PublicBrokerError,
    secret_store::SecretStore,
    secrets::{read_broker_secret_bundle, write_broker_secret_bundle},
};

pub const CLIENT_KEY_ENV: &str = "DRIGGSBY_CLIENT_KEY";

const CLIENT_GRANT_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedClientGrant {
    pub grant: BrokerClientGrant,
    pub client_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientGrantCredentials {
    pub client_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrokerClientGrant {
    pub schema_version: u8,
    pub grant_id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_id: Option<String>,
    pub client_key_sha256: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disconnected_at: Option<String>,
}

impl BrokerClientGrant {
    pub fn is_active(&self) -> bool {
        self.schema_version == CLIENT_GRANT_SCHEMA_VERSION && self.disconnected_at.is_none()
    }
}

pub fn create_client_grant(
    secret_store: &dyn SecretStore,
    broker_id: &str,
    display_name: &str,
    integration_id: Option<&str>,
) -> Result<CreatedClientGrant> {
    let mut bundle = read_broker_secret_bundle(secret_store, broker_id)?
        .ok_or_else(|| anyhow::anyhow!("The local CLI auth state is incomplete."))?;
    let now = now_rfc3339()?;
    let grant_id = generate_token("lc_", 16);
    let client_key = generate_token("dby_client_v1_", 32);
    let grant = BrokerClientGrant {
        schema_version: CLIENT_GRANT_SCHEMA_VERSION,
        grant_id,
        display_name: display_name.to_string(),
        integration_id: integration_id.map(ToString::to_string),
        client_key_sha256: hash_client_key(&client_key),
        created_at: now,
        last_used_at: None,
        disconnected_at: None,
    };

    bundle.client_grants.push(grant.clone());
    write_broker_secret_bundle(secret_store, broker_id, &bundle)?;
    Ok(CreatedClientGrant { grant, client_key })
}

pub fn disconnect_other_grants_for_integration(
    secret_store: &dyn SecretStore,
    broker_id: &str,
    integration_id: &str,
    keep_grant_id: &str,
) -> Result<()> {
    let mut bundle = read_broker_secret_bundle(secret_store, broker_id)?
        .ok_or_else(|| anyhow::anyhow!("The local CLI auth state is incomplete."))?;
    let now = now_rfc3339()?;
    let mut changed = false;
    for grant in &mut bundle.client_grants {
        if grant.disconnected_at.is_none()
            && grant.grant_id != keep_grant_id
            && grant.integration_id.as_deref() == Some(integration_id)
        {
            grant.disconnected_at = Some(now.clone());
            changed = true;
        }
    }
    if changed {
        write_broker_secret_bundle(secret_store, broker_id, &bundle)?;
    }
    Ok(())
}

pub fn list_client_grants(
    secret_store: &dyn SecretStore,
    broker_id: &str,
) -> Result<Vec<BrokerClientGrant>> {
    let Some(bundle) = read_broker_secret_bundle(secret_store, broker_id)? else {
        return Ok(Vec::new());
    };
    Ok(bundle
        .client_grants
        .into_iter()
        .filter(BrokerClientGrant::is_active)
        .collect())
}

pub fn disconnect_client_grant(
    secret_store: &dyn SecretStore,
    broker_id: &str,
    selector: &str,
) -> Result<Vec<BrokerClientGrant>> {
    let mut bundle = read_broker_secret_bundle(secret_store, broker_id)?
        .ok_or_else(|| anyhow::anyhow!("The local CLI auth state is incomplete."))?;
    let now = now_rfc3339()?;
    let mut disconnected = Vec::new();
    for grant in &mut bundle.client_grants {
        if grant.disconnected_at.is_none() && grant.integration_id.as_deref() == Some(selector) {
            grant.disconnected_at = Some(now.clone());
            disconnected.push(grant.clone());
        }
    }
    if !disconnected.is_empty() {
        write_broker_secret_bundle(secret_store, broker_id, &bundle)?;
    }
    Ok(disconnected)
}

pub fn verify_client_grant(
    secret_store: &dyn SecretStore,
    broker_id: &str,
    credentials: &ClientGrantCredentials,
) -> Result<()> {
    let bundle =
        read_broker_secret_bundle(secret_store, broker_id)?.ok_or_else(not_connected_error)?;
    let client_key_hash = hash_client_key(&credentials.client_key);
    if bundle
        .client_grants
        .iter()
        .any(|grant| grant.is_active() && grant.client_key_sha256 == client_key_hash)
    {
        return Ok(());
    }
    Err(not_connected_error().into())
}

pub fn missing_client_grant_error() -> PublicBrokerError {
    PublicBrokerError::new(
        "Client not connected to Driggsby.\n\nNext:\n  npx driggsby@latest mcp connect",
    )
}

fn not_connected_error() -> PublicBrokerError {
    PublicBrokerError::new(
        "Client not connected or was disconnected.\n\nNext:\n  npx driggsby@latest mcp connect",
    )
}

fn generate_token(prefix: &str, byte_count: usize) -> String {
    let mut bytes = vec![0_u8; byte_count];
    rand::rng().fill_bytes(&mut bytes);
    format!("{prefix}{}", hex_string(&bytes))
}

fn hash_client_key(client_key: &str) -> String {
    hex_string(&Sha256::digest(client_key.as_bytes()))
}

fn hex_string(bytes: &[u8]) -> String {
    let mut rendered = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(rendered, "{byte:02x}");
    }
    rendered
}

fn now_rfc3339() -> Result<String> {
    Ok(time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339)?)
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Mutex};

    use anyhow::Result;

    use crate::{auth::dpop::Jwk, broker::secret_store::SecretStore};

    use super::{
        ClientGrantCredentials, create_client_grant, disconnect_client_grant, list_client_grants,
        verify_client_grant,
    };

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
    fn grants_can_be_verified_and_disconnected() -> Result<()> {
        let store = TestSecretStore::default();
        let broker_id = "broker-id";
        let bundle = crate::broker::secrets::BrokerSecretBundle::new(
            "local-token".to_string(),
            test_private_jwk(),
        );
        crate::broker::secrets::write_broker_secret_bundle(&store, broker_id, &bundle)?;

        let created = create_client_grant(&store, broker_id, "Claude Code", Some("claude-code"))?;
        assert_eq!(list_client_grants(&store, broker_id)?.len(), 1);

        verify_client_grant(
            &store,
            broker_id,
            &ClientGrantCredentials {
                client_key: created.client_key.clone(),
            },
        )?;

        assert_eq!(
            disconnect_client_grant(&store, broker_id, "claude-code")?.len(),
            1
        );
        assert!(list_client_grants(&store, broker_id)?.is_empty());
        let error = verify_client_grant(
            &store,
            broker_id,
            &ClientGrantCredentials {
                client_key: created.client_key,
            },
        )
        .err()
        .map(|error| error.to_string());
        assert!(error.is_some_and(|message| message.contains("not connected")));
        Ok(())
    }

    #[test]
    fn disconnect_does_not_match_display_name() -> Result<()> {
        let store = TestSecretStore::default();
        let broker_id = "broker-id";
        let bundle = crate::broker::secrets::BrokerSecretBundle::new(
            "local-token".to_string(),
            test_private_jwk(),
        );
        crate::broker::secrets::write_broker_secret_bundle(&store, broker_id, &bundle)?;

        create_client_grant(&store, broker_id, "Raycast CLI", None)?;

        assert!(disconnect_client_grant(&store, broker_id, "Raycast CLI")?.is_empty());
        assert_eq!(list_client_grants(&store, broker_id)?.len(), 1);
        Ok(())
    }

    #[test]
    fn verify_client_grant_does_not_mutate_secret_bundle() -> Result<()> {
        let store = TestSecretStore::default();
        let broker_id = "broker-id";
        let bundle = crate::broker::secrets::BrokerSecretBundle::new(
            "local-token".to_string(),
            test_private_jwk(),
        );
        crate::broker::secrets::write_broker_secret_bundle(&store, broker_id, &bundle)?;

        let created = create_client_grant(&store, broker_id, "Codex", Some("codex"))?;
        let before = crate::broker::secrets::read_broker_secret_bundle(&store, broker_id)?;

        verify_client_grant(
            &store,
            broker_id,
            &ClientGrantCredentials {
                client_key: created.client_key,
            },
        )?;

        let before =
            before.ok_or_else(|| anyhow::anyhow!("expected test bundle before verification"))?;
        let after = crate::broker::secrets::read_broker_secret_bundle(&store, broker_id)?
            .ok_or_else(|| anyhow::anyhow!("expected test bundle after verification"))?;
        assert_eq!(before.client_grants, after.client_grants);
        Ok(())
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
