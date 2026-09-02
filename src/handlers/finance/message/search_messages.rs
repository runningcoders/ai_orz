//! Handler: POST /api/v1/finance/messages/search - Search messages with hybrid search

use crate::models::message::Message;
use crate::pkg::RequestContext;
use crate::service::dao::message::MessageSearch;
use crate::service::domain::message;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::message::{MessageSearchResult, SearchMessagesRequest, SearchMessagesResponse};
use common::error::{Result, bail_err, err};

#[register_handler_tool(
    id = "search_messages",
    name = "Search Messages",
    description = "Search messages by free-text keyword using hybrid FTS5 + vector semantic ranking, optionally filtered by project_id, task_id, from_id, or to_id. Returns ranked results with match_type and relevance scores. Use list_messages to browse chronologically instead.",
    params = "common::api::message::SearchMessagesRequest",
    tags = "messaging"
)]
#[generate_http_handler]
pub async fn search_messages(
    ctx: RequestContext,
    params: SearchMessagesRequest,
) -> Result<SearchMessagesResponse> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| err!(InvalidRequest, "当前请求缺少组织上下文"))?;
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let search = MessageSearch {
        keyword: params.keyword,
        query_vector: None,
        top_k: params.limit.map(|l| l as i32).or(Some(20)),
        filters: crate::service::dao::message::MessageQuery {
            organization_id: Some(org_id),
            project_id: params.project_id,
            task_id: params.task_id,
            from_id: params.from_id,
            to_id: params.to_id,
            limit: params.limit.or(Some(20)),
            ..Default::default()
        },
    };

    let messages = message::domain().management().search(ctx, search).await?;
    let results: Vec<MessageSearchResult> =
        messages.into_iter().map(message_to_search_result).collect();

    Ok(SearchMessagesResponse {
        messages: results.clone(),
        total: results.len(),
    })
}

fn message_to_search_result(message: Message) -> MessageSearchResult {
    let match_info = message.search_match.as_ref();
    MessageSearchResult {
        message_id: message.po.id,
        project_id: message.po.project_id,
        task_id: message.po.task_id,
        from_id: message.po.from_id,
        from_role: message.po.from_role as i32,
        to_id: message.po.to_id,
        to_role: message.po.to_role as i32,
        message_type: message.po.message_type as i32,
        content: message.po.content,
        created_at: message.po.created_at,
        match_type: match_info.map(|m| match m.match_type {
            crate::models::vector::MatchType::Hybrid => "hybrid".to_string(),
            crate::models::vector::MatchType::Vector => "vector".to_string(),
            crate::models::vector::MatchType::Keyword => "keyword".to_string(),
        }),
        fts_rank: match_info.and_then(|m| m.fts_rank),
        vector_distance: match_info.and_then(|m| m.vector_distance),
    }
}
