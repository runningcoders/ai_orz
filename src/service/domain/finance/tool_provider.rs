//! Tool Provider 子模块实现
//!
//! 工具提供商配置管理 + Agent 工具借用（绑定）关系
//! 注意：这属于财务管理（计费相关）

use crate::models::tool::Tool;
use crate::pkg::RequestContext;
use crate::pkg::tool_registry::http;
use crate::pkg::tool_registry::shell_tool;
use crate::service::domain::finance::{FinanceDomainImpl, ToolProviderManage};
use common::api::RuntimeReady;
use common::enums::{ControlMode, ToolProtocol, ToolStatus};
use common::error::{Result, bail_err, err};

#[async_trait::async_trait]
impl ToolProviderManage for FinanceDomainImpl {
    /// 创建 Tool
    async fn create_tool(&self, ctx: RequestContext, tool: &Tool) -> Result<()> {
        validate_tool_management_policy(tool)?;
        self.tool_dal.create_tool(ctx.clone(), &tool.po).await
    }

    /// 获取 Tool
    async fn get_tool(&self, ctx: RequestContext, tool_id: &str) -> Result<Option<Tool>> {
        self.tool_dal
            .get_by_id(ctx.clone(), tool_id.to_string())
            .await
    }

    /// 获取 Tool（带附带信息选项）
    async fn get_tool_with_options(
        &self,
        ctx: RequestContext,
        tool_id: &str,
        options: crate::service::dal::tool::ToolFetchOptions,
    ) -> Result<Option<Tool>> {
        self.tool_dal.get_tool(ctx.clone(), tool_id, options).await
    }

    /// 通用综合查询
    async fn query_tools(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::tool::ToolQuery,
    ) -> Result<common::api::PagedResult<Tool>> {
        self.tool_dal.query(ctx.clone(), query).await
    }

    /// 列出所有 Tool
    async fn list_tools(&self, ctx: RequestContext) -> Result<Vec<Tool>> {
        self.tool_dal.list_enabled(ctx.clone()).await
    }

    /// 列出所有启用工具的 distinct tags
    async fn list_tool_tags(&self, ctx: RequestContext) -> Result<Vec<String>> {
        // 排除系统保留标签：
        // - neural：天生拥有的能力，不作为工具包分发/浏览
        // - internal：仅用于 handler 自身元数据，不对外暴露
        const RESERVED: &[&str] = &["neural", "internal"];
        let all = self.tool_dal.list_tags(ctx.clone()).await?;
        let mut filtered: Vec<String> = all
            .into_iter()
            .filter(|t| !RESERVED.iter().any(|r| t == r))
            .collect();
        filtered.sort();
        filtered.dedup();
        Ok(filtered)
    }

    /// 同步内置工具到 DB（将 builtin tools registry 中的工具写入 DB）
    async fn sync_builtin_tools(&self, ctx: RequestContext) -> Result<usize> {
        self.tool_dal.sync_builtin_tools_to_db(ctx).await
    }

    /// 内置工具就绪自愈（平台侧唯一修复器；主体见 [`FinanceDomainImpl::reconcile_builtin_tool_readiness_impl`]）
    async fn reconcile_builtin_tool_readiness(&self, ctx: RequestContext) -> Result<usize> {
        self.reconcile_builtin_tool_readiness_impl(ctx).await
    }

    /// 更新 Tool
    async fn update_tool(&self, ctx: RequestContext, tool: &Tool) -> Result<()> {
        validate_tool_management_policy(tool)?;
        self.tool_dal.update_tool(ctx.clone(), tool).await
    }

    /// 删除 Tool
    async fn delete_tool(&self, ctx: RequestContext, tool: &Tool) -> Result<()> {
        self.tool_dal.delete_tool(ctx.clone(), &tool.po.id).await
    }

    /// ===== 工具借用（绑定）管理 =====
    /// Agent 借用工具（绑定）
    ///
    /// 校验：internal 标签工具为内部系统自用，不可绑定给 Agent。
    async fn bind_tool_to_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        // 校验：内部系统工具不能绑定给 Agent
        let tool = self
            .tool_dal
            .get_by_id(ctx.clone(), tool_id.to_string())
            .await?
            .ok_or_else(|| err!(InvalidRequest, "Tool {} not found", tool_id))?;
        if tool.po.get_tags().iter().any(|t| t == "internal") {
            bail_err!(
                InvalidRequest,
                "Internal system tool {} cannot be bound to Agent",
                tool_id
            );
        }
        let created_by = ctx.caller_id_or_system();
        self.tool_dal
            .add_tool_to_agent(ctx.clone(), agent_id, tool_id, Some(created_by))
            .await
    }

    /// Agent 归还工具（解绑）
    async fn unbind_tool_from_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tool_id: &str,
    ) -> Result<()> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        self.tool_dal
            .remove_tool_from_agent(ctx.clone(), agent_id, tool_id)
            .await
    }

    /// 获取 Agent 借用的所有工具 ID
    async fn get_agent_bound_tool_ids(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<String>> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        let tools = self
            .tool_dal
            .list_tools_for_agent_full(ctx.clone(), agent_id)
            .await?;
        Ok(tools.into_iter().map(|t| t.po.id).collect())
    }

    /// 获取 Agent 借用的所有工具
    async fn list_agent_tools(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Tool>> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        self.tool_dal
            .list_tools_for_agent_full(ctx.clone(), agent_id)
            .await
    }

    /// 搜索工具（向量 + 关键词混合搜索）
    async fn search_tools(
        &self,
        ctx: RequestContext,
        params: crate::service::dao::tool::ToolSearch,
    ) -> Result<common::api::PagedResult<Tool>> {
        self.tool_dal.search(ctx.clone(), params).await
    }

    /// 查询组织级工具调用汇总（工作台顶栏运行时读数用）
    async fn tool_call_stats(
        &self,
        ctx: RequestContext,
        minutes: u32,
    ) -> Result<common::models::ToolStats> {
        self.tool_dal.tool_call_stats(ctx.clone(), minutes).await
    }
}

fn validate_tool_management_policy(tool: &Tool) -> Result<()> {
    // ToolProtocol and ControlMode are orthogonal (per design docs):
    //   Builtin ≠ Auto, Http ≠ Manual; whether a tool enters Rig is decided
    //   solely by ControlMode.  Mcp and Shell tools are restricted to Manual
    //   because both run through the self-built dispatch chain (MCP has its
    //   own tool-calling protocol; Shell tools need the confirmation gate).
    if matches!(tool.po.protocol, ToolProtocol::Mcp | ToolProtocol::Shell)
        && !matches!(tool.po.control_mode, ControlMode::Manual)
    {
        bail_err!(
            InvalidRequest,
            "Mcp/Shell Tool only supports Manual control mode"
        );
    }

    match tool.po.protocol {
        ToolProtocol::Http => {
            http::validate_tool_po_config(&tool.po)
                .map_err(|err| err!(InvalidRequest, "{}", err).with_source(err))?;
        }
        ToolProtocol::Shell => {
            shell_tool::validate_tool_po_config(&tool.po)
                .map_err(|err| err!(InvalidRequest, "{}", err).with_source(err))?;
        }
        _ => {}
    }

    Ok(())
}

// ==================== 内置工具就绪自愈（平台侧唯一修复器） ====================

/// 单次自愈批量上限（护栏：一次启动最多修这么多，规模异常立刻停手）
pub(crate) const SELF_HEAL_MAX_PER_RUN: usize = 20;

/// 同类修复去重窗口（护栏：7 天内同一工具只修一次，防抖、防来回翻动）
pub(crate) const SELF_HEAL_DEDUP_WINDOW_MS: u64 = 7 * 24 * 60 * 60 * 1000;

/// 留痕锚点：写入 trace `metadata.heal_kind`，既做审计也做 7 天去重的识别键
pub(crate) const SELF_HEAL_KIND: &str = "builtin_tool_readiness_self_heal";

/// 允许自愈的字段 —— **严格单一字段 `config.command`**
const SELF_HEAL_ALLOWED_FIELD: &str = "command";

impl FinanceDomainImpl {
    /// 内置工具就绪自愈主体（实现见 trait 文档：六条 AND 闸门 + 全套护栏）
    ///
    /// 失败只记 warn 不向外冒泡：这是启动期「尽力而为」增强，任何异常都不得
    /// 影响内置工具同步本身。
    pub(crate) async fn reconcile_builtin_tool_readiness_impl(
        &self,
        ctx: RequestContext,
    ) -> Result<usize> {
        let Some(logger) = crate::pkg::tool_tracing::logger::ToolCallLogger::try_get() else {
            // 无留痕通道即不放行写库（护栏：修复必须可追溯）
            log_warn!("跳过内置工具就绪自愈：工具调用日志器未初始化（无法留痕）");
            return Ok(0);
        };

        let registry = crate::pkg::tool_registry::get_registry();
        let mut healed = 0usize;

        for tool_id in registry.list_builtin_ids() {
            if healed >= SELF_HEAL_MAX_PER_RUN {
                log_warn!(
                    "内置工具就绪自愈达到单次上限 {}，本轮余下工具留待下次启动",
                    SELF_HEAL_MAX_PER_RUN
                );
                break;
            }

            // 闸门 1：取到 Tool 且必须是 Builtin（工厂自带判定，这里是双保险）
            let Some(tool) = self
                .tool_dal
                .get_by_id(ctx.clone(), tool_id.clone())
                .await?
            else {
                continue;
            };
            if tool.po.protocol != ToolProtocol::Builtin {
                continue;
            }
            // 闸门 2：禁用工具不动（用户明确关掉的工具不该被"好心"改配置）
            if tool.po.status != ToolStatus::Enabled {
                continue;
            }
            let Some(factory) = registry.get_builtin_factory(&tool.po.id) else {
                continue;
            };

            // 闸门 3：只处理「找不到命令」这一类未就绪
            let readiness = factory.probe_readiness(&tool.po);
            let RuntimeReady::NotReady { reason, .. } = &readiness else {
                continue;
            };
            if !is_healable_readiness_reason(reason) {
                continue;
            }

            // 闸门 4：工具自己给出自愈提议（机制多态），平台不猜
            let Some(proposal) = factory.propose_self_heal(&tool.po) else {
                log_info!("内置工具 {} 未就绪且无可自愈方案（交人工处理）", tool.po.id);
                continue;
            };
            let Some(new_command) = extract_single_command(&proposal) else {
                continue;
            };

            // 闸门 5：幂等 + 差异集严格等于 {config.command}
            let old_command = tool.po.cli_command();
            if is_noop_heal(old_command.as_deref(), &new_command) {
                continue;
            }

            // 闸门 6a：7 天去重（查留痕，不额外建表）
            if recently_healed(logger, &tool.po.id) {
                log_info!("内置工具 {} 7 天内已自愈过，本轮跳过（去重）", tool.po.id);
                continue;
            }

            // 闸门 6b：写入前再核一次规模上限
            if exceeds_self_heal_budget(healed) {
                break;
            }

            // ---------- 落库：只换 config.command，其余键原样保留 ----------
            let mut new_po = tool.po.clone();
            new_po.config = with_healed_command(&new_po.config, &new_command);
            let healed_tool = Tool::from_po_for_management(new_po);
            if let Err(err) = self.tool_dal.update_tool(ctx.clone(), &healed_tool).await {
                log_warn!("内置工具 {} 自愈写库失败：{}", tool.po.id, err);
                continue;
            }

            // ---------- 留痕：old_command 是一键回滚依据 ----------
            log_heal_trace(
                logger,
                build_heal_trace_entry(
                    &tool.po.id,
                    &tool.po.name,
                    old_command.as_deref(),
                    &new_command,
                    &format!("{:?}", proposal),
                ),
            );

            // ---------- 让新判定立刻可见（否则最长 30s 仍显示旧状态） ----------
            crate::service::domain::runtime::invalidate_readiness_cache(&tool.po.id);

            healed += 1;
            log_info!(
                "内置工具 {} 命令路径已自愈：{:?} → {}（留痕含 old_command，可一键回滚）",
                tool.po.id,
                old_command,
                new_command
            );
        }

        Ok(healed)
    }
}

/// 从自愈提议里提取 `command` 单字段；多余字段丢弃并 warn（红线：单一字段）
pub(crate) fn extract_single_command(proposal: &serde_json::Value) -> Option<String> {
    let obj = proposal.as_object()?;
    let value = obj
        .get(SELF_HEAL_ALLOWED_FIELD)?
        .as_str()?
        .trim()
        .to_string();
    if value.is_empty() {
        return None;
    }
    let extra: Vec<&str> = obj
        .keys()
        .map(String::as_str)
        .filter(|k| *k != SELF_HEAL_ALLOWED_FIELD)
        .collect();
    if !extra.is_empty() {
        log_warn!(
            "自愈提议含多余字段 {:?}，已丢弃（只允许修 config.{}）",
            extra,
            SELF_HEAL_ALLOWED_FIELD
        );
    }
    Some(value)
}

/// 只替换 `config.command`，其余 config 键（timeout_ms / install_hint 等）原样保留
pub(crate) fn with_healed_command(config: &serde_json::Value, command: &str) -> serde_json::Value {
    let mut object = match config.as_object() {
        Some(map) => map.clone(),
        None => serde_json::Map::new(),
    };
    object.insert(
        SELF_HEAL_ALLOWED_FIELD.to_string(),
        serde_json::Value::String(command.to_string()),
    );
    serde_json::Value::Object(object)
}

/// 7 天去重：查留痕里同工具、同类修复的最近一条
fn recently_healed(
    logger: &crate::pkg::tool_tracing::logger::ToolCallLogger,
    tool_id: &str,
) -> bool {
    let now_ms = common::constants::utils::current_timestamp_ms().max(0) as u64;
    let since = now_ms.saturating_sub(SELF_HEAL_DEDUP_WINDOW_MS);
    let query = crate::pkg::tool_tracing::logger::ToolCallQuery {
        tool_id: Some(tool_id.to_string()),
        started_after: Some(since),
        limit: Some(20),
        ..Default::default()
    };
    let now_ms = common::constants::utils::current_timestamp_ms().max(0) as u64;
    match logger.query_calls(query) {
        Ok(entries) => entries
            .iter()
            .filter(|entry| is_self_heal_entry(entry))
            .any(|entry| is_within_dedup_window(entry.started_at, now_ms)),
        // 查询失败不阻断（去重是优化不是正确性前提），但如实告警
        Err(err) => {
            log_warn!("自愈去重查询失败（按未修复继续）：{}", err);
            false
        }
    }
}

/// 判定一条 trace 是否为同类自愈记录
pub(crate) fn is_self_heal_entry(entry: &crate::pkg::tool_tracing::entry::ToolCallEntry) -> bool {
    entry
        .metadata
        .get("heal_kind")
        .and_then(|v| v.as_str())
        .is_some_and(|kind| kind == SELF_HEAL_KIND)
}

/// 就绪原因闸门：只有「找不到命令」这一类可自愈
pub(crate) fn is_healable_readiness_reason(reason: &str) -> bool {
    reason == "cli_not_installed"
}

/// 幂等闸门：提议值与现值相同即视为已修复
pub(crate) fn is_noop_heal(old_command: Option<&str>, new_command: &str) -> bool {
    old_command.is_some_and(|old| old == new_command)
}

/// 批量上限闸门：已修数量是否已达单次上限
pub(crate) fn exceeds_self_heal_budget(healed: usize) -> bool {
    healed >= SELF_HEAL_MAX_PER_RUN
}

/// 去重窗口判定：`healed_at` 是否仍落在 7 天窗口内
///
/// 异常时间戳（0 / 未来）一律按「窗口内」保守处理——宁可少修一次，
/// 也不要在时间不可信时反复改写用户配置。
pub(crate) fn is_within_dedup_window(healed_at: u64, now_ms: u64) -> bool {
    if healed_at == 0 || healed_at > now_ms {
        return true;
    }
    now_ms.saturating_sub(healed_at) < SELF_HEAL_DEDUP_WINDOW_MS
}

/// 构造自愈留痕 entry（`metadata.old_command` 即回滚依据）
pub(crate) fn build_heal_trace_entry(
    tool_id: &str,
    tool_name: &str,
    old_command: Option<&str>,
    new_command: &str,
    proposal: &str,
) -> crate::pkg::tool_tracing::entry::ToolCallEntry {
    use crate::pkg::tool_tracing::entry::{ToolCallEntry, ToolCallStatus};

    let now = common::constants::utils::current_timestamp_ms().max(0) as u64;
    ToolCallEntry {
        call_id: format!("selfheal-{}-{}", tool_id, now),
        tool_id: tool_id.to_string(),
        tool_name: tool_name.to_string(),
        started_at: now,
        finished_at: now,
        duration_ms: 0,
        input: serde_json::json!({
            "operation": "config.command 自动修复",
            "detail": "内置工具就绪自愈：命令不可解析但可被发现，按护栏修正配置"
        }),
        output: Some(serde_json::json!({
            "healed": true,
            "field": SELF_HEAL_ALLOWED_FIELD,
            "new_command": new_command,
        })),
        status: ToolCallStatus::Completed,
        metadata: serde_json::json!({
            "heal_kind": SELF_HEAL_KIND,
            "old_command": old_command,
            "new_command": new_command,
            "proposal": proposal,
            "rollback": "把 metadata.old_command 写回 config.command 即回滚",
            "guard": {
                "single_field": SELF_HEAL_ALLOWED_FIELD,
                "max_per_run": SELF_HEAL_MAX_PER_RUN,
                "dedup_window_days": 7,
            }
        }),
        ..Default::default()
    }
}

/// 写自愈留痕（与其它工具调用共用 `tool_call_trace` 同一出口）
fn log_heal_trace(
    logger: &crate::pkg::tool_tracing::logger::ToolCallLogger,
    entry: crate::pkg::tool_tracing::entry::ToolCallEntry,
) {
    if let Err(err) = logger.log_call(entry) {
        log_warn!("自愈留痕写入失败（配置已改，请以日志为准）：{}", err);
    }
}
