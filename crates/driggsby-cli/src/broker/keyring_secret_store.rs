use anyhow::Result;
use rand::Rng;

use super::secret_store::SecretStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyringAvailability {
    Available,
    Unavailable,
}

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
    pub fn availability(&self) -> KeyringAvailability {
        if !keyring_session_is_configured() {
            return KeyringAvailability::Unavailable;
        }

        #[cfg(target_os = "linux")]
        {
            return KeyringAvailability::Available;
        }

        #[cfg(not(target_os = "linux"))]
        {
            if self.probe_available() {
                KeyringAvailability::Available
            } else {
                KeyringAvailability::Unavailable
            }
        }
    }

    pub fn is_preferred_for_fresh_install(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            return linux_graphical_session_is_configured(|key| std::env::var_os(key));
        }

        #[cfg(not(target_os = "linux"))]
        {
            true
        }
    }

    pub fn is_available(&self) -> bool {
        self.probe_available()
    }

    fn probe_available(&self) -> bool {
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

fn keyring_session_is_configured() -> bool {
    #[cfg(target_os = "linux")]
    {
        return linux_secret_service_session_is_configured(|key| std::env::var_os(key));
    }

    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

#[cfg(target_os = "linux")]
fn linux_secret_service_session_is_configured(
    mut get_var: impl FnMut(&str) -> Option<std::ffi::OsString>,
) -> bool {
    get_var("DBUS_SESSION_BUS_ADDRESS").is_some_and(|value| !value.is_empty())
}

#[cfg(target_os = "linux")]
fn linux_graphical_session_is_configured(
    mut get_var: impl FnMut(&str) -> Option<std::ffi::OsString>,
) -> bool {
    ["DISPLAY", "WAYLAND_DISPLAY"]
        .iter()
        .any(|key| get_var(key).is_some_and(|value| !value.is_empty()))
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

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_keyring_requires_dbus_session_before_probe() {
        use std::ffi::OsString;

        assert!(!super::linux_secret_service_session_is_configured(|_| None));
        assert!(super::linux_secret_service_session_is_configured(|key| {
            (key == "DBUS_SESSION_BUS_ADDRESS")
                .then(|| OsString::from("unix:path=/run/user/1000/bus"))
        }));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_fresh_keyring_preference_requires_graphical_session() {
        use std::ffi::OsString;

        assert!(!super::linux_graphical_session_is_configured(|_| None));
        assert!(super::linux_graphical_session_is_configured(|key| {
            (key == "DISPLAY").then(|| OsString::from(":0"))
        }));
    }

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
