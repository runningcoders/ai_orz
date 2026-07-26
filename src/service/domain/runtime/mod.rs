//! Runtime Domain 模块
//!
//! 【定位】运行时执行层 - 只负责动态执行逻辑，不负责任何静态配置管理
//!
//! 包含子模块：
//! - memory: 运行时记忆管理（读取历史、写入思考 Trace）
//! - awakening: Agent 唤醒主流程
//! - tool_execution: 工具实际执行（单次/批量）
//! - context_assembly: Prompt 上下文组装（纯函数，无 async）

use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;

use crate::models::agent::Agent;
use crate::models::memory::{Memory, MemoryCreateParams, MemoryTrace};
use crate::models::message::Message;
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_tracing::logger::ToolCallLogger;
use crate::service::dal::agent::AgentDal;
use crate::service::dal::agent_a2a::A2aAgentDal;
use crate::service::dal::agent_codex::CodexAgentDal;
use crate::service::dal::brain::BrainDal;
use crate::service::dal::mcp_tool::McpToolDal;
use crate::service::dal::memory::TraversalStrategy;
use crate::service::dal::tool::ToolDal;
use crate::service::dao::memory::{MemoryQuery, MemorySearch};
use common::enums::AgentRuntimeState;
use common::error::Result;

// ==================== traits 定义 ===================

/// Runtime Domain 总 trait
///
/// 聚合运行时领域所有子功能 trait
pub trait RuntimeDomain: Send + Sync + Debug {
    /// 记忆管理能力
    fn memory(&self) -> &dyn RuntimeMemory;
    /// 唤醒能力
    fn awakening(&self) -> &dyn RuntimeAwakening;
    /// 工具执行能力
    fn tool_execution(&self) -> &dyn RuntimeToolExecution;

    /// 查询 Agent 运行时状态
    fn agent_runtime_state(&self, agent_id: &str) -> AgentRuntimeState;

    /// Agent 是否处于不可用状态（忙碌或休息）
    fn is_agent_unavailable(&self, agent_id: &str) -> bool;

    /// 让 Agent 进入休息状态并执行记忆沉淀
    ///
    /// 流程：
    /// 1. 将 Agent 状态设置为休息中
    /// 2. 将未沉淀的短期记忆总结并沉淀为长期知识
    /// 3. 将 Agent 状态恢复为空闲
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - agent_id: Agent ID
    /// - settle_limit: 每次处理的短期记忆数量上限
    ///
    /// # 返回
    /// - 成功返回创建的知识节点数量
    fn rest_and_settle(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        settle_limit: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize>> + Send + '_>>;
}

/// 记忆管理 trait
///
/// 定义记忆读取、思考 Trace 写入等接口
#[async_trait]
pub trait RuntimeMemory: Send + Sync {
    // === 内部使用方法（保持不变） ===

    /// 读取最近短期记忆
    async fn get_recent_context(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<Memory>>;

    /// 写入思考 Trace
    ///
    /// 直接接收 MemoryTrace 结构体，内部可做统一信息补充
    async fn write_thinking_trace(&self, ctx: RequestContext, trace: MemoryTrace)
    -> Result<Memory>;

    // === 公开方法（供 Handler/神经工具调用） ===

    /// 混合搜索记忆（关键词 + 向量语义）
    async fn search(&self, ctx: RequestContext, search: MemorySearch) -> Result<Vec<Memory>>;

    /// 通用关系型查询
    async fn query(&self, ctx: RequestContext, query: MemoryQuery) -> Result<Vec<Memory>>;

    /// 创建记忆
    async fn create(&self, ctx: RequestContext, params: MemoryCreateParams) -> Result<Vec<Memory>>;

    /// 更新记忆
    async fn update(&self, ctx: RequestContext, memory: Memory) -> Result<Memory>;

    /// 删除记忆
    async fn delete(&self, ctx: RequestContext, memory: Memory) -> Result<()>;

    /// 知识图谱遍历
    async fn traverse_graph(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        strategy: TraversalStrategy,
    ) -> Result<Vec<Memory>>;

    /// 将未沉淀的短期记忆总结并沉淀为长期知识
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - agent_id: Agent ID
    /// - limit: 每次处理的短期记忆数量上限
    ///
    /// # 返回
    /// - 成功返回创建的知识节点列表
    async fn settle(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<Memory>>;
}

/// 唤醒能力 trait
///
/// 定义 Agent 唤醒相关的核心业务接口
#[async_trait]
pub trait RuntimeAwakening: Send + Sync {
    /// 装配 Agent 的 Brain
    ///
    /// 对于 Local agent：加载 tools，通过 BrainDal.wake_brain 构造带 Cortex 的 Brain
    /// 对于 External agent（Cli/Remote）：构造不带 Cortex 的虚拟 Brain
    ///
    /// 调用时机：consumer 处理消息前，如果 agent.brain 为 None 则调用此方法装配。
    /// 幂等：如果 agent.brain 已存在则直接返回。
    ///
    /// 返回 enriched ctx：wake_brain 内部查询 ModelProvider 后会补充
    /// `model_provider_id` / `model_name`，调用方应使用返回的 ctx 替换原 ctx，
    /// 保证后续 awaken/think 链路的 ctx 字段完整（避免监控日志缺 model_name）。
    async fn wake_agent_brain(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
    ) -> Result<RequestContext>;

    /// 唤醒 Agent 并执行一次思考
    ///
    /// 【分层原则】
    /// - 外部传入：Agent、Message（由上层 Domain 加载好传入）
    /// - 内部获取：Memory、工具、技能（Runtime Domain 内部直接访问）
    ///
    /// 【流程】
    /// 1. 读取最近短期记忆
    /// 2. 收集关联的 Trace ID 列表
    /// 3. 拼装 Prompt
    /// 4. 记录输入 Trace
    /// 5. 调用模型推理
    /// 6. 记录输出 Trace
    /// 7. 返回结果
    async fn awaken(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        message: &Message,
    ) -> Result<AwakeningResult>;
}

/// 工具执行 trait
///
/// 定义工具实际执行的接口
#[async_trait]
pub trait RuntimeToolExecution: Send + Sync {
    /// Execute one tool by standard Tool ID.
    ///
    /// Runtime Domain owns protocol routing:
    /// - MCP tools go through `McpToolDal`;
    /// - Builtin/HTTP tools go through generic `ToolDal`.
    async fn call_tool_by_id(
        &self,
        ctx: RequestContext,
        tool_id: String,
        args: serde_json::Value,
    ) -> std::result::Result<crate::models::tool::ToolExecutionResult, common::error::Error>;

    /// Execute one already-loaded tool.
    ///
    /// Callers that already have a `Tool` should use this method to avoid a
    /// second Tool lookup. Protocol routing is still owned by Runtime Domain.
    async fn call_tool(
        &self,
        ctx: RequestContext,
        tool: &crate::models::tool::Tool,
        args: serde_json::Value,
    ) -> std::result::Result<crate::models::tool::ToolExecutionResult, common::error::Error>;

    /// Execute a message-mode Manual tool call for one Agent.
    ///
    /// This entry point owns runtime authorization for `ToolCallRequest`:
    /// the tool must be bound to the Agent and must use `ControlMode::Manual`.
    async fn call_manual_tool_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: String,
        tool_id: String,
        args: serde_json::Value,
    ) -> std::result::Result<crate::models::tool::ToolExecutionResult, common::error::Error>;

    /// Query tool call trace entries with access scope enforced by Runtime Domain.
    async fn query_tool_call_entries(
        &self,
        ctx: RequestContext,
        query: crate::pkg::tool_tracing::logger::ToolCallQuery,
    ) -> Result<Vec<crate::pkg::tool_tracing::entry::ToolCallEntry>>;

    /// Get one tool call trace entry by call ID with access scope enforced by Runtime Domain.
    async fn get_tool_call_entry_by_id(
        &self,
        ctx: RequestContext,
        query: crate::pkg::tool_tracing::logger::ToolCallQuery,
    ) -> Result<Option<crate::pkg::tool_tracing::entry::ToolCallEntry>>;
}

// ==================== 子模块  ====================
// 注意：子模块必须在 trait 定义之后导入，这样子模块才能看到这些 trait

mod awakening;
mod busy_guard;
mod memory;
mod tool_call_query;
mod tool_execution;

#[cfg(test)]
mod tool_execution_test;

// DefaultPromptBuilder 已迁移到 dal/agent.rs，由 AgentDal.prompt_builder() 提供
pub use crate::service::dal::agent::{DefaultPromptBuilder, build_conversation_prompt};
pub(crate) use tool_call_query::status_from_dto;

// ==================== 实现 ====================

/// Runtime Domain 实现
///
/// 聚合所有运行时子功能实现
struct RuntimeDomainImpl {
    brain_dal: Arc<dyn BrainDal>,
    tool_dal: Arc<dyn ToolDal>,
    mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
    agent_dal: Arc<dyn AgentDal>,
    /// CLI 类型外部 Agent Dal（提供 CliPromptBuilder）
    codex_agent_dal: Arc<CodexAgentDal>,
    /// A2A 远程类型外部 Agent Dal（提供 RemotePromptBuilder）
    a2a_agent_dal: Arc<A2aAgentDal>,
    tool_call_logger: Arc<ToolCallLogger>,
}

impl std::fmt::Debug for RuntimeDomainImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeDomainImpl")
            .field("brain_dal", &"<BrainDal>")
            .field("tool_dal", &"<ToolDal>")
            .field("mcp_tool_dal", &"<McpToolDal>")
            .field("agent_dal", &"<AgentDal>")
            .field("codex_agent_dal", &"<CodexAgentDal>")
            .field("a2a_agent_dal", &"<A2aAgentDal>")
            .field("tool_call_logger", &"<ToolCallLogger>")
            .finish()
    }
}

impl Clone for RuntimeDomainImpl {
    fn clone(&self) -> Self {
        Self {
            brain_dal: self.brain_dal.clone(),
            tool_dal: self.tool_dal.clone(),
            mcp_tool_dal: self.mcp_tool_dal.clone(),
            agent_dal: self.agent_dal.clone(),
            codex_agent_dal: self.codex_agent_dal.clone(),
            a2a_agent_dal: self.a2a_agent_dal.clone(),
            tool_call_logger: self.tool_call_logger.clone(),
        }
    }
}

impl RuntimeDomainImpl {
    fn new(brain_dal: Arc<dyn BrainDal>) -> Self {
        let agent_dal = crate::service::dal::agent::dal();
        let codex_agent_dal = Arc::new(CodexAgentDal::new(agent_dal.clone()));
        let a2a_agent_dal = Arc::new(A2aAgentDal::new(agent_dal.clone()));
        Self {
            brain_dal,
            tool_dal: crate::service::dal::tool::dal(),
            mcp_tool_dal: crate::service::dal::mcp_tool::dal(),
            agent_dal,
            codex_agent_dal,
            a2a_agent_dal,
            tool_call_logger: Arc::new(ToolCallLogger::get().clone()),
        }
    }

    /// 创建带显式 Tool DAL 依赖的 Domain 实例。
    ///
    /// 仅用于测试 Runtime 协议路由，避免依赖全局单例与真实外部 runtime。
    #[cfg(test)]
    fn new_with_tool_dals(
        brain_dal: Arc<dyn BrainDal>,
        tool_dal: Arc<dyn ToolDal>,
        mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
        agent_dal: Arc<dyn AgentDal>,
    ) -> Self {
        let codex_agent_dal = Arc::new(CodexAgentDal::new(agent_dal.clone()));
        let a2a_agent_dal = Arc::new(A2aAgentDal::new(agent_dal.clone()));
        Self {
            brain_dal,
            tool_dal,
            mcp_tool_dal,
            agent_dal,
            codex_agent_dal,
            a2a_agent_dal,
            tool_call_logger: Arc::new(ToolCallLogger::get().clone()),
        }
    }

    /// 创建带显式所有依赖的 Domain 实例（测试用）。
    #[cfg(test)]
    fn new_with_all(
        brain_dal: Arc<dyn BrainDal>,
        tool_dal: Arc<dyn ToolDal>,
        mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
        agent_dal: Arc<dyn AgentDal>,
        tool_call_logger: Arc<ToolCallLogger>,
    ) -> Self {
        let codex_agent_dal = Arc::new(CodexAgentDal::new(agent_dal.clone()));
        let a2a_agent_dal = Arc::new(A2aAgentDal::new(agent_dal.clone()));
        Self {
            brain_dal,
            tool_dal,
            mcp_tool_dal,
            agent_dal,
            codex_agent_dal,
            a2a_agent_dal,
            tool_call_logger,
        }
    }

    /// 获取 Brain DAL 引用
    fn brain_dal(&self) -> &dyn BrainDal {
        &*self.brain_dal
    }

    /// 根据 agent.kind 返回对应的 PromptBuilder
    ///
    /// 工厂方法：awakening 组装 prompt 时调用此方法获取 builder。
    /// 三种 Agent 类型都通过各自 Dal 的 prompt_builder() 方法获取，保持对齐：
    /// Local → AgentDal.prompt_builder() → DefaultPromptBuilder
    /// Cli   → CodexAgentDal.prompt_builder() → CliPromptBuilder（未来实现）
    /// Remote → A2aAgentDal.prompt_builder() → RemotePromptBuilder（未来实现）
    fn prompt_builder(
        &self,
        agent: &Agent,
    ) -> Box<dyn crate::models::prompt_builder::PromptBuilder> {
        use common::enums::AgentKind;
        match agent.po.kind {
            AgentKind::Local => self.agent_dal.prompt_builder(),
            AgentKind::Cli => self.codex_agent_dal.prompt_builder(),
            AgentKind::Remote => self.a2a_agent_dal.prompt_builder(),
        }
    }
}

impl RuntimeDomain for RuntimeDomainImpl {
    fn memory(&self) -> &dyn RuntimeMemory {
        self
    }
    fn awakening(&self) -> &dyn RuntimeAwakening {
        self
    }
    fn tool_execution(&self) -> &dyn RuntimeToolExecution {
        self
    }

    fn agent_runtime_state(&self, agent_id: &str) -> AgentRuntimeState {
        crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global().get_state(agent_id)
    }

    fn is_agent_unavailable(&self, agent_id: &str) -> bool {
        crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global().is_unavailable(agent_id)
    }

    fn rest_and_settle(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        settle_limit: usize,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize>> + Send + '_>> {
        let state_manager = crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global();
        state_manager.set_resting(agent_id);

        let ctx_clone = ctx.clone();
        let agent_id_clone = agent_id.to_string();
        log_info!(
            ctx,
            "rest_and_settle",
            "agent_id={}, 开始休息并执行记忆沉淀",
            agent_id
        );

        let self_clone = self.clone();
        Box::pin(async move {
            let nodes = self_clone
                .memory()
                .settle(ctx_clone.clone(), &agent_id_clone, settle_limit)
                .await?;

            state_manager.set_idle(&agent_id_clone);

            log_info!(
                ctx_clone,
                "rest_and_settle",
                "agent_id={}, 休息结束，沉淀了 {} 个知识节点",
                agent_id_clone,
                nodes.len()
            );
            Ok(nodes.len())
        })
    }
}

// ==================== 单例 ====================

use std::sync::OnceLock;

static RUNTIME_DOMAIN: OnceLock<Arc<dyn RuntimeDomain>> = OnceLock::new();

/// 获取 Runtime Domain 单例
pub fn domain() -> Arc<dyn RuntimeDomain> {
    RUNTIME_DOMAIN.get().cloned().unwrap()
}

/// 创建新的 Runtime Domain 实例（用于测试，每次测试创建独立实例保证隔离）
pub fn new(brain_dal: Arc<dyn BrainDal>) -> Arc<dyn RuntimeDomain> {
    let domain = RuntimeDomainImpl::new(brain_dal);
    Arc::new(domain)
}

/// 创建新的 Runtime Domain 实例并显式注入 Tool DAL（用于测试）。
#[cfg(test)]
pub fn new_with_tool_dals(
    brain_dal: Arc<dyn BrainDal>,
    tool_dal: Arc<dyn ToolDal>,
    mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
    agent_dal: Arc<dyn AgentDal>,
) -> Arc<dyn RuntimeDomain> {
    let domain =
        RuntimeDomainImpl::new_with_tool_dals(brain_dal, tool_dal, mcp_tool_dal, agent_dal);
    Arc::new(domain)
}

/// 创建新的 Runtime Domain 实例并注入所有依赖（用于测试）。
#[cfg(test)]
pub fn new_with_all(
    brain_dal: Arc<dyn BrainDal>,
    tool_dal: Arc<dyn ToolDal>,
    mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
    agent_dal: Arc<dyn AgentDal>,
    tool_call_logger: Arc<ToolCallLogger>,
) -> Arc<dyn RuntimeDomain> {
    let domain = RuntimeDomainImpl::new_with_all(
        brain_dal,
        tool_dal,
        mcp_tool_dal,
        agent_dal,
        tool_call_logger,
    );
    Arc::new(domain)
}

/// 初始化 Runtime Domain（使用全局单例 DAO）
pub fn init() {
    let runtime_domain = RuntimeDomainImpl::new(crate::service::dal::brain::dal());
    let _ = RUNTIME_DOMAIN.set(Arc::new(runtime_domain));
}

// ==================== 结果结构体 ====================

/// 唤醒结果
#[derive(Debug, Clone)]
pub struct AwakeningResult {
    /// Agent ID
    pub agent_id: String,
    /// 本次产生的 Trace ID 列表（输入 + 输出）
    pub trace_ids: Vec<String>,
    /// 原始输入（完整 Prompt）
    pub raw_input: String,
    /// 原始输出（模型返回）
    pub raw_output: String,
}
