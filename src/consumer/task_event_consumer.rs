//! Task event consumer (AOP async)
//!
//! 订阅 `task.status_changed` 事件，对 `TaskStatus::Completed` 的状态变更做两件事：
//! 1. **后继就绪补发**：给依赖本任务且前置已全量就绪的后继任务补发 `TaskAssignment`
//! 2. 项目 Owner Agent 通知（`TaskDispatchNotification`），驱动 Agent Loop Engine
//!    的 Layer 2 补偿机制
//!
//! 设计要点：
//! - `ConsumeMode::Async`：异步消费，发送方（DAL 层 update_status）不阻塞
//! - 仅处理 `TaskStatus::Completed` 事件（其他状态变更暂不通知）
//! - **① 后继补发无条件执行**：`dispatch_ready_successors` 是 DAG 的机械推进责任方，放在所有门禁**之前**，绝不因 Owner 通知被跳过而停摆
//! - **Owner 通知唯一门禁**：⑤ **同一项目内**已有「跟进类」未处理通知（`TaskDispatchNotification` / `ProjectFollowupNotification`）→ 跳过（跨类型统一去重，作用域是项目而非 Owner —— 按 Agent 去重会让多项目 Owner 被先挂上 Pending 的项目永久占门，口径见 message domain）
//! - 消息内容使用 `build_task_dispatch_content` 构建意图指令
//! - 消息填充 `project_id` + `task_id` 上下文字段，MessageConsumer 自动补充上下文
//!
//! # 为什么必须在这里补发后继任务
//!
//! 项目管理里 Owner 会把整张 DAG **一次性全部分派**出去，后继任务的
//! TaskAssignment 往往先于它的前置任务完成而到达。`wake_gate_policy` 的
//! `dependency_unmet` 门闩会跳过这类唤醒，而 AOP **没有延迟重投能力**
//! （只有 Retry/Discard，见 `pkg/aop/core/consumer.rs`）—— 跳过即 ack 终结。
//! 若没有这里的补发，后继任务会永久停在 Pending 且**没有任何报错**：
//! DAG 的推进责任会从一条确定的消息，转嫁给「Owner Agent 记得重新派发」这个
//! 概率性行为上。门闩负责省资源，补发负责不丢活，两者必须成对存在。
//!
//! # 为什么不再用「谁在忙」来决定是否通知（已删除的两道旧门禁）
//!
//! 历史实现有另两道门禁：② 项目内已有除本次 assignee 以外的 Busy Agent → 跳过；
//! ③ Owner 运行态不可用（Busy/Resting）→ 跳过。二者的共同假设是「有人在跑 ⇒
//! 项目已在推进 ⇒ 通知冗余」。这个假设不成立：
//!
//! - **② 把「别人忙」当成「别人会推进」**：忙碌来源可能是与本项目无关的对话、
//!   临时追问、自己调工具。忙完之后若没有新的任务状态事件，就没有下一轮触发，
//!   Owner 再也等不到通知 —— 静默饿死，只留下一条 INFO 日志。
//! - **③ 把运行态当成「消息不会被处理」**：队列里同一 `order_key`（= agent_id）
//!   的事件本就串行排队（见 `pkg/aop/queue/in_memory.rs::ack` 的门闩两段互斥），
//!   真正的缺口只是「Agent 忙但不是本队列造成的」（沉淀 Resting / A2A / 直接唤醒）。
//!   为这个缺口丢掉整条通知，代价远大于收益。
//!
//! 结论：**Owner 通知是机械性兜底，不该被任何「可能有人处理过」的猜测抑制。**
//! 重复唤起的代价只是 Owner 多跑一轮编排（正文已明确告知「可能已处理过，请先核对」），
//! 而漏掉一次的代价是项目永久停在原地。所以只保留 ⑤ 这一道**确定性去重**
//! （同项目已有未处理通知，确实不需要第二条），其余一律放行。
//!
//! Owner 忙碌时消息不会被丢弃：落库为 `Pending` 后若消费侧拿不到运行态
//! （`try_set_busy` 失败），事件按退避重投；彻底放弃时消息仍停在 `Pending`，
//! 由 `scheduler::handle_pending_notification_redelivery` 定时重投兜底。

use async_trait::async_trait;
use common::enums::message::{MessageRole, MessageType};
use common::enums::task::TaskStatus;
use common::enums::{AssigneeType, EventTopic};
use common::error::Result;
use std::collections::HashMap;

use crate::models::events::TaskStatusChangedEvent;
use crate::models::task::Task;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::service::domain::message::{SendTaskAssignmentCommand, SendToAgentCommand};

pub struct TaskEventConsumer;

impl TaskEventConsumer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TaskEventConsumer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Consumer for TaskEventConsumer {
    fn name(&self) -> &str {
        "task_event"
    }

    fn subscriptions(&self) -> Vec<Subscription> {
        vec![Subscription::new(EventTopic::TaskStatusChanged)]
    }

    fn consume_mode(&self) -> ConsumeMode {
        ConsumeMode::Async
    }

    async fn on_event(&self, ctx: RequestContext, event: serde_json::Value) -> Result<()> {
        let event: TaskStatusChangedEvent = serde_json::from_value(event).map_err(|e| {
            common::error::Error::internal(format!(
                "failed to deserialize TaskStatusChangedEvent: {}",
                e
            ))
        })?;

        // 仅处理任务完成事件
        if event.new_status != TaskStatus::Completed {
            return Ok(());
        }

        // 项目级通知：必须有 project_id 才能定位 Owner Agent
        let Some(project_id) = &event.project_id else {
            return Ok(());
        };

        // ctx 已由框架从事件 carrier 还原（保留 log_id 等链路标识）

        // 查询项目的 Owner Agent
        let project = crate::service::domain::project::domain()
            .project_manage()
            .get(ctx.clone(), project_id)
            .await?;

        let Some(project) = project else {
            return Ok(());
        };

        // 补齐组织上下文（系统触发链路 ctx 无组织绑定，见 mod.rs helper 说明）
        // 提前到两路投递之前：后继补发与 Owner 通知共用同一份身份与组织上下文
        let ctx =
            crate::consumer::enrich_org_from_project_user(&ctx, &project.po.root_user_id).await;

        // 【身份分层模型】触发器按「被触达事项的归属」选择发送方身份：
        // - 项目归属用户非空 → **以用户身份中继**（from_role=User）：任务是用户发起的，
        //   调度结论用户需要感知；Agent 的 Final 自动回复会回到该用户（消息链自然
        //   路由，无需白名单特判），也不涉及伪造——这是上下文中继。
        // - 无归属用户（如 A2A 项目）→ 万不得已落 System：Final 无人可投递自然丢弃。
        //   二者都不会把回复路由回 Agent 自身 → 无自唤醒循环。
        let (from_id, from_role) = if project.po.root_user_id.is_empty() {
            ("system", MessageRole::System)
        } else {
            (project.po.root_user_id.as_str(), MessageRole::User)
        };

        // ① 后继就绪补发：门闩（wake_gate_policy::dependency_unmet）跳过的
        // 「前置还没跑完」的任务，在这里于前置完成时重新投递。失败只记日志 ——
        // 补偿路径不应把 TaskStatusChanged 事件打回重试，那会连带阻塞 Owner 通知。
        //
        // ⚠️ 无条件执行：这是 DAG 机械推进的责任方，必须排在下方所有 Owner 通知
        // 门禁**之前**；门禁只决定「是否唤醒 Owner」，绝不决定「是否推进 DAG」。
        if let Err(e) =
            dispatch_ready_successors(&ctx, project_id, &event.task_id, from_id, from_role).await
        {
            log_warn!(
                &ctx,
                "task_dispatch",
                task_id = %event.task_id,
                project_id = %project_id,
                error = ?e,
                "后继就绪补发失败（不阻塞 Owner 通知）"
            );
        }

        // ② 项目 Owner Agent 通知（Layer 2 补偿，驱动 Owner 审视全局编排）
        //
        // 只保留 ⑤ 这一道确定性去重：同一项目已有跟进类通知在途 → 确有第二条也无意义。
        // 「项目内有 Agent 在忙」「Owner 不可用」都不再作为跳过理由 —— 那是在用
        // 「可能有人处理过」的猜测替代「确实没处理」，会把兜底通知静默吃掉。
        // 详细论证见文件头「为什么不再用『谁在忙』来决定是否通知」。
        let Some(owner_agent_id) = &project.po.owner_agent_id else {
            return Ok(());
        };

        // ⑤ 跨类型去重：**该项目**已有「跟进类」的未处理通知
        // （TaskDispatchNotification 或 ProjectFollowupNotification）→ 本次不再重复投递，
        // 避免同一项目各来一条。类型清单收敛在 message domain 单点，见 trait 文档。
        //
        // ⚠️ 作用域是项目不是 Owner：Owner 同时带多个项目时，按 Agent 去重会让先挂上
        // Pending 的那个项目一直占着门，其余项目的完成通知被永久饿死。
        let message_domain = crate::service::domain::message::domain();

        if message_domain
            .has_pending_followup_for_project(ctx.clone(), project_id)
            .await?
        {
            log_debug!(
                &ctx,
                "task_dispatch",
                agent_id = %owner_agent_id,
                task_id = %event.task_id,
                "已有 Pending 的跟进类消息，跳过本次通知"
            );
            return Ok(());
        }

        // 构建消息内容（意图指令嵌入消息本体）
        let content = crate::service::domain::message::builder::build_task_dispatch_content(
            &event.task_title,
            event.new_status,
            event.progress,
        );

        // 发送消息（填充 project_id + task_id，MessageConsumer 自动补充上下文）
        let cmd = SendToAgentCommand {
            from_id,
            from_role,
            to_agent_id: owner_agent_id,
            content: &content,
            project_id: Some(project_id),
            task_id: Some(&event.task_id),
            reply_to_id: None,
            external_key: None,
            thread_id: None,
            attachment_ids: None,
            message_type: MessageType::TaskDispatchNotification,
        };

        let log_ctx = ctx.clone();
        if let Err(e) = message_domain.delivery().send_to_agent(ctx, cmd).await {
            log_warn!(
                &log_ctx,
                "task_dispatch",
                task_id = %event.task_id,
                project_id = %project_id,
                agent_id = %owner_agent_id,
                error = ?e,
                "发送任务调度通知失败"
            );
        }

        Ok(())
    }
}

/// 依赖已就绪的后继任务自动补发
///
/// 这是 `wake_gate_policy::dependency_unmet` 的另一半：门闩把「前置还没跑完」的
/// TaskAssignment 跳过了，而 AOP **没有延迟重投**（只有 Retry/Discard），跳过
/// 即 ack 终结 —— 没有这里的补发，后继任务会永久停在 Pending 且不报错。
///
/// 幂等来自三道闸（缺一都可能重复派发）：
/// 1. 后继必须仍是 `Pending` —— 已被认领、已在跑、已完结的任务不重复派发
/// 2. 全部前置都必须是 `Completed`（含本次刚完成的这一个）
/// 3. 承接 Agent 上不能有未投递的同类型指派消息（沿用 Owner 通知的去重口径）
async fn dispatch_ready_successors(
    ctx: &RequestContext,
    project_id: &str,
    finished_task_id: &str,
    from_id: &str,
    from_role: MessageRole,
) -> Result<()> {
    let tasks = crate::service::domain::project::domain()
        .task_manage()
        .list_by_project(ctx.clone(), project_id)
        .await?;

    // id → Task 索引：项目内任务量可控，一次全量拉取胜过为 JSON 依赖列写反向 SQL
    let index: HashMap<&str, &Task> = tasks.iter().map(|t| (t.po.id.as_str(), t)).collect();

    for task in &tasks {
        let deps = task.po.get_dependencies();

        // 只关心「刚完成的这个任务」这条边上的直接后继
        if !deps.iter().any(|d| d == finished_task_id) {
            continue;
        }
        // 已被认领 / 已开始的任务不该被重复指派
        if task.po.status != TaskStatus::Pending {
            continue;
        }
        // 指派给人的任务走人的列表，不走消息
        if task.po.assignee_type != AssigneeType::Agent {
            continue;
        }
        // 全部前置必须真完成。索引查不到的依赖（脏数据 / 跨项目引用）判为未就绪：
        // 宁可不发，也不要在输入不齐时启动下游
        let all_ready = deps.iter().all(|d| {
            index
                .get(d.as_str())
                .is_some_and(|t| t.po.status == TaskStatus::Completed)
        });
        if !all_ready {
            continue;
        }

        let message_domain = crate::service::domain::message::domain();
        if message_domain
            .has_unfinished_message_for_agent(
                ctx.clone(),
                &task.po.assignee_id,
                MessageType::TaskAssignment,
            )
            .await?
        {
            log_debug!(
                &ctx,
                "task_dispatch",
                task_id = %task.po.id,
                agent_id = %task.po.assignee_id,
                "该 Agent 已有未投递的任务指派，跳过补发"
            );
            continue;
        }

        let cmd = SendTaskAssignmentCommand {
            task_id: &task.po.id,
            task_title: &task.po.title,
            task_description: Some(task.po.description.as_str()),
            from_id,
            from_role,
            to_agent_id: &task.po.assignee_id,
            project_id: Some(project_id),
        };

        if let Err(e) = message_domain
            .delivery()
            .send_task_assignment(ctx.clone(), cmd)
            .await
        {
            log_warn!(
                &ctx,
                "task_dispatch",
                task_id = %task.po.id,
                agent_id = %task.po.assignee_id,
                error = ?e,
                "前置依赖已就绪，但任务指派补发失败"
            );
        } else {
            log_info!(
                &ctx,
                "task_dispatch",
                task_id = %task.po.id,
                agent_id = %task.po.assignee_id,
                "前置依赖已全部就绪，补发任务指派"
            );
        }
    }

    Ok(())
}
