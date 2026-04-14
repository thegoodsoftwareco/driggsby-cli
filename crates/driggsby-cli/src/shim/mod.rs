use std::{env, path::Path, sync::Arc};

use anyhow::Result;
use rmcp::{
    ServerHandler, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, Content, ErrorCode, ErrorData, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
    },
    service::{RequestContext, RoleServer},
    transport::stdio,
};

use crate::{
    broker::public_error::PublicBrokerError,
    broker::{
        client::{call_broker_tool, list_broker_tools},
        grants::{CLIENT_KEY_ENV, ClientGrantCredentials, missing_client_grant_error},
        launch::ensure_broker_running,
        resolve_secret_store::resolve_secret_store,
    },
    cli::format::format_status_text,
    runtime_paths::RuntimePaths,
};

const LOCAL_STATUS_TOOL_NAME: &str = "get_local_cli_status";

#[derive(Clone)]
struct BrokerShimServer {
    client_credentials: Option<ClientGrantCredentials>,
    runtime_paths: RuntimePaths,
    secret_store: Arc<dyn crate::broker::secret_store::SecretStore>,
}

impl ServerHandler for BrokerShimServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        let remote = self.remote_tools().await.map_err(to_mcp_error)?;
        let mut tools = vec![local_status_tool().map_err(to_mcp_error)?];
        tools.extend(remote);
        Ok(ListToolsResult {
            tools,
            meta: None,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        if request.name.as_ref() == LOCAL_STATUS_TOOL_NAME {
            let status_text = self.local_status_text().await.map_err(to_mcp_error)?;
            return Ok(CallToolResult::success(vec![Content::text(status_text)]));
        }

        let Some(client_credentials) = self.client_credentials.as_ref() else {
            return Err(to_mcp_error(missing_client_grant_error().into()));
        };
        let tools = self.remote_tools().await.map_err(to_mcp_error)?;
        if !tools.iter().any(|tool| tool.name == request.name.as_ref()) {
            return Err(ErrorData::new(
                ErrorCode::INVALID_REQUEST,
                "That Driggsby tool is not available in this session anymore. Ask the client to refresh its tool list and try again.",
                None,
            ));
        }

        let result = call_broker_tool(
            &self.runtime_paths,
            self.secret_store.as_ref(),
            client_credentials,
            request.name.as_ref(),
            request.arguments.map(serde_json::Value::Object),
        )
        .await
        .map_err(to_mcp_error)?
        .ok_or_else(|| {
            ErrorData::new(
                ErrorCode::INTERNAL_ERROR,
                "The local Driggsby CLI service is not responding yet.",
                None,
            )
        })?;
        serde_json::from_value(result).map_err(|error| to_mcp_error(error.into()))
    }
}

impl BrokerShimServer {
    async fn remote_tools(&self) -> Result<Vec<Tool>> {
        let Some(client_credentials) = self.client_credentials.as_ref() else {
            return Ok(Vec::new());
        };
        let tools = list_broker_tools(
            &self.runtime_paths,
            self.secret_store.as_ref(),
            client_credentials,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("The local Driggsby CLI service is not responding yet."))?;
        let raw_tools = tools
            .get("tools")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        raw_tools
            .into_iter()
            .map(serde_json::from_value)
            .collect::<std::result::Result<Vec<Tool>, _>>()
            .map_err(Into::into)
    }

    async fn local_status_text(&self) -> Result<String> {
        let status = crate::broker::client::get_broker_status(
            &self.runtime_paths,
            self.secret_store.as_ref(),
        )
        .await?;
        let status = crate::broker::installation::resolve_broker_status_for_display(
            &self.runtime_paths,
            status,
            true,
        )?;
        Ok(format_status_text(&status))
    }
}

pub async fn run_mcp_server_command(
    runtime_paths: &RuntimePaths,
    current_exe: &Path,
) -> Result<()> {
    let resolved_secret_store = resolve_secret_store(runtime_paths)?;
    let secret_store: Arc<dyn crate::broker::secret_store::SecretStore> =
        Arc::from(resolved_secret_store.store);
    ensure_broker_running(runtime_paths, secret_store.as_ref(), current_exe).await?;
    let service = BrokerShimServer {
        client_credentials: read_client_credentials_from_env(),
        runtime_paths: runtime_paths.clone(),
        secret_store,
    }
    .serve(stdio())
    .await?;
    service.waiting().await?;
    Ok(())
}

fn read_client_credentials_from_env() -> Option<ClientGrantCredentials> {
    let client_key = env::var(CLIENT_KEY_ENV).ok()?;
    Some(ClientGrantCredentials { client_key })
}

fn local_status_tool() -> Result<Tool> {
    Ok(Tool::new(
        LOCAL_STATUS_TOOL_NAME,
        "Report readiness and connectivity for the local Driggsby CLI.",
        Arc::new(serde_json::from_value(serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }))?),
    ))
}

fn to_mcp_error(error: anyhow::Error) -> ErrorData {
    if let Some(public_error) = error.downcast_ref::<PublicBrokerError>() {
        return ErrorData::new(
            ErrorCode::INTERNAL_ERROR,
            public_error.message().to_string(),
            None,
        );
    }

    ErrorData::new(
        ErrorCode::INTERNAL_ERROR,
        "Driggsby could not complete that request. Check the input and try again.\n\nNext:\n  npx driggsby@latest status",
        None,
    )
}
