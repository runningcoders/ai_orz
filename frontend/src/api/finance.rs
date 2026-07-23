//! Finance 域 API - 模型提供商、工具、附件、MCP 服务器、消息渠道

use common::api::{
    AttachmentDetail, CallModelResponse, CreateMcpServerRequest, CreateMcpServerResponse,
    CreateModelProviderRequest, CreateModelProviderResponse, CreateTextAttachmentRequest,
    CreateToolRequest, CreateToolResponse,
    DebugCallToolResponse,
    GetModelProviderResponse, GetToolResponse, ListMcpServersResponse, ListModelProvidersResponse,
    ListToolsResponse, SwitchEmbeddingProviderRequest, SwitchEmbeddingProviderResponse,
    TestConnectionResponse, TestMessageChannelConnectionResponse,
    UpdateModelProviderRequest, UpdateModelProviderResponse, UpdateToolRequest, UpdateToolResponse,
};
use web_sys::FormData;

use super::{api_delete, api_get, api_get_or_default, api_post, api_post_empty, api_post_multipart, api_put, api_put_empty, ApiError};

// ===== 模型提供商 =====

pub async fn list_model_providers() -> Result<ListModelProvidersResponse, ApiError> {
    api_get_or_default("/api/v1/finance/model-providers").await
}

pub async fn get_model_provider(id: &str, stats_options: Option<&super::StatsOptions>) -> Result<GetModelProviderResponse, ApiError> {
    let url = super::build_url_with_stats(&format!("/api/v1/finance/model-providers/{}", id), stats_options);
    api_get(&url).await
}

pub async fn create_model_provider(req: CreateModelProviderRequest) -> Result<CreateModelProviderResponse, ApiError> {
    api_post("/api/v1/finance/model-providers", &req).await
}

#[allow(dead_code)]
pub async fn update_model_provider(id: &str, req: UpdateModelProviderRequest) -> Result<UpdateModelProviderResponse, ApiError> {
    api_put(&format!("/api/v1/finance/model-providers/{}", id), &req).await
}

/// 启用/禁用模型提供商
/// 启用 Embedding Provider 时可能返回 409（需切换），前端可通过 ApiError.error_code 检测
pub async fn toggle_model_provider(id: &str, status: i32) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/finance/model-providers/{}", id), &body).await
}

pub async fn delete_model_provider(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/finance/model-providers/{}", id)).await
}

pub async fn test_model_provider_connection(id: &str) -> Result<TestConnectionResponse, ApiError> {
    let body = serde_json::json!({});
    api_post(&format!("/api/v1/finance/model-providers/{}/test", id), &body).await
}

pub async fn call_model_provider(id: &str, prompt: &str) -> Result<CallModelResponse, ApiError> {
    let body = serde_json::json!({ "prompt": prompt });
    api_post(&format!("/api/v1/finance/model-providers/{}/call", id), &body).await
}

/// 切换 Embedding Provider（需用户确认）
/// 返回 ApiError 以便前端检测 409 embedding_provider_switch_required 错误
pub async fn switch_embedding_provider(id: &str) -> Result<SwitchEmbeddingProviderResponse, ApiError> {
    let body = SwitchEmbeddingProviderRequest {
        id: id.to_string(),
        confirm: true,
    };
    api_post(&format!("/api/v1/finance/model-providers/{}/switch", id), &body).await
}

// ===== 工具管理 =====

pub async fn list_tools() -> Result<ListToolsResponse, ApiError> {
    api_get_or_default("/api/v1/finance/tools").await
}

pub async fn get_tool(id: &str, stats_options: Option<&super::StatsOptions>) -> Result<GetToolResponse, ApiError> {
    let url = super::build_url_with_stats(&format!("/api/v1/finance/tools/{}", id), stats_options);
    api_get(&url).await
}

#[allow(dead_code)]
pub async fn create_tool(req: CreateToolRequest) -> Result<CreateToolResponse, ApiError> {
    api_post("/api/v1/finance/tools", &req).await
}

#[allow(dead_code)]
pub async fn update_tool(id: &str, req: UpdateToolRequest) -> Result<UpdateToolResponse, ApiError> {
    api_put(&format!("/api/v1/finance/tools/{}", id), &req).await
}

pub async fn update_tool_status(id: &str, status: i32) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/finance/tools/{}/status", id), &body).await
}

pub async fn delete_tool(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/finance/tools/{}", id)).await
}

/// 工具调试调用（管理员专用）
///
/// 在工具详情页直接调用工具进行调试，返回执行结果 + tool_call_id。
/// 需 Admin 及以上权限。
pub async fn debug_call_tool(id: &str, args: &serde_json::Value) -> Result<DebugCallToolResponse, ApiError> {
    api_post(&format!("/api/v1/finance/tools/{}/debug-call", id), args).await
}

// ===== 消息渠道 =====

pub async fn list_message_channels() -> Result<common::api::ListMessageChannelsResponse, ApiError> {
    api_get_or_default("/api/v1/finance/message-channels").await
}

pub async fn create_message_channel(req: common::api::CreateMessageChannelRequest) -> Result<common::api::CreateMessageChannelResponse, ApiError> {
    api_post("/api/v1/finance/message-channels", &req).await
}

pub async fn update_message_channel_status(id: &str, status: i32) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/finance/message-channels/{}/status", id), &body).await
}

pub async fn delete_message_channel(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/finance/message-channels/{}", id)).await
}

pub async fn test_message_channel(id: &str) -> Result<TestMessageChannelConnectionResponse, ApiError> {
    let body = serde_json::json!({});
    api_post(&format!("/api/v1/finance/message-channels/{}/test", id), &body).await
}

// ===== MCP 服务器 =====

pub async fn list_mcp_servers() -> Result<ListMcpServersResponse, ApiError> {
    api_get_or_default("/api/v1/finance/mcp-servers").await
}

pub async fn create_mcp_server(req: CreateMcpServerRequest) -> Result<CreateMcpServerResponse, ApiError> {
    api_post("/api/v1/finance/mcp-servers", &req).await
}

pub async fn update_mcp_server_status(id: &str, status: i32) -> Result<(), ApiError> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/finance/mcp-servers/{}/status", id), &body).await
}

pub async fn delete_mcp_server(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/finance/mcp-servers/{}", id)).await
}

pub async fn sync_mcp_tools(id: &str) -> Result<(), ApiError> {
    let body = serde_json::json!({});
    api_post_empty(&format!("/api/v1/finance/mcp-servers/{}/tools/sync", id), &body).await
}

// ===== 附件管理 =====

pub async fn list_attachments() -> Result<Vec<AttachmentDetail>, ApiError> {
    api_get_or_default("/api/v1/finance/attachments").await
}

pub async fn create_text_attachment(req: CreateTextAttachmentRequest) -> Result<AttachmentDetail, ApiError> {
    api_post("/api/v1/finance/attachments/text", &req).await
}

/// 上传文件附件（multipart/form-data）
/// 需要传入已经构造好的 FormData（含 `file` 字段和 `purpose` 字段）
pub async fn upload_attachment(form: FormData) -> Result<AttachmentDetail, ApiError> {
    api_post_multipart("/api/v1/finance/attachments/upload", form).await
}

#[allow(dead_code)]
pub async fn get_attachment(id: &str) -> Result<AttachmentDetail, ApiError> {
    api_get(&format!("/api/v1/finance/attachments/{}", id)).await
}

pub async fn delete_attachment(id: &str) -> Result<(), ApiError> {
    api_delete(&format!("/api/v1/finance/attachments/{}", id)).await
}
