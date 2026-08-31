//! Agent API DTO contract tests.

use super::{ApiResponse, GetAgentResponse, UpdateAgentStatusRequest, UpdateAgentStatusResponse};
use crate::api::agent::ListAgentsRequest;
use crate::enums::AgentStatus;

/// 回归测试：分页 GET 参数应通过 `RawQuery -> serde_json::Value`（宏的 (false,true)/(true,true)
/// GET 路径）解析，而不是 axum 的 `Query`（底层 serde_urlencoded，不支持 `#[serde(flatten)]`，
/// 会在 `limit=500` 时报 `invalid type: string "500", expected usize`）。
/// 这里直接验证被拍平的 `PaginationParams` 能从带类型推断的 serde_json::Value 正确解析。
#[test]
fn query_flatten_limit_via_json_value() {
    // 模拟宏生成的 RawQuery 解析：把 "500" 识别为数字而非字符串
    let mut obj = serde_json::Map::new();
    obj.insert("limit".into(), serde_json::json!(500));
    let v = serde_json::Value::Object(obj);
    let req: ListAgentsRequest = serde_json::from_value(v).unwrap();
    assert_eq!(req.pagination.limit, Some(500));
}

#[test]
fn update_agent_status_request_serializes_status_enum() {
    let request = UpdateAgentStatusRequest {
        id: "agent-1".to_string(),
        status: AgentStatus::PendingOnboard,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert_eq!(json, r#"{"id":"agent-1","status":"PendingOnboard"}"#);

    let decoded: UpdateAgentStatusRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.id, "agent-1");
    assert_eq!(decoded.status, AgentStatus::PendingOnboard);
}

#[test]
fn update_agent_status_response_uses_agent_detail_contract() {
    let response: ApiResponse<UpdateAgentStatusResponse> = ApiResponse::success(GetAgentResponse {
        id: "agent-1".to_string(),
        name: "LifecycleAgent".to_string(),
        roles: vec!["worker".to_string()],
        description: Some("A lifecycle test agent".to_string()),
        capabilities: Some(vec!["coding".to_string()]),
        soul: Some("Be helpful".to_string()),
        kind: "local".to_string(),
        model_provider_id: "provider-1".to_string(),
        external_config: None,
        runtime_config: None,
        status: AgentStatus::PendingOnboard as i32,
        created_at: 1,
        updated_at: 2,
        runtime_state: 0,
        current_message_id: None,
        current_task_id: None,
        current_project_id: None,
        tools: vec![],
        stats: None,
        model_call_stats: None,
        tools_overview: None,
        skills_overview: None,
    });

    assert!(response.is_success());
    let data = response.data.expect("response should contain agent detail");
    assert_eq!(data.id, "agent-1");
    assert_eq!(data.status, AgentStatus::PendingOnboard as i32);
}
