//! tests 单元测试（拆分自 doubao_search.rs）
//!
//! 文件瘦身：原 794 行 → 613 行，测试体 182 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::pkg::request_context_test_support::new_test_ctx;

fn test_ctx() -> RequestContext {
    let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").unwrap();
    new_test_ctx("test-user", pool)
}

#[test]
fn configured_timeout_reads_search_tool_config() {
    // config 为空 → 内置缺省
    assert_eq!(configured_timeout_ms(&Value::Null), DEFAULT_TIMEOUT_MS);
    // 显式 timeout_ms → 生效（键名与 common::config::SearchToolConfig 同源）
    assert_eq!(
        configured_timeout_ms(&json!({ "timeout_ms": 1_234 })),
        1_234
    );
    // 形状不符（非数字）→ 回退缺省，不 panic
    assert_eq!(
        configured_timeout_ms(&json!({ "timeout_ms": "abc" })),
        DEFAULT_TIMEOUT_MS
    );
}

#[test]
fn factory_po_metadata() {
    let po = DoubaoSearchToolFactory.create_po();
    assert_eq!(po.id, "doubao_search");
    assert_eq!(po.control_mode, ControlMode::Auto);
    assert_eq!(po.protocol, ToolProtocol::Builtin);
    assert_eq!(po.get_tags(), vec!["search", "network"]);
}

#[test]
fn snippet_truncation() {
    let short = "hello world";
    let (out, truncated) = truncate_snippet(short);
    assert_eq!(out, short);
    assert!(!truncated);

    let long = "x".repeat(SNIPPET_MAX_CHARS + 10);
    let (out, truncated) = truncate_snippet(&long);
    assert!(truncated);
    assert!(out.ends_with("..."));
    assert!(out.chars().count() <= SNIPPET_MAX_CHARS + 3);
}

#[test]
fn time_range_mapping() {
    assert_eq!(map_time_range("day").unwrap().range, "OneDay");
    assert_eq!(map_time_range("week").unwrap().range, "OneWeek");
    assert_eq!(map_time_range("month").unwrap().range, "OneMonth");
    assert_eq!(map_time_range("year").unwrap().range, "OneYear");
    assert!(map_time_range("invalid").is_none());
}

#[test]
fn credential_requirements_use_generic_token_with_platform() {
    let reqs = credential_requirements();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].kind, CredentialKind::GenericToken);
    assert_eq!(reqs[0].platform.as_deref(), Some(PLATFORM));
    assert!(reqs[0].field.is_none());
    assert!(reqs[0].enhancer.is_none());
    match &reqs[0].binding {
        CredentialBinding::Internal { field } => assert_eq!(field, "api_key"),
        other => panic!("expected Internal binding, got {:?}", other),
    }
}

#[tokio::test]
async fn call_with_empty_query_returns_error_json() {
    let tool = DoubaoSearchCoreTool::new(DoubaoSearchToolFactory.create_po());
    let result = tool
        .call(test_ctx(), json!({ "query": "  " }))
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("query"));
}

#[tokio::test]
async fn call_with_invalid_search_type_returns_error_json() {
    let tool = DoubaoSearchCoreTool::new(DoubaoSearchToolFactory.create_po());
    let result = tool
        .call(
            test_ctx(),
            json!({ "query": "rust", "search_type": "image" }),
        )
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("search_type"));
}

#[tokio::test]
async fn call_with_invalid_time_range_returns_error_json() {
    let tool = DoubaoSearchCoreTool::new(DoubaoSearchToolFactory.create_po());
    let result = tool
        .call(test_ctx(), json!({ "query": "rust", "time_range": "hour" }))
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert!(result["error"].as_str().unwrap().contains("time_range"));
}

#[tokio::test]
async fn call_without_check_returns_api_key_missing_guidance() {
    let tool = DoubaoSearchCoreTool::new(DoubaoSearchToolFactory.create_po());
    let result = tool
        .call(test_ctx(), json!({ "query": "rust" }))
        .await
        .unwrap();
    assert_eq!(result["success"], false);
    assert_eq!(result["error_code"], "api_key_missing");
    let guidance = result["guidance"].as_str().unwrap();
    assert!(
        guidance.contains("doubao_search"),
        "guidance should mention platform"
    );
}

#[test]
fn check_injects_api_key_from_resolved_requirement() {
    let mut tool = DoubaoSearchCoreTool::new(DoubaoSearchToolFactory.create_po());
    assert_eq!(tool.api_key, None);
    let resolved = vec![crate::pkg::credential::ResolvedRequirement {
        requirement: credential_requirements().pop().unwrap(),
        value: "doubao-test-key".to_string(),
    }];
    tool.check(&resolved).unwrap();
    assert_eq!(tool.api_key.as_deref(), Some("doubao-test-key"));
}

#[test]
fn factory_and_instance_requirements_are_consistent() {
    let tool = DoubaoSearchCoreTool::new(DoubaoSearchToolFactory.create_po());
    assert_eq!(
        DoubaoSearchToolFactory.credential_requirements(),
        tool.credential_requirements()
    );
    assert_eq!(
        tool.credential_requirements()[0].kind,
        CredentialKind::GenericToken
    );
    assert_eq!(
        tool.credential_requirements()[0].platform.as_deref(),
        Some(PLATFORM)
    );
}

#[test]
fn parse_sse_response_accumulates_summary() {
    let sse = concat!(
        "data: {\"Result\":{\"WebResults\":[{\"Title\":\"Rust\",\"URL\":\"https://rust-lang.org\",\"Summary\":\"Rust lang\"}],\"Summary\":\"Rust is\"}}\n",
        "data: {\"Result\":{\"Summary\":\" a systems language.\"}}\n",
        "data: {\"Result\":{\"SearchUsage\":{\"TokenUsage\":42,\"Count\":1}}}\n",
        "data: [DONE]\n",
    );
    let result = parse_sse_response(sse);
    assert_eq!(result.web_results.len(), 1);
    assert_eq!(result.web_results[0].title, "Rust");
    assert_eq!(result.summary, "Rust is a systems language.");
    assert_eq!(result.search_usage.unwrap().token_usage, 42);
}

#[test]
fn parse_json_response_extracts_results() {
    let json = r#"{
        "Result": {
            "WebResults": [
                {"Title": "Test", "URL": "https://example.com", "SiteName": "Example", "Summary": "A test result", "PublishTime": "2026-08-24"}
            ],
            "SearchUsage": {"Count": 1, "TokenUsage": 10}
        }
    }"#;
    let result = parse_response(json, "application/json");
    assert_eq!(result.web_results.len(), 1);
    assert_eq!(result.web_results[0].title, "Test");
    assert_eq!(result.web_results[0].site_name, "Example");
    assert_eq!(result.search_usage.unwrap().count, 1);
}
