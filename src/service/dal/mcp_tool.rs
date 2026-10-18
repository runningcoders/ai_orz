//! MCP Tool DAL.
//!
//! Protocol-specific DAL for MCP tools. It keeps MCP server lookup and runtime
//! dependency preparation out of the generic `ToolDal`.

use crate::models::mcp_server::McpServerStatus;
use crate::models::mcp_server::{McpServerPo, McpTransport};
use crate::models::tool::{Tool, ToolExecutionRequest, ToolPo};
use crate::pkg::RequestContext;
use crate::pkg::credential::FetchedCredential;
use crate::pkg::tool_registry::mcp::{McpCredentialInjection, McpToolConfig, RemoteMcpTool};
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use crate::service::dal::lark::LarkCredentialDal as _;
use crate::service::dao::mcp_server::McpServerDao;
use crate::service::dao::tool::{ToolDao, ToolQuery};
use crate::service::dao::tool_call::{self, McpToolCallDao};
use anyhow::Result as AnyhowResult;
use async_trait::async_trait;
use common::api::{ListMcpToolsByServerRequest, PagedResult};
use common::enums::{ControlMode, ToolProtocol, ToolStatus};
use common::error::{Result, bail_err, err};
use common::models::{CredentialBinding, CredentialDetail, CredentialKind, CredentialRequirement};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

static MCP_TOOL_DAL: OnceLock<Arc<dyn McpToolDal + Send + Sync>> = OnceLock::new();

/// Get global MCP Tool DAL.
pub fn dal() -> Arc<dyn McpToolDal + Send + Sync> {
    MCP_TOOL_DAL.get().cloned().unwrap()
}

/// Initialize global MCP Tool DAL using global DAO singletons.
pub fn init() {
    use crate::service::dao::{mcp_server, tool};
    let _ = MCP_TOOL_DAL.set(new(tool::dao(), mcp_server::dao(), tool_call::mcp_dao()));
}

/// Create MCP Tool DAL with explicit dependencies.
pub fn new(
    tool_dao: Arc<dyn ToolDao + Send + Sync>,
    mcp_server_dao: Arc<dyn McpServerDao + Send + Sync>,
    mcp_tool_call_dao: Arc<dyn McpToolCallDao + Send + Sync>,
) -> Arc<dyn McpToolDal + Send + Sync> {
    Arc::new(McpToolDalImpl {
        tool_dao,
        mcp_server_dao,
        mcp_tool_call_dao,
    })
}

#[async_trait]
pub trait McpToolDal: Send + Sync {
    /// Get a complete executable MCP Tool by standard ToolPo id.
    async fn get_by_id(&self, ctx: RequestContext, tool_id: String) -> Result<Option<Tool>>;

    /// Sync remote MCP tools from one server into standard ToolPo records.
    async fn sync_from_server(&self, ctx: RequestContext, server_id: &str) -> Result<usize>;

    /// List standard Tool records synced from one MCP Server.
    async fn list_by_server(
        &self,
        ctx: RequestContext,
        params: ListMcpToolsByServerRequest,
    ) -> Result<PagedResult<Tool>>;

    /// Execute an MCP tool with unified execution params (D26).
    ///
    /// `ToolExecutionRequest.tool` is the PO carrier: the executable instance
    /// is re-assembled per call (connection isolation, D23) and receives
    /// credential injections via `CoreTool::check` (D22 create → check → call).
    /// 成功时返回 (Value, ToolCallEntry)，entry.call_id 为真实 call_id。
    async fn call_tool(
        &self,
        ctx: RequestContext,
        request: ToolExecutionRequest,
    ) -> Result<(Value, ToolCallEntry)>;

    /// Invalidate cached MCP runtime/session for a server.
    fn invalidate_server(&self, server_id: &str);
}

pub struct McpToolDalImpl {
    tool_dao: Arc<dyn ToolDao + Send + Sync>,
    mcp_server_dao: Arc<dyn McpServerDao + Send + Sync>,
    mcp_tool_call_dao: Arc<dyn McpToolCallDao + Send + Sync>,
}

#[async_trait]
impl McpToolDal for McpToolDalImpl {
    async fn get_by_id(&self, ctx: RequestContext, tool_id: String) -> Result<Option<Tool>> {
        let Some(po) = self
            .tool_dao
            .get_by_id(ctx.clone(), tool_id.clone())
            .await?
        else {
            return Ok(None);
        };

        ensure_mcp_tool(&po)?;
        Ok(Some(self.assemble_executable_tool(ctx, &po).await?))
    }

    async fn sync_from_server(&self, ctx: RequestContext, server_id: &str) -> Result<usize> {
        let Some(server) = self
            .mcp_server_dao
            .find_by_id(ctx.clone(), server_id)
            .await?
        else {
            bail_err!(ResourceNotFound, "MCP server not found: {}", server_id);
        };

        let remote_tools = self
            .mcp_tool_call_dao
            .list_mcp_tools(
                &server,
                &resolve_sync_credential_injections(&ctx, &server).await?,
            )
            .await?;
        let remote_tool_ids: std::collections::HashSet<String> = remote_tools
            .iter()
            .map(|remote_tool| mcp_tool_record_id(&server.id, &remote_tool.name))
            .collect();
        let mut synced = 0;

        for remote_tool in &remote_tools {
            // MCP 服务器是**外部不可信来源**：其 inputSchema 会原样进出站 tools 数组，
            // 不合法的 schema 会毒死持有该工具的 Agent 的每一轮模型调用（详见
            // common::models::validate_tool_parameters_schema 的规则与依据）。
            // 处理方式：跳过该工具 + 告警，同一 server 的其余工具不受影响。
            if let Err(reason) =
                common::models::validate_tool_parameters_schema(&remote_tool.input_schema)
            {
                log_warn!(
                    ctx,
                    "mcp_tool_sync",
                    server_id = %server.id,
                    tool_name = %remote_tool.name,
                    reason = %reason,
                    "MCP 工具的 inputSchema 不符合跨 provider 安全子集，已跳过该工具"
                );
                continue;
            }
            // LLM 可见性护栏：出站前 `to_llm_schema` 会把内部 `$ref` 就地展开，
            // 展开后**仍然**未收敛的位置（残留 `$ref` / 可压平的 `oneOf` /
            // 可折叠的可空 `anyOf`）意味着模型看到的字段定义不完整——典型症状是
            // 嵌套对象参数被传成 `"null"` 字符串，MCP server 宽松接受后状态仍是
            // Completed，日志零报错 ⇒ 静默丢参（2026-10-18 实证：
            // add_transaction.location 7/7 丢光 LocationInfo 的 9 个键）。
            // 只告警、不拦：schema 本身可用，只是对模型不友好。
            let llm_view = common::llm_schema::to_llm_schema(remote_tool.input_schema.clone());
            let gaps = common::llm_schema::unflattened_positions(&llm_view);
            if !gaps.is_empty() {
                log_warn!(
                    ctx,
                    "mcp_tool_sync",
                    server_id = %server.id,
                    tool_name = %remote_tool.name,
                    positions = %gaps.join(", "),
                    "MCP 工具的 inputSchema 收敛后仍有对模型不可见的位置，嵌套参数有丢字段风险"
                );
            }
            let mut po = build_synced_tool_po(&server, remote_tool, ctx.user_id.clone());
            if let Some(existing) = self.tool_dao.get_by_id(ctx.clone(), po.id.clone()).await? {
                ensure_sync_target_matches(&existing, &po)?;
                po.created_at = existing.created_at;
                po.created_by = existing.created_by;
                po.status = if existing.status == ToolStatus::Stale {
                    ToolStatus::Enabled
                } else {
                    existing.status
                };
                po.updated_by = ctx.user_id.clone();
                self.tool_dao.update_tool(ctx.clone(), &po).await?;
            } else {
                self.tool_dao.create_tool(ctx.clone(), &po).await?;
            }
            synced += 1;
        }

        let existing_enabled_tools = self
            .tool_dao
            .query(
                ctx.clone(),
                ToolQuery {
                    protocol: Some(ToolProtocol::Mcp),
                    status: Some(ToolStatus::Enabled),
                    mcp_server_id: Some(server.id.clone()),
                    ..Default::default()
                },
            )
            .await?;

        for mut existing in existing_enabled_tools.items {
            if remote_tool_ids.contains(&existing.id) {
                continue;
            }
            existing.status = ToolStatus::Stale;
            existing.updated_by = ctx.user_id.clone();
            existing.touch(ctx.user_id.clone());
            self.tool_dao.update_tool(ctx.clone(), &existing).await?;
        }

        Ok(synced)
    }

    async fn list_by_server(
        &self,
        ctx: RequestContext,
        params: ListMcpToolsByServerRequest,
    ) -> Result<PagedResult<Tool>> {
        let server_exists = self
            .mcp_server_dao
            .find_by_id(ctx.clone(), &params.server_id)
            .await?
            .is_some();
        if !server_exists {
            bail_err!(
                ResourceNotFound,
                "MCP server not found: {}",
                params.server_id
            );
        }

        let base_query = ToolQuery {
            keyword: params.keyword.clone(),
            protocol: Some(ToolProtocol::Mcp),
            status: params.status,
            mcp_server_id: Some(params.server_id.clone()),
            ..Default::default()
        };

        let all = self.tool_dao.query(ctx.clone(), base_query.clone()).await?;
        let total = all.total;

        let page = self
            .tool_dao
            .query(
                ctx,
                ToolQuery {
                    pagination: params.pagination,
                    ..base_query
                },
            )
            .await?;

        Ok(PagedResult {
            items: page
                .items
                .into_iter()
                .map(Tool::from_po_for_management)
                .collect(),
            total,
        })
    }

    async fn call_tool(
        &self,
        ctx: RequestContext,
        request: ToolExecutionRequest,
    ) -> Result<(Value, ToolCallEntry)> {
        // per-call 重组装可执行实例（连接隔离 D23）→ check 注入（D22）→ execute
        let mut executable = self
            .assemble_executable_tool(ctx.clone(), &request.tool)
            .await
            .map_err(|e| common::error::Error::tool_call_failed(e.to_string()).with_source(e))?;
        executable.our_tool.check(&request.resolved)?;
        self.mcp_tool_call_dao
            .execute(ctx, &executable, request.args)
            .await
            .map_err(Into::into)
    }

    fn invalidate_server(&self, server_id: &str) {
        self.mcp_tool_call_dao.invalidate_mcp_server(server_id);
    }
}

impl McpToolDalImpl {
    async fn assemble_executable_tool(&self, ctx: RequestContext, po: &ToolPo) -> Result<Tool> {
        ensure_mcp_tool(po)?;
        ensure_mcp_tool_enabled(po)?;
        let config = parse_mcp_tool_config(po)?;

        let Some(server) = self
            .mcp_server_dao
            .find_by_id(ctx, &config.server_id)
            .await?
        else {
            bail_err!(
                ResourceNotFound,
                "MCP server not found for tool {}: {}",
                po.id,
                config.server_id
            );
        };
        ensure_mcp_server_enabled(&server)?;

        let Some(our_tool) = self.mcp_tool_call_dao.assemble_mcp_core_tool(po, &server)? else {
            bail_err!(InvalidRequest, "MCP tool is not executable: {}", po.id);
        };

        Ok(Tool {
            po: po.clone(),
            our_tool,
            search_match: None,
            stats: None,
        })
    }
}

fn build_synced_tool_po(
    server: &crate::models::mcp_server::McpServerPo,
    remote_tool: &RemoteMcpTool,
    creator: Option<String>,
) -> ToolPo {
    let id = mcp_tool_record_id(&server.id, &remote_tool.name);
    let description = remote_tool
        .description
        .clone()
        .unwrap_or_else(|| format!("MCP tool {} from server {}", remote_tool.name, server.name));
    let mut po = ToolPo::new(
        id.clone(),
        id,
        description,
        ToolProtocol::Mcp,
        serde_json::json!({
            "server_id": server.id,
            "tool_name": remote_tool.name,
        }),
        Some(remote_tool.input_schema.clone()),
        vec![
            server.name.clone(),
            "mcp".to_string(),
            server.id.clone(),
            remote_tool.name.clone(),
        ],
        creator,
    );
    po.control_mode = ControlMode::Manual;
    po
}

fn mcp_tool_record_id(server_id: &str, tool_name: &str) -> String {
    format!("mcp.{}.{}", server_id, tool_name)
}

fn ensure_mcp_tool(po: &ToolPo) -> Result<()> {
    if po.protocol != ToolProtocol::Mcp {
        bail_err!(InvalidRequest, "Tool {} is not an MCP tool", po.id);
    }
    Ok(())
}

fn ensure_mcp_tool_enabled(po: &ToolPo) -> Result<()> {
    if po.status != ToolStatus::Enabled {
        bail_err!(InvalidRequest, "MCP tool disabled: {}", po.id);
    }
    Ok(())
}

fn ensure_mcp_server_enabled(server: &crate::models::mcp_server::McpServerPo) -> Result<()> {
    if server.status != McpServerStatus::Enabled {
        bail_err!(InvalidRequest, "MCP server disabled: {}", server.id);
    }
    Ok(())
}

fn ensure_sync_target_matches(existing: &ToolPo, synced: &ToolPo) -> Result<()> {
    if existing.protocol != ToolProtocol::Mcp {
        bail_err!(
            Conflict,
            "MCP sync target id {} already exists as non-MCP tool",
            existing.id
        );
    }

    let existing_config = parse_mcp_tool_config(existing)?;
    let synced_config = parse_mcp_tool_config(synced)?;
    if existing_config.server_id != synced_config.server_id
        || existing_config.tool_name != synced_config.tool_name
    {
        bail_err!(
            Conflict,
            "MCP sync target id {} already binds to {}/{}",
            existing.id,
            existing_config.server_id,
            existing_config.tool_name
        );
    }

    Ok(())
}

fn parse_mcp_tool_config(po: &ToolPo) -> Result<McpToolConfig> {
    let config: AnyhowResult<McpToolConfig> = serde_json::from_value(po.config.clone())
        .map_err(|e| anyhow::anyhow!("invalid mcp tool config for {}: {}", po.id, e));
    config.map_err(|e| {
        err!(
            InvalidRequest,
            "invalid mcp tool config for {}: {}",
            po.id,
            e
        )
        .with_source::<common::error::Error>(e.into())
    })
}
// ==================== sync 凭据编排（Task 3.3 补完，AMan 拍板方案 A） ====================

/// sync 路径凭据解析与注入值构造（domain `resolve_tool_credentials` 同构编排）。
///
/// 生产路由二元化（沿 D17 v1.5 先例）：
/// - `LarkApp` → 渠道路径（`LarkCredentialDal::resolve_credentials_for_user`，明文态）；
/// - 其余 kind → `ctx.user_id`（缺失即未命中）→ `UserDal::find_default_credential`（DB 加密态）。
///
/// 任一需求未命中 → 引导性错误（带 server id；零凭据零完整 URL 回显），sync 失败。
/// 解密与取值复用 `pkg::credential::resolve_requirements`（AES-256-GCM 链路零明文；
/// 明文兼容形态仅测试直通，生产值均带 `enc:v1:` 前缀）。
///
/// 注：runtime domain 的 `resolve_tool_credentials` 为 `pub(super)`（仅 runtime domain
/// 内可见），dal 侧不做跨模块调用，按先例同构自持编排。
async fn resolve_sync_credential_injections(
    ctx: &RequestContext,
    server: &McpServerPo,
) -> Result<Vec<McpCredentialInjection>> {
    let requirements = &server.config().credential_requirements;
    if requirements.is_empty() {
        return Ok(Vec::new());
    }

    let mut fetched = Vec::with_capacity(requirements.len());
    for requirement in requirements {
        let fetched_credential = match requirement.kind {
            // 生产路由（D17 v1.5）：LarkApp 走渠道路径（明文态 + 派生属性）
            CredentialKind::LarkApp => {
                let creds = crate::service::dal::lark::dal()
                    .resolve_credentials_for_user(ctx)
                    .await?
                    .ok_or_else(|| sync_credential_missing_error(server, requirement))?;
                FetchedCredential {
                    credential_id: creds.0.app_id.clone(), // 生产端无独立 id，以 app_id 代
                    detail: CredentialDetail::LarkApp {
                        app_id: creds.0.app_id,
                        app_secret: creds.0.app_secret,
                        encrypt_key: None,
                        verification_token: None,
                    },
                    attributes: BTreeMap::from([("identity_mode".to_string(), creds.1)]),
                    already_decrypted: true,
                }
            }
            // 其余 kind 统一走 user dal find_default（纯单轨 D27，无兜底）
            _ => {
                let Some(user_id) = ctx.user_id.clone() else {
                    return Err(sync_credential_missing_error(server, requirement));
                };
                let credential = crate::service::dal::user::dal()
                    .find_default_credential(
                        ctx.clone(),
                        &user_id,
                        requirement.kind,
                        requirement.platform.as_deref(),
                    )
                    .await?
                    .ok_or_else(|| sync_credential_missing_error(server, requirement))?;
                FetchedCredential {
                    credential_id: credential.id().to_string(),
                    detail: credential.detail().clone(),
                    attributes: BTreeMap::new(),
                    already_decrypted: false,
                }
            }
        };
        fetched.push(fetched_credential);
    }

    let resolved = crate::pkg::credential::resolve_requirements(requirements, &fetched).await?;

    // binding → 注入形态映射（check() 先例同构：Stdio 仅 Env；StreamableHttp Header/Query）
    let streamable_http = matches!(server.transport, McpTransport::StreamableHttp);
    let mut injections = Vec::with_capacity(resolved.len());
    for item in resolved {
        let injection = match (&item.requirement.binding, streamable_http) {
            (CredentialBinding::Env { name }, false) => McpCredentialInjection::Env {
                name: name.clone(),
                value: item.value.clone(),
            },
            (CredentialBinding::Header { name }, true) => McpCredentialInjection::Header {
                name: name.clone(),
                value: item.value.clone(),
            },
            (CredentialBinding::Query { name }, true) => McpCredentialInjection::Query {
                name: name.clone(),
                value: item.value.clone(),
            },
            _ => {
                return Err(err!(
                    InvalidRequest,
                    "MCP server {} 凭据注入点与传输方式不匹配（Stdio 仅支持 env，StreamableHttp 支持 header/query）",
                    server.id
                ));
            }
        };
        injections.push(injection);
    }
    Ok(injections)
}

/// 凭据未命中引导性错误（带 server id；零凭据零完整 URL 回显）
fn sync_credential_missing_error(
    server: &McpServerPo,
    requirement: &CredentialRequirement,
) -> common::error::Error {
    let kind_desc = match &requirement.platform {
        Some(platform) => format!("{}/{}", requirement.kind.as_str(), platform),
        None => requirement.kind.as_str().to_string(),
    };
    err!(
        InvalidRequest,
        "MCP server {} 同步失败：缺少 {} 凭据，请在设置 → 身份凭证录入并设为默认后再同步",
        server.id,
        kind_desc
    )
}
