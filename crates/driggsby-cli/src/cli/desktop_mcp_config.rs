use std::{
    env, fs,
    io::Write as _,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use rand::Rng;
use serde_json::{Map, Value, json};

use crate::broker::grants::{CLIENT_KEY_ENV, CreatedClientGrant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DesktopMcpConfigClient {
    ClaudeDesktop,
}

impl DesktopMcpConfigClient {
    fn config_path(self) -> Result<PathBuf> {
        if !cfg!(target_os = "macos") {
            bail!("Claude Desktop automatic setup is only supported on macOS.");
        }
        let home = env::var_os("HOME").ok_or_else(|| {
            anyhow::anyhow!("Driggsby could not find the home directory for this user.")
        })?;
        let relative = match self {
            Self::ClaudeDesktop => "Library/Application Support/Claude/claude_desktop_config.json",
        };
        Ok(PathBuf::from(home).join(relative))
    }
}

pub(super) fn install_desktop_mcp_config(
    client: DesktopMcpConfigClient,
    created: &CreatedClientGrant,
) -> Result<()> {
    let path = client.config_path()?;
    install_desktop_mcp_config_at_path(&path, created)
}

fn install_desktop_mcp_config_at_path(path: &Path, created: &CreatedClientGrant) -> Result<()> {
    let mut config = read_desktop_config(path)?.unwrap_or_else(|| Value::Object(Map::new()));
    let object = config_object_mut(&mut config)?;
    let servers = object
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    let servers = servers
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("The MCP server config is invalid."))?;
    servers.insert("driggsby".to_string(), server_config(created));
    write_desktop_config(path, &config)?;
    Ok(())
}

fn server_config(created: &CreatedClientGrant) -> Value {
    let mut env = Map::new();
    env.insert(
        CLIENT_KEY_ENV.to_string(),
        Value::String(created.client_key.clone()),
    );
    json!({
        "command": "npx",
        "args": ["-y", "driggsby@latest", "mcp-server"],
        "env": env,
    })
}

fn read_desktop_config(path: &Path) -> Result<Option<Value>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(serde_json::from_str(&contents).map_err(|_| {
            anyhow::anyhow!(
                "{} MCP config is not valid JSON.",
                infer_display_name_from_path(path)
            )
        })?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| {
            format!(
                "Driggsby could not read {} MCP config.",
                infer_display_name_from_path(path)
            )
        }),
    }
}

fn config_object_mut(config: &mut Value) -> Result<&mut Map<String, Value>> {
    config
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("The MCP config must be a JSON object."))
}

fn write_desktop_config(path: &Path, config: &Value) -> Result<()> {
    let Some(parent) = path.parent() else {
        bail!("Driggsby could not resolve the MCP config directory.");
    };
    fs::create_dir_all(parent)?;
    let contents = serde_json::to_string_pretty(config)?;
    let temp_path = temporary_path(path);
    let mut file = create_owner_only_file(&temp_path)?;
    file.write_all(format!("{contents}\n").as_bytes())
        .with_context(|| {
            format!(
                "Driggsby could not write {} MCP config.",
                infer_display_name_from_path(path)
            )
        })?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temp_path, path).with_context(|| {
        format!(
            "Driggsby could not replace {} MCP config.",
            infer_display_name_from_path(path)
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn create_owner_only_file(path: &Path) -> Result<fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .with_context(|| "Driggsby could not create a temporary MCP config file.")
    }

    #[cfg(not(unix))]
    {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| "Driggsby could not create a temporary MCP config file.")
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    let process_id = std::process::id();
    let mut nonce = [0_u8; 8];
    rand::rng().fill_bytes(&mut nonce);
    let nonce = hex_string(&nonce);
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "mcp-config".to_string());
    path.with_file_name(format!("{name}.{process_id}.{nonce}.tmp"))
}

fn hex_string(bytes: &[u8]) -> String {
    let mut rendered = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(rendered, "{byte:02x}");
    }
    rendered
}

fn infer_display_name_from_path(path: &Path) -> &'static str {
    if path.to_string_lossy().contains("Claude") {
        return "Claude Desktop";
    }
    "desktop client"
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::install_desktop_mcp_config_at_path;

    #[test]
    fn claude_desktop_config_round_trips() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let path = temp_dir.path().join("claude_desktop_config.json");
        let created = crate::broker::grants::CreatedClientGrant {
            grant: crate::broker::grants::BrokerClientGrant {
                schema_version: 1,
                grant_id: "lc_id".to_string(),
                display_name: "Claude Desktop".to_string(),
                integration_id: Some("claude-desktop".to_string()),
                client_key_sha256: "hash".to_string(),
                created_at: "2026-04-13T00:00:00Z".to_string(),
                last_used_at: None,
                disconnected_at: None,
            },
            client_key: "dby_client_v1_secret".to_string(),
        };

        install_desktop_mcp_config_at_path(&path, &created)?;
        let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
        assert_eq!(
            config["mcpServers"]["driggsby"],
            json!({
                "command": "npx",
                "args": ["-y", "driggsby@latest", "mcp-server"],
                "env": {
                    "DRIGGSBY_CLIENT_KEY": "dby_client_v1_secret",
                },
            })
        );

        Ok(())
    }
}
