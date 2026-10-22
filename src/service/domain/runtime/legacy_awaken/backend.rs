//! 思考执行层：`Deliberate` / `Compact` / `Consolidate` 里**会触碰 LLM** 的那部分
//!
//! 抽这个缝有两个目的：
//!
//! 1. **集成测试能在不打模型的前提下跑完真实流转。** 红线：集成测试报
//!    `connect error ... api.openai.com` 默认当真 bug。所以新链路的集成用例
//!    必须从这里注入后端，而不是裸调 `domain` —— 否则「测试通过」的含义
//!    取决于本机能不能连到 OpenAI。
//! 2. **它是后续把 480 行 `run_think_loop` 下沉成图内节点的替换点。**
//!    到时候只是多一个 `ThinkBackend` 实现，节点代码一行不用动。
//!
//! ⚠️ 缝的开法是「按语义切」，不是「把函数原样搬过来」：
//! `think` = 跑一轮完整思考（工具循环 + 退出策略裁决），
//! `compact` = 把对话压成短期记忆。这两个是 `legacy_awaken` 里唯二的
//! LLM 触碰点，其余 Provision（装配）与 Consolidate（收尾）都是纯 I/O，
//! 保持真实实现才有测试价值。

use super::context::AwakenDeps;
use crate::models::cortex_types::{ChatMessage, ToolDescriptor};
use crate::pkg::agent_runtime_state::AgentRuntimeStateManager;
use crate::service::domain::runtime::compaction::CompactOutcome;
use crate::service::domain::runtime::types::{ThinkLoopParams, ThinkLoopResult};
use async_trait::async_trait;
use common::enums::ThinkingScene;
use common::error::Result;

/// 一次「思考」的输入：全部由总线 + `AwakenDeps` 装配而来，后端不自行取数
pub struct ThinkInput {
    pub initial_messages: Vec<ChatMessage>,
    pub tools: Vec<ToolDescriptor>,
    pub max_rounds: usize,
    pub total_rounds: usize,
}

/// 一次「压缩」的输入
pub struct CompactInput {
    pub messages: Vec<ChatMessage>,
    pub trace_ids: Vec<String>,
    /// `true` = 压缩产物剥掉摘要标记后作为给用户的回复（轮次耗尽退出）
    pub with_user_reply: bool,
}

#[async_trait]
pub trait ThinkBackend: Send + Sync + std::fmt::Debug {
    async fn think(&self, deps: &AwakenDeps, input: ThinkInput) -> Result<ThinkLoopResult>;
    async fn compact(&self, deps: &AwakenDeps, input: CompactInput) -> Result<CompactOutcome>;
}

/// 生产后端：转调 `RuntimeDomainImpl` 的老函数，行为与 `awaken` 完全等价
#[derive(Debug)]
pub struct LegacyBackend;

#[async_trait]
impl ThinkBackend for LegacyBackend {
    async fn think(&self, deps: &AwakenDeps, input: ThinkInput) -> Result<ThinkLoopResult> {
        // 老链路每轮都要重设：BusyGuard 的 Drop 会清掉 think_runtime
        AgentRuntimeStateManager::global()
            .set_think_runtime(&deps.agent.po.id, deps.think_runtime.clone());

        deps.domain
            .run_think_loop(
                ThinkLoopParams::new(
                    deps.ctx.clone(),
                    &deps.agent,
                    ThinkingScene::Awaken,
                    &deps.trace_id,
                    input.initial_messages,
                    &input.tools,
                )
                .with_rounds(input.max_rounds, input.total_rounds)
                .with_monitoring(&deps.think_runtime, deps.policy.as_ref())
                .with_progress(&deps.progress),
            )
            .await
    }

    async fn compact(&self, deps: &AwakenDeps, input: CompactInput) -> Result<CompactOutcome> {
        deps.domain
            .compact_context(
                deps.ctx.clone(),
                &input.messages,
                &deps.agent,
                &deps.trace_id,
                &input.trace_ids,
                input.with_user_reply,
            )
            .await
    }
}
