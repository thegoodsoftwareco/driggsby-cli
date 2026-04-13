use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    json_file::{read_json_file, remove_file_if_present, write_json_file},
    runtime_paths::RuntimePaths,
};

const SESSION_SNAPSHOT_SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerRemoteSession {
    pub schema_version: u8,
    pub access_token: String,
    pub access_token_expires_at: String,
    pub authenticated_at: String,
    pub client_id: String,
    pub issuer: String,
    pub redirect_uri: String,
    pub refresh_token: String,
    pub resource: String,
    pub scope: String,
    pub token_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerRemoteSessionSummary {
    pub access_token_expires_at: String,
    pub authenticated_at: String,
    pub client_id: String,
    pub issuer: String,
    pub redirect_uri: String,
    pub resource: String,
    pub scope: String,
    pub token_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerRemoteSessionSnapshot {
    pub schema_version: u8,
    pub session: BrokerRemoteSessionSummary,
}

pub fn read_broker_remote_session_snapshot(
    runtime_paths: &RuntimePaths,
) -> Result<Option<BrokerRemoteSessionSnapshot>> {
    read_json_file(&runtime_paths.session_snapshot_path)
}

pub fn write_broker_remote_session_snapshot(
    runtime_paths: &RuntimePaths,
    session: &BrokerRemoteSession,
) -> Result<()> {
    write_json_file(
        &runtime_paths.session_snapshot_path,
        &BrokerRemoteSessionSnapshot {
            schema_version: SESSION_SNAPSHOT_SCHEMA_VERSION,
            session: summarize_broker_remote_session(session),
        },
    )
}

pub fn clear_broker_remote_session_snapshot(runtime_paths: &RuntimePaths) -> Result<()> {
    remove_file_if_present(&runtime_paths.session_snapshot_path)
}

pub fn summarize_broker_remote_session(
    session: &BrokerRemoteSession,
) -> BrokerRemoteSessionSummary {
    BrokerRemoteSessionSummary {
        access_token_expires_at: session.access_token_expires_at.clone(),
        authenticated_at: session.authenticated_at.clone(),
        client_id: session.client_id.clone(),
        issuer: session.issuer.clone(),
        redirect_uri: session.redirect_uri.clone(),
        resource: session.resource.clone(),
        scope: session.scope.clone(),
        token_type: session.token_type.clone(),
    }
}
