//! Finance 域 API - 模型提供商、工具、附件、MCP 服务器、消息渠道

use common::api::{
    CreateModelProviderRequest, CreateModelProviderResponse, CreateToolRequest,
    CreateToolResponse, DeleteModelProviderResponse, DeleteToolResponse,
    GetModelProviderResponse, GetToolResponse, ListModelProvidersResponse, ListToolsResponse,
    TestConnectionResponse, UpdateModelProviderRequest, UpdateModelProviderResponse,
    UpdateToolRequest, UpdateToolResponse,
};

use super::{api_delete, api_get, api_get_or_default, api_post, api_post_empty, api_put, api_put_empty};

// ===== 模型提供商 =====

pub async fn list_model_providers() -> Result<ListModelProvidersResponse, String> {
    api_get_or_default("/api/v1/finance/model-providers").await
}

pub async fn get_model_provider(id: &str) -> Result<GetModelProviderResponse, String> {
    api_get(&format!("/api/v1/finance/model-providers/{}", id)).await
}

pub async fn create_model_provider(req: CreateModelProviderRequest) -> Result<CreateModelProviderResponse, String> {
    api_post("/api/v1/finance/model-providers", &req).await
}

pub async fn update_model_provider(id: &str, req: UpdateModelProviderRequest) -> Result<UpdateModelProviderResponse, String> {
    api_put(&format!("/api/v1/finance/model-providers/{}", id), &req).await
}

pub async fn delete_model_provider(id: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/finance/model-providers/{}", id)).await
}

pub async fn test_model_provider_connection(id: &str) -> Result<TestConnectionResponse, String> {
    let body = serde_json::json!({});
    api_post(&format!("/api/v1/finance/model-providers/{}/test", id), &body).await
}

// ===== 工具管理 =====

pub async fn list_tools() -> Result<ListToolsResponse, String> {
    api_get_or_default("/api/v1/finance/tools").await
}

pub async fn get_tool(id: &str) -> Result<GetToolResponse, String> {
    api_get(&format!("/api/v1/finance/tools/{}", id)).await
}

pub async fn create_tool(req: CreateToolRequest) -> Result<CreateToolResponse, String> {
    api_post("/api/v1/finance/tools", &req).await
}

pub async fn update_tool(id: &str, req: UpdateToolRequest) -> Result<UpdateToolResponse, String> {
    api_put(&format!("/api/v1/finance/tools/{}", id), &req).await
}

pub async fn update_tool_status(id: &str, status: i32) -> Result<(), String> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/finance/tools/{}/status", id), &body).await
}

pub async fn delete_tool(id: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/finance/tools/{}", id)).await
}

// ===== 消息渠道 =====

pub async fn list_message_channels() -> Result<common::api::ListMessageChannelsResponse, String> {
    api_get_or_default("/api/v1/finance/message-channels").await
}

pub async fn create_message_channel(req: common::api::CreateMessageChannelRequest) -> Result<common::api::CreateMessageChannelResponse, String> {
    api_post("/api/v1/finance/message-channels", &req).await
}

pub async fn update_message_channel_status(id: &str, status: i32) -> Result<(), String> {
    let body = serde_json::json!({ "status": status });
    api_put_empty(&format!("/api/v1/finance/message-channels/{}/status", id), &body).await
}

pub async fn delete_message_channel(id: &str) -> Result<(), String> {
    api_delete(&format!("/api/v1/finance/message-channels/{}", id)).await
}
