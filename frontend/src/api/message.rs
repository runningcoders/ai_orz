//! Message 域 API - 消息发送和列表查询

use common::api::{
    ListMessagesResponse, SearchMessagesResponse, SendMessageToAgentParams,
    SendMessageToAgentResponse,
};

use super::{ApiError, api_get, api_post};

pub async fn send_message_to_agent(
    req: SendMessageToAgentParams,
) -> Result<SendMessageToAgentResponse, ApiError> {
    api_post("/api/v1/finance/messages/agents", &req).await
}

pub async fn load_latest_messages(
    req: common::api::ListMessagesRequest,
) -> Result<ListMessagesResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("project_id", req.project_id.clone()),
        ("task_id", req.task_id.clone()),
        ("from_id", req.from_id.clone()),
        ("to_id", req.to_id.clone()),
        ("limit", req.limit.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/finance/messages{}", qs)).await
}

pub async fn load_older_messages(
    req: common::api::ListMessagesRequest,
) -> Result<ListMessagesResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("project_id", req.project_id.clone()),
        ("task_id", req.task_id.clone()),
        ("from_id", req.from_id.clone()),
        ("to_id", req.to_id.clone()),
        ("before_timestamp", req.before_timestamp.map(|v| v.to_string())),
        ("limit", req.limit.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/finance/messages{}", qs)).await
}

#[allow(dead_code)]
pub async fn poll_new_messages(
    req: common::api::ListMessagesRequest,
) -> Result<ListMessagesResponse, ApiError> {
    let qs = super::build_query_string(&[
        ("project_id", req.project_id.clone()),
        ("task_id", req.task_id.clone()),
        ("from_id", req.from_id.clone()),
        ("to_id", req.to_id.clone()),
        ("after_timestamp", req.after_timestamp.map(|v| v.to_string())),
        ("limit", req.limit.map(|v| v.to_string())),
    ]);
    api_get(&format!("/api/v1/finance/messages{}", qs)).await
}

pub async fn search_messages(
    req: common::api::SearchMessagesRequest,
) -> Result<SearchMessagesResponse, ApiError> {
    api_post("/api/v1/finance/messages/search", &req).await
}
