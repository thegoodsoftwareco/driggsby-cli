use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::session::BrokerRemoteSessionSummary;
use crate::auth::dpop::Jwk;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BrokerRemoteAccessState {
    #[serde(rename = "ready")]
    Ready,
    #[serde(rename = "not_connected")]
    NotConnected,
    #[serde(rename = "reauth_required")]
    ReauthRequired,
    #[serde(rename = "temporarily_unavailable")]
    TemporarilyUnavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerMetadata {
    pub schema_version: u8,
    pub broker_id: String,
    pub created_at: String,
    pub dpop: BrokerDpopMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerDpopMetadata {
    pub algorithm: String,
    pub public_jwk: Jwk,
    pub thumbprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerReadiness {
    pub installed: bool,
    pub broker_id: Option<String>,
    pub dpop_thumbprint: Option<String>,
    pub local_auth_token_present: bool,
    pub private_key_present: bool,
    pub remote_session_present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerStatus {
    pub installed: bool,
    pub broker_running: bool,
    pub broker_id: Option<String>,
    pub dpop_thumbprint: Option<String>,
    pub remote_mcp_ready: bool,
    pub remote_access_detail: Option<String>,
    pub remote_access_state: Option<BrokerRemoteAccessState>,
    pub next_step_command: Option<String>,
    pub remote_session: Option<BrokerRemoteSessionSummary>,
    pub socket_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResult {
    pub ok: bool,
    pub broker_id: String,
    pub cli_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetStatusResult {
    pub status: BrokerStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownResult {
    pub stopped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerProofClaims {
    pub aud: String,
    pub challenge: String,
    pub payload_sha256: String,
    pub request_id: String,
    pub request_method: String,
    pub sub: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerRequest {
    pub auth_token: String,
    pub challenge: String,
    pub id: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerResponse {
    pub broker_proof: String,
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
