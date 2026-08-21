//! Runtime Tool Execution 具体实现

use crate::models::tool::{Tool, ToolExecutionRequest, ToolExecutionResult};
use crate::pkg::credential::{FetchedCredential, ResolvedRequirement};
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_registry::tool_readiness;
use crate::pkg::tool_tracing::entry::ToolCallEntry;
use crate::pkg::tool_tracing::logger::ToolCallQuery;
use crate::service::domain::runtime::{RuntimeDomainImpl, RuntimeToolExecution};
use common::api::RuntimeReady;
use common::enums::{ControlMode, ToolProtocol, ToolStatus};
use common::error::{Error, Result, bail_err};
use common::models::{CredentialDetail, CredentialKind, CredentialRequirement};
use serde_json::Value;
use std::collections::BTreeMap;

#[async_trait::async_trait]
impl RuntimeToolExecution for RuntimeDomainImpl {
    async fn call_tool_by_id(
        &self,
        ctx: RequestContext,
        tool_id: String,
        args: Value,
    ) -> Result<ToolExecutionResult> {
        let tool = self
            .tool_dal
            .get_by_id(ctx.clone(), tool_id.clone())
            .await?
            .ok_or_else(|| {
                common::error::Error::tool_call_failed(format!("Tool not found: {}", tool_id))
            })?;

        self.call_tool(ctx, &tool, args).await
    }

    async fn call_tool(
        &self,
        ctx: RequestContext,
        tool: &Tool,
        args: Value,
    ) -> Result<ToolExecutionResult> {
        let tool_id = tool.po.id.clone();
        ensure_tool_enabled(&tool_id, &tool.po.status)?;

        // 凭据编排单点（D26）：requirements → resolve →
        // 未命中直接返回结构化引导（不构造实例、不进 DAL）；
        // 命中（含空需求 → Some(empty)）经统一传参进 DAL，由 per-call
        // 重组装的实例经 `CoreTool::check` 注入（D22 create → check → call）。
        let requirements =
            crate::pkg::tool_registry::get_registry().credential_requirements(&tool.po);
        let Some(resolved) = self.resolve_tool_credentials(&ctx, &requirements).await? else {
            return Ok(ToolExecutionResult::new(
                crate::pkg::credential::credential_missing_json(&requirements[0]),
                tool_id.clone(),
                format!("credential_missing_{}", uuid::Uuid::now_v7()),
            ));
        };

        let request = ToolExecutionRequest {
            tool: tool.po.clone(),
            args,
            resolved,
        };
        let execution = match tool.po.protocol {
            ToolProtocol::Mcp => self.mcp_tool_dal.call_tool(ctx, request).await,
            ToolProtocol::Builtin | ToolProtocol::Http => {
                self.tool_dal.call_tool(ctx, request).await
            }
        };

        let (result, entry) = match execution {
            Ok((value, entry)) => (value, entry),
            Err(error) => {
                // 修复：保留原 error 的 field（含 trace_ref），不再构造新 Error 丢弃 field
                let mapped_message: String = match tool.po.protocol {
                    ToolProtocol::Mcp => map_mcp_tool_error(&tool_id, &error),
                    ToolProtocol::Builtin | ToolProtocol::Http => {
                        // 脱敏：不暴露底层错误细节给 LLM，避免路径/配置泄露
                        format!("tool {} execution failed", tool_id)
                    }
                };
                let mut new_err = common::error::Error::new(
                    common::error::ErrorCode::ToolExecutionFailed,
                    mapped_message,
                );
                if let Some(field) = error.field() {
                    new_err = new_err.with_field(field.clone());
                }
                new_err = new_err.with_source(error);
                return Err(new_err);
            }
        };

        // 使用 ToolCallDao::execute 生成的真实 call_id（entry.call_id）
        Ok(ToolExecutionResult::new(
            result,
            entry.tool_id.clone(),
            entry.call_id.clone(),
        ))
    }

    async fn call_manual_tool_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: String,
        tool_id: String,
        args: Value,
    ) -> Result<ToolExecutionResult> {
        let ctx = ctx.to_builder().agent_id(&agent_id).build();

        // 先从绑定工具中查找
        let bound_tools = self
            .tool_dal
            .list_tools_for_agent_full(ctx.clone(), &agent_id)
            .await?;

        let bound_tool = bound_tools.into_iter().find(|tool| tool.po.id == tool_id);

        // 如果绑定工具中没有，检查是否是神经工具或已安装工具包
        let tool = match bound_tool {
            Some(tool) => tool,
            None => {
                // 获取 agent 的 installed_tags
                // 修复：agent 不存在时返回错误，而非 unwrap_or_default 静默退化为空 vec
                // 之前只要工具带 neural 标签就能执行，削弱授权语义
                let agent = self
                    .agent_dal
                    .find_by_id(ctx.clone(), &agent_id)
                    .await?
                    .ok_or_else(|| {
                        common::error::Error::tool_call_failed(format!(
                            "Agent {} not found, cannot authorize tool call",
                            agent_id
                        ))
                    })?;
                let installed_tags = agent.po.get_installed_tags();

                // 构建 tag 过滤列表：neural + installed_tags（OR 语义）
                // SQL 层直接过滤，避免全量加载工具到内存
                let mut tag_filter = vec!["neural".to_string()];
                tag_filter.extend(installed_tags.clone());

                let candidate_tools = self
                    .tool_dal
                    .query(
                        ctx.clone(),
                        crate::service::dao::tool::ToolQuery {
                            tags: Some(tag_filter),
                            enabled_only: Some(true),
                            ..Default::default()
                        },
                    )
                    .await?;

                // 在 SQL 过滤后的候选工具中按 ID 精确匹配
                candidate_tools
                    .items
                    .into_iter()
                    .find(|t| t.po.id == tool_id)
                    .ok_or_else(|| {
                        if installed_tags.is_empty() {
                            common::error::Error::tool_call_failed(format!(
                                "Manual tool call denied: tool {} is not bound to agent {}, not a neural tool, and agent has no installed tool packs",
                                tool_id, agent_id
                            ))
                        } else {
                            common::error::Error::tool_call_failed(format!(
                                "Manual tool call denied: tool {} is not bound to agent {}, not a neural tool, and does not belong to any installed tool pack (installed: {:?})",
                                tool_id, agent_id, installed_tags
                            ))
                        }
                    })?
            }
        };

        if tool.po.control_mode != ControlMode::Manual {
            let msg: String = format!(
                "Manual tool call denied: tool {} has control mode {:?}",
                tool_id, tool.po.control_mode
            );
            let msg: String = msg;
            return Err(common::error::Error::new(
                common::error::ErrorCode::ToolExecutionFailed,
                msg,
            ));
        }

        self.call_tool(ctx, &tool, args).await
    }

    async fn query_tool_call_entries(
        &self,
        ctx: RequestContext,
        query: ToolCallQuery,
    ) -> Result<Vec<ToolCallEntry>> {
        let query = super::tool_call_query::with_context_scope(ctx, query)?;
        Ok(self.tool_call_logger.query_calls(query)?)
    }

    async fn get_tool_call_entry_by_id(
        &self,
        ctx: RequestContext,
        query: ToolCallQuery,
    ) -> Result<Option<ToolCallEntry>> {
        super::tool_call_query::ensure_call_id_present(&query)?;
        let query = super::tool_call_query::with_context_scope(ctx, query)?;
        let mut entries = self.tool_call_logger.query_calls(query)?;
        Ok(entries.pop())
    }

    async fn tool_readiness(&self, ctx: &RequestContext, tool: &Tool) -> RuntimeReady {
        // ① CLI 型（D28「CLI 型 = po.config.command」不变式）
        if let Some((command, install_hint)) = cli_tool_source(&tool.po) {
            let cache_key = tool.po.id.clone();
            if let Some((status, at)) = readiness_cache().lock().unwrap().get(&cache_key)
                && at.elapsed() < READINESS_CACHE_TTL
            {
                return status.clone();
            }
            let status = tool_readiness::cli_binary_readiness(
                &command,
                install_hint.as_deref().unwrap_or_default(),
                "或在工具配置中修改命令路径",
            );
            readiness_cache()
                .lock()
                .unwrap()
                .insert(cache_key, (status.clone(), std::time::Instant::now()));
            return status;
        }

        // ② key 型：凭据需求非空 → 复用 resolve_tool_credentials 取数判定（按当前查看者）
        let requirements =
            crate::pkg::tool_registry::get_registry().credential_requirements(&tool.po);
        if requirements.is_empty() {
            // ③ 两者皆无 → Ready
            return RuntimeReady::Ready;
        }
        let cache_key = format!(
            "{}|{}",
            tool.po.id,
            ctx.user_id.clone().unwrap_or_default()
        );
        if let Some((status, at)) = readiness_cache().lock().unwrap().get(&cache_key)
            && at.elapsed() < READINESS_CACHE_TTL
        {
            return status.clone();
        }
        let status = match self.resolve_tool_credentials(ctx, &requirements).await {
            Ok(Some(_)) => RuntimeReady::Ready,
            Ok(None) => RuntimeReady::NotReady {
                reason: "api_key_missing".to_string(),
                hint: tool_readiness::credential_missing_hint(&requirements[0]),
            },
            // 探测异常不阻塞调用方（best-effort）
            Err(_) => RuntimeReady::Unknown,
        };
        readiness_cache()
            .lock()
            .unwrap()
            .insert(cache_key, (status.clone(), std::time::Instant::now()));
        status
    }
}

// ==================== readiness TTL 缓存与 CLI 数据源 ====================

/// 就绪判定缓存 TTL：窗口内重复列表请求复用上次判定
const READINESS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);

/// 就绪判定缓存（key 型按 `tool_id|user_id`、CLI 型按 `tool_id`）
static READINESS_CACHE: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, (RuntimeReady, std::time::Instant)>>,
> = std::sync::OnceLock::new();

fn readiness_cache() -> &'static std::sync::Mutex<
    std::collections::HashMap<String, (RuntimeReady, std::time::Instant)>,
> {
    READINESS_CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// 清除指定工具的就绪缓存（测试专用；生产无主动失效调用方，依赖 TTL 自然过期）
#[cfg(test)]
pub(super) fn invalidate_readiness_cache(tool_id: &str) {
    let prefix = format!("{}|", tool_id);
    readiness_cache()
        .lock()
        .unwrap()
        .retain(|k, _| !k.starts_with(&prefix) && k != tool_id);
}

/// 测试辅助：把指定工具的缓存时间戳拨回 TTL 前（模拟过期触发重新判定）
#[cfg(test)]
pub(super) fn expire_readiness_cache(tool_id: &str) {
    let prefix = format!("{}|", tool_id);
    readiness_cache()
        .lock()
        .unwrap()
        .iter_mut()
        .for_each(|(k, (_, at))| {
            if k == tool_id || k.starts_with(&prefix) {
                *at = std::time::Instant::now() - READINESS_CACHE_TTL
                    - std::time::Duration::from_secs(1);
            }
        });
}

/// CLI 型工具的命令与安装引导来源：PO config 优先，Builtin 工厂默认 PO 兜底。
///
/// 存量 DB PO config 无 `command`（sync 不刷新运维所有权字段 config）→
/// 以工厂默认 PO 兜底，零迁移；两者皆无 → `None`（非 CLI 型）。
fn cli_tool_source(po: &crate::models::tool::ToolPo) -> Option<(String, Option<String>)> {
    if let Some(command) = po.cli_command() {
        return Some((command, po.cli_install_hint()));
    }
    if po.protocol != ToolProtocol::Builtin {
        return None;
    }
    let factory_po = crate::pkg::tool_registry::get_registry()
        .get_builtin_factory(&po.id)?
        .create_po();
    Some((factory_po.cli_command()?, factory_po.cli_install_hint()))
}

impl RuntimeDomainImpl {
    /// 工具调用编排取数（D17 编排链 ①②③ 单点）：生产路由 → pkg 纯函数加工；
    /// 任一未命中 → `Ok(None)`（调用方出引导）。
    ///
    /// 生产路由二元化（D17 v1.5）：
    /// - `LarkApp` 走渠道路径（`LarkCredentialDal::resolve_credentials_for_user`，
    ///   明文态 + 派生属性 identity_mode，D24）；
    /// - 其余 kind 统一走 `UserDal::find_default_credential`（DB 加密态，纯单轨 D27）。
    pub(super) async fn resolve_tool_credentials(
        &self,
        ctx: &RequestContext,
        requirements: &[CredentialRequirement],
    ) -> Result<Option<Vec<ResolvedRequirement>>> {
        if requirements.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let mut fetched = Vec::with_capacity(requirements.len());
        for requirement in requirements {
            let fetched_credential = match requirement.kind {
                // 生产路由（D17 v1.5）：LarkApp 走渠道路径，附派生属性（D24）
                CredentialKind::LarkApp => {
                    self.lark_credentials
                        .resolve_credentials_for_user(ctx)
                        .await?
                        .map(|(creds, mode)| FetchedCredential {
                            credential_id: creds.app_id.clone(), // 生产端无独立 id，以 app_id 代
                            detail: CredentialDetail::LarkApp {
                                app_id: creds.app_id,
                                app_secret: creds.app_secret,
                                encrypt_key: None,
                                verification_token: None,
                            },
                            attributes: BTreeMap::from([("identity_mode".to_string(), mode)]),
                            already_decrypted: true,
                        })
                }
                // 其余 kind 统一走 user dal find_default（tavily 纯单轨 D27，无兜底）
                _ => {
                    let Some(user_id) = ctx.user_id.clone() else {
                        return Ok(None);
                    };
                    self.user_dal
                        .find_default_credential(
                            ctx.clone(),
                            &user_id,
                            requirement.kind,
                            requirement.platform.as_deref(),
                        )
                        .await?
                        .map(|credential| FetchedCredential {
                            credential_id: credential.id().to_string(),
                            detail: credential.detail().clone(),
                            attributes: BTreeMap::new(),
                            already_decrypted: false,
                        })
                }
            };
            let Some(credential) = fetched_credential else {
                return Ok(None);
            };
            fetched.push(credential);
        }
        Ok(Some(
            crate::pkg::credential::resolve_requirements(requirements, &fetched).await?,
        ))
    }
}

fn ensure_tool_enabled(tool_id: &str, status: &ToolStatus) -> Result<()> {
    if *status != ToolStatus::Enabled {
        bail_err!(
            ToolExecutionFailed,
            "Tool execution denied: tool {} has status {:?}",
            tool_id,
            status
        );
    }

    Ok(())
}

fn map_mcp_tool_error(tool_id: &str, error: &Error) -> String {
    let message = error.to_string();
    let normalized = message.to_lowercase();

    if normalized.contains("timed out") || normalized.contains("timeout") {
        format!("MCP tool call timed out for tool_id: {}", tool_id)
    } else if normalized.contains("server") && normalized.contains("not found") {
        format!("MCP server not found for tool_id: {}", tool_id)
    } else if normalized.contains("server") && normalized.contains("disabled") {
        format!("MCP server disabled for tool_id: {}", tool_id)
    } else if normalized.contains("tool") && normalized.contains("disabled") {
        format!("MCP tool disabled: {}", tool_id)
    } else if normalized.contains("tool") && normalized.contains("not found") {
        format!("MCP tool not found: {}", tool_id)
    } else {
        format!("MCP tool call failed for tool_id: {}", tool_id)
    }
}
