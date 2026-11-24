//! Task event consumer (AOP async)
//!
//! 订阅 `task.status.changed` 事件，对 `TaskStatus::Completed` 的状态变更只做一件事：
//! **后继就绪补发**：给依赖本任务且前置已全量就绪的后继任务补发 `TaskAssignment`
//! （DAG 依赖是项目内概念，游离任务跳过）。
//!
//! ⚠️ 【2026-11-15 职责收窄】完成交付与知会已迁至 `TaskCompletionConsumer`（Sync）：
//! 交付消息必须在 `publish` 返回时落库，不能押在 Async 内存队列上；两条链路合并
//! 会重复投递。下方「Owner 通知」相关历史注释描述的是迁移前形态，其门禁与去重
//! 已随知会一并迁出，本消费者不再发任何完成通知。
//!
//! 设计要点：
//! - `ConsumeMode::Async`：异步消费，发送方（DAL 层 update_status）不阻塞
//! - 仅处理 `TaskStatus::Completed` 事件（其他状态变更暂不通知）
//! - **交付身份优先取事件快照**：`root_user_id` / `initiator_agent_id` /
//!   `trigger_message_id` 由 `publish_status_changed` 在发布时写入；仅旧格式事件
//!   缺快照时才回退反查项目行（升级兼容，不是主路径）
//! - **有无 project_id 只决定多一层项目 owner，不是处理前提**：旧实现在这里
//!   直接 return，游离任务完成后整条链路无人推进
//! - **① 后继补发无条件执行**：`dispatch_ready_successors` 是 DAG 的机械推进责任方，放在所有门禁**之前**，绝不因 Owner 通知被跳过而停摆
//! - 【已迁出】Owner 通知门禁与调度文案随完成知会一并迁至 `TaskCompletionConsumer`；
//!   本消费者不再有任何通知门禁，后继补发无条件执行
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
use common::enums::message::MessageType;
use common::enums::task::TaskStatus;
use common::enums::{AssigneeType, EventTopic};
use common::error::Result;
use std::collections::HashMap;

use crate::models::events::TaskStatusChangedEvent;
use crate::models::task::Task;
use crate::pkg::RequestContext;
use crate::pkg::aop::{ConsumeMode, Consumer, Subscription};
use crate::service::domain::message::SendTaskAssignmentCommand;

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

        // 【分层交付】有无 project_id 只决定「多一层项目 owner」，不是处理前提。
        // 旧实现在这里直接 return，游离任务完成后整条链路无人推进。
        let project_id = event.project_id.as_deref();

        // 项目行只查一次，同时承担两个用途：owner（项目层知会对象）+ 旧事件
        // 缺快照时的归属用户回退。查询失败不当作「无项目」—— 那样会把知会
        // 静默吃掉，只打日志并继续用事件快照。
        let project = match project_id {
            Some(pid) => match crate::service::domain::project::domain()
                .project_manage()
                .get(ctx.clone(), pid)
                .await
            {
                Ok(p) => p,
                Err(e) => {
                    log_warn!(
                        &ctx,
                        "task_dispatch",
                        project_id = %pid,
                        error = ?e,
                        "查询项目失败，完成知会改用事件快照身份"
                    );
                    None
                }
            },
            None => None,
        };
        // ⚠️ 归属用户优先取**发布时快照**：Async 队列里动作落库与消费之间有
        // 时间窗，反查可能拿到被改过的行，把结果送给错的人。
        // 快照为空只可能是旧格式事件（滚动升级时内存队列残留 / 手工构造），
        // 这时退回项目行补齐，避免误落 System 身份导致 Final 无人可投递。
        let root_user_id = if event.root_user_id.is_empty() {
            project
                .as_ref()
                .map(|p| p.po.root_user_id.as_str())
                .unwrap_or_default()
        } else {
            event.root_user_id.as_str()
        };

        let ctx = crate::consumer::enrich_org_from_project_user(&ctx, root_user_id).await;

        // 【身份三分解】派发者 / 接收者 / 所属用户各就各位，不再做「用户身份中继」：
        // - 派发者按**每个被触达任务**的 `initiator_agent_id` 推导（见
        //   `consumer::task_sender_identity`），管理面直建才退回用户；
        // - 所属用户 `root_user_id` 独立落在消息行，供被唤醒 Agent 取工具凭据；
        // - 因此执行者 Agent 的 Final 天然回到它自己的发起者（Peer），不需要
        //   `TaskHandoffToInitiator` 这条改道补丁（已删除）。

        // ① 后继就绪补发：门闩（wake_gate_policy::dependency_unmet）跳过的
        // 「前置还没跑完」的任务，在这里于前置完成时重新投递。失败只记日志 ——
        // 补偿路径不应把 TaskStatusChanged 事件打回重试，那会连带阻塞 Owner 通知。
        //
        // ⚠️ 无条件执行：这是 DAG 机械推进的责任方，必须排在下方所有 Owner 通知
        // 门禁**之前**；门禁只决定「是否唤醒 Owner」，绝不决定「是否推进 DAG」。
        if let Some(project_id) = project_id
            && let Err(e) =
                dispatch_ready_successors(&ctx, project_id, &event.task_id, root_user_id).await
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

        // 【2026-11-15】完成交付与 Agent 知会已全部迁至 TaskCompletionConsumer(Sync)。
        // 这里不再发任何完成通知：两条链路并存会造成同一完成事实两次出站，且
        // Async 内存队列承载交付有丢件窗口。DAG 补发保持异步即可（它不是交付）。
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
    fallback_root_user_id: &str,
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

        // 派发者 = 这条后继任务自己的发起者（层层摘要的上一跳）；
        // 归属用户优先用任务行快照，缺失才回落事件里的项目归属。
        let root_user_id = if task.po.root_user_id.is_empty() {
            fallback_root_user_id
        } else {
            task.po.root_user_id.as_str()
        };
        let (from_id, from_role) =
            crate::consumer::task_sender_identity(&task.po.initiator_agent_id, root_user_id);

        let cmd = SendTaskAssignmentCommand {
            task_id: &task.po.id,
            task_title: &task.po.title,
            task_description: Some(task.po.description.as_str()),
            from_id,
            from_role,
            to_agent_id: &task.po.assignee_id,
            root_user_id,
            project_id: Some(project_id),
            // DAG 后继补发是事件驱动、拿不到本轮入口消息，但任务行已把
            // 「这条任务诞生于哪条消息」持久化下来，直接沿它挂回同一条链。
            reply_to_id: (!task.po.trigger_message_id.is_empty())
                .then_some(task.po.trigger_message_id.as_str()),
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
