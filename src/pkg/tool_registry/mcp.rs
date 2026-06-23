//! MCP (Model Context Protocol) tool provider.
//!
//! MCP tools are database-registered tools. `ToolPo.config` stores only the
//! binding from a standard tool record to an MCP server/tool pair:
//! `{ "server_id": "...", "tool_name": "..." }`.
//! Server connection details, credentials, headers, env, and commands belong to
//! `McpServerPo.config` and must not be duplicated into each MCP tool config.

use crate::models::mcp_server::{McpServerPo, McpTransport};
use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::request_context::RequestContext;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use common::enums::ToolProtocol;
use rig::tool::ToolError;
use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::process::Command;

/// MCP tool binding configuration stored in `ToolPo.config`.
///
/// This intentionally contains no server credentials or transport config. The
/// MCP-specific DAL loads the referenced `McpServerPo` and passes those deps to
/// the MCP runtime factory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct McpToolConfig {
    /// ID of the MCP server/provider record.
    pub server_id: String,
    /// Name of the concrete tool exposed by that MCP server.
    pub tool_name: String,
}

/// Minimal MCP client runtime.
///
/// The first executable runtime path is stdio-only. Streamable HTTP stays behind
/// an explicit not-implemented error until its SSRF and header-safety rules are
/// implemented in a later phase.
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

    pub async fn call_tool(
        &self,
        server: &McpServerPo,
        tool_name: &str,
        args: Value,
    ) -> Result<Value> {
        match server.transport {
            McpTransport::Stdio => self.call_stdio_tool(server, tool_name, args).await,
            McpTransport::StreamableHttp => Err(anyhow!(
                "MCP streamable HTTP runtime is not implemented yet"
            )),
        }
    }

    async fn call_stdio_tool(
        &self,
        server: &McpServerPo,
        tool_name: &str,
        args: Value,
    ) -> Result<Value> {
        let arguments = match args {
            Value::Object(map) => map,
            other => {
                return Err(anyhow!(
                    "MCP tool arguments must be a JSON object, got {}",
                    other
                ));
            }
        };

        let config = server.config();
        let command = config
            .command
            .as_deref()
            .map(str::trim)
            .filter(|command| !command.is_empty())
            .ok_or_else(|| anyhow!("MCP stdio server {} command is required", server.id))?;

        let mut process = Command::new(resolve_command_path(command)?);
        process.args(&config.args);
        process.env_clear();
        for (key, value) in &config.env {
            process.env(key, value);
        }

        let transport = TokioChildProcess::new(process)
            .map_err(|e| anyhow!("failed to spawn MCP stdio server {}: {}", server.id, e))?;
        let mut client = tokio::time::timeout(
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
        .map_err(|e| {
            anyhow!(
                "failed to initialize MCP stdio server {} session: {}",
                server.id,
                e
            )
        })?;

        let params = CallToolRequestParams::new(tool_name.to_string()).with_arguments(arguments);

        let call_result = match tokio::time::timeout(
            Duration::from_millis(config.timeout_ms),
            client.peer().call_tool(params),
        )
        .await
        {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(e)) => Err(anyhow!(
                "MCP tool {} on server {} call failed: {}",
                tool_name,
                server.id,
                e
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
        if let Err(e) = close_result {
            return Err(anyhow!(
                "MCP tool {} on server {} completed but session close failed: {}",
                tool_name,
                server.id,
                e
            ));
        }

        serde_json::to_value(result)
            .map_err(|e| anyhow!("failed to serialize MCP tool result: {}", e))
    }

    #[cfg(test)]
    pub fn is_invalidated(&self, server_id: &str) -> bool {
        self.invalidated_servers.lock().unwrap().contains(server_id)
    }
}

fn resolve_command_path(command: &str) -> Result<PathBuf> {
    let path = PathBuf::from(command);
    if path.components().count() > 1 || path.is_absolute() {
        return Ok(path);
    }

    let paths = std::env::var_os("PATH").ok_or_else(|| {
        anyhow!(
            "PATH is required to resolve MCP stdio command {}; use an absolute path instead",
            command
        )
    })?;

    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(anyhow!(
        "MCP stdio command {} was not found in PATH; use an absolute path instead",
        command
    ))
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
            ));
        }

        Ok(Self {
            po,
            config,
            server: Some(deps.server),
            client_runtime: Some(deps.client_runtime),
        })
    }

    pub fn config(&self) -> &McpToolConfig {
        &self.config
    }
}

#[async_trait]
impl CoreTool for McpCoreTool {
    async fn call(&self, _ctx: RequestContext, args: Value) -> Result<Value, ToolError> {
        let server = self.server.as_ref().ok_or_else(|| {
            ToolError::ToolCallError(
                format!(
                    "MCP tool {} on server {} is not executable: MCP runtime is not enabled",
                    self.config.tool_name, self.config.server_id
                )
                .into(),
            )
        })?;
        let client_runtime = self.client_runtime.as_ref().ok_or_else(|| {
            ToolError::ToolCallError(
                format!(
                    "MCP tool {} on server {} is not executable: MCP runtime is not enabled",
                    self.config.tool_name, self.config.server_id
                )
                .into(),
            )
        })?;

        client_runtime
            .call_tool(server, &self.config.tool_name, args)
            .await
            .map_err(|e| ToolError::ToolCallError(e.to_string().into()))
    }

    fn po(&self) -> &ToolPo {
        &self.po
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
        return Err(anyhow!("mcp tool factory only accepts ToolProtocol::Mcp"));
    }

    let config: McpToolConfig = serde_json::from_value(po.config.clone())
        .map_err(|e| anyhow!("invalid mcp tool config for {}: {}", po.id, e))?;
    validate_config(&config)?;
    Ok(config)
}

fn validate_config(config: &McpToolConfig) -> Result<()> {
    if config.server_id.trim().is_empty() {
        return Err(anyhow!("mcp tool server_id is required"));
    }

    if config.tool_name.trim().is_empty() {
        return Err(anyhow!("mcp tool tool_name is required"));
    }

    Ok(())
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod mcp_tests;
