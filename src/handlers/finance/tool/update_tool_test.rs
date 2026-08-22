use async_trait::async_trait;
use common::api::UpdateToolRequest;
use common::enums::{ToolProtocol, ToolStatus};
use common::error::Result;
use serde_json::{Value, json};
use sqlx::SqlitePool;

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::tool_registry::{BuiltinToolFactory, get_registry};
use crate::service::dao::tool;

use super::update_tool::update_tool;

fn init_test_singletons() {
    // ToolCallLogger 必须先于 domain::init_all() 初始化
    // （RuntimeDomainImpl::new 构造时取 logger 单例，未初始化会 panic，
    // 参照 mcp_tool_handler_test 的初始化顺序）
    crate::pkg::request_context_test_support::ensure_test_tool_call_logger();
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

// ==================== Builtin 工具 config 更新管道（D28 Step 1b） ====================

/// 测试专用 Builtin 工厂 ID（独立 ID 避免与其他测试的 registry 注册竞争；
/// 注册后不 unregister——本文件两个测试并行运行共享 registry，中途
/// unregister 会与另一测试的 lookup 竞争，唯一 ID 泄漏对其他测试无影响）
const BUILTIN_TEST_TOOL_ID: &str = "builtin-update-test-tool";

fn builtin_test_tool_po() -> ToolPo {
    ToolPo::new(
        BUILTIN_TEST_TOOL_ID.to_string(),
        "builtin-update-test-tool".to_string(),
        "test builtin tool".to_string(),
        ToolProtocol::Builtin,
        json!({ "command": "echo" }),
        None,
        vec![],
        Some("creator".to_string()),
    )
}

#[derive(Clone)]
struct BuiltinUpdateTestFactory;

impl BuiltinToolFactory for BuiltinUpdateTestFactory {
    fn create_po(&self) -> ToolPo {
        builtin_test_tool_po()
    }
    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(BuiltinUpdateTestTool { po })
    }
}

#[derive(Clone)]
struct BuiltinUpdateTestTool {
    po: ToolPo,
}

#[async_trait]
impl CoreTool for BuiltinUpdateTestTool {
    fn po(&self) -> &ToolPo {
        &self.po
    }

    async fn call(&self, _ctx: crate::pkg::RequestContext, _args: Value) -> Result<Value> {
        Ok(Value::Null)
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn update_tool_builtin_config_only_update_succeeds(pool: SqlitePool) {
    init_test_singletons();
    get_registry().register_builtin_factory(Box::new(BuiltinUpdateTestFactory));
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("editor", pool);
    let po = builtin_test_tool_po();
    tool::new_tool_dao()
        .create_tool(ctx.clone(), &po)
        .await
        .expect("builtin test tool should be inserted");

    let response = update_tool(
        ctx.clone(),
        UpdateToolRequest {
            id: BUILTIN_TEST_TOOL_ID.to_string(),
            config: Some(json!({
                "command": "/usr/local/bin/gh",
                "timeout_ms": 30_000,
            })),
            ..Default::default()
        },
    )
    .await
    .expect("builtin config-only update should succeed");

    // 响应：config 生效，工厂所有权字段保持原样
    assert_eq!(response.name, po.name);
    assert_eq!(response.protocol, ToolProtocol::Builtin);
    assert_eq!(
        response.config,
        Some(json!({
            "command": "/usr/local/bin/gh",
            "timeout_ms": 30_000,
        }))
    );

    // 持久层：config 更新 + updated_by 记录，工厂所有权字段未被覆盖
    let persisted = tool::new_tool_dao()
        .get_by_id(ctx, BUILTIN_TEST_TOOL_ID.to_string())
        .await
        .expect("tool lookup should succeed")
        .expect("tool should still exist");
    assert_eq!(persisted.name, po.name);
    assert_eq!(persisted.description, po.description);
    assert_eq!(
        persisted.config,
        json!({
            "command": "/usr/local/bin/gh",
            "timeout_ms": 30_000,
        })
    );
    assert_eq!(persisted.updated_by.as_deref(), Some("editor"));
}

#[sqlx::test(migrations = "./migrations")]
async fn update_tool_builtin_factory_field_edits_rejected(pool: SqlitePool) {
    init_test_singletons();
    get_registry().register_builtin_factory(Box::new(BuiltinUpdateTestFactory));
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("editor", pool);
    let po = builtin_test_tool_po();
    tool::new_tool_dao()
        .create_tool(ctx.clone(), &po)
        .await
        .expect("builtin test tool should be inserted");

    // 工厂所有权字段（name）修改被拒绝
    let err = update_tool(
        ctx.clone(),
        UpdateToolRequest {
            id: BUILTIN_TEST_TOOL_ID.to_string(),
            name: Some("should-not-work".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect_err("builtin factory field edit must be rejected");
    assert_eq!(err.code_enum(), common::error::ErrorCode::InvalidRequest);
    assert!(err.to_string().contains("内置工具仅支持修改 config"));

    // 启停别名 enabled 同样走 update_tool 时被拒绝（启停专用通道为 update_tool_status）
    let err = update_tool(
        ctx.clone(),
        UpdateToolRequest {
            id: BUILTIN_TEST_TOOL_ID.to_string(),
            enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .expect_err("builtin enabled alias edit must be rejected");
    assert_eq!(err.code_enum(), common::error::ErrorCode::InvalidRequest);

    // 持久层未被改动
    let persisted = tool::new_tool_dao()
        .get_by_id(ctx, BUILTIN_TEST_TOOL_ID.to_string())
        .await
        .expect("tool lookup should succeed")
        .expect("tool should still exist");
    assert_eq!(persisted.name, po.name);
    assert_eq!(persisted.config, json!({ "command": "echo" }));
    assert_eq!(persisted.status, ToolStatus::Enabled);
    assert_eq!(persisted.updated_by.as_deref(), Some("creator"));
}
