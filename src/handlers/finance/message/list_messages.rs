//! Handler: GET /api/v1/messages - List messages with filtering

use crate::pkg::RequestContext;
use crate::service::dao::message::MessageQuery;
use crate::service::domain::message;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::message::{ListMessagesRequest, ListMessagesResponse, MessageListItem};
use common::enums::MessageStatus;
use common::error::{Result, bail_err, err};

/// List messages with optional filtering by project, task, from_id, to_id, before/after timestamp
///
/// `project_id` 三态（见 `common::constants::message::DEFAULT_CONVERSATION_PROJECT_ID`）：
/// - 不传 / `None` → **不过滤**，返回全部会话的消息
/// - `__default__` → 只要**默认会话**（`project_id IS NULL`）的消息
/// - 真实 project id → 该项目会话
///
/// 分页模式（**返回结果恒按 `created_at` 正序**，与前端「上拉翻页时插入到列表头部」的用法一致）：
/// - 初始加载：不带时间游标 → 取最新的 `limit` 条
/// - 上拉翻页：传 `before_timestamp` → 取该时间点**之前**、离它最近的 `limit` 条
/// - 下拉轮询：传 `after_timestamp` → 取该时间点**之后**最早的 `limit` 条
///
/// 两个时间游标都是**开区间**且**下推到 SQL**（`MessageQuery::created_after / created_before`），
/// 因此 `limit` 语义准确 —— 返回条数不超过 `limit`，调用方无需超量取值。
#[register_handler_tool(
    id = "list_messages",
    name = "List Chat Messages",
    description = "List chat messages filtered by project_id, task_id, from_id, to_id, root_id, or status with time-window pagination: pass before_timestamp to page older history or after_timestamp to poll for new messages. Pass status=1 to return only unprocessed (pending) messages — useful to inspect what work an agent or project still has queued. Pass root_id to fetch an entire message thread / discussion chain. Omit project_id to search across all conversations; pass project_id=\"__default__\" to restrict to the default (project-less) conversation only. Returns messages with a total count. Use search_messages for keyword lookup.",
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

    // 取数顺序 ≠ 展示顺序：
    // - 翻旧页 / 无游标 → DESC + LIMIT，「离游标最近的一页」（再反转成正序返回）
    // - 轮询新消息      → ASC  + LIMIT，「游标之后最早的一页」（取出来已是正序）
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
        root_id: params.root_id.clone(),
        // 显式指定状态时走 status_in（DAO 侧据此不再叠加 `status != 0` 的软删除过滤）；
        // 不传则保持默认行为（排除已撤回）。判定见 `push_query_filters`。
        status_in: params.status.map(|s| vec![MessageStatus::from(s)]),
        // 时间游标下推到 SQL（开区间），不再「超量取 limit + 100 条再内存过滤」
        created_after: params.after_timestamp,
        created_before: params.before_timestamp,
        limit: Some(limit),
        offset: None,
        order_by: Some(order_by),
        ..Default::default()
    };

    let messages = message::domain().management().query(ctx, query).await?;

    // DESC 取数的两个分支需反转成时间正序；after 分支取数本就是正序
    let mut sorted = messages;
    if params.after_timestamp.is_none() {
        sorted.reverse();
    }

    let total = sorted.len();
    let messages: Vec<MessageListItem> = sorted
        .iter()
        .map(|m| {
            // 只有当 file_type 有值时才视为附件消息
            let file_meta = m.po.file_type.map(|_| {
                let fm = &m.po.file_meta.0;
                let name = fm
                    .file_path
                    .rsplit('/')
                    .next()
                    .unwrap_or(&fm.file_path)
                    .to_string();
                common::api::message::FileMetaInfo {
                    name,
                    mime_type: fm.mime_type.clone(),
                    size: fm.file_size,
                }
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
                root_id: m.po.root_id.clone(),
                created_at: m.po.created_at,
                file_type: m.po.file_type.map(|ft| ft as i32),
                file_meta,
            }
        })
        .collect();

    Ok(ListMessagesResponse { messages, total })
}
