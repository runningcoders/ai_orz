//! Runtime Memory 具体实现

use crate::error::AppError;
use crate::models::memory::Memory;
use crate::pkg::request_context::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use crate::service::domain::runtime::{RuntimeDomainImpl, RuntimeMemory, ThinkingTraceType};

#[async_trait::async_trait]
impl RuntimeMemory for RuntimeDomainImpl {
    async fn get_recent_context(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        _task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Memory>, AppError> {
        // 直接透传到 DAL 层
        // 注意：当前 MemoryQuery 还没有 task_id 字段，后续可扩展
        use crate::service::dal::memory::dal;

        let query = MemoryQuery {
            agent_id: Some(agent_id.to_string()),
            memory_type: Some(common::enums::MemoryType::ShortTerm),
            limit: Some(limit),
            ..Default::default()
        };

        dal().query(ctx, query).await
    }

    async fn write_thinking_trace(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        trace_type: ThinkingTraceType,
        input: &str,
        output: Option<&str>,
        trace_id: Option<String>,
    ) -> Result<Memory, AppError> {
        let role = match trace_type {
            ThinkingTraceType::Input => common::enums::MemoryRole::System,
            ThinkingTraceType::Output => common::enums::MemoryRole::Assistant,
            ThinkingTraceType::ToolCall => common::enums::MemoryRole::System,
            ThinkingTraceType::ToolResult => common::enums::MemoryRole::System,
        };

        // 构造 MemoryTrace
        use crate::models::memory::{MemoryCreateParams, MemoryTrace};
        use crate::service::dal::memory::dal;
        use std::collections::HashMap;

        let mut trace = MemoryTrace::new(
            agent_id.to_string(),
            ctx.log_id.clone(),
            ctx.uid(),
            ctx.organization_id.clone().unwrap_or_default(),
            role,
            input.to_string(),
            ctx.task_id().cloned(),
        );

        // 如果传入了 trace_id，覆盖自动生成的
        if let Some(tid) = trace_id {
            trace.id = tid;
        }

        // 如果有 output，回填完成状态
        if let Some(out) = output {
            trace.complete(out.to_string());
        }

        let mut results = dal()
            .create(ctx, MemoryCreateParams::AppendTraces(vec![trace]))
            .await?;

        results
            .pop()
            .ok_or_else(|| AppError::Internal("Write trace failed, no memory returned".to_string()))
    }
}
