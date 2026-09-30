//! Message Recall - 消息撤回编排
//!
//! 「撤回」是**状态语义**（逻辑作废，落 `messages.status = Recalled`），与
//! 「取消」这条**信号语义**（`AgentThinkRuntime::cancel_flag`）正交：
//! 撤一条**未处理**的消息只需改状态；撤一条**正在处理**的消息 = 先发取消信号、
//! 再改状态（本条实现的两条分支）。
//!
//! 为什么撤回之后还要「取消」而不是只改状态：撤回**无法打断已经在跑的思考循环**
//! （它已经把消息读进上下文了），只能阻止**后续**消费。已在飞的消息若不取消，
//! 它会跑完全程、产出回复 —— 与「撤回」的语义正好相反。

use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::service::domain::message::MessageDomainImpl;
use common::enums::{MessageStatus, UserRole};
use common::error::{Error, Result, bail_err};

/// 撤回结果（Domain 层语义，由 Adapter 映射为 `RecallMessageResponse`）
///
/// 幂等 / 不可撤回这两种「未产生效果」的结局用**枚举变体**表达，而不是错误码：
/// 撤回一条已处理或已撤回的消息不是失败，把它变成 `Err` 会让工具调用整体报错、
/// 模型误以为操作失败而反复重试。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecallOutcome {
    /// 成功撤回未处理（`Pending`）的消息
    Recalled,
    /// 成功撤回正在处理（in-flight）的消息，并已向持有它的 Agent 发出取消信号
    ///
    /// `cancelled = false` 表示运行时状态在本次操作的两步之间被清理（思考恰好结束），
    /// 状态标记仍然生效。
    RecalledInFlight { agent_id: String, cancelled: bool },
    /// 此前已撤回（幂等，不算错误）
    AlreadyRecalled,
    /// 消息已处理 / 已失败，撤回无意义（no-op，不算错误）
    NotRecallable { status: MessageStatus },
}

impl MessageDomainImpl {
    /// 撤回一条消息：权限 gate → 幂等 / 状态 gate → 在飞转取消 → 写 `Recalled`
    ///
    /// 入口唯一（REST handler 与神经工具 `recall_message` 共用同一个 handler，
    /// 最终都落到本方法），权限判据与状态判据只在这里写一遍。
    pub(crate) async fn recall_message_impl(
        &self,
        ctx: RequestContext,
        message_id: &str,
        reason: Option<&str>,
    ) -> Result<RecallOutcome> {
        // ① 读消息：必须走**不过滤撤回态**的读路径 —— `find_by_id` 带 `status != 0`
        //    软删除过滤，撤回态对它不可见，否则重复撤回会退化成 404（语义应为幂等成功）。
        let message = self
            .message_dal
            .find_by_id_with_recalled(ctx.clone(), message_id)
            .await?
            .ok_or_else(|| Error::not_found(format!("消息 {} 不存在", message_id)))?;

        // ② 幂等：已撤回直接返回，不重复写库、不重复发取消信号
        if message.po.status == MessageStatus::Recalled {
            return Ok(RecallOutcome::AlreadyRecalled);
        }

        // ③ 权限 gate（发送方 / 归属用户 / Owner Agent / SuperAdmin）
        self.ensure_can_recall(&ctx, &message).await?;

        // ④ 在飞判定：以**运行时状态**为准。
        //    `MessageStatus::Processing` 当前没有任何写入路径（生产只写 Pending / Processed），
        //    在飞消息在库里仍是 `Pending` —— 只看 DB 会把「正在跑」误判成「排队中」，
        //    于是只改状态、不发取消信号，Agent 会继续跑完并回复。
        let busy_agent_id =
            AgentRuntimeStateManager::global().find_busy_agent_by_message(message_id);

        if let Some(agent_id) = busy_agent_id {
            // 红线：撤回者不得是「正在处理这条消息的那个 Agent 自己」——
            // 那是「中断自己的思考」，语义属于 `cancel_thinking`；用撤回工具自断会
            // 把自己变成半截状态（消息已撤回、本轮仍在跑）。
            if ctx.agent_id().map(|s| s.as_str()) == Some(agent_id.as_str()) {
                bail_err!(
                    InvalidRequest,
                    "recall_message 不能撤回\"自己正在处理\"的消息（{}）：那是中断自己的思考，\
                     请改用 cancel_thinking 工具",
                    message_id
                );
            }

            let cancelled = AgentRuntimeStateManager::global().cancel_thinking(&agent_id);
            self.message_dal
                .update_status(ctx.clone(), message_id, MessageStatus::Recalled)
                .await?;

            log_info!(
                &ctx,
                "recall_message",
                message_id = %message_id,
                agent_id = %agent_id,
                cancelled = cancelled,
                reason = reason.unwrap_or(""),
                "撤回在飞消息：已发取消信号并标记 Recalled（取消在轮次边界生效）"
            );

            return Ok(RecallOutcome::RecalledInFlight {
                agent_id,
                cancelled,
            });
        }

        // ⑤ 排队中：只有 `Pending` 可撤回；终态消息幂等返回 not_recallable
        match message.po.status {
            MessageStatus::Pending | MessageStatus::Processing => {
                self.message_dal
                    .update_status(ctx.clone(), message_id, MessageStatus::Recalled)
                    .await?;

                log_info!(
                    &ctx,
                    "recall_message",
                    message_id = %message_id,
                    status = ?message.po.status,
                    reason = reason.unwrap_or(""),
                    "撤回未处理消息：消费者出队时将被守卫跳过"
                );

                Ok(RecallOutcome::Recalled)
            }
            status @ (MessageStatus::Processed | MessageStatus::Failed) => {
                log_info!(
                    &ctx,
                    "recall_message",
                    message_id = %message_id,
                    status = ?status,
                    "消息已处理/已失败，撤回无意义（幂等 no-op）"
                );
                Ok(RecallOutcome::NotRecallable { status })
            }
            // Recalled 已在 ② 短路，此处不可达；`MessageStatus` 是完整的，为防新增
            // 变体时静默落入错误分支，这里显式兜底为「不可撤回」而非 panic。
            MessageStatus::Recalled => Ok(RecallOutcome::AlreadyRecalled),
        }
    }

    /// 撤回权限 gate：满足任一判据即可（单点实现，REST 与神经工具共用）
    ///
    /// | 判据 | 取值 |
    /// |------|------|
    /// | ① 发送方 | `ctx.message_sender_id() == message.from_id` |
    /// | ② 归属用户 | 消息所属项目 `project.root_user_id == ctx.uid()` |
    /// | ③ Owner Agent | 消息所属项目 `project.owner_agent_id == ctx.agent_id()` |
    /// | ④ SuperAdmin | `ctx.user_role() == Some(UserRole::SuperAdmin)` |
    /// | ⑤ 收件方 | `ctx.agent_id() == message.to_id`（拒收发给自己的未处理消息） |
    ///
    /// ⚠️ 判据 ① 必须用 `message_sender_id()`（而非 `uid()`）：Agent 唤醒态 / AOP
    /// 沉淀态 ctx 的 `caller_type` 是 `System`、**只有 `agent_id`、天生无 `user_id`**，
    /// 只认用户身份的 gate 会让整族工具在沉淀期恒被拒。
    ///
    /// ⚠️ 判据 ②③④ 在**工具侧**（Agent 唤醒态）会被链路继承的身份意外放宽：
    /// `ContextCarrier` 携带并沿链路还原 `user_id` / `user_role`，而
    /// `consumer/message.rs::rebuild_context` 只覆写 `agent_id`（= `to_id`），
    /// 不覆写 `user_id` / `user_role` ⇒ 工具侧这两个字段的语义是「这条链路最初由谁
    /// 发起」。链路上游为项目归属用户 → 下游 Agent 继承该项目全量撤回权；上游为
    /// SuperAdmin → 下游 Agent 同样带 SuperAdmin 角色（判据 ④ 命中）。该形态在
    /// `hr/skill.rs` 的「Admin Bypass」有既有先例，属系统级议题，不在本方法内单独收紧。
    async fn ensure_can_recall(&self, ctx: &RequestContext, message: &Message) -> Result<()> {
        // ① 发送方本人（用户 / Agent 皆可）
        let sender_id = ctx.message_sender_id();
        if !sender_id.is_empty() && sender_id == message.po.from_id {
            return Ok(());
        }

        // ⑤ 收件方本人：有权「拒收」**发给自己的**消息。
        //
        //    为什么需要：消息按接收方串行排队，而队列只认先来后到。需求变更后若自己
        //    队列里堆着一批已作废的指令，**收件方原本没有任何手段把它们丢掉**——只能
        //    等发送方 / Owner 发现并主动撤回。收件方是被唤醒的那一方，对「这条还该不
        //    该做」掌握最直接的信息，因此把拒收权交给它。
        //
        //    ⚠️ 这里**不校验 status**：下游状态 gate 已把 `Processed` / `Failed` 归一
        //    为 `NotRecallable`（no-op），故本判据实际只对未处理消息生效。也不担心它
        //    抢在在飞判定之前放行——若这条消息正是收件方自己在处理的，步骤 ④ 会以其
        //    持有者身份命中「不得撤回自己正在处理的消息」并引导改用 `cancel_thinking`。
        if let Some(agent_id) = ctx.agent_id()
            && message.po.to_id == *agent_id
        {
            return Ok(());
        }

        // ④ SuperAdmin（治理面兜底：用户发现 Agent 之间跑偏时可直接介入）
        if ctx.user_role() == Some(UserRole::SuperAdmin as i32) {
            return Ok(());
        }

        // ② / ③ 需要项目上下文；消息无 project_id（如默认会话）时这两条判据不成立
        if let Some(project_id) = message.po.project_id.as_deref()
            && let Some(project) = self.project_dal.find_by_id(ctx.clone(), project_id).await?
        {
            // ② 归属用户
            if let Some(user_id) = ctx.user_id().filter(|s| !s.is_empty())
                && project.po.root_user_id == *user_id
            {
                return Ok(());
            }
            // ③ Owner Agent
            if let Some(agent_id) = ctx.agent_id()
                && project.po.owner_agent_id.as_deref() == Some(agent_id.as_str())
            {
                return Ok(());
            }
        }

        log_warn!(
            ctx,
            "recall_message",
            message_id = %message.po.id,
            from_role = ?message.po.from_role,
            caller_type = ?ctx.caller_type(),
            "撤回被权限 gate 拒绝"
        );

        Err(Error::forbidden(
            "无权撤回该消息：只有消息发送方、消息收件方（仅限尚未处理的消息）、\
             消息所属项目的归属用户 / Owner Agent，或超级管理员（SuperAdmin）可以撤回",
        ))
    }
}
