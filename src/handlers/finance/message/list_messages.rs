//! Handler: GET /api/v1/messages - List messages with filtering

use crate::pkg::RequestContext;
use crate::service::dao::message::MessageQuery;
use crate::service::domain::message;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::message::{ListMessagesRequest, ListMessagesResponse, MessageListItem};
use common::error::{Result, err, bail_err};

/// List messages with optional filtering by project, task, from_id, to_id, before/after timestamp
///
/// 分页模式：
/// - 初始加载 / 上拉翻页：传 `before_timestamp` → 返回 created_at < before_timestamp 的消息，按 DESC 排序
/// - 下拉轮询新消息：传 `after_timestamp` → 返回 created_at > after_timestamp 的消息，按 ASC 排序
/// - 无时间过滤：默认返回最新消息，按 DESC 排序
#[register_handler_tool(
    id = "list_messages",
    name = "list_messages",
    description = "List messages with optional filtering by project, task, sender, receiver, with bidirectional pagination",
    params = "common::api::message::ListMessagesRequest",
    neural,
    tags = "messaging"
)]
#[generate_http_handler]
pub async fn list_messages(
    ctx: RequestContext,
    params: ListMessagesRequest,
) -> Result<ListMessagesResponse> {
    let org_id = ctx
        .organization_id
        .clone()
        .ok_or_else(|| err!(InvalidRequest, "当前请求缺少组织上下文"))?;
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    let limit = params.limit.unwrap_or(10);

    let order_by = if params.after_timestamp.is_some() {
        "created_at ASC".to_string()
    } else {
        "created_at DESC".to_string()
    };

    let query = MessageQuery {
        organization_id: Some(org_id),
        project_id: params.project_id.clone(),
        task_id: params.task_id.clone(),
        from_id: params.from_id.clone(),
        to_id: params.to_id.clone(),
        limit: Some(limit + 100),
        offset: None,
        order_by: Some(order_by),
        ..Default::default()
    };

    let messages = message::domain().management().query(ctx, query).await?;

    let filtered: Vec<_> = match (params.before_timestamp, params.after_timestamp) {
        (Some(before), None) => {
            messages
                .into_iter()
                .filter(|m| m.po.created_at < before)
                .take(limit)
                .collect()
        }
        (None, Some(after)) => {
            messages
                .into_iter()
                .filter(|m| m.po.created_at > after)
                .collect()
        }
        (Some(before), Some(after)) => {
            messages
                .into_iter()
                .filter(|m| m.po.created_at > after && m.po.created_at < before)
                .collect()
        }
        (None, None) => {
            messages.into_iter().take(limit).collect()
        }
    };

    let mut sorted = filtered;
    if params.after_timestamp.is_none() {
        sorted.reverse();
    }

    let total = sorted.len();
    let messages: Vec<MessageListItem> = sorted
        .iter()
        .map(|m| {
            // 只有当 file_type 有值时才视为附件消息
            let file_meta = m.po.file_type.and_then(|_| {
                let fm = &m.po.file_meta.0;
                let name = fm.file_path.rsplit('/').next().unwrap_or(&fm.file_path).to_string();
                Some(common::api::message::FileMetaInfo {
                    name,
                    mime_type: fm.mime_type.clone(),
                    size: fm.file_size,
                })
            });
            MessageListItem {
                message_id: m.po.id.clone(),
                project_id: m.po.project_id.clone(),
                task_id: m.po.task_id.clone(),
                from_id: m.po.from_id.clone(),
                from_role: m.po.from_role as i32,
                to_id: m.po.to_id.clone(),
                to_role: m.po.to_role as i32,
                message_type: m.po.message_type as i32,
                status: m.po.status as i32,
                content: m.po.content.clone(),
                reply_to_id: m.po.reply_to_id.clone(),
                created_at: m.po.created_at,
                file_type: m.po.file_type.map(|ft| ft as i32),
                file_meta,
            }
        })
        .collect();

    Ok(ListMessagesResponse { messages, total })
}