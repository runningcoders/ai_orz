//! Handler: POST /api/v1/artifacts/query - Artifact 通用查询接口
//!
//! 与 list_artifacts 的区别：list 是列表场景语法糖（GET + query param，按 project 维度），
//! query 是完整查询能力（POST + body），支持跨项目查询等复杂场景。

use super::response;
use crate::pkg::RequestContext;
use crate::service::dal::artifact::ArtifactQuery;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{ArtifactDetail, ArtifactQueryRequest, PagedResult};
use common::error::Result;

/// Artifact 通用查询（POST body，支持完整查询能力）
#[register_handler_tool(
    id = "query_artifacts",
    name = "Query Artifacts",
    description = "Query artifacts with full filtering support (project_id, task_id, file_type, source_type, etc.)",
    params = "common::api::ArtifactQueryRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn query_artifacts(
    ctx: RequestContext,
    params: ArtifactQueryRequest,
) -> Result<PagedResult<ArtifactDetail>> {
    let page = project::domain()
        .artifact_manage()
        .query(
            ctx,
            ArtifactQuery {
                project_id: params.project_id,
                task_id: params.task_id,
                file_type: params.file_type,
                source_type: params.source_type,
                pagination: params.pagination,
            },
        )
        .await?;

    Ok(page.map(|a| response::to_detail(&a)))
}
