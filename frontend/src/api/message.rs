//! Message 域 API - 消息发送和列表查询

use common::api::{
    ListMessagesResponse, SearchMessagesRequest, SearchMessagesResponse, SendMessageToAgentParams, SendMessageToAgentResponse,
};

use super::{api_get, api_post};

pub async fn send_message_to_agent(
    req: SendMessageToAgentParams,
) -> Result<SendMessageToAgentResponse, String> {
    api_post("/api/v1/finance/messages/agents", &req).await
}

pub async fn load_latest_messages(
    project_id: Option<&str>,
    limit: Option<usize>,
) -> Result<ListMessagesResponse, String> {
    let mut params = Vec::new();
    if let Some(pid) = project_id {
        params.push(format!("project_id={}", url_encode(pid)));
    }
    if let Some(l) = limit {
        params.push(format!("limit={}", l));
    }
    let query = if params.is_empty() { String::new() } else { format!("?{}", params.join("&")) };
    api_get(&format!("/api/v1/finance/messages{}", query)).await
}

pub async fn load_older_messages(
    project_id: Option<&str>,
    before_timestamp: i64,
    limit: Option<usize>,
) -> Result<ListMessagesResponse, String> {
    let mut params = vec![format!("before_timestamp={}", before_timestamp)];
    if let Some(pid) = project_id {
        params.push(format!("project_id={}", url_encode(pid)));
    }
    if let Some(l) = limit {
        params.push(format!("limit={}", l));
    }
    api_get(&format!("/api/v1/finance/messages?{}", params.join("&"))).await
}

#[allow(dead_code)]
pub async fn poll_new_messages(
    project_id: Option<&str>,
    after_timestamp: i64,
) -> Result<ListMessagesResponse, String> {
    let mut params = vec![format!("after_timestamp={}", after_timestamp)];
    if let Some(pid) = project_id {
        params.push(format!("project_id={}", url_encode(pid)));
    }
    api_get(&format!("/api/v1/finance/messages?{}", params.join("&"))).await
}

pub async fn search_messages(keyword: &str, project_id: Option<&str>) -> Result<SearchMessagesResponse, String> {
    let params = SearchMessagesRequest {
        keyword: if keyword.is_empty() { None } else { Some(keyword.to_string()) },
        project_id: project_id.map(|s| s.to_string()),
        task_id: None,
        from_id: None,
        to_id: None,
        limit: Some(20),
    };
    api_post("/api/v1/finance/messages/search", &params).await
}

fn url_encode(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('+', "%2B")
        .replace('#', "%23")
        .replace('?', "%3F")
}