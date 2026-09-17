//! Cron 触发器消费者（业务层）
//!
//! 作为 AOP 事件中心的订阅者，消费 CRON_TRIGGER 事件。
//! 业务逻辑通过调用 domain 层完成；**重量级动作（如记忆沉淀）只派发事件**，
//! 交给对应的 Async 消费者执行 —— 本消费者是 Sync，跑在 cron 轮询线程里，
//! 在这里做长任务会把其它定时触发器全部堵住。
//!
//! 与 AOP 框架解耦：AOP 只负责事件流转，本模块负责业务编排。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::events::{AgentSettleEvent, CronTriggerEvent};
use crate::pkg::RequestContext;
use crate::pkg::aop::Event;
use crate::pkg::aop::{ConsumeMode, Consumer, RetryDecision, Subscription};
use crate::service::domain::runtime::domain as runtime_domain;
use common::enums::EventTopic;
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

    fn subscriptions(&self) -> Vec<Subscription> {
        // `.notify_producer()` 是**必需**的：`mark_trigger_executed` 已从生产者的
        // `tick()` 搬到 `CronTriggerProducer::on_consumed`（修 P3），不声明它就永不回调
        // → 触发器再也推进不了 `next_run_at` → 每个 tick 都重复触发同一个触发器。
        //
        // ⚠️ **不能**再声明 `.ordered()`：本消费者是 `ConsumeMode::Sync`，注册期硬校验
        // 会直接 `Err`（内联执行、无队列无门闩，声明 ordered 是谎言）。
        vec![Subscription::new(EventTopic::CronTrigger).notify_producer()]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Sync
    }

    /// **永不放弃**：一次执行失败**不能**被当成"这次就算执行过了"
    ///
    /// ⚠️ **必须**覆写默认的 `decide_retry`：默认策略会按「永久性错误码」判 `Discard`，
    /// 而框架对 `Discard` 的处理是 ack + 照常回调 `on_consumed` → 于是失败的那一次
    /// 会被 `mark_trigger_executed` 标记成"已执行"（触发器推进到下一个 cron 点）。
    /// 一次瞬时 DB 抖动就能让一个日级触发器**丢掉一整天**。
    ///
    /// 返回 `Retry` 的语义是「交给下一次机会」：本消费者是 Sync，投递结论没有重投
    /// 驱动者 —— 真正的重试驱动是「没 mark → 下个 tick 又被 `list_due_triggers` 捞到」。
    fn decide_retry(&self, _err: &str, _attempt: u32) -> RetryDecision {
        RetryDecision::Retry
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

        let params = payload.action_params();

        match payload.action.as_str() {
            "agent_rest" => {
                if let Err(e) = self.handle_agent_rest(&event, &params).await {
                    // 单次触发失败只告警不上抛：避免 nack 重试风暴（下个周期会重新沉淀）
                    sys_error!(
                        "agent_rest action failed for trigger {} (id: {}): {}",
                        event.trigger_name,
                        event.trigger_id,
                        e
                    );
                }
            }
            "project_followup" => self.handle_project_followup(&params).await?,
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
    /// agent_rest 动作：**只派发沉淀请求**，不在此处执行沉淀
    ///
    /// 作用域：payload 指定 `agent_id` 时只派发该 Agent；**缺省（系统默认触发器）
    /// 则扫描所有存在未沉淀短期记忆的 Agent 逐个派发**。
    ///
    /// 执行交给 `agent.awakening` 消费者（`consumer/message.rs::handle_settle_request`），
    /// 本消费者只做两件事：解析目标 Agent + publish 事件。
    /// **不要在这里改回同步调用 `load_and_settle`**：
    /// - 本消费者是 `ConsumeMode::Sync`，同步跑一场沉淀（LLM 往返，实测数分钟）会把整个
    ///   cron 轮询堵住，其它触发器（工具日志清理 / 目录对账）只能干等
    /// - Agent 忙时同步路径只能「跳过」，而触发器随后就会把 `next_run_at` 推到下一个
    ///   cron 点（日触发 = 次日）→ 一次跳过丢一天。走队列则 `order_key = agent_id`
    ///   使沉淀与发给同一 Agent 的消息落在同一条队列上串行，忙时不丢、也不刷重试日志
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

        // 逐个 Agent 派发沉淀请求。order_key = agent_id 与 message.created（接收者为
        // Agent 时）同源，且两者现在同属 agent.awakening 消费者的同一条队列 →
        // 同 Agent 的沉淀与消息在**队列层**就串行，无需依赖运行期抢占失败来兜底。
        for agent_id in &agent_ids {
            crate::pkg::aop::publish(
                &ctx,
                AgentSettleEvent::new(agent_id, settle_limit, &event.trigger_name),
            )
            .await;
        }

        sys_info!(
            "agent_rest 完成: 已派发 {} 个 Agent 的沉淀请求（trigger_id: {}），执行由 agent.awakening 消费者承担",
            agent_ids.len(),
            event.trigger_id
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

            // 4. 补齐组织上下文（系统触发链路 ctx 无组织绑定，见 mod.rs helper 说明）
            let ctx =
                crate::consumer::enrich_org_from_project_user(&ctx, &project.po.root_user_id).await;

            // 5. 发送消息（填充 project_id 上下文，MessageConsumer 会自动补充 project 信息）
            //
            // 【身份分层模型】触发器按「被触达事项的归属」选择发送方身份：
            // - 项目归属用户非空 → **以用户身份中继**（from_role=User）：巡检的是用户
            //   发起的项目，检查结论用户需要感知；Agent 的 Final 自动回复会回到该用户
            //   （消息链自然路由，无需白名单特判），也不涉及伪造——这是上下文中继。
            // - 无归属用户（如 A2A 项目）→ 万不得已落 System：Final 无人可投递自然丢弃。
            //   二者都不会把回复路由回 Agent 自身 → 无自唤醒循环。
            let (from_id, from_role) = if project.po.root_user_id.is_empty() {
                ("system", MessageRole::System)
            } else {
                (project.po.root_user_id.as_str(), MessageRole::User)
            };
            let cmd = SendToAgentCommand {
                from_id,
                from_role,
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

/// Cron 触发器 payload 外框：`{"action":"<动作>","extra":{...}}`
///
/// ⚠️ `extra` 必须是**具名字段**，不能改回 `#[serde(flatten)]`：flatten 会把 `extra`
/// 这个键名本身也收进 `Value`，得到 `{"extra":{"settle_limit":10}}` 再往下传，
/// 于是 `AgentRestPayload` 解析时 `agent_id` / `settle_limit` 全部静默降级为 `None`
/// —— 表现为「指定单个 Agent 的沉淀退化成全局扫描，且 settle_limit 永远被忽略」，
/// 且**不报错**。存量 payload 形态见 `.ai_orz/ai_orz.db::cron_triggers` 与
/// `docs/wiki/.../定时任务 API.md`（系统默认触发器 seed 见
/// `service/domain/system/mod.rs::ensure_system_cron_triggers`）。
#[derive(Debug, Serialize, Deserialize)]
struct CronTriggerPayload {
    action: String,
    /// 动作参数；缺省（历史用户只写 `{"action":"x"}`）时为空对象语义
    #[serde(default)]
    extra: Value,
}

impl CronTriggerPayload {
    /// 动作参数（保证拿到的是对象，缺省/`null` 归一为空对象）
    fn action_params(&self) -> Value {
        if self.extra.is_object() {
            self.extra.clone()
        } else {
            Value::Object(serde_json::Map::new())
        }
    }
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

    /// 外框解析：`{"action":..,"extra":{..}}` 里的 `extra` 必须原样透传给动作层。
    ///
    /// 回归 `#[serde(flatten)] extra: Value` 那个坑：flatten 会把 `extra` 键名本身
    /// 也收进 Value（`{"extra":{"settle_limit":10}}`），参数静默丢失、且不报错。
    #[test]
    fn test_cron_trigger_payload_passes_extra_through() {
        // 与真实存量 payload（`.ai_orz/ai_orz.db::cron_triggers`）逐字一致
        let payload: CronTriggerPayload =
            serde_json::from_str(r#"{"action":"agent_rest","extra":{"settle_limit":10}}"#)
                .expect("系统默认 agent_rest payload 应可解析");
        assert_eq!(payload.action, "agent_rest");
        assert_eq!(
            payload.action_params(),
            serde_json::json!({"settle_limit": 10})
        );

        // 透传到 AgentRestPayload 后参数仍在（旧实现这里 settle_limit 会丢成 None）
        let rest: AgentRestPayload =
            serde_json::from_value(payload.action_params()).expect("透传后应可解析");
        assert_eq!(rest.settle_limit, Some(10));

        // 指定单个 Agent 的写法同样必须透传（否则退化成全局扫描）
        let payload: CronTriggerPayload = serde_json::from_str(
            r#"{"action":"agent_rest","extra":{"agent_id":"agent-001","settle_limit":5}}"#,
        )
        .expect("指定 agent_id 的 payload 应可解析");
        let rest: AgentRestPayload =
            serde_json::from_value(payload.action_params()).expect("透传后应可解析");
        assert_eq!(rest.agent_id.as_deref(), Some("agent-001"));
        assert_eq!(rest.settle_limit, Some(5));
    }

    /// 缺省 / `null` 的 `extra` 归一为空对象，动作层解析不应因 `null` 报错
    /// （避免又回到「解析失败 → 只打日志 → 沉淀静默不执行」的老路）。
    #[test]
    fn test_cron_trigger_payload_tolerates_missing_extra() {
        for raw in [
            r#"{"action":"tool_log_cleanup"}"#,
            r#"{"action":"tool_log_cleanup","extra":null}"#,
            r#"{"action":"project_followup","extra":{}}"#,
        ] {
            let payload: CronTriggerPayload =
                serde_json::from_str(raw).unwrap_or_else(|e| panic!("{} 应可解析: {}", raw, e));
            assert_eq!(
                payload.action_params(),
                serde_json::json!({}),
                "raw={}",
                raw
            );
        }

        // 无 extra 的 agent_rest 必须仍能解析成「全局沉淀」语义
        let payload: CronTriggerPayload =
            serde_json::from_str(r#"{"action":"agent_rest"}"#).expect("应可解析");
        let rest: AgentRestPayload = serde_json::from_value(payload.action_params())
            .expect("空参数应解析成全局沉淀而不是报错");
        assert_eq!(rest.agent_id, None);
        assert_eq!(rest.settle_limit, None);
    }

    /// 未知字段不能影响外框解析（用户可能自行加注释字段）
    #[test]
    fn test_cron_trigger_payload_ignores_unknown_fields() {
        let payload: CronTriggerPayload =
            serde_json::from_str(r#"{"action":"agent_rest","note":"手写","extra":{}}"#)
                .expect("应可解析");
        assert_eq!(payload.action, "agent_rest");
        assert_eq!(payload.action_params(), serde_json::json!({}));
    }
}
