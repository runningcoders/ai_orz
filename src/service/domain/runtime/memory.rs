//! Runtime Memory 具体实现

use crate::error::AppError;
use crate::models::memory::{Memory, MemoryTrace};
use crate::pkg::request_context::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::{RuntimeDomainImpl, RuntimeMemory};

#[async_trait::async_trait]
impl RuntimeMemory for RuntimeDomainImpl {
    async fn get_recent_context(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Memory>, AppError> {
        use crate::service::dal::memory::dal;
        dal().query(
            ctx,
            MemoryQuery {
                agent_id: Some(agent_id.to_string()),
                memory_type: Some(common::enums::MemoryType::ShortTerm),
                limit: Some(limit),
                ..Default::default()
            },
        ).await
    }

    /// 写入思考 Trace
    ///
    /// 直接接收外部构造的 MemoryTrace，内部只负责调用 DAL 写入
    /// 调用方提前构造 trace 可以拿到 trace_id 注入 Prompt
    async fn write_thinking_trace(
        &self,
        ctx: RequestContext,
        mut trace: MemoryTrace,
    ) -> Result<Memory, AppError> {
        use crate::models::memory::MemoryCreateParams;
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
            .ok_or_else(|| AppError::Internal("Write trace failed, no memory returned".to_string()))
    }
}
