use super::{Tool, ToolPo};
use common::enums::{ToolProtocol, ToolStatus};

fn test_tool_with_status(status: ToolStatus) -> Tool {
    let mut po = ToolPo::new(
        "tool-status-test".to_string(),
        "tool-status-test".to_string(),
        "Tool status transition test".to_string(),
        ToolProtocol::Http,
        serde_json::json!({"endpoint":"https://example.com/tool"}),
        Some(serde_json::json!({"type":"object"})),
        vec!["test".to_string()],
        Some("creator".to_string()),
    );
    po.status = status;

    Tool::from_po_for_management(po)
}

#[test]
fn enabled_tool_can_transition_to_enabled_or_disabled() {
    let tool = test_tool_with_status(ToolStatus::Enabled);

    assert_eq!(
        tool.available_statuses(),
        vec![ToolStatus::Enabled, ToolStatus::Disabled]
    );
    assert!(tool.can_transition_to(ToolStatus::Enabled));
    assert!(tool.can_transition_to(ToolStatus::Disabled));
}

#[test]
fn disabled_tool_can_transition_to_disabled_or_enabled() {
    let tool = test_tool_with_status(ToolStatus::Disabled);

    assert_eq!(
        tool.available_statuses(),
        vec![ToolStatus::Disabled, ToolStatus::Enabled]
    );
    assert!(tool.can_transition_to(ToolStatus::Disabled));
    assert!(tool.can_transition_to(ToolStatus::Enabled));
}

#[test]
fn stale_tool_is_sync_owned_and_cannot_be_manually_enabled_or_disabled() {
    let mut tool = test_tool_with_status(ToolStatus::Stale);

    assert_eq!(tool.available_statuses(), vec![ToolStatus::Stale]);
    assert!(tool.can_transition_to(ToolStatus::Stale));
    assert!(!tool.can_transition_to(ToolStatus::Enabled));
    assert!(!tool.can_transition_to(ToolStatus::Disabled));

    let enabled_err = tool
        .transition_status(ToolStatus::Enabled, "editor")
        .expect_err("stale tool cannot be manually enabled; MCP sync must restore it");
    assert!(enabled_err.msg.contains("cannot transition"));
    assert_eq!(tool.po.status, ToolStatus::Stale);

    let disabled_err = tool
        .transition_status(ToolStatus::Disabled, "editor")
        .expect_err("stale tool cannot be manually disabled");
    assert!(disabled_err.msg.contains("cannot transition"));
    assert_eq!(tool.po.status, ToolStatus::Stale);
}

#[test]
fn transition_status_updates_status_modifier_and_timestamp() {
    let mut tool = test_tool_with_status(ToolStatus::Enabled);
    let old_updated_at = tool.po.updated_at;

    tool.transition_status(ToolStatus::Disabled, "editor")
        .expect("transition to disabled should succeed");

    assert_eq!(tool.po.status, ToolStatus::Disabled);
    assert_eq!(tool.po.updated_by.as_deref(), Some("editor"));
    assert!(tool.po.updated_at >= old_updated_at);
}

// 注：原 to_tool_prompt 相关测试已删除
// 工具列表不再注入 Prompt，统一通过 OpenAI tools API 协议层传递，
// 由 CortexDao.think 在调用模型时序列化为 function calling 协议。
