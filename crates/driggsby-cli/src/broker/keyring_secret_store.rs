use anyhow::Result;
use rand::Rng;

use super::secret_store::SecretStore;

#[derive(Debug, Clone)]
pub struct KeyringSecretStore {
    service_name: String,
}

impl Default for KeyringSecretStore {
    fn default() -> Self {
        Self {
            service_name: "driggsby.cli".to_string(),
        }
    }
}

impl KeyringSecretStore {
    pub fn is_available(&self) -> bool {
        let mut probe = [0_u8; 12];
        rand::rng().fill_bytes(&mut probe);
        let account = format!("driggsby-probe-{}", hex_string(&probe));
        let secret = format!("probe-{}", hex_string(&probe));

        let entry = match keyring::Entry::new(&self.service_name, &account) {
            Ok(entry) => entry,
            Err(_) => return false,
        };

        let set_ok = entry.set_password(&secret).is_ok();
        let read_ok = entry.get_password().ok().as_deref() == Some(secret.as_str());
        let _ = entry.delete_credential();
        set_ok && read_ok
    }
}

impl SecretStore for KeyringSecretStore {
    fn set_secret(&self, account: &str, secret: &str) -> Result<()> {
        keyring::Entry::new(&self.service_name, account)?.set_password(secret)?;
        Ok(())
    }

    fn get_secret(&self, account: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(&self.service_name, account)?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn delete_secret(&self, account: &str) -> Result<bool> {
        let entry = keyring::Entry::new(&self.service_name, account)?;
        match entry.delete_credential() {
            Ok(()) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

fn hex_string(bytes: &[u8]) -> String {
    let mut rendered = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(rendered, "{byte:02x}");
    }
    rendered
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use serial_test::serial;

    use super::{KeyringSecretStore, SecretStore};

    #[test]
    #[serial]
    fn keyring_round_trips_probe_style_account_names() -> Result<()> {
        let store = KeyringSecretStore::default();
        if !store.is_available() {
            return Ok(());
        }

        let account = "driggsby-probe-test-account";
        let secret = "probe-secret";

        store.set_secret(account, secret)?;
        let loaded = store.get_secret(account)?;
        let removed = store.delete_secret(account)?;

        assert_eq!(loaded.as_deref(), Some(secret));
        assert!(removed);
        Ok(())
    }

    #[test]
    #[serial]
    fn keyring_round_trips_broker_style_account_names() -> Result<()> {
        let store = KeyringSecretStore::default();
        if !store.is_available() {
            return Ok(());
        }

        let account = "driggsby__019d754f-2ca2-73b0-bf51-3c689d49c469__dpop-private-jwk";
        let secret = "{\"kty\":\"EC\"}";

        store.set_secret(account, secret)?;
        let loaded = store.get_secret(account)?;
        let removed = store.delete_secret(account)?;

        assert_eq!(loaded.as_deref(), Some(secret));
        assert!(removed);
        Ok(())
    }
}
