//! 记忆轨迹查询：分页扫描每日 JSONL + 单条详情

use super::MemoryDalImpl;
use crate::models::memory::MemoryTrace;
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryTraceQuery;
use common::api::PagedResult;
use common::error::Result;

impl MemoryDalImpl {
    pub(super) async fn query_traces(
        &self,
        ctx: RequestContext,
        query: MemoryTraceQuery,
    ) -> Result<PagedResult<MemoryTrace>> {
        self.memory_dao.query_traces(ctx, query).await
    }

    pub(super) async fn get_trace(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        trace_id: &str,
    ) -> Result<Option<MemoryTrace>> {
        self.memory_dao.get_trace(ctx, agent_id, trace_id).await
    }
}
