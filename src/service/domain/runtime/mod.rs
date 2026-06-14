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

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::models::memory::{Memory, MemoryTrace};
use crate::models::message::Message;
use crate::pkg::request_context::RequestContext;
use crate::service::dal::brain::BrainDal;

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
}

/// 记忆管理 trait
///
/// 定义记忆读取、思考 Trace 写入等接口
#[async_trait]
pub trait RuntimeMemory: Send + Sync {
    /// 读取最近短期记忆
    async fn get_recent_context(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Memory>, AppError>;

    /// 写入思考 Trace
    ///
    /// 直接接收 MemoryTrace 结构体，内部可做统一信息补充
    async fn write_thinking_trace(
        &self,
        ctx: RequestContext,
        trace: MemoryTrace,
    ) -> Result<Memory, AppError>;
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
    ) -> Result<AwakeningResult, AppError>;
}

/// 工具执行 trait
///
/// 定义工具实际执行的接口
#[async_trait]
pub trait RuntimeToolExecution: Send + Sync {
    // （预留）后续实现工具执行能力
}

// ==================== 子模块  ====================
// 注意：子模块必须在 trait 定义之后导入，这样子模块才能看到这些 trait

mod awakening;
mod context_assembly;
mod memory;
mod tool_execution;

pub use context_assembly::{PromptBuilder, build_conversation_prompt};

// ==================== 实现 ====================

/// Runtime Domain 实现
///
/// 聚合所有运行时子功能实现
struct RuntimeDomainImpl {
    brain_dal: Arc<dyn BrainDal>,
}

impl std::fmt::Debug for RuntimeDomainImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeDomainImpl")
            .field("brain_dal", &"<BrainDal>")
            .finish()
    }
}

impl Clone for RuntimeDomainImpl {
    fn clone(&self) -> Self {
        Self {
            brain_dal: self.brain_dal.clone(),
        }
    }
}

impl RuntimeDomainImpl {
    /// 创建 Domain 实例
    fn new(brain_dal: Arc<dyn BrainDal>) -> Self {
        Self { brain_dal }
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
