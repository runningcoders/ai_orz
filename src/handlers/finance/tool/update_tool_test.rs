use common::api::UpdateToolRequest;
use common::enums::{ToolProtocol, ToolStatus};
use serde_json::json;
use sqlx::SqlitePool;

use common::error::Result;
use crate::models::tool::ToolPo;
use crate::pkg::RequestContext;
use crate::service::dao::tool;

use super::update_tool::update_tool;

fn init_test_singletons() {
    let _ = crate::config::init();
    crate::service::dao::init_all();
    crate::service::dal::init_all();
    crate::service::domain::init_all();
}

fn test_mcp_tool_po(id: &str, status: ToolStatus) -> ToolPo {
    let mut po = ToolPo::new(
        id.to_string(),
        id.to_string(),
        "test MCP tool".to_string(),
        ToolProtocol::Mcp,
        json!({
            "server_id": "test-server",
            "tool_name": "test-tool",
        }),
        Some(json!({"type": "object", "properties": {}})),
        vec!["mcp".to_string()],
        Some("creator".to_string()),
    );
    po.status = status;
    po
}

#[sqlx::test(migrations = "./migrations")]
async fn update_tool_enabled_cannot_manually_restore_stale_tool(pool: SqlitePool) {
    init_test_singletons();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("editor", pool);
    let stale_tool = test_mcp_tool_po("stale-mcp-tool", ToolStatus::Stale);
    tool::new_tool_dao()
        .create_tool(ctx.clone(), &stale_tool)
        .await
        .expect("stale tool should be inserted");

    let err = update_tool(
        ctx.clone(),
        UpdateToolRequest {
            id: stale_tool.id.clone(),
            enabled: Some(true),
            ..Default::default()
        },
    )
    .await
    .expect_err("generic update must not restore Stale tools");

    assert!(err.code_enum() == common::error::ErrorCode::InvalidRequest);
    let persisted = tool::new_tool_dao()
        .get_by_id(ctx, stale_tool.id)
        .await
        .expect("tool lookup should succeed")
        .expect("tool should still exist");
    assert_eq!(persisted.status, ToolStatus::Stale);
}

#[sqlx::test(migrations = "./migrations")]
async fn update_tool_enabled_can_toggle_non_stale_tool_through_status_machine(pool: SqlitePool) {
    init_test_singletons();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("editor", pool);
    let enabled_tool = test_mcp_tool_po("enabled-mcp-tool", ToolStatus::Enabled);
    tool::new_tool_dao()
        .create_tool(ctx.clone(), &enabled_tool)
        .await
        .expect("enabled tool should be inserted");

    let response = update_tool(
        ctx.clone(),
        UpdateToolRequest {
            id: enabled_tool.id.clone(),
            enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect("non-stale enabled alias should still support normal toggling");

    assert_eq!(response.status, ToolStatus::Disabled);
    let persisted = tool::new_tool_dao()
        .get_by_id(ctx, enabled_tool.id)
        .await
        .expect("tool lookup should succeed")
        .expect("tool should still exist");
    assert_eq!(persisted.status, ToolStatus::Disabled);
    assert_eq!(persisted.updated_by.as_deref(), Some("editor"));
}
