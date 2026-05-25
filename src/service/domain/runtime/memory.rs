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
        content: &str,
        trace_id: Option<String>,
    ) -> Result<Memory, AppError> {
        let role = match trace_type {
            ThinkingTraceType::Input => common::enums::MemoryRole::System,
            ThinkingTraceType::Output => common::enums::MemoryRole::Assistant,
            ThinkingTraceType::ToolCall => common::enums::MemoryRole::System,
            ThinkingTraceType::ToolResult => common::enums::MemoryRole::System,
        };

        // 复用传入的 trace_id，或生成新的
        let trace_id = trace_id.unwrap_or_else(|| {
            format!("trace-{}-{}", agent_id, chrono::Utc::now().timestamp_nanos())
        });

        // 构造 MemoryTrace 并写入
        use crate::models::memory::{MemoryCreateParams, MemoryTrace};
        use crate::service::dal::memory::dal;
        use std::collections::HashMap;

        let trace = MemoryTrace {
            id: trace_id,
            agent_id: agent_id.to_string(),
            task_id: ctx.task_id().cloned(),
            log_id: ctx.log_id.clone(),
            user_id: ctx.uid(),
            organization_id: ctx.organization_id.clone().unwrap_or_default(),
            role,
            content: content.to_string(),
            created_at: chrono::Utc::now().timestamp(),
            metadata: HashMap::new(),
            position: None,
        };

        let mut results = dal()
            .create(ctx, MemoryCreateParams::AppendTraces(vec![trace]))
            .await?;

        results
            .pop()
            .ok_or_else(|| AppError::Internal("Write trace failed, no memory returned".to_string()))
    }
}
