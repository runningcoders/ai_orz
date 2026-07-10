//! Runtime Memory 具体实现

use common::error::{Error, Result, err};
use crate::models::memory::{Memory, MemoryCreateParams, MemoryTrace};
use crate::pkg::request_context::RequestContext;
use crate::service::dao::memory::{MemoryQuery, MemorySearch};
use crate::service::domain::runtime::{RuntimeDomainImpl, RuntimeMemory};

#[async_trait::async_trait]
impl RuntimeMemory for RuntimeDomainImpl {
    // === 内部使用方法 ===

    async fn get_recent_context(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        let ctx = ctx
            .to_builder()
            .agent_id(agent_id)
            .try_task_id(task_id)
            .build();
        use crate::service::dal::memory::dal;
        dal()
            .query(
                ctx,
                MemoryQuery {
                    agent_id: Some(agent_id.to_string()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(limit),
                    ..Default::default()
                },
            )
            .await
    }

    /// 写入思考 Trace
    ///
    /// 直接接收外部构造的 MemoryTrace，内部只负责调用 DAL 写入
    /// 调用方提前构造 trace 可以拿到 trace_id 注入 Prompt
    async fn write_thinking_trace(
        &self,
        ctx: RequestContext,
        mut trace: MemoryTrace,
    ) -> Result<Memory> {
        use crate::service::dal::memory::dal;

        // 可选：内部统一补充信息（如果缺失）
        if trace.log_id.is_empty() {
            trace.log_id = ctx.log_id.clone();
        }
        if trace.user_id.is_empty() {
            trace.user_id = ctx.uid();
        }
        if trace.organization_id.is_empty() {
            trace.organization_id = ctx.organization_id.clone().unwrap_or_default();
        }
        if trace.task_id.is_none() {
            trace.task_id = ctx.task_id().cloned();
        }

        let mut results = dal()
            .create(ctx, MemoryCreateParams::AppendTraces(vec![trace]))
            .await?;

        results
            .pop()
            .ok_or_else(|| err!(Internal, "Write trace failed, no memory returned"))
    }

    // === 公开方法（供 Handler/神经工具调用） ===

    async fn search(
        &self,
        ctx: RequestContext,
        search: MemorySearch,
    ) -> Result<Vec<Memory>> {
        use crate::service::dal::memory::dal;
        dal().search(ctx, search).await
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: MemoryQuery,
    ) -> Result<Vec<Memory>> {
        use crate::service::dal::memory::dal;
        dal().query(ctx, query).await
    }

    async fn create(
        &self,
        ctx: RequestContext,
        params: MemoryCreateParams,
    ) -> Result<Vec<Memory>> {
        use crate::service::dal::memory::dal;
        dal().create(ctx, params).await
    }

    async fn update(
        &self,
        ctx: RequestContext,
        memory: Memory,
    ) -> Result<Memory> {
        use crate::service::dal::memory::dal;
        dal().update(ctx, memory).await
    }

    async fn delete(
        &self,
        ctx: RequestContext,
        memory: Memory,
    ) -> Result<()> {
        use crate::service::dal::memory::dal;
        dal().delete(ctx, memory).await
    }
}
