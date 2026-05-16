//! Runtime Memory 模块
//!
//! 【定位】运行时记忆读写入口 - 最薄封装 DAL 层
//!
//! 设计原则：
//! - 零重复定义：所有参数结构体 100% 复用 DAL 层
//! - 调用方完全控制：写什么、怎么查完全由调用方构造 PO 决定
//! - 后续扩展：语法糖方法在 Domain 层加，不影响核心接口
//!
//! 内存语义：
//! - write: 写入记忆（JSONL 追加 / SQLite 索引）
//! - search: 混合搜索（关键词 + 向量 + 过滤）
//! - query: 普通查询（仅过滤条件，无向量）

use async_trait::async_trait;
use std::fmt::Debug;
use std::sync::Arc;

use crate::error::AppError;
use crate::models::memory::{Memory, MemoryCreateParams};
use crate::pkg::request_context::RequestContext;
use crate::service::dal::memory::MemoryDal;
use crate::service::dao::memory::{MemoryQuery, MemorySearch};

/// Runtime Memory 主 trait
///
/// 所有参数 100% 复用 DAL 层定义：
/// - MemoryCreateParams: 枚举控制写入类型（4 阶段模式）
/// - MemorySearch: 混合搜索（关键词 + 向量 + 过滤）
/// - MemoryQuery: 普通查询过滤
/// - Memory: 业务实体（PO + 搜索匹配信息）
#[async_trait]
pub trait RuntimeMemory: Send + Sync + Debug {
    /// 写入记忆
    ///
    /// 完全由调用方通过 MemoryCreateParams 控制写入内容：
    /// - AppendTraces: 仅写 JSONL 细节（阶段 1）
    /// - CreateShortTerm: 基于已有 trace 创建短期记忆索引（阶段 2）
    /// - CreateKnowledgeNode: 长期知识节点（可选带引用）
    /// - CreateRelations: 知识节点关系
    async fn write(&self, ctx: RequestContext, params: MemoryCreateParams) -> Result<Vec<Memory>, AppError>;

    /// 混合搜索记忆
    ///
    /// 支持：关键词搜索 + 向量语义搜索 + 业务过滤
    /// 具体走哪种搜索由 DAL 层根据参数自动决策
    async fn search(&self, ctx: RequestContext, search: MemorySearch) -> Result<Vec<Memory>, AppError>;

    /// 普通查询记忆
    ///
    /// 仅业务过滤，无向量/关键词搜索。
    /// 等价于 search(filters) 但语义更明确。
    async fn query(&self, ctx: RequestContext, query: MemoryQuery) -> Result<Vec<Memory>, AppError>;
}

/// Runtime Memory 默认实现
///
/// 最薄封装：直接透传调用 DAL 层
#[derive(Clone)]
pub struct RuntimeMemoryImpl {
    dal: Arc<dyn MemoryDal>,
}

impl std::fmt::Debug for RuntimeMemoryImpl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeMemoryImpl")
            .field("dal", &"Arc<dyn MemoryDal>")
            .finish()
    }
}

impl RuntimeMemoryImpl {
    /// 创建新的 RuntimeMemory 实例
    pub fn new() -> Self {
        Self {
            dal: crate::service::dal::memory::dal(),
        }
    }
}

impl Default for RuntimeMemoryImpl {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RuntimeMemory for RuntimeMemoryImpl {
    async fn write(&self, ctx: RequestContext, params: MemoryCreateParams) -> Result<Vec<Memory>, AppError> {
        // 直接透传 DAL 层
        self.dal.create(ctx, params).await
    }

    async fn search(&self, ctx: RequestContext, search: MemorySearch) -> Result<Vec<Memory>, AppError> {
        // 直接透传 DAL 层
        self.dal.search(ctx, search).await
    }

    async fn query(&self, ctx: RequestContext, query: MemoryQuery) -> Result<Vec<Memory>, AppError> {
        // 直接透传 DAL 层
        self.dal.query(ctx, query).await
    }
}

/// Thread-safe singleton instance
static RUNTIME_MEMORY_INSTANCE: std::sync::OnceLock<RuntimeMemoryImpl> = std::sync::OnceLock::new();

/// Get the global RuntimeMemory instance
pub fn instance() -> &'static dyn RuntimeMemory {
    RUNTIME_MEMORY_INSTANCE.get_or_init(RuntimeMemoryImpl::new)
}

#[cfg(test)]
mod tests {
    use common::enums::{MemoryRole, MemoryType};
    use sqlx::SqlitePool;

    use super::*;
    use crate::pkg::request_context::RequestContext;

    /// 测试 write - 追加 trace
    #[tokio::test]
    async fn test_runtime_memory_write_append_traces() {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        let ctx = RequestContext::new_simple("test-user", pool);

        // 构造 trace
        let traces = vec![crate::models::memory::MemoryTrace {
            id: "test-trace-001".to_string(),
            agent_id: "agent-001".to_string(),
            task_id: None,
            log_id: "log-001".to_string(),
            user_id: "user-001".to_string(),
            organization_id: "org-001".to_string(),
            role: MemoryRole::User,
            content: "Hello, World!".to_string(),
            created_at: 1234567890,
            metadata: std::collections::HashMap::new(),
            position: None,
        }];

        // 调用 Runtime Memory write
        let result = instance()
            .write(ctx, MemoryCreateParams::AppendTraces(traces))
            .await;

        assert!(result.is_ok());
        let memories = result.unwrap();
        assert_eq!(memories.len(), 1);
    }
}
