//! Runtime Cancel Thinking - 取消思考编排
//!
//! 「取消思考」是**信号语义**：向目标 Agent 的思考运行时翻 `cancel_flag`，使其在
//! **当前轮次边界**退出（不抢占正在进行的 LLM 调用）。它与消息域的「撤回」
//! （**状态语义**，`messages.status = Recalled`）正交：撤回决定「这条消息还要不要
//! 处理」，本工具决定「现在这一轮停不停」。撤回一条**在飞**消息时，内部就是用本原语。
//!
//! 权限取向：本工具**刻意只做存在性校验、不设关系门**，与既有跨 Agent 工具
//! （`send_message_to_agent` / `send_task_assignment_message`）保持一致——Agent 工具面
//! 本就不设授权层，`handlers/hr/agent/*` 全族没有任何 `forbidden` 判据。若只给取消
//! 单独加门，会造出「能给对方发消息、能派任务，却停不了对方」的能力错位；而取消是
//! **无持久化副作用**的信号，最坏后果只是让对方提前结束本轮（队列里的下一条消息
//! 照常唤醒它）。
//!
//! ⚠️ 无法做租户门：`agents` 表没有 `organization_id` 列（`migrations/20260420000000_initial.sql`），
//! 全库也没有 Agent↔组织关联表 ⇒ Agent 维度上**不存在可依赖的组织隔离**。
//!
//! 为什么必须补存在性校验：此前 handler 直接投递信号，`cancel_thinking("<不存在的ID>")`
//! 与「Agent 当前空闲」返回**完全一样**（`success=false`），调用方既无法区分「没有这个
//! Agent」和「Agent 没在思考」，也会把拼错 ID 的错误静默吞掉。校验后语义二分：
//! 目标不存在 → `NotFound`（404）；目标存在但空闲 → `NotThinking`（正常结果）。

use crate::pkg::RequestContext;
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::service::domain::runtime::RuntimeDomainImpl;
use common::error::{Result, err};

/// 取消思考的结果（Domain 层语义，由 Adapter 映射为 `CancelThinkingResponse`）
///
/// 「目标存在但未在思考」用枚举变体表达而**不是错误码**：重复取消 / 取消一个刚好
/// 结束思考的 Agent 都不是失败，把它变成 `Err` 会让工具调用整体报错、模型误以为
/// 操作失败而反复重试。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelOutcome {
    /// 目标 Agent 正在思考，取消信号已投递（在当前轮次边界生效）
    Cancelled,
    /// 目标 Agent 存在，但当前未在思考 → 幂等 no-op，**不是错误**
    NotThinking,
}

impl RuntimeDomainImpl {
    /// 取消 Agent 思考：存在性校验 → 投递取消信号
    ///
    /// 入口唯一（REST handler 与神经工具 `cancel_thinking` 共用同一个 handler，最终都
    /// 落到本方法），校验与信号投递只在这里写一遍。
    pub(crate) async fn cancel_thinking_impl(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<CancelOutcome> {
        // ① 存在性校验：区分「没有这个 Agent」与「Agent 当前空闲」。
        //    Agent 之间按 ID 点名停对方思考时，拿到的 ID 可能拼错或指向已删除的
        //    Agent；静默返回「未在思考」会让模型以为操作成功，不再纠正。
        if self
            .agent_dal
            .find_by_id(ctx.clone(), agent_id)
            .await?
            .is_none()
        {
            // 用 `NotFound`（而非 `Error::not_found()` 的 `ResourceNotFound`）与
            // `hr/agent.rs` 全族的「Agent {} 不存在」保持一致 —— 同一实体、同一句文案，
            // 调用方/模型不该因为走的是哪个域而拿到不同的错误码。
            return Err(err!(NotFound, "Agent {} 不存在", agent_id));
        }

        // ② 投递取消信号：内存标志，**轮次边界**生效 —— 不抢占正在进行的 LLM 调用，
        //    因此不要向用户承诺「已经立刻停住了」。
        let cancelled = AgentRuntimeStateManager::global().cancel_thinking(agent_id);

        log_info!(
            &ctx,
            "cancel_thinking",
            agent_id = %agent_id,
            cancelled = cancelled,
            "取消思考信号已投递（在轮次边界生效）"
        );

        Ok(if cancelled {
            CancelOutcome::Cancelled
        } else {
            CancelOutcome::NotThinking
        })
    }
}
