use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rand::Rng;
use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use serde::{Deserialize, Serialize};

use crate::{
    json_file::{read_json_file, remove_file_if_present, write_json_file},
    runtime_paths::RuntimePaths,
};

use super::secret_store::SecretStore;

const FILE_SECRET_KEY_BYTES: usize = 32;
const FILE_SECRET_STORE_INCOMPLETE_MESSAGE: &str = "The local Driggsby file-backed secret store is incomplete. Run `npx driggsby@latest logout` and then `npx driggsby@latest login`.";
const FILE_SECRET_STORE_INVALID_MESSAGE: &str = "The local Driggsby file-backed secret store is invalid. Run `npx driggsby@latest logout` and then `npx driggsby@latest login`.";
const FILE_SECRET_STORE_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSecretsFile {
    schema_version: u8,
    secrets: BTreeMap<String, StoredSecretRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredSecretRecord {
    auth_tag_base64: String,
    ciphertext_base64: String,
    iv_base64: String,
}

#[derive(Debug, Clone)]
pub struct FileSecretStore {
    encryption_key_path: PathBuf,
    secrets_path: PathBuf,
}

impl FileSecretStore {
    pub fn new(runtime_paths: &RuntimePaths) -> Self {
        Self {
            encryption_key_path: runtime_paths.state_dir.join("cli-secrets.key"),
            secrets_path: runtime_paths.config_dir.join("cli-secrets.json"),
        }
    }

    pub fn has_stored_secrets(&self) -> Result<bool> {
        let stored = self.read_stored_secrets_file()?;
        Ok(stored.map(|file| !file.secrets.is_empty()).unwrap_or(false))
    }

    fn read_stored_secrets_file(&self) -> Result<Option<StoredSecretsFile>> {
        read_json_file(&self.secrets_path)
    }

    pub fn clear_all_files(&self) -> Result<()> {
        remove_file_if_present(&self.secrets_path)?;
        remove_file_if_present(&self.encryption_key_path)?;
        Ok(())
    }

    fn read_or_create_encryption_key(&self) -> Result<Vec<u8>> {
        if let Some(existing) = self.read_encryption_key(false)? {
            return Ok(existing);
        }

        create_owner_only_directory(
            self.secrets_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Missing secrets directory."))?,
        )?;
        create_owner_only_directory(
            self.encryption_key_path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Missing key directory."))?,
        )?;

        let mut key = vec![0_u8; FILE_SECRET_KEY_BYTES];
        rand::rng().fill_bytes(&mut key);
        fs::write(&self.encryption_key_path, STANDARD.encode(&key)).with_context(|| {
            format!(
                "Could not write local secret-store key at {}",
                self.encryption_key_path.display()
            )
        })?;
        set_owner_only_permissions(&self.encryption_key_path)?;
        Ok(key)
    }

    fn read_encryption_key(&self, error_if_missing: bool) -> Result<Option<Vec<u8>>> {
        match fs::read_to_string(&self.encryption_key_path) {
            Ok(contents) => {
                let decoded = STANDARD
                    .decode(contents.trim())
                    .context(FILE_SECRET_STORE_INVALID_MESSAGE)?;
                if decoded.len() != FILE_SECRET_KEY_BYTES {
                    bail!(FILE_SECRET_STORE_INVALID_MESSAGE);
                }
                Ok(Some(decoded))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !error_if_missing => {
                Ok(None)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && error_if_missing => {
                bail!(FILE_SECRET_STORE_INCOMPLETE_MESSAGE)
            }
            Err(error) => Err(error).with_context(|| {
                format!(
                    "Could not read local secret-store key at {}",
                    self.encryption_key_path.display()
                )
            }),
        }
    }
}

impl SecretStore for FileSecretStore {
    fn set_secret(&self, account: &str, secret: &str) -> Result<()> {
        let encryption_key = self.read_or_create_encryption_key()?;
        let mut stored = self
            .read_stored_secrets_file()?
            .unwrap_or(StoredSecretsFile {
                schema_version: FILE_SECRET_STORE_SCHEMA_VERSION,
                secrets: BTreeMap::new(),
            });
        stored.secrets.insert(
            account.to_string(),
            encrypt_secret(secret, &encryption_key)?,
        );
        write_json_file(&self.secrets_path, &stored)?;
        Ok(())
    }

    fn get_secret(&self, account: &str) -> Result<Option<String>> {
        let stored = self.read_stored_secrets_file()?;
        let Some(file) = stored else {
            return Ok(None);
        };
        let Some(secret) = file.secrets.get(account) else {
            return Ok(None);
        };
        let encryption_key = self
            .read_encryption_key(true)?
            .ok_or_else(|| anyhow::anyhow!(FILE_SECRET_STORE_INCOMPLETE_MESSAGE))?;
        Ok(Some(decrypt_secret(secret, &encryption_key)?))
    }

    fn delete_secret(&self, account: &str) -> Result<bool> {
        let Some(mut stored) = self.read_stored_secrets_file()? else {
            return Ok(false);
        };
        if stored.secrets.remove(account).is_none() {
            return Ok(false);
        }
        if stored.secrets.is_empty() {
            remove_file_if_present(&self.secrets_path)?;
            remove_file_if_present(&self.encryption_key_path)?;
            return Ok(true);
        }
        write_json_file(&self.secrets_path, &stored)?;
        Ok(true)
    }
}

fn encrypt_secret(secret: &str, encryption_key: &[u8]) -> Result<StoredSecretRecord> {
    let mut iv = [0_u8; NONCE_LEN];
    rand::rng().fill_bytes(&mut iv);
    let unbound = UnboundKey::new(&AES_256_GCM, encryption_key)
        .map_err(|_| anyhow::anyhow!(FILE_SECRET_STORE_INVALID_MESSAGE))?;
    let key = LessSafeKey::new(unbound);
    let nonce = Nonce::assume_unique_for_key(iv);
    let mut in_out = secret.as_bytes().to_vec();
    key.seal_in_place_append_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|_| anyhow::anyhow!(FILE_SECRET_STORE_INVALID_MESSAGE))?;
    let tag_index = in_out
        .len()
        .checked_sub(16)
        .ok_or_else(|| anyhow::anyhow!(FILE_SECRET_STORE_INVALID_MESSAGE))?;
    let (ciphertext, auth_tag) = in_out.split_at(tag_index);

    Ok(StoredSecretRecord {
        auth_tag_base64: STANDARD.encode(auth_tag),
        ciphertext_base64: STANDARD.encode(ciphertext),
        iv_base64: STANDARD.encode(iv),
    })
}

fn decrypt_secret(secret: &StoredSecretRecord, encryption_key: &[u8]) -> Result<String> {
    let iv = STANDARD.decode(&secret.iv_base64)?;
    let auth_tag = STANDARD.decode(&secret.auth_tag_base64)?;
    let ciphertext = STANDARD.decode(&secret.ciphertext_base64)?;
    if iv.len() != NONCE_LEN {
        bail!(FILE_SECRET_STORE_INVALID_MESSAGE);
    }

    let mut in_out = ciphertext;
    in_out.extend_from_slice(&auth_tag);
    let unbound = UnboundKey::new(&AES_256_GCM, encryption_key)
        .map_err(|_| anyhow::anyhow!(FILE_SECRET_STORE_INVALID_MESSAGE))?;
    let key = LessSafeKey::new(unbound);
    let mut nonce_bytes = [0_u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&iv);
    let plaintext = key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce_bytes),
            Aad::empty(),
            &mut in_out,
        )
        .map_err(|_| anyhow::anyhow!(FILE_SECRET_STORE_INVALID_MESSAGE))?;
    Ok(String::from_utf8(plaintext.to_vec())?)
}

fn create_owner_only_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path)?;
    set_owner_only_permissions(path)
}

fn set_owner_only_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::runtime_paths::RuntimePaths;

    use super::{FileSecretStore, SecretStore};

    #[test]
    fn file_secret_store_round_trips_without_platform_keyring() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let config_dir = temp_dir.path().join("config");
        let state_dir = temp_dir.path().join("state");
        let runtime_paths = RuntimePaths {
            metadata_path: config_dir.join("cli-metadata.json"),
            session_snapshot_path: config_dir.join("cli-session.json"),
            socket_path: state_dir.join("cli.sock"),
            lock_path: state_dir.join("cli.lock"),
            config_dir,
            state_dir,
        };
        let store = FileSecretStore::new(&runtime_paths);

        store.set_secret("account", "secret")?;
        assert_eq!(store.get_secret("account")?.as_deref(), Some("secret"));
        assert!(store.has_stored_secrets()?);
        assert!(store.delete_secret("account")?);
        assert_eq!(store.get_secret("account")?, None);
        assert!(!store.has_stored_secrets()?);

        Ok(())
    }
}
