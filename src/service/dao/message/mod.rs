//! Message DAO 模块

use common::error::Result;
use crate::models::message::{MessagePo, ToolCallMessage};
use crate::models::vector::VectorIndexParams;
use crate::pkg::RequestContext;
use common::enums::MessageStatus;

/// 消息通用查询条件
///
/// 支持组合查询，所有字段都是 Option：
/// - None 表示不限制该条件
/// - Some(value) 表示必须匹配该值
#[derive(Debug, Clone, Default)]
pub struct MessageQuery {
    /// 按消息 ID 查询（通常返回单条）
    pub id: Option<String>,
    /// 按多个消息 ID 批量查询（用于向量搜索结果回填）
    pub ids: Option<Vec<String>>,
    /// 按任务 ID 查询
    pub task_id: Option<String>,
    /// 按项目 ID 查询
    pub project_id: Option<String>,
    /// 按发送方 ID 查询
    pub from_id: Option<String>,
    /// 按接收方 ID 查询
    pub to_id: Option<String>,
    /// 按状态 IN 查询（支持多选）
    pub status_in: Option<Vec<MessageStatus>>,
    /// 限制返回条数（分页）
    pub limit: Option<usize>,
    /// 跳过条数（分页）
    pub offset: Option<usize>,
    /// 排序规则，如 "created_at ASC", "created_at DESC"
    pub order_by: Option<String>,
    /// 关键词搜索（用于 FTS5 全文检索，由 search_messages 方法使用）
    pub keyword: Option<String>,
    /// 按组织 ID 查询（多租户隔离）
    pub organization_id: Option<String>,
}

/// ✅ 消息搜索统一入参（关键词搜索 + 向量语义搜索共用）
#[derive(Debug, Clone, Default)]
pub struct MessageSearch {
    /// 关键词搜索查询（用于 FTS5 全文检索）
    pub keyword: Option<String>,
    /// 查询向量（用于向量语义搜索，DAL 层填充）
    pub query_vector: Option<Vec<f32>>,
    /// 返回 Top K 结果（向量搜索专用）
    pub top_k: Option<i32>,
    /// ✅ 业务过滤条件（直接复用 MessageQuery）
    pub filters: MessageQuery,
}

// ==================== 接口 ====================

/// Message DAO 接口
#[async_trait::async_trait]
pub trait MessageDao: Send + Sync {
    /// 插入一条新消息
    async fn insert(&self, ctx: RequestContext, message: &MessagePo) -> Result<()>;

    /// 通用查询方法
    ///
    /// 支持组合查询条件，所有字段都是 Option
    /// 示例：
    /// ```ignore
    /// let messages = dao.query(ctx, MessageQuery {
    ///     task_id: Some("task-123".to_string()),
    ///     status_in: Some(vec![MessageStatus::Pending, MessageStatus::Processing]),
    ///     limit: Some(10),
    ///     ..Default::default()
    /// }).await?;
    /// ```
    async fn query(&self, ctx: RequestContext, query: MessageQuery) -> Result<Vec<MessagePo>>;

    /// 根据 ID 查找消息
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<MessagePo>>;

    /// 根据任务 ID 查询所有消息，按创建时间升序排列
    /// 如果传入 limit 则限制返回数量
    async fn list_by_task_id(
        &self,
        ctx: RequestContext,
        task_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<MessagePo>>;

    /// 根据项目 ID 查询所有消息，按创建时间升序排列
    /// 如果传入 limit 则限制返回数量
    async fn list_by_project_id(
        &self,
        ctx: RequestContext,
        project_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<MessagePo>>;

    /// 根据来源 ID 查询所有消息
    async fn list_by_from_id(
        &self,
        ctx: RequestContext,
        from_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<MessagePo>>;

    /// 根据目标 ID 查询所有消息
    async fn list_by_to_id(
        &self,
        ctx: RequestContext,
        to_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<MessagePo>>;

    /// 删除消息（软删除可以用 status，但消息一般不删除，这里留作审计，所以接口只做物理删除保留）
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;

    /// 统计指定任务的消息数量
    async fn count_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<u64>;

    /// 统计符合查询条件的消息数量（复用 query 的 filter 逻辑，只跑 COUNT 不跑 LIST）
    async fn count(&self, ctx: RequestContext, query: MessageQuery) -> Result<u64>;

    /// 删除任务下所有消息（清空任务对话）
    async fn delete_by_task_id(&self, ctx: RequestContext, task_id: &str) -> Result<()>;

    /// 更新消息处理状态
    async fn update_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: MessageStatus,
    ) -> Result<()>;

    /// 根据多个状态查询消息（用于启动恢复未处理消息）
    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: Vec<MessageStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<MessagePo>>;

    /// 创建工具调用请求消息（便捷方法）
    /// 工具调用请求由 Agent 发起，请求执行某个工具
    async fn create_tool_call_request(
        &self,
        ctx: RequestContext,
        req: ToolCallMessage,
    ) -> Result<MessagePo>;

    /// 创建工具调用结果消息（便捷方法）
    /// 工具调用结果由执行器返回，包含执行结果
    async fn create_tool_call_result(
        &self,
        ctx: RequestContext,
        res: ToolCallMessage,
    ) -> Result<MessagePo>;

    /// 全文检索消息
    ///
    /// 使用 FTS5 MATCH + BM25 排序，返回匹配的消息及 FTS 相关性评分。
    ///
    /// # 参数
    /// - ctx: 请求上下文
    /// - search: 统一搜索参数（关键词 + 业务过滤）
    /// # 返回
    /// - 匹配的消息列表（按 BM25 相关性排序），每条携带 `fts_rank`（越小越相关）
    async fn search_messages(
        &self,
        ctx: RequestContext,
        search: MessageSearch,
    ) -> Result<Vec<(MessagePo, Option<f32>)>>;
}

// ==================== MessageVectorDao Trait ====================

/// ✅ Message Vector DAO trait - 仅负责消息向量索引的 CRUD，与基础消息数据完全解耦
#[async_trait::async_trait]
pub trait MessageVectorDao: Send + Sync {
    /// 插入或更新消息的向量索引
    async fn upsert_vector(
        &self,
        ctx: RequestContext,
        message_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<()>;

    /// 纯向量语义搜索，返回完整的向量行数据 + 相似度距离
    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<crate::models::vector::VectorSearchHit>>;

    /// 获取指定消息的完整向量行数据（包含元信息）
    async fn get_vector_row(
        &self,
        ctx: RequestContext,
        message_id: &str,
    ) -> Result<Option<crate::models::vector::VectorRow>>;

    /// 删除消息的向量索引
    async fn delete_vector(&self, ctx: RequestContext, message_id: &str) -> Result<()>;

    /// 清空所有向量索引
    async fn clear_collection(&self, ctx: RequestContext) -> Result<()>;
}

pub mod sqlite;
pub mod vector;

pub use self::sqlite::{dao, init as init_base, new};
pub use self::vector::{dao as vector_dao, init as init_vector, new as new_message_vector_dao};

/// 统一初始化所有 Message DAO 单例
pub fn init() {
    init_base();
    init_vector();
}

#[cfg(test)]
mod sqlite_test;
#[cfg(test)]
mod vector_test;
