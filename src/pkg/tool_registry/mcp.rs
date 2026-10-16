//! MCP (Model Context Protocol) tool provider.
//!
//! MCP tools are database-registered tools. `ToolPo.config` stores only the
//! binding from a standard tool record to an MCP server/tool pair:
//! `{ "server_id": "...", "tool_name": "..." }`.
//! Server connection details, credentials, and commands belong to
//! `McpServerPo.config` and must not be duplicated into each MCP tool config.

use crate::models::mcp_server::{McpServerConfig, McpServerPo, McpTransport};
use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::shell_env;
use anyhow::anyhow;
use async_trait::async_trait;
use common::enums::ToolProtocol;
use common::error::{Result, err};
use reqwest::header::{HeaderName, HeaderValue};
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::{
    RoleClient, ServiceExt, model::CallToolRequestParams, service::RunningService,
    transport::TokioChildProcess,
};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::process::Command;

/// `McpToolConfig` 已下沉至 [`common::config::McpToolConfig`]（前后端共享 SSOT），此处仅转发兼容；
/// 其不含 server 凭据/传输配置，MCP DAL 另加载被引用的 `McpServerPo` 传给运行时工厂。
pub use common::config::McpToolConfig;

/// Tool metadata discovered from a remote MCP server via `tools/list`.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteMcpTool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

/// MCP 凭据注入值（凭据解析链经 `check()` 产出，绑定形态随值携带）。
///
/// 消费按 transport 分派：Stdio 取 Env 形态（子进程环境变量）；StreamableHttp 取
/// Header 形态（连接请求头，initialize 及后续请求均带，http.rs 先例语义）与 Query
/// 形态（endpoint URL 查询参数）。绑定形态不可丢：裸 (name, value) 混存无法区分
/// 注入点（http.rs 先例即 Header/Query 分桶承载）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpCredentialInjection {
    /// 注入 stdio 子进程环境变量
    Env { name: String, value: String },
    /// 注入 StreamableHttp 请求头
    Header { name: String, value: String },
    /// 注入 StreamableHttp endpoint URL 查询参数
    Query { name: String, value: String },
}

/// Minimal MCP client runtime.
///
/// Both transports use per-call short-lived sessions (stdio spawns a fresh
/// child process per call; Streamable HTTP opens a fresh StreamableHttpClient
/// session per call/list and closes it afterwards), so credentials only exist
/// for the duration of a single call. Credential injections carry their
/// binding kind (env/header/query) and each transport consumes the proper
/// injection point. Streamable HTTP connection targets are admin-configured
/// server URLs (management-plane gated, same trust model as stdio commands).
#[derive(Debug, Default)]
pub struct McpClientRuntime {
    invalidated_servers: Mutex<HashSet<String>>,
}

impl McpClientRuntime {
    pub fn invalidate_server(&self, server_id: &str) {
        self.invalidated_servers
            .lock()
            .unwrap()
            .insert(server_id.to_string());
    }

    fn clear_invalidated_server(&self, server_id: &str) {
        self.invalidated_servers.lock().unwrap().remove(server_id);
    }

    pub async fn call_tool(
        &self,
        server: &McpServerPo,
        tool_name: &str,
        args: Value,
        credential_injections: &[McpCredentialInjection],
    ) -> Result<Value> {
        let arguments = match args {
            Value::Object(map) => map,
            _other => {
                return Err(anyhow!("MCP tool arguments must be a JSON object").into());
            }
        };
        let result = match server.transport {
            McpTransport::Stdio => {
                let env_injections = env_injection_pairs(credential_injections);
                self.call_stdio_tool(server, tool_name, arguments, &env_injections)
                    .await
            }
            McpTransport::StreamableHttp => {
                self.call_http_tool(server, tool_name, arguments, credential_injections)
                    .await
            }
        };
        if result.is_ok() {
            self.clear_invalidated_server(&server.id);
        }
        result
    }

    pub async fn list_tools(
        &self,
        server: &McpServerPo,
        credential_injections: &[McpCredentialInjection],
    ) -> Result<Vec<RemoteMcpTool>> {
        let result = match server.transport {
            McpTransport::Stdio => {
                let env_injections = env_injection_pairs(credential_injections);
                self.list_stdio_tools(server, &env_injections).await
            }
            McpTransport::StreamableHttp => {
                self.list_http_tools(server, credential_injections).await
            }
        };
        if result.is_ok() {
            self.clear_invalidated_server(&server.id);
        }
        result
    }

    async fn list_stdio_tools(
        &self,
        server: &McpServerPo,
        env_injections: &[(String, String)],
    ) -> Result<Vec<RemoteMcpTool>> {
        let config = server.config();
        let mut client = connect_stdio_client(server, &config, env_injections).await?;

        let list_result = match tokio::time::timeout(
            Duration::from_millis(config.timeout_ms),
            client.peer().list_all_tools(),
        )
        .await
        {
            Ok(Ok(tools)) => Ok(tools),
            Ok(Err(_e)) => Err(anyhow!("MCP tools/list on server {} failed", server.id)),
            Err(_) => Err(anyhow!(
                "MCP tools/list on server {} timed out after {}ms",
                server.id,
                config.timeout_ms
            )),
        };

        let close_result = client.close().await;
        let tools = list_result?;
        if let Err(_e) = close_result {
            return Err(anyhow!(mcp_session_close_failed_message(
                "stdio",
                &server.id,
                "tools/list",
            ))
            .into());
        }

        Ok(tools
            .into_iter()
            .map(|tool| RemoteMcpTool {
                name: tool.name.to_string(),
                description: tool.description.map(|description| description.to_string()),
                input_schema: Value::Object(tool.input_schema.as_ref().clone()),
            })
            .collect())
    }

    async fn call_stdio_tool(
        &self,
        server: &McpServerPo,
        tool_name: &str,
        arguments: Map<String, Value>,
        env_injections: &[(String, String)],
    ) -> Result<Value> {
        let config = server.config();
        let mut client = connect_stdio_client(server, &config, env_injections).await?;

        let params = CallToolRequestParams::new(tool_name.to_string()).with_arguments(arguments);

        let call_result = match tokio::time::timeout(
            Duration::from_millis(config.timeout_ms),
            client.peer().call_tool(params),
        )
        .await
        {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_e)) => Err(anyhow!(
                "MCP tool {} on server {} call failed",
                tool_name,
                server.id
            )),
            Err(_) => Err(anyhow!(
                "MCP tool {} on server {} timed out after {}ms",
                tool_name,
                server.id,
                config.timeout_ms
            )),
        };

        let close_result = client.close().await;
        let result = call_result?;
        if let Err(_e) = close_result {
            return Err(anyhow!(mcp_session_close_failed_message(
                "stdio",
                &server.id,
                &format!("tool call {}", tool_name),
            ))
            .into());
        }

        serde_json::to_value(result)
            .map_err(|e| anyhow!("failed to serialize MCP tool result: {}", e).into())
    }

    async fn list_http_tools(
        &self,
        server: &McpServerPo,
        credential_injections: &[McpCredentialInjection],
    ) -> Result<Vec<RemoteMcpTool>> {
        let config = server.config();
        let mut client = connect_http_client(server, &config, credential_injections).await?;

        let list_result = match tokio::time::timeout(
            Duration::from_millis(config.timeout_ms),
            client.peer().list_all_tools(),
        )
        .await
        {
            Ok(Ok(tools)) => Ok(tools),
            Ok(Err(_e)) => Err(anyhow!("MCP tools/list on server {} failed", server.id)),
            Err(_) => Err(anyhow!(
                "MCP tools/list on server {} timed out after {}ms",
                server.id,
                config.timeout_ms
            )),
        };

        let close_result = client.close().await;
        let tools = list_result?;
        if let Err(_e) = close_result {
            return Err(anyhow!(mcp_session_close_failed_message(
                "streamable HTTP",
                &server.id,
                "tools/list",
            ))
            .into());
        }

        Ok(tools
            .into_iter()
            .map(|tool| RemoteMcpTool {
                name: tool.name.to_string(),
                description: tool.description.map(|description| description.to_string()),
                input_schema: Value::Object(tool.input_schema.as_ref().clone()),
            })
            .collect())
    }

    async fn call_http_tool(
        &self,
        server: &McpServerPo,
        tool_name: &str,
        arguments: Map<String, Value>,
        credential_injections: &[McpCredentialInjection],
    ) -> Result<Value> {
        let config = server.config();
        let mut client = connect_http_client(server, &config, credential_injections).await?;

        let params = CallToolRequestParams::new(tool_name.to_string()).with_arguments(arguments);

        let call_result = match tokio::time::timeout(
            Duration::from_millis(config.timeout_ms),
            client.peer().call_tool(params),
        )
        .await
        {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_e)) => Err(anyhow!(
                "MCP tool {} on server {} call failed",
                tool_name,
                server.id
            )),
            Err(_) => Err(anyhow!(
                "MCP tool {} on server {} timed out after {}ms",
                tool_name,
                server.id,
                config.timeout_ms
            )),
        };

        let close_result = client.close().await;
        let result = call_result?;
        if let Err(_e) = close_result {
            return Err(anyhow!(mcp_session_close_failed_message(
                "streamable HTTP",
                &server.id,
                &format!("tool call {}", tool_name),
            ))
            .into());
        }

        serde_json::to_value(result)
            .map_err(|e| anyhow!("failed to serialize MCP tool result: {}", e).into())
    }

    #[cfg(test)]
    pub fn is_invalidated(&self, server_id: &str) -> bool {
        self.invalidated_servers.lock().unwrap().contains(server_id)
    }
}

async fn connect_stdio_client(
    server: &McpServerPo,
    config: &McpServerConfig,
    env_injections: &[(String, String)],
) -> common::error::Result<RunningService<RoleClient, ()>> {
    let command = config
        .command
        .as_deref()
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .ok_or_else(|| anyhow!("MCP stdio server {} command is required", server.id))?;

    let mut process = Command::new(resolve_command_path(command)?);
    process.args(&config.args);
    // 零继承红线：先清空环境，再仅注入本实例 check 生命周期的凭据值（D22/D23）。
    // 每次调用独立连接（用后即关）：连接天然按 (server, 调用者实例) 隔离，
    // 凭据只存在于当次子进程，不存在跨用户复用面。
    process.env_clear();
    for (key, value) in env_injections {
        process.env(key, value);
    }
    // 零继承红线针对凭据（见上），PATH 不含凭据；但没有 PATH 时 npx/uvx 这类
    // server 内部再起子进程必然失败，故补一条补全后的 PATH（统一出口）
    if let Some(path) = shell_env::inherited_path() {
        process.env("PATH", path);
    }

    let transport = TokioChildProcess::new(process)
        .map_err(|_e| anyhow!("failed to spawn MCP stdio server {}", server.id))?;
    let service = tokio::time::timeout(
        Duration::from_millis(config.connect_timeout_ms),
        ().serve(transport),
    )
    .await
    .map_err(|_| {
        anyhow!(
            "MCP stdio server {} session initialization timed out after {}ms",
            server.id,
            config.connect_timeout_ms
        )
    })?
    .map_err(|_e| {
        anyhow!(
            "failed to initialize MCP stdio server {} session",
            server.id
        )
    })?;

    Ok(service)
}

async fn connect_http_client(
    server: &McpServerPo,
    config: &McpServerConfig,
    credential_injections: &[McpCredentialInjection],
) -> common::error::Result<RunningService<RoleClient, ()>> {
    let url = config
        .url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| anyhow!("MCP streamable HTTP server {} url is required", server.id))?;

    let header_injections: Vec<(String, String)> = credential_injections
        .iter()
        .filter_map(|injection| match injection {
            McpCredentialInjection::Header { name, value } => Some((name.clone(), value.clone())),
            _ => None,
        })
        .collect();
    let query_injections: Vec<(String, String)> = credential_injections
        .iter()
        .filter_map(|injection| match injection {
            McpCredentialInjection::Query { name, value } => Some((name.clone(), value.clone())),
            _ => None,
        })
        .collect();

    // Header 形态：注入连接请求头，initialize 及后续全部请求由 transport 统一携带
    //（rmcp StreamableHttpClientTransportConfig::custom_headers 全请求应用，http.rs 先例语义）。
    let custom_headers = streamable_http_custom_headers(&server.id, &header_injections)?;
    // Query 形态：在 endpoint URL 追加查询参数（高德场景 key=<AMAP_KEY>）。
    let endpoint = streamable_http_endpoint(&server.id, url, &query_injections)?;

    // per-call 短会话（沿 stdio 形态语义：每次调用独立连接、用后即关）：
    // 凭据只存在于当次会话请求，不存在跨调用复用面。
    let transport_config =
        StreamableHttpClientTransportConfig::with_uri(endpoint).custom_headers(custom_headers);
    let transport = StreamableHttpClientTransport::from_config(transport_config);

    let service = tokio::time::timeout(
        Duration::from_millis(config.connect_timeout_ms),
        ().serve(transport),
    )
    .await
    .map_err(|_| {
        anyhow!(
            "MCP streamable HTTP server {} session initialization timed out after {}ms",
            server.id,
            config.connect_timeout_ms
        )
    })?
    .map_err(|_e| {
        anyhow!(
            "failed to initialize MCP streamable HTTP server {} session",
            server.id
        )
    })?;

    Ok(service)
}

/// 构建 StreamableHttp endpoint URL：把 Query 形态凭据追加为查询参数
///（percent-encoding 走 serde_urlencoded；URL 已含查询参数时用 `&` 续接）。
/// endpoint 含凭据值，严禁进入任何错误消息。
fn streamable_http_endpoint(
    server_id: &str,
    url: &str,
    query_injections: &[(String, String)],
) -> Result<String> {
    if query_injections.is_empty() {
        return Ok(url.to_string());
    }
    let encoded = serde_urlencoded::to_string(query_injections)
        .map_err(|_e| anyhow!("MCP server {} query credential encoding failed", server_id))?;
    let separator = if url.contains('?') { '&' } else { '?' };
    Ok(format!("{url}{separator}{encoded}"))
}

/// 构建 StreamableHttp 注入请求头映射。名字/值非法时报错且不回显凭据值。
fn streamable_http_custom_headers(
    server_id: &str,
    header_injections: &[(String, String)],
) -> Result<HashMap<HeaderName, HeaderValue>> {
    let mut headers = HashMap::with_capacity(header_injections.len());
    for (name, value) in header_injections {
        let name = HeaderName::from_str(name)
            .map_err(|_| anyhow!("MCP server {} credential header name is invalid", server_id))?;
        let value = HeaderValue::from_str(value).map_err(|_| {
            anyhow!(
                "MCP server {} credential header value is invalid",
                server_id
            )
        })?;
        headers.insert(name, value);
    }
    Ok(headers)
}

/// Stdio 侧只消费 Env 形态注入（`check()` 已按 transport 限定注入点合法性，此处过滤兜底）。
fn env_injection_pairs(credential_injections: &[McpCredentialInjection]) -> Vec<(String, String)> {
    credential_injections
        .iter()
        .filter_map(|injection| match injection {
            McpCredentialInjection::Env { name, value } => Some((name.clone(), value.clone())),
            _ => None,
        })
        .collect()
}

fn mcp_session_close_failed_message(transport: &str, server_id: &str, operation: &str) -> String {
    format!(
        "MCP {} session close failed after {} on server {}",
        transport, operation, server_id
    )
}

fn resolve_command_path(command: &str) -> Result<PathBuf> {
    let path = PathBuf::from(command);
    if path.components().count() > 1 || path.is_absolute() {
        return Ok(path);
    }

    // 用补全后的 PATH 解析：服务进程 PATH 常缺 /opt/homebrew/bin 等目录，
    // 否则 npx / uvx 这类命令会被判成 not found
    let paths = shell_env::inherited_path().ok_or_else(|| -> common::error::Error {
        anyhow!("PATH is required to resolve MCP stdio command; use an absolute path instead")
            .into()
    })?;

    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(anyhow!("MCP stdio command was not found in PATH; use an absolute path instead").into())
}

/// Runtime dependencies needed to build an executable MCP CoreTool.
#[derive(Debug, Clone)]
pub struct McpToolDeps {
    pub server: McpServerPo,
    pub client_runtime: Arc<McpClientRuntime>,
}

/// Executable MCP core tool created from `ToolPo + McpToolConfig + deps`.
#[derive(Debug, Clone)]
pub struct McpCoreTool {
    po: ToolPo,
    config: McpToolConfig,
    server: Option<McpServerPo>,
    client_runtime: Option<Arc<McpClientRuntime>>,
    /// check 写入的凭据注入值（绑定形态随值携带；D22：凭据是实例状态，实例单次使用不复用。
    /// runtime 按 transport 分派消费：Stdio→Env 环境变量，StreamableHttp→Header 请求头/Query URL 参数）
    credential_injections: Vec<McpCredentialInjection>,
}

impl McpCoreTool {
    /// Build an MCP core tool stub from a persistent ToolPo.
    pub fn from_po(po: ToolPo) -> Result<Self> {
        let config = parse_and_validate_config(&po)?;
        Ok(Self {
            po,
            config,
            server: None,
            client_runtime: None,
            credential_injections: Vec::new(),
        })
    }

    /// Build an MCP core tool with server/runtime deps prepared by McpToolDal.
    pub fn from_po_with_deps(po: ToolPo, deps: McpToolDeps) -> Result<Self> {
        let config = parse_and_validate_config(&po)?;
        if config.server_id != deps.server.id {
            return Err(anyhow!(
                "mcp tool server_id {} does not match MCP server {}",
                config.server_id,
                deps.server.id
            )
            .into());
        }

        Ok(Self {
            po,
            config,
            server: Some(deps.server),
            client_runtime: Some(deps.client_runtime),
            credential_injections: Vec::new(),
        })
    }

    pub fn config(&self) -> &McpToolConfig {
        &self.config
    }
}

#[async_trait]
impl CoreTool for McpCoreTool {
    async fn call(&self, _ctx: RequestContext, args: Value) -> Result<Value> {
        let server = self.server.as_ref().ok_or_else(|| {
            err!(
                ToolExecutionFailed,
                "MCP tool {} on server {} is not executable: MCP runtime is not enabled",
                self.config.tool_name,
                self.config.server_id
            )
        })?;
        let client_runtime = self.client_runtime.as_ref().ok_or_else(|| {
            err!(
                ToolExecutionFailed,
                "MCP tool {} on server {} is not executable: MCP runtime is not enabled",
                self.config.tool_name,
                self.config.server_id
            )
        })?;

        client_runtime
            .call_tool(
                server,
                &self.config.tool_name,
                args,
                &self.credential_injections,
            )
            .await
            .map_err(|e| {
                let msg: String = e.to_string();
                common::error::Error::new(common::error::ErrorCode::ToolExecutionFailed, msg)
            })
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }

    fn credential_requirements(&self) -> Vec<common::models::CredentialRequirement> {
        self.server
            .as_ref()
            .map(|server| server.config().credential_requirements)
            .unwrap_or_default()
    }

    fn check(&mut self, resolved: &[crate::pkg::credential::ResolvedRequirement]) -> Result<()> {
        // 注入点合法性随 transport 分派（配置期 validate_requirements 已限定，此处防御兜底）：
        // Stdio 仅 Env；StreamableHttp（McpHttp scope）支持 Header + Query（拓展单1 放宽）。
        // 注入值连绑定形态统一存 credential_injections，由 runtime 按 transport 分派消费（拓展单2）。
        let streamable_http = self
            .server
            .as_ref()
            .is_some_and(|server| matches!(server.transport, McpTransport::StreamableHttp));
        let mut injections = Vec::with_capacity(resolved.len());
        for item in resolved {
            match &item.requirement.binding {
                common::models::CredentialBinding::Env { name } if !streamable_http => {
                    injections.push(McpCredentialInjection::Env {
                        name: name.clone(),
                        value: item.value.clone(),
                    });
                }
                common::models::CredentialBinding::Header { name } if streamable_http => {
                    injections.push(McpCredentialInjection::Header {
                        name: name.clone(),
                        value: item.value.clone(),
                    });
                }
                common::models::CredentialBinding::Query { name } if streamable_http => {
                    injections.push(McpCredentialInjection::Query {
                        name: name.clone(),
                        value: item.value.clone(),
                    });
                }
                _ => {
                    return Err(err!(
                        InvalidRequest,
                        "MCP 凭据注入点与传输方式不匹配（Stdio 仅支持 env，StreamableHttp 支持 header/query）"
                    ));
                }
            }
        }
        self.credential_injections = injections;
        Ok(())
    }
}

/// Create an executable MCP tool stub from ToolPo.
pub fn create_tool(po: ToolPo) -> Result<Box<dyn CoreTool>> {
    Ok(Box::new(McpCoreTool::from_po(po)?))
}

/// Create an MCP tool using explicit server/runtime dependencies.
pub fn create_mcp_tool(po: ToolPo, deps: McpToolDeps) -> Result<Box<dyn CoreTool + Send + Sync>> {
    Ok(Box::new(McpCoreTool::from_po_with_deps(po, deps)?))
}

/// Validate an MCP ToolPo without constructing the executable runtime.
pub fn validate_tool_po_config(po: &ToolPo) -> Result<()> {
    parse_and_validate_config(po).map(|_| ())
}

fn parse_and_validate_config(po: &ToolPo) -> Result<McpToolConfig> {
    if po.protocol != ToolProtocol::Mcp {
        return Err(anyhow!("mcp tool factory only accepts ToolProtocol::Mcp").into());
    }

    let config: McpToolConfig = serde_json::from_value(po.config.clone())
        .map_err(|e| anyhow!("invalid mcp tool config for {}: {}", po.id, e))
        .map_err(Into::<common::error::Error>::into)?;
    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &McpToolConfig) -> Result<()> {
    if config.server_id.trim().is_empty() {
        return Err(anyhow!("mcp tool server_id is required").into());
    }

    if config.tool_name.trim().is_empty() {
        return Err(anyhow!("mcp tool tool_name is required").into());
    }

    Ok(())
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod mcp_tests;
