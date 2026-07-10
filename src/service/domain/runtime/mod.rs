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

use common::enums::AgentRuntimeState;
use common::error::Result;
use crate::models::agent::Agent;
use crate::models::memory::{Memory, MemoryCreateParams, MemoryTrace};
use crate::models::message::Message;
use crate::pkg::request_context::RequestContext;
use crate::pkg::tool_tracing::logger::ToolCallLogger;
use crate::service::dao::memory::{MemoryQuery, MemorySearch};
use crate::service::dal::brain::BrainDal;
use crate::service::dal::mcp_tool::McpToolDal;
use crate::service::dal::tool::ToolDal;

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
        task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Memory>>;

    /// 写入思考 Trace
    ///
    /// 直接接收 MemoryTrace 结构体，内部可做统一信息补充
    async fn write_thinking_trace(
        &self,
        ctx: RequestContext,
        trace: MemoryTrace,
    ) -> Result<Memory>;

    // === 公开方法（供 Handler/神经工具调用） ===

    /// 混合搜索记忆（关键词 + 向量语义）
    async fn search(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>>;

    /// 通用关系型查询
    async fn query(
        &self,
        ctx: RequestContext,
        query: MemoryQuery,
    ) -> Result<Vec<Memory>>;

    /// 创建记忆
    async fn create(
        &self,
        ctx: RequestContext,
        params: MemoryCreateParams,
    ) -> Result<Vec<Memory>>;

    /// 更新记忆
    async fn update(
        &self,
        ctx: RequestContext,
        memory: Memory,
    ) -> Result<Memory>;

    /// 删除记忆
    async fn delete(
        &self,
        ctx: RequestContext,
        memory: Memory,
    ) -> Result<()>;
}

/// 唤醒能力 trait
///
/// 定义 Agent 唤醒相关的核心业务接口
#[async_trait]
pub trait RuntimeAwakening: Send + Sync {
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
mod context_assembly;
mod memory;
mod tool_call_query;
mod tool_execution;

#[cfg(test)]
mod tool_execution_test;

pub use context_assembly::{PromptBuilder, build_conversation_prompt};
pub(crate) use tool_call_query::status_from_dto;

// ==================== 实现 ====================

/// Runtime Domain 实现
///
/// 聚合所有运行时子功能实现
struct RuntimeDomainImpl {
    brain_dal: Arc<dyn BrainDal>,
    tool_dal: Arc<dyn ToolDal>,
    mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
    tool_call_logger: Arc<ToolCallLogger>,
}

impl std::fmt::Debug for RuntimeDomainImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeDomainImpl")
            .field("brain_dal", &"<BrainDal>")
            .field("tool_dal", &"<ToolDal>")
            .field("mcp_tool_dal", &"<McpToolDal>")
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
            tool_call_logger: self.tool_call_logger.clone(),
        }
    }
}

impl RuntimeDomainImpl {
    fn new(brain_dal: Arc<dyn BrainDal>) -> Self {
        Self {
            brain_dal,
            tool_dal: crate::service::dal::tool::dal(),
            mcp_tool_dal: crate::service::dal::mcp_tool::dal(),
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
    ) -> Self {
        Self {
            brain_dal,
            tool_dal,
            mcp_tool_dal,
            tool_call_logger: Arc::new(ToolCallLogger::get().clone()),
        }
    }

    /// 创建带显式所有依赖的 Domain 实例（测试用）。
    #[cfg(test)]
    fn new_with_all(
        brain_dal: Arc<dyn BrainDal>,
        tool_dal: Arc<dyn ToolDal>,
        mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
        tool_call_logger: Arc<ToolCallLogger>,
    ) -> Self {
        Self {
            brain_dal,
            tool_dal,
            mcp_tool_dal,
            tool_call_logger,
        }
    }

    /// 获取 Brain DAL 引用
    fn brain_dal(&self) -> &dyn BrainDal {
        &*self.brain_dal
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
        crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global()
            .get_state(agent_id)
    }

    fn is_agent_unavailable(&self, agent_id: &str) -> bool {
        crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global()
            .is_unavailable(agent_id)
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
) -> Arc<dyn RuntimeDomain> {
    let domain = RuntimeDomainImpl::new_with_tool_dals(brain_dal, tool_dal, mcp_tool_dal);
    Arc::new(domain)
}

/// 创建新的 Runtime Domain 实例并注入所有依赖（用于测试）。
#[cfg(test)]
pub fn new_with_all(
    brain_dal: Arc<dyn BrainDal>,
    tool_dal: Arc<dyn ToolDal>,
    mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
    tool_call_logger: Arc<ToolCallLogger>,
) -> Arc<dyn RuntimeDomain> {
    let domain = RuntimeDomainImpl::new_with_all(brain_dal, tool_dal, mcp_tool_dal, tool_call_logger);
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
