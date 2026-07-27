//! Finance 域 API - 模型提供商、工具、附件、MCP 服务器、消息渠道

use common::api::{
    AttachmentDetail, CallModelRequest, CallModelResponse, CreateMcpServerRequest,
    CreateMcpServerResponse, CreateModelProviderRequest, CreateModelProviderResponse,
    CreateTextAttachmentRequest, CreateToolRequest, CreateToolResponse, DebugCallToolRequest,
    DebugCallToolResponse, GetModelProviderRequest, GetModelProviderResponse, GetToolRequest,
    GetToolResponse, ListMcpServersResponse, ListModelProvidersResponse, ListToolsRequest,
    PagedResult, QueryToolCallEntriesRequest, QueryToolCallEntriesResponse,
    SwitchEmbeddingProviderRequest, SwitchEmbeddingProviderResponse, TestConnectionResponse,
    TestMessageChannelConnectionResponse, ToolListItem, ToolQueryRequest,
    UpdateAttachmentContentRequest, UpdateMcpServerStatusRequest,
    UpdateMessageChannelStatusRequest, UpdateModelProviderRequest, UpdateModelProviderResponse,
    UpdateModelProviderStatusRequest, UpdateToolRequest, UpdateToolResponse,
    UpdateToolStatusRequest,
};
use web_sys::FormData;

use super::{
    ApiError, api_delete, api_get, api_get_or_default, api_post, api_post_empty,
    api_post_multipart, api_put, api_put_empty,
};

// ===== 模型提供商 =====

pub async fn list_model_providers() -> Result<ListModelProvidersResponse, ApiError> {
    api_get_or_default("/api/v1/finance/model-providers").await
}

pub async fn get_model_provider(
    req: GetModelProviderRequest,
) -> Result<GetModelProviderResponse, ApiError> {
    let qs = super::build_query_string(&[
        (
            "with_model_call_stats",
            req.with_model_call_stats.map(|v| v.to_string()),
        ),
        (
            "stats_start_time",
            req.stats_start_time.map(|v| v.to_string()),
        ),
        ("stats_end_time", req.stats_end_time.map(|v| v.to_string())),
        ("stats_interval", req.stats_interval.clone()),
    ]);
    api_get(&format!("/api/v1/finance/model-providers/{}{}", req.id, qs)).await
}

pub async fn create_model_provider(
    req: CreateModelProviderRequest,
) -> Result<CreateModelProviderResponse, ApiError> {
    api_post("/api/v1/finance/model-providers", &req).await
}

pub async fn update_model_provider(
    req: UpdateModelProviderRequest,
) -> Result<UpdateModelProviderResponse, ApiError> {
    api_put(&format!("/api/v1/finance/model-providers/{}", req.id), &req).await
}

/// 启用/禁用模型提供商
/// 启用 Embedding Provider 时可能返回 409（需切换），前端可通过 ApiError.error_code 检测
pub async fn toggle_model_provider(req: UpdateModelProviderStatusRequest) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": req.status });
    api_put_empty(
        &format!("/api/v1/finance/model-providers/{}", req.id),
        &body,
    )
    .await
}

pub async fn delete_model_provider(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/finance/model-providers/{}", id)).await
}

pub async fn test_model_provider_connection(id: &str) -> Result<TestConnectionResponse, ApiError> {
    let body = serde_json::json!({});
    api_post(
        &format!("/api/v1/finance/model-providers/{}/test", id),
        &body,
    )
    .await
}

pub async fn call_model_provider(req: CallModelRequest) -> Result<CallModelResponse, ApiError> {
    let body = serde_json::json!({ "prompt": req.prompt });
    api_post(
        &format!("/api/v1/finance/model-providers/{}/call", req.id),
        &body,
    )
    .await
}

/// 切换 Embedding Provider（需用户确认）
/// 返回 ApiError 以便前端检测 409 embedding_provider_switch_required 错误
pub async fn switch_embedding_provider(
    req: SwitchEmbeddingProviderRequest,
) -> Result<SwitchEmbeddingProviderResponse, ApiError> {
    api_post(
        &format!("/api/v1/finance/model-providers/{}/switch", req.id),
        &req,
    )
    .await
}

// ===== 工具管理 =====

pub async fn list_tools(req: ListToolsRequest) -> Result<PagedResult<ToolListItem>, ApiError> {
    let url = super::build_pagination_url("/api/v1/finance/tools", &req.pagination);
    api_get(&url).await
}

pub async fn query_tools(req: &ToolQueryRequest) -> Result<PagedResult<ToolListItem>, ApiError> {
    api_post("/api/v1/finance/tools/query", req).await
}

pub async fn get_tool(req: GetToolRequest) -> Result<GetToolResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("with_stats", req.with_stats.map(|v| v.to_string())),
        (
            "stats_time_start",
            req.stats_time_start.map(|v| v.to_string()),
        ),
        ("stats_time_end", req.stats_time_end.map(|v| v.to_string())),
        ("stats_interval", req.stats_interval.clone()),
    ]);
    api_get(&format!("/api/v1/finance/tools/{}{}", req.id, qs)).await
}

#[allow(dead_code)]
pub async fn create_tool(req: CreateToolRequest) -> Result<CreateToolResponse, ApiError> {
    api_post("/api/v1/finance/tools", &req).await
}

#[allow(dead_code)]
pub async fn update_tool(req: UpdateToolRequest) -> Result<UpdateToolResponse, ApiError> {
    api_put(&format!("/api/v1/finance/tools/{}", req.id), &req).await
}

pub async fn update_tool_status(req: UpdateToolStatusRequest) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": req.status });
    api_put_empty(&format!("/api/v1/finance/tools/{}/status", req.id), &body).await
}

pub async fn delete_tool(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/finance/tools/{}", id)).await
}

/// 工具调试调用（管理员专用）
///
/// 在工具详情页直接调用工具进行调试，返回执行结果 + tool_call_id。
/// 需 Admin 及以上权限。
pub async fn debug_call_tool(req: DebugCallToolRequest) -> Result<DebugCallToolResponse, ApiError> {
    let body = serde_json::json!({ "args": req.args });
    api_post(
        &format!("/api/v1/finance/tools/{}/debug-call", req.id),
        &body,
    )
    .await
}

// ===== 消息渠道 =====

pub async fn list_message_channels() -> Result<common::api::ListMessageChannelsResponse, ApiError> {
    api_get_or_default("/api/v1/finance/message-channels").await
}

pub async fn create_message_channel(
    req: common::api::CreateMessageChannelRequest,
) -> Result<common::api::CreateMessageChannelResponse, ApiError> {
    api_post("/api/v1/finance/message-channels", &req).await
}

pub async fn update_message_channel_status(
    req: UpdateMessageChannelStatusRequest,
) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": req.status as i32 });
    api_put_empty(
        &format!("/api/v1/finance/message-channels/{}/status", req.id),
        &body,
    )
    .await
}

pub async fn delete_message_channel(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/finance/message-channels/{}", id)).await
}

pub async fn test_message_channel(
    id: &str,
) -> Result<TestMessageChannelConnectionResponse, ApiError> {
    let body = serde_json::json!({});
    api_post(
        &format!("/api/v1/finance/message-channels/{}/test", id),
        &body,
    )
    .await
}

// ===== MCP 服务器 =====

pub async fn list_mcp_servers() -> Result<ListMcpServersResponse, ApiError> {
    api_get_or_default("/api/v1/finance/mcp-servers").await
}

pub async fn create_mcp_server(
    req: CreateMcpServerRequest,
) -> Result<CreateMcpServerResponse, ApiError> {
    api_post("/api/v1/finance/mcp-servers", &req).await
}

pub async fn update_mcp_server_status(req: UpdateMcpServerStatusRequest) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": req.status as i32 });
    api_put_empty(
        &format!("/api/v1/finance/mcp-servers/{}/status", req.id),
        &body,
    )
    .await
}

pub async fn delete_mcp_server(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/finance/mcp-servers/{}", id)).await
}

pub async fn sync_mcp_tools(id: &str) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    api_post_empty(
        &format!("/api/v1/finance/mcp-servers/{}/tools/sync", id),
        &body,
    )
    .await
}

// ===== 附件管理 =====

pub async fn list_attachments() -> Result<Vec<AttachmentDetail>, ApiError> {
    api_get_or_default("/api/v1/finance/attachments").await
}

pub async fn create_text_attachment(
    req: CreateTextAttachmentRequest,
) -> Result<AttachmentDetail, ApiError> {
    api_post("/api/v1/finance/attachments/text", &req).await
}

/// 上传文件附件（multipart/form-data）
/// 需要传入已经构造好的 FormData（含 `file` 字段和 `purpose` 字段）
pub async fn upload_attachment(form: FormData) -> Result<AttachmentDetail, ApiError> {
    api_post_multipart("/api/v1/finance/attachments/upload", form).await
}

pub async fn get_attachment(id: &str) -> Result<AttachmentDetail, ApiError> {
    api_get(&format!("/api/v1/finance/attachments/{}", id)).await
}

pub async fn delete_attachment(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/finance/attachments/{}", id)).await
}

// ===== Attachment 内容 =====

/// 获取附件内容（仅 text 类型附件可获取）
pub async fn get_attachment_content(
    id: &str,
) -> Result<common::api::AttachmentContentResponse, ApiError> {
    api_get(&format!("/api/v1/finance/attachments/{}/content", id)).await
}

/// 更新文本附件内容
pub async fn update_attachment_content(
    req: UpdateAttachmentContentRequest,
) -> Result<common::api::AttachmentContentResponse, ApiError> {
    api_put(
        &format!("/api/v1/finance/attachments/{}/content", req.id),
        &req,
    )
    .await
}

// ===== MCP Server 详情 =====

pub async fn get_mcp_server(id: &str) -> Result<common::api::GetMcpServerResponse, ApiError> {
    api_get(&format!("/api/v1/finance/mcp-servers/{}", id)).await
}

// ===== Message Channel 详情 =====

pub async fn get_message_channel(
    id: &str,
) -> Result<common::api::GetMessageChannelResponse, ApiError> {
    api_get(&format!("/api/v1/finance/message-channels/{}", id)).await
}

// ===== 工具调用记录 =====

pub async fn query_tool_call_entries(
    params: &QueryToolCallEntriesRequest,
) -> Result<QueryToolCallEntriesResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("call_id", params.call_id.clone()),
        ("agent_id", params.agent_id.clone()),
        ("project_id", params.project_id.clone()),
        ("task_id", params.task_id.clone()),
        ("tool_id", params.tool_id.clone()),
        ("status", params.status.map(|s| format!("{:?}", s))),
        ("started_after", params.started_after.map(|v| v.to_string())),
        (
            "started_before",
            params.started_before.map(|v| v.to_string()),
        ),
        ("limit", params.limit.map(|v| v.to_string())),
    ]);
    api_get_or_default(&format!("/api/v1/finance/tool-call-entries{}", qs)).await
}

pub async fn get_tool_call_entry(
    call_id: &str,
) -> Result<common::api::GetToolCallEntryResponse, ApiError> {
    api_get(&format!("/api/v1/finance/tool-call-entries/{}", call_id)).await
}
