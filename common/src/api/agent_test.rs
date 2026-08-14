//! Agent API DTO contract tests.

use super::{ApiResponse, GetAgentResponse, UpdateAgentStatusRequest, UpdateAgentStatusResponse};
use crate::enums::AgentStatus;

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
        tools: vec![],
        stats: None,
        model_call_stats: None,
    });

    assert!(response.is_success());
    let data = response.data.expect("response should contain agent detail");
    assert_eq!(data.id, "agent-1");
    assert_eq!(data.status, AgentStatus::PendingOnboard as i32);
}
