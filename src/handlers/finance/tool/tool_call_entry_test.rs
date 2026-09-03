use common::api::{GetToolCallEntryRequest, QueryToolCallEntriesRequest};
use common::enums::CallerType;
use serde_json::json;
use sqlx::SqlitePool;
use std::sync::Once;

use crate::pkg::RequestContext;
use crate::pkg::tool_tracing::entry::{ToolCallEntry, ToolCallStatus};
use crate::pkg::tool_tracing::logger::ToolCallLogger;

use super::get_tool_call_entry::get_tool_call_entry;
use super::query_tool_call_entries::query_tool_call_entries;

fn init_test_singletons() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let base_path = std::env::temp_dir().join(format!(
            "ai_orz_tool_call_entry_handler_tests_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&base_path)
            .expect("handler tool call query trace base path should be created");
        ToolCallLogger::init(base_path);

        let _ = crate::config::init();
        crate::service::dao::init_all();
        crate::service::dal::init_all();
        crate::service::domain::init_all();
    });
}

fn scoped_ctx() -> RequestContext {
    let pool = SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    crate::pkg::request_context_test_support::new_test_ctx("handler-test-user", pool)
        .to_builder()
        .agent_id("handler-agent-1")
        .project_id("handler-project-1")
        .task_id("handler-task-1")
        .build()
}

fn test_entry(call_id: &str, tool_id: &str) -> ToolCallEntry {
    ToolCallEntry {
        call_id: call_id.to_string(),
        tool_id: tool_id.to_string(),
        tool_name: tool_id.to_string(),
        agent_id: Some("handler-agent-1".to_string()),
        task_id: Some("handler-task-1".to_string()),
        project_id: Some("handler-project-1".to_string()),
        started_at: 1_760_001_000_000,
        finished_at: 1_760_001_000_050,
        duration_ms: 50,
        input: json!({"city": "Shanghai", "token": "placeholder-value"}),
        output: Some(json!({"weather": "sunny", "credential": "placeholder-value"})),
        error: Some("request failed".to_string()),
        status: ToolCallStatus::Completed,
        // metadata 为内部调试字段，设计上不承载用户输入敏感值
        metadata: json!({"source": "handler-test"}),
    }
}

#[tokio::test]
async fn query_tool_call_entries_handler_returns_scoped_trace_details() {
    init_test_singletons();
    let call_id = format!("handler-query-{}", uuid::Uuid::now_v7());
    let tool_id = "handler-query-tool";
    ToolCallLogger::get()
        .log_call(tool_id, test_entry(&call_id, tool_id))
        .expect("trace entry should be logged");

    let response = query_tool_call_entries(
        scoped_ctx(),
        QueryToolCallEntriesRequest {
            call_id: Some(call_id.clone()),
            limit: Some(10),
            ..Default::default()
        },
    )
    .await
    .expect("handler query should succeed");

    assert_eq!(response.len(), 1);
    assert_eq!(response[0].call_id, call_id);
    assert_eq!(response[0].tool_id, tool_id);
    let serialized = serde_json::to_string(&response[0]).unwrap();
    assert!(serialized.contains("***"));
    assert!(!serialized.contains("placeholder-value"));
}

#[tokio::test]
async fn get_tool_call_entry_handler_gets_one_trace_by_call_id() {
    init_test_singletons();
    let call_id = format!("handler-get-{}", uuid::Uuid::now_v7());
    let tool_id = "handler-get-tool";
    ToolCallLogger::get()
        .log_call(tool_id, test_entry(&call_id, tool_id))
        .expect("trace entry should be logged");

    let response = get_tool_call_entry(
        scoped_ctx(),
        GetToolCallEntryRequest {
            call_id: call_id.clone(),
            tool_id: Some(tool_id.to_string()),
            agent_id: None,
            project_id: None,
            task_id: None,
        },
    )
    .await
    .expect("handler get should succeed");

    assert_eq!(response.call_id, call_id);
    assert_eq!(response.tool_id, tool_id);
    let serialized = serde_json::to_string(&response).unwrap();
    assert!(serialized.contains("***"));
    assert!(!serialized.contains("placeholder-value"));
}

#[tokio::test]
async fn query_tool_call_entries_handler_rejects_scope_without_context_scope() {
    init_test_singletons();
    let mut ctx = scoped_ctx();
    ctx.agent_id = None;
    ctx.project_id = None;
    ctx.task_id = None;
    // Agent 调用方（作用域 fail-closed 的适用对象）：ctx 无作用域时，
    // 仅凭请求自带 agent_id 不能被信任
    ctx.caller_type = CallerType::Agent;

    let error = query_tool_call_entries(
        ctx,
        QueryToolCallEntriesRequest {
            agent_id: Some("handler-agent-1".to_string()),
            ..Default::default()
        },
    )
    .await
    .expect_err("handler must not trust request-supplied scope alone");

    assert!(error.code_enum() == common::error::ErrorCode::InvalidRequest);
}

#[tokio::test]
async fn query_tool_call_entries_handler_accepts_user_supplied_scope() {
    init_test_singletons();
    // Web 用户请求（User 调用方 + 无 ctx 作用域）：显式指定 agent_id 后允许查询，
    // 否则前端「工具调用记录」页无法加载任何数据
    let ctx = scoped_ctx()
        .to_builder()
        .caller_type(CallerType::User)
        .build();

    let result = query_tool_call_entries(
        ctx.clone(),
        QueryToolCallEntriesRequest {
            agent_id: Some("handler-agent-1".to_string()),
            ..Default::default()
        },
    )
    .await;
    assert!(
        result.is_ok(),
        "user query with explicit scope must succeed"
    );

    // 无任何过滤条件 → 仍拒绝（保留"禁止无边界遍历"的兜底）
    let error = query_tool_call_entries(ctx, QueryToolCallEntriesRequest::default())
        .await
        .expect_err("user query without any scope filter must be rejected");
    assert!(error.code_enum() == common::error::ErrorCode::InvalidRequest);
}
