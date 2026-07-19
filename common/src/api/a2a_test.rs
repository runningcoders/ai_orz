//! A2A 协议实体测试

use super::a2a::*;
use serde_json::json;

#[test]
fn agent_card_serializes_correctly() {
    let card = AgentCard {
        name: "Test Org".to_string(),
        description: Some("test".to_string()),
        version: "0.3.0".to_string(),
        url: "http://localhost/a2a".to_string(),
        capabilities: AgentCapabilities {
            streaming: true,
            push_notifications: false,
        },
        skills: vec![AgentSkill {
            id: "chat".to_string(),
            name: "对话".to_string(),
            description: None,
            tags: vec![],
        }],
        default_input_modes: vec!["text".to_string()],
        default_output_modes: vec!["text".to_string()],
    };

    let json = serde_json::to_value(&card).unwrap();
    assert_eq!(json["name"], "Test Org");
    assert_eq!(json["version"], "0.3.0");
    assert_eq!(json["capabilities"]["streaming"], true);
    assert_eq!(json["skills"][0]["id"], "chat");
}

#[test]
fn jsonrpc_response_success_serializes_correctly() {
    let resp = JsonRpcResponse::success(json!(1), json!({"id": "task-1"}));
    let json_val = serde_json::to_value(&resp).unwrap();
    assert_eq!(json_val["jsonrpc"], "2.0");
    assert_eq!(json_val["id"], 1);
    assert_eq!(json_val["result"]["id"], "task-1");
    assert!(json_val.get("error").is_none() || json_val["error"].is_null());
}

#[test]
fn jsonrpc_response_error_serializes_correctly() {
    let resp = JsonRpcResponse::error(json!(1), -32601, "Method not found".to_string());
    let json_val = serde_json::to_value(&resp).unwrap();
    assert_eq!(json_val["jsonrpc"], "2.0");
    assert_eq!(json_val["error"]["code"], -32601);
    assert_eq!(json_val["error"]["message"], "Method not found");
}

#[test]
fn a2a_message_part_text_tagged_correctly() {
    let part = A2aMessagePart::Text {
        text: "hello".to_string(),
    };
    let json_val = serde_json::to_value(&part).unwrap();
    assert_eq!(json_val["type"], "text");
    assert_eq!(json_val["text"], "hello");
}

#[test]
fn a2a_task_state_serializes_snake_case() {
    let state = A2aTaskState::InputRequired;
    let json_val = serde_json::to_value(&state).unwrap();
    assert_eq!(json_val, "input_required");
}

#[test]
fn send_task_params_deserializes_correctly() {
    let json_str = r#"{
        "id": "client-task-1",
        "message": {
            "role": "user",
            "parts": [{"type": "text", "text": "你好"}]
        }
    }"#;
    let params: SendTaskParams = serde_json::from_str(json_str).unwrap();
    assert_eq!(params.id, "client-task-1");
    assert_eq!(params.message.role, "user");
    assert_eq!(params.message.parts.len(), 1);
    assert!(params.session_id.is_none());
}
