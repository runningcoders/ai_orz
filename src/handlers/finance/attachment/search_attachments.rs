//! Handler: POST /api/v1/finance/attachments/search - 混合搜索附件

use crate::models::vector::MatchType;
use crate::pkg::RequestContext;
use crate::service::dao::attachment::AttachmentSearch;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AttachmentSearchResult, PagedResult, SearchAttachmentsRequest};
use common::error::{Result, bail_err};

use super::response::to_detail;

/// 混合搜索当前用户的附件（FTS5 关键词 + 向量语义）。
///
/// `neural` 必需：让 Agent 能按 AI 写入的摘要/用途检索历史附件，而不只是按 ID 精确取。
/// 归属范围由 handler 强制注入 `ctx.uid()`，请求 DTO 不接受 owner 参数（越权红线）。
#[register_handler_tool(
    id = "search_attachments",
    name = "Search Attachments",
    description = "Hybrid-search the current user's attachments by keyword (FTS5 full-text plus vector semantic matching, ranked by relevance). Searches file names and the AI-recorded summaries (description = what the file contains, usage = inferred purpose), so you can find past attachments by what they are about, not only by ID. Optionally filter by file_type (Document/Image/Audio/Video/Binary) or purpose. Only your own attachments are returned. Use get_attachment to read one by ID, get_attachment_content to read a text file.",
    params = "common::api::SearchAttachmentsRequest",
    neural,
    tags = "attachment"
)]
#[generate_http_handler]
pub async fn search_attachments(
    ctx: RequestContext,
    params: SearchAttachmentsRequest,
) -> Result<PagedResult<AttachmentSearchResult>> {
    let uid = ctx.uid();
    if uid.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if params
        .keyword
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        bail_err!(InvalidRequest, "搜索关键词不能为空");
    }

    let search = AttachmentSearch {
        keyword: params.keyword,
        root_user_id: Some(uid), // 归属强注入：不接受前端传 owner
        purpose: params.purpose,
        file_type: params.file_type,
        pagination: params.pagination,
    };

    let page = domain()
        .attachment_manage()
        .search_attachments(ctx, search)
        .await?;

    Ok(page.map(|a| {
        let search_match = a.search_match.as_ref();
        AttachmentSearchResult {
            match_type: search_match.map(|m| match m.match_type {
                MatchType::Hybrid => "hybrid".to_string(),
                MatchType::Vector => "vector".to_string(),
                MatchType::Keyword => "keyword".to_string(),
            }),
            fts_rank: search_match.and_then(|m| m.fts_rank),
            vector_distance: search_match.and_then(|m| m.vector_distance),
            attachment: to_detail(&a),
        }
    }))
}
