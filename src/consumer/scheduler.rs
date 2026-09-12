//! Cron 触发器消费者（业务层）
//!
//! 作为 AOP 事件中心的订阅者，消费 CRON_TRIGGER 事件。
//! 业务逻辑通过调用 domain 层完成（如 RuntimeAwakening.sleep_and_settle）。
//!
//! 与 AOP 框架解耦：AOP 只负责事件流转，本模块负责业务编排。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::handlers::hr::agent::settle_memory::load_and_settle;
use crate::models::events::CronTriggerEvent;
use crate::pkg::RequestContext;
use crate::pkg::aop::Event;
use crate::pkg::aop::{ConsumeMode, Consumer, EventKind};
use crate::service::domain::runtime::domain as runtime_domain;
use common::error::{Error, Result};

// ==================== 消费者实现 ====================

/// Cron 触发器消费者
///
/// 订阅 CRON_TRIGGER 事件，按 payload.action 分发到不同 domain 处理。
/// 作为 AOP 的 Sync 消费者，事件发布时直接调用 on_event。
pub struct CronTriggerConsumer;

impl Default for CronTriggerConsumer {
    fn default() -> Self {
        Self::new()
    }
}

impl CronTriggerConsumer {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Consumer for CronTriggerConsumer {
    fn name(&self) -> &str {
        "cron_trigger"
    }

    fn interested_events(&self) -> Vec<EventKind> {
        vec![EventKind::new("cron.trigger")]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    async fn on_event(&self, _ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: CronTriggerEvent = serde_json::from_value(event).map_err(|e| {
            Error::internal(format!("failed to deserialize cron trigger event: {}", e))
        })?;

        sys_debug!(
            "received cron trigger event: {} (trigger_id: {}, action to be parsed)",
            event.id(),
            event.trigger_id
        );

        let payload: CronTriggerPayload = serde_json::from_str(&event.payload).map_err(|e| {
            Error::bad_request(format!(
                "invalid cron trigger payload for trigger {}: {}",
                event.trigger_id, e
            ))
        })?;

        sys_info!(
            "cron trigger fired: {} (trigger_id: {}, action: {})",
            event.trigger_name,
            event.trigger_id,
            payload.action
        );

        match payload.action.as_str() {
            "agent_rest" => {
                if let Err(e) = self.handle_agent_rest(&event, &payload.extra).await {
                    // 单次触发失败只告警不上抛：避免 nack 重试风暴（下个周期会重新沉淀）
                    sys_error!(
                        "agent_rest action failed for trigger {} (id: {}): {}",
                        event.trigger_name,
                        event.trigger_id,
                        e
                    );
                }
            }
            "project_followup" => self.handle_project_followup(&payload.extra).await?,
            "tool_log_cleanup" => self.handle_tool_log_cleanup().await?,
            "directory_reconcile" => self.handle_directory_reconcile().await?,
            _ => {
                sys_warn!(
                    "unknown action '{}' for trigger {} (id: {})",
                    payload.action,
                    event.trigger_name,
                    event.trigger_id
                );
            }
        }

        Ok(())
    }
}

// ==================== 业务编排（调用 domain 层）====================

impl CronTriggerConsumer {
    /// agent_rest 动作：加载 Agent 并调用 sleep_and_settle 执行记忆沉淀
    ///
    /// 复用 settle_memory handler 的 load_and_settle 公共函数，保证与神经工具触发的
    /// 沉淀流程完全一致（查询短期记忆 → 拼装 prompt → 加载 Agent → 唤醒 Brain → sleep_and_settle）。
    ///
    /// 作用域：payload 指定 `agent_id` 时只沉淀该 Agent；**缺省（系统默认触发器）
    /// 则扫描所有存在未沉淀短期记忆的 Agent 逐个沉淀**——单个 Agent 失败不阻断其余。
    async fn handle_agent_rest(&self, event: &CronTriggerEvent, extra: &Value) -> Result<()> {
        let payload: AgentRestPayload = serde_json::from_value(extra.clone()).map_err(|e| {
            Error::bad_request(format!(
                "invalid agent_rest payload for trigger {}: {}",
                event.trigger_id, e
            ))
        })?;

        let ctx = RequestContext::new_system();
        // 不传 settle_limit 时用自适应上限：实际批量由上下文预算决定，
        // 该字段仅作为条数上限提示（见 settle_memory::PENDING_MAX_ITEMS）。
        let settle_limit = payload
            .settle_limit
            .unwrap_or(crate::handlers::hr::agent::settle_memory::PENDING_MAX_ITEMS);

        // 解析本次要沉淀的 Agent 集合：指定 agent_id → 单个；缺省 → 全部有待沉淀记忆的 Agent
        let agent_ids: Vec<String> = match &payload.agent_id {
            Some(id) => vec![id.clone()],
            None => {
                use crate::service::dao::memory::{MemoryQuery, MemorySortOrder};
                use common::enums::{MemoryStatus, MemoryType};

                // 索引表规模有限（只含摘要），取全量 Active 短期记忆后去重即可。
                // 上限是防御值：正常远达不到；触达时下次周期会继续处理剩余部分。
                let pending = runtime_domain()
                    .memory()
                    .query(
                        ctx.clone(),
                        MemoryQuery {
                            status: Some(MemoryStatus::Active),
                            memory_type: Some(MemoryType::ShortTerm),
                            limit: Some(1000),
                            order: MemorySortOrder::OldestFirst,
                            ..Default::default()
                        },
                    )
                    .await?;
                let mut ids: Vec<String> = pending
                    .iter()
                    .filter_map(|m| {
                        crate::handlers::hr::agent::settle_memory::short_term_of(m)
                            .map(|i| i.agent_id.clone())
                    })
                    .collect();
                ids.sort();
                ids.dedup();
                ids
            }
        };

        sys_info!(
            "agent_rest action triggered by {} (trigger_id: {}, agents: {:?})",
            event.trigger_name,
            event.trigger_id,
            agent_ids
        );

        let mut total_settled = 0usize;
        let mut ok_agents = 0usize;
        for agent_id in &agent_ids {
            match load_and_settle(ctx.clone(), agent_id, settle_limit).await {
                Ok(0) => {}
                Ok(n) => {
                    total_settled += n;
                    ok_agents += 1;
                }
                Err(e) => {
                    // 单个 Agent 沉淀失败（如模型调用异常）不影响其余 Agent
                    sys_warn!("agent_rest: Agent {} 沉淀失败，跳过: {}", agent_id, e);
                }
            }
        }

        sys_info!(
            "agent_rest 完成: 扫描 {} 个 Agent，{} 个成功沉淀，共沉淀 {} 条短期记忆",
            agent_ids.len(),
            ok_agents,
            total_settled
        );

        Ok(())
    }

    /// project_followup 动作：对所有进行中且有 Owner Agent 的项目发送跟进通知
    ///
    /// 定时补偿场景（Agent Loop Engine 场景 3）：扫描所有 InProgress 且
    /// owner_agent_id 非空的项目，向 Owner Agent 发送 ProjectFollowupNotification
    /// 消息，驱动其检查项目进度并处理阻塞任务。
    /// 预检查 Agent 运行时状态：Busy/Resting 时跳过，避免无意义 nack 堆积。
    async fn handle_project_followup(&self, _extra: &Value) -> Result<()> {
        use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
        use crate::service::domain::message::SendToAgentCommand;
        use crate::service::domain::message::builder::build_project_followup_content;
        use crate::service::domain::message::domain as message_domain;
        use crate::service::domain::project::domain as project_domain;
        use common::enums::{MessageRole, MessageType};

        let ctx = RequestContext::new_system();

        // 1. 查询所有进行中且有 Owner Agent 的项目
        let projects = project_domain()
            .project_manage()
            .list_in_progress_with_owner(ctx.clone())
            .await?;

        for project in projects {
            let owner_agent_id = match &project.po.owner_agent_id {
                Some(id) => id,
                None => continue,
            };

            // 2. 预检查：Agent 必须空闲才发送，避免无意义 nack 堆积
            let state = AgentRuntimeStateManager::global().get_state(owner_agent_id);
            if state.is_unavailable() {
                sys_info!("Agent {} 当前 {:?}，跳过项目跟进", owner_agent_id, state);
                continue;
            }

            // 3. 构建消息内容（意图指令嵌入消息本体）
            let content = build_project_followup_content(&project.po.name);

            // 4. 发送消息（填充 project_id 上下文，MessageConsumer 会自动补充 project 信息）
            let cmd = SendToAgentCommand {
                from_id: "system",
                from_role: MessageRole::System,
                to_agent_id: owner_agent_id,
                content: &content,
                project_id: Some(&project.po.id),
                task_id: None,
                reply_to_id: None,
                external_key: None,
                attachment_ids: None,
                message_type: MessageType::ProjectFollowupNotification,
            };

            if let Err(e) = message_domain()
                .delivery()
                .send_to_agent(ctx.clone(), cmd)
                .await
            {
                sys_warn!("发送项目跟进消息失败: agent={}, err={}", owner_agent_id, e);
            }
        }

        Ok(())
    }

    /// tool_log_cleanup 动作：清理超期工具运行日志（① 运行时输出 TTL）
    ///
    /// 保留天数读 `[tool_log].retention_days` 配置（0 = 不清理）；
    /// Running 进程日志所在日期目录受保护跳过（见 tool_log_retention 模块）。
    async fn handle_tool_log_cleanup(&self) -> Result<()> {
        let config = crate::config::get();
        let retention_days = config.tool_log.retention_days;
        if retention_days == 0 {
            sys_info!("tool_log_cleanup: retention_days = 0，跳过清理");
            return Ok(());
        }

        let base_path = config.base_data_path();
        let report = tokio::task::spawn_blocking(move || {
            crate::pkg::tool_log_retention::cleanup_tool_logs(&base_path, retention_days)
        })
        .await
        .map_err(|e| Error::internal(format!("tool log cleanup task join error: {}", e)))?;

        sys_info!(
            "tool_log_cleanup 完成: 扫描 {} 个日志根，删除 {} 个日期目录 / {} 个文件，释放 {} 字节，{} 个目录因 Running 保护跳过",
            report.scanned_roots,
            report.removed_dirs,
            report.removed_files,
            report.freed_bytes,
            report.skipped_dirs
        );
        Ok(())
    }

    /// directory_reconcile 动作：逐 Active 对端双向目录同步（推本地 + 拉对端）
    ///
    /// 兑底推送丢失/失败的场景，保证目录最终一致（变更推送由
    /// FederationDirectoryConsumer 消费 organization.changed 事件承担时效性）。
    async fn handle_directory_reconcile(&self) -> Result<()> {
        let ctx = RequestContext::new_system();
        let report = crate::service::domain::organization::domain()
            .organization_manage()
            .reconcile_directories(ctx)
            .await?;

        sys_info!(
            "directory_reconcile 完成: peers={} pushed={} pulled_written={}",
            report.peers,
            report.pushed,
            report.pulled_written
        );
        Ok(())
    }
}

// ==================== 辅助类型 ====================

#[derive(Debug, Serialize, Deserialize)]
struct CronTriggerPayload {
    action: String,
    #[serde(flatten)]
    extra: Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct AgentRestPayload {
    /// 目标 Agent；缺省 = 系统级全局沉淀（扫描所有存在未沉淀短期记忆的 Agent）
    #[serde(default)]
    agent_id: Option<String>,
    settle_limit: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 系统默认触发器的 payload **没有 agent_id**——历史版本因 AgentRestPayload
    /// 必填 agent_id 导致 agent_rest 每次触发都在解析阶段失败（沉淀从未执行）。
    /// 锁定：缺省 agent_id 可解析为 None（全局沉淀语义）。
    #[test]
    fn test_agent_rest_payload_without_agent_id_parses() {
        let payload: AgentRestPayload =
            serde_json::from_str(r#"{"settle_limit":10}"#).expect("系统默认 payload 应可解析");
        assert_eq!(payload.agent_id, None);
        assert_eq!(payload.settle_limit, Some(10));

        // 兼容指定单个 Agent 的旧语义
        let payload: AgentRestPayload =
            serde_json::from_str(r#"{"agent_id":"agent-001","settle_limit":5}"#)
                .expect("指定 agent_id 的 payload 应可解析");
        assert_eq!(payload.agent_id.as_deref(), Some("agent-001"));
        assert_eq!(payload.settle_limit, Some(5));
    }
}
