//! 沉淀链路的思考执行层：**唯一**会触碰 LLM 的地方
//!
//! 与 `legacy_awaken::backend` 同构：抽这个缝是为了让集成测试能在不打模型的前提下
//! 跑完真实流转（注入脚本后端），而不是靠构造特定 LLM 响应去「诱使」链路走到某分支。
//!
//! ⚠️ 与 awaken 的差异：每个批次都有自己的 `think_runtime` / `policy` / `trace_id`，
//! 所以签名收的是 `&PendingBatch`，而不是像 awaken 那样从 `deps` 取 run 级单例。

use super::context::{PendingBatch, SettleDeps};
use crate::models::cortex_types::{ChatMessage, ToolDescriptor};
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::service::domain::runtime::types::{ThinkLoopParams, ThinkLoopResult};
use async_trait::async_trait;
use common::enums::ThinkingScene;
use common::error::Result;
use std::sync::Arc;

/// 一次「沉淀思考」的输入：全部由节点装配而来，后端不自行取数
pub struct SettleThinkInput {
    pub initial_messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDescriptor>,
    pub max_rounds: usize,
}

#[async_trait]
pub trait SettleBackend: Send + Sync + std::fmt::Debug {
    async fn think(
        &self,
        deps: &SettleDeps,
        batch: &PendingBatch,
        input: SettleThinkInput,
    ) -> Result<ThinkLoopResult>;
}

/// 生产后端：转调 `RuntimeDomainImpl::run_think_loop`，行为与老 `sleep_and_settle` 等价
#[derive(Debug)]
pub struct LegacySettleBackend;

#[async_trait]
impl SettleBackend for LegacySettleBackend {
    async fn think(
        &self,
        deps: &SettleDeps,
        batch: &PendingBatch,
        input: SettleThinkInput,
    ) -> Result<ThinkLoopResult> {
        crate::log_info!(
            &deps.ctx,
            "legacy_settle",
            "backend think start, agent_id={}, trace_id={}, max_rounds={}, messages={}, tools={}",
            deps.agent.po.id,
            batch.trace.id,
            input.max_rounds,
            input.initial_messages.len(),
            input.tools.len(),
        );

        // 老链路每轮都要重设：BusyGuard 的 Drop 会清掉 think_runtime
        AgentRuntimeStateManager::global()
            .set_think_runtime(&deps.agent.po.id, Arc::clone(&batch.think_runtime));

        // ⚠️ 沉淀**不调用** `compact_context`（全项目只有 awaken 主循环会调用）：
        // 批内素材必须原样进模型。一旦压缩，既会多出一条短期记忆（把待沉淀队列
        // 变成自续料），又会丢掉沉淀所需的原始细节。上下文溢出时策略直接返回
        // `ContextOverflow`，本批记忆保持 Active 交给下一次沉淀重试。
        let result = deps
            .domain
            .run_think_loop(
                ThinkLoopParams::new(
                    deps.ctx.clone(),
                    &deps.agent,
                    ThinkingScene::Settle,
                    &batch.trace.id,
                    input.initial_messages,
                    &input.tools,
                )
                .with_rounds(input.max_rounds, 0)
                .with_monitoring(&batch.think_runtime, &batch.policy),
            )
            .await;

        match &result {
            Ok(_) => crate::log_info!(
                &deps.ctx,
                "legacy_settle",
                "backend think done, agent_id={}, trace_id={}",
                deps.agent.po.id,
                batch.trace.id
            ),
            Err(e) => crate::log_error!(
                &deps.ctx,
                "legacy_settle",
                "backend think failed, agent_id={}, trace_id={}, error={}",
                deps.agent.po.id,
                batch.trace.id,
                e
            ),
        }
        result
    }
}
