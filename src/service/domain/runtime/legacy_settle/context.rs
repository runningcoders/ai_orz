//! `legacy_settle` 的运行依赖容器 + 上下文供给器装配
//!
//! 与 `legacy_awaken::context` 同构：不可序列化的强类型依赖（`domain` /
//! `agent` / `ctx` / 批次状态）放 [`SettleDeps`]，可 JSON 往返的（工具 / 技能 /
//! 工作空间路径 / Prompt）走总线。
//!
//! 与 awaken 的差异只有一处：**批次是 run 级共享槽位**。`prepare` 取一批写入
//! `batch`，`settle` 消费掉它再回环 —— 它不能进总线（含 `MemoryTrace` /
//! `Box<dyn Policy>` 等不可序列化对象）。

use super::pending::{PENDING_MAX_ITEMS, batch_item_cap, settle_budget_tokens};
use crate::models::agent::Agent;
use crate::models::memory::MemoryTrace;
use crate::models::skill::SkillPo;
use crate::pkg::agent_runtime_state::AgentThinkRuntime;
use crate::pkg::policy::Policy;
use crate::pkg::request_context::RequestContext;
use crate::pkg::think_path::{ProviderSet, Stimulus};
use crate::service::domain::runtime::RuntimeDomainImpl;
use crate::service::domain::runtime::types::ThinkingOptions;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 一个待沉淀批次：取数结果 + 本批独立的思考运行时
///
/// 每批单独一个 `MemoryTrace` 与策略组：批次是相互独立的沉淀单元，
/// 某一批溢出/超轮次不应污染下一批的取消信号与轮次记账。
pub(crate) struct PendingBatch {
    pub trace: MemoryTrace,
    /// 本批记忆 id（仅 `Final` 才标记 Settled）
    pub ids: Vec<String>,
    pub think_runtime: Arc<AgentThinkRuntime>,
    pub policy: Arc<dyn Policy>,
}

impl std::fmt::Debug for PendingBatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingBatch")
            .field("trace_id", &self.trace.id)
            .field("items", &self.ids.len())
            .finish()
    }
}

/// 整次 run 的聚合统计（最终写入一条统计事件）
#[derive(Debug, Default, Clone)]
pub(crate) struct SettleAggregate {
    /// 实际调用模型的批次数（= 统计事件 call_count）
    pub attempts: usize,
    /// 终态 `Final` 并成功标记的批次数
    pub batches: usize,
    /// 真正沉淀（标记 Settled）的记忆条数
    pub memories: usize,
    /// 阻断本轮的原因（模型侧 `ContextOverflow` / `MaxRoundsExceeded` / `Cancelled`，
    /// 或存储侧 `MarkSettledFailed`）
    ///
    /// 只要出现过阻断，本轮就此结束：继续跑只会反复选中同一批记忆。
    ///
    /// ⚠️ `MarkSettledFailed` 是最容易被漏掉的一种：`mark_short_term_settled`
    /// 把 update 失败吞成 `Ok(marked)`，返回的是成功而非错误，必须靠
    /// `marked < ids.len()` 主动判出来。
    pub blocked: Option<String>,
}

/// 一次 legacy_settle 的全部运行依赖
pub(crate) struct SettleDeps {
    pub domain: Arc<RuntimeDomainImpl>,
    /// LLM 触碰点（思考）的可替换后端
    ///
    /// 生产注入 `LegacySettleBackend`；集成测试替换为脚本后端，从而在**不打模型**
    /// 的前提下跑完真实流转。
    pub backend: Arc<dyn super::backend::SettleBackend>,
    pub ctx: RequestContext,
    pub agent: Arc<Agent>,
    pub options: Arc<ThinkingOptions>,
    /// 沉淀 Prompt 里引用的历史 trace（独立沉淀通常为空）
    pub trace_ids: Vec<String>,
    /// 单批条数上限提示（实际批量由上下文预算决定）
    pub settle_limit: usize,
    /// 本次 run 单批记忆摘要的 token 预算
    pub budget_tokens: usize,
    /// 当前批次：`prepare` 写入、`settle` 消费
    pub batch: Mutex<Option<PendingBatch>>,
    pub aggregate: Mutex<SettleAggregate>,
    /// 本轮 run 的锚点 trace id（首个批次）；无待沉淀时保持 None
    pub run_anchor: Mutex<Option<String>>,
    pub started_at: Instant,
}

impl std::fmt::Debug for SettleDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettleDeps")
            .field("agent_id", &self.agent.po.id)
            .field("budget_tokens", &self.budget_tokens)
            .field("settle_limit", &self.settle_limit)
            .finish()
    }
}

impl SettleDeps {
    /// 本批候选集上限：调用方提示与硬上限取小
    pub fn item_cap(&self) -> usize {
        batch_item_cap(self.settle_limit)
    }

    /// 沉淀没有入站消息：刺激只作为 run 的构造参数存在
    pub fn stimulus(&self) -> Stimulus {
        Stimulus {
            source: "settle".to_string(),
            text: String::new(),
            message_id: None,
        }
    }
}

/// 一次 legacy_settle 的运行入参
pub(crate) struct SettleInput {
    /// 发起沉淀的那个 domain 实例（**不取全局单例**，测试才能注入独立 DB 池）
    pub domain: Arc<RuntimeDomainImpl>,
    pub ctx: RequestContext,
    /// 已装配 Brain、含 tools + skills 的 Agent
    pub agent: Agent,
    pub options: ThinkingOptions,
    /// 沉淀 Prompt 引用的历史 trace（独立沉淀为空）
    pub trace_ids: Vec<String>,
    /// 单批条数上限提示
    pub settle_limit: usize,
}

/// 装配一次 legacy_settle 的运行依赖（生产后端）
pub(crate) fn build_deps(input: SettleInput) -> Arc<SettleDeps> {
    build_deps_with_backend(Arc::new(super::backend::LegacySettleBackend), input)
}

/// 同上，但 LLM 后端可由调用方指定
///
/// 集成测试用它注入脚本后端：**在不打模型的前提下**跑完真实装配 + 真实收尾。
pub(crate) fn build_deps_with_backend(
    backend: Arc<dyn super::backend::SettleBackend>,
    input: SettleInput,
) -> Arc<SettleDeps> {
    let SettleInput {
        domain,
        ctx,
        agent,
        options,
        trace_ids,
        settle_limit,
    } = input;
    let budget_tokens = settle_budget_tokens(&agent);
    Arc::new(SettleDeps {
        domain,
        backend,
        ctx,
        agent: Arc::new(agent),
        options: Arc::new(options),
        trace_ids,
        settle_limit: settle_limit.min(PENDING_MAX_ITEMS),
        budget_tokens,
        batch: Mutex::new(None),
        aggregate: Mutex::new(SettleAggregate::default()),
        run_anchor: Mutex::new(None),
        started_at: Instant::now(),
    })
}

/// 沉淀场景额外放通的**只读佐证工具**（按工具 id 白名单）
///
/// `ThinkingScene::Settle` 的 tag 白名单只有 `neural` / `memory` / `self_evolution`，
/// 但沉淀经常需要回看原始代码、查外部资料来确认记忆里的结论是否成立。这里按 id
/// 放通三类只读能力：
/// - `fs_read`：回看工作区代码 / 文档
/// - `tavily_search` / `doubao_search` / `http_fetch`：查外部资料
///
/// 为什么不用 tag：`fs` tag 同时覆盖 `fs_write`，加 tag 会连带放通写盘；
/// `shell_exec` 可执行任意命令，同样不能进白名单。沉淀是无人值守的内循环，
/// 只给它「读」的能力，不给「写」和「执行」。
pub(crate) const SETTLE_READONLY_TOOLS: &[&str] =
    &["fs_read", "tavily_search", "doubao_search", "http_fetch"];

/// 装配沉淀链路的上下文供给器集合
///
/// 复用 `legacy_awaken` 的 Provider 实现（路径口径 / 序列化口径只写一遍），
/// 但**不注册 `BusinessProvider`**：沉淀没有消息链、当前消息资源与压缩回写。
pub fn build_providers(deps: Arc<SettleDeps>) -> ProviderSet {
    use crate::models::cortex_types::ToolDescriptor;
    use crate::service::domain::runtime::legacy_awaken::context::{
        SkillsProvider, ToolsProvider, WorkspaceProvider,
    };

    let tools: Vec<ToolDescriptor> = deps
        .agent
        .tools()
        .iter()
        // 场景 tag 面 + 沉淀专用的只读佐证 id 白名单（fs_read / tavily_search / http_fetch）
        .filter(|t| {
            deps.options.scene.is_tool_allowed(&t.po.get_tags())
                || SETTLE_READONLY_TOOLS.contains(&t.po.id.as_str())
        })
        // 沉淀场景不把「写短期记忆」摆给模型：待沉淀队列是本轮的**输入**，
        // 写回去等于自续料（见 think_loop::is_settle_queue_write）。
        // 运行时已有拦截兜底，这里再把它从工具表移除，省掉一次注定被驳回的调用轮次。
        .filter(|t| t.po.id != crate::service::domain::runtime::compaction::SHORT_TERM_WRITE_TOOL)
        .map(ToolDescriptor::from)
        .collect();
    let skills: Vec<SkillPo> = deps
        .agent
        .skills()
        .iter()
        .filter(|s| deps.options.scene.is_tool_allowed(&s.po.parse_tags()))
        .map(|s| s.po.clone())
        .collect();

    let workspace = WorkspaceProvider::new(
        deps.ctx.clone(),
        deps.agent.po.id.clone(),
        deps.options.project.as_ref().map(|p| p.po.id.clone()),
    );

    ProviderSet::new()
        .register(Arc::new(workspace))
        .register(Arc::new(ToolsProvider::new(tools)))
        .register(Arc::new(SkillsProvider::new(skills)))
}
