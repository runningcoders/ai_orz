//! Unit tests for tool call tracing module

use crate::models::tool::{CoreTool, ToolPo};
use crate::pkg::RequestContext;
use async_trait::async_trait;
use common::enums::{ToolProtocol, ToolStatus};
use common::error::{Result, err};
use serde_json::{Value, json};
use sqlx::sqlite::SqlitePoolOptions;
use std::{fs, process, sync::Once};
use tempfile::tempdir;

use super::entry::{ToolCallEntry, ToolCallStatus};
use super::logger::ToolCallLogger;
use super::tool_call_logger::LoggingDecorator;

#[test]
fn test_logger_creates_correct_directory_structure() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base_path.clone());

    // Get writer for a tool - writer creates directory on first write
    let tool_id = "test-tool-123";
    let writer = logger.writer_for_tool(tool_id);

    // Do an empty write to create directory
    let _ = writer.append(&json!({}));

    // Verify directory structure is created: {base}/tools/{tool_id}/call_trace/
    let expected_dir = base_path.join("tools").join(tool_id).join("call_trace");
    assert!(expected_dir.exists());
    assert!(expected_dir.is_dir());
}

#[test]
fn test_log_and_read_entry_roundtrip() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base_path);

    let tool_id = "test-tool-roundtrip";

    // Create a test entry
    let entry = ToolCallEntry {
        call_id: "test-call-1".to_string(),
        tool_id: tool_id.to_string(),
        tool_name: "Test Tool".to_string(),
        agent_id: Some("agent-456".to_string()),
        task_id: Some("task-789".to_string()),
        project_id: None,
        started_at: 1744000000000,
        finished_at: 1744000001000,
        duration_ms: 1000,
        input: json!({ "param1": "value1", "param2": 42 }),
        output: Some(json!({ "result": "success" })),
        error: None,
        status: ToolCallStatus::Completed,
        metadata: json!({ "source": "unit_test" }),
    };

    // Log the entry
    let result = logger.log_call(tool_id, entry.clone());
    assert!(result.is_ok(), "Logging should succeed: {:?}", result);

    let (date, line_number) = result.unwrap();

    // Read it back
    let read_result = logger.read_call(tool_id, &date, line_number);
    assert!(
        read_result.is_ok(),
        "Reading should succeed: {:?}",
        read_result
    );

    let read_entry = read_result.unwrap();

    // Verify all fields match
    assert_eq!(read_entry.call_id, entry.call_id);
    assert_eq!(read_entry.tool_id, entry.tool_id);
    assert_eq!(read_entry.tool_name, entry.tool_name);
    assert_eq!(read_entry.agent_id, entry.agent_id);
    assert_eq!(read_entry.task_id, entry.task_id);
    assert_eq!(read_entry.project_id, entry.project_id);
    assert_eq!(read_entry.started_at, entry.started_at);
    assert_eq!(read_entry.finished_at, entry.finished_at);
    assert_eq!(read_entry.duration_ms, entry.duration_ms);
    assert_eq!(read_entry.input, entry.input);
    assert_eq!(read_entry.output, entry.output);
    assert_eq!(read_entry.error, entry.error);
    assert_eq!(read_entry.status, entry.status);
    assert_eq!(read_entry.metadata, entry.metadata);
}

#[test]
fn query_calls_filters_and_returns_latest_matching_entry_by_default() {
    let temp_dir = tempdir().unwrap();
    let logger = ToolCallLogger::new(temp_dir.path().to_path_buf());

    let old_entry = ToolCallEntry {
        call_id: "call-old".to_string(),
        tool_id: "tool-a".to_string(),
        tool_name: "Tool A".to_string(),
        agent_id: Some("agent-1".to_string()),
        task_id: Some("task-1".to_string()),
        project_id: Some("project-1".to_string()),
        started_at: 1000,
        finished_at: 1100,
        duration_ms: 100,
        input: json!({"index": 1}),
        output: Some(json!({"ok": true})),
        error: None,
        status: ToolCallStatus::Completed,
        metadata: json!(null),
    };
    let latest_entry = ToolCallEntry {
        call_id: "call-latest".to_string(),
        started_at: 3000,
        finished_at: 3100,
        ..old_entry.clone()
    };
    let other_agent_entry = ToolCallEntry {
        call_id: "call-other-agent".to_string(),
        agent_id: Some("agent-2".to_string()),
        started_at: 5000,
        finished_at: 5100,
        ..old_entry.clone()
    };

    logger.log_call("tool-a", old_entry).unwrap();
    logger.log_call("tool-a", latest_entry.clone()).unwrap();
    logger.log_call("tool-a", other_agent_entry).unwrap();

    let results = logger
        .query_calls(super::logger::ToolCallQuery {
            agent_id: Some("agent-1".to_string()),
            ..Default::default()
        })
        .expect("query should succeed");

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].call_id, latest_entry.call_id);
}

#[test]
fn query_calls_supports_call_id_and_cross_tool_filters() {
    let temp_dir = tempdir().unwrap();
    let logger = ToolCallLogger::new(temp_dir.path().to_path_buf());

    let entry_a = ToolCallEntry {
        call_id: "shared-filter-call-a".to_string(),
        tool_id: "tool-a".to_string(),
        tool_name: "Tool A".to_string(),
        agent_id: Some("agent-1".to_string()),
        task_id: Some("task-a".to_string()),
        project_id: Some("project-1".to_string()),
        started_at: 1000,
        finished_at: 1100,
        duration_ms: 100,
        input: json!({"tool": "a"}),
        output: Some(json!({"ok": true})),
        error: None,
        status: ToolCallStatus::Completed,
        metadata: json!(null),
    };
    let entry_b = ToolCallEntry {
        call_id: "target-call".to_string(),
        tool_id: "tool-b".to_string(),
        tool_name: "Tool B".to_string(),
        task_id: Some("task-b".to_string()),
        status: ToolCallStatus::Failed,
        started_at: 2000,
        finished_at: 2100,
        input: json!({"tool": "b"}),
        output: None,
        error: Some("failed".to_string()),
        ..entry_a.clone()
    };

    logger.log_call("tool-a", entry_a).unwrap();
    logger.log_call("tool-b", entry_b.clone()).unwrap();

    let by_call_id = logger
        .read_call_by_id(None, "target-call")
        .expect("read by call id should succeed")
        .expect("target call should exist");
    assert_eq!(by_call_id.tool_id, "tool-b");

    let failed_for_project = logger
        .query_calls(super::logger::ToolCallQuery {
            project_id: Some("project-1".to_string()),
            status: Some(ToolCallStatus::Failed),
            limit: Some(10),
            ..Default::default()
        })
        .expect("query should succeed");

    assert_eq!(failed_for_project.len(), 1);
    assert_eq!(failed_for_project[0].call_id, entry_b.call_id);
}

#[test]
fn test_multiple_entries_append_correctly() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base_path);

    let tool_id = "test-tool-multiple";

    // Append multiple entries
    let mut line_numbers = Vec::new();
    let mut dates = Vec::new();
    let mut entries = Vec::new();

    for i in 0..5 {
        let entry = ToolCallEntry {
            call_id: format!("call-{}", i),
            tool_id: tool_id.to_string(),
            tool_name: "Multiple Test".to_string(),
            agent_id: None,
            task_id: None,
            project_id: None,
            started_at: 1744000000000 + (i as u64 * 1000),
            finished_at: 1744000000000 + (i as u64 * 1000) + 500,
            duration_ms: 500,
            input: json!({ "index": i }),
            output: Some(json!({ "index_squared": i * i })),
            error: None,
            status: ToolCallStatus::Completed,
            metadata: json!(null),
        };
        let result = logger.log_call(tool_id, entry.clone());
        assert!(result.is_ok());
        let (date, line) = result.unwrap();
        line_numbers.push(line);
        dates.push(date);
        entries.push(entry);
    }

    // All entries should have same date
    let first_date = &dates[0];
    assert!(dates.iter().all(|d| d == first_date));

    // Read back each entry and verify
    for (i, (line, expected)) in line_numbers.iter().zip(entries.iter()).enumerate() {
        let read_result = logger.read_call(tool_id, &dates[0], *line);
        assert!(read_result.is_ok(), "Entry {} should be readable", i);
        let read_entry = read_result.unwrap();
        assert_eq!(read_entry.call_id, expected.call_id);
        assert_eq!(read_entry.input, expected.input);
        assert_eq!(read_entry.output, expected.output);
    }
}

#[test]
fn test_failed_entry_logged_correctly() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base_path);

    let tool_id = "test-tool-failure";

    let entry = ToolCallEntry {
        call_id: "failed-call-1".to_string(),
        tool_id: tool_id.to_string(),
        tool_name: "Failing Tool".to_string(),
        agent_id: None,
        task_id: None,
        project_id: None,
        started_at: 1744000000000,
        finished_at: 1744000000500,
        duration_ms: 500,
        input: json!({ "bad_param": "oops" }),
        output: None,
        error: Some("Parameter validation failed: bad_param is invalid".to_string()),
        status: ToolCallStatus::Failed,
        metadata: json!(null),
    };

    let result = logger.log_call(tool_id, entry.clone());
    assert!(result.is_ok());

    let (date, line) = result.unwrap();
    let read_entry = logger.read_call(tool_id, &date, line).unwrap();

    assert_eq!(read_entry.status, ToolCallStatus::Failed);
    assert_eq!(
        read_entry.error,
        Some("Parameter validation failed: bad_param is invalid".to_string())
    );
    assert!(read_entry.output.is_none());
}

#[derive(Clone)]
struct FakeCoreTool {
    po: ToolPo,
    result: Value,
}

#[async_trait]
impl CoreTool for FakeCoreTool {
    async fn call(&self, _ctx: RequestContext, _args: Value) -> Result<Value> {
        Ok(self.result.clone())
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

fn init_test_tool_call_logger() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let base_path =
            std::env::temp_dir().join(format!("ai_orz_tool_trace_tests_{}", process::id()));
        fs::create_dir_all(&base_path).expect("test tool trace base path should be created");
        ToolCallLogger::init(base_path);
    });
}

fn fake_tool_po(protocol: ToolProtocol) -> ToolPo {
    let mut po = ToolPo::new(
        "fake-http-tool".to_string(),
        "fake-http-tool".to_string(),
        "Fake HTTP tool".to_string(),
        protocol,
        json!({}),
        None,
        vec![],
        Some("test".to_string()),
    );
    po.status = ToolStatus::Enabled;
    po
}

#[derive(Clone)]
struct FailingFakeCoreTool {
    po: ToolPo,
}

#[async_trait]
impl CoreTool for FailingFakeCoreTool {
    async fn call(&self, _ctx: RequestContext, _args: Value) -> Result<Value> {
        Err(err!(
            ToolExecutionFailed,
            Tool,
            "http request failed for https://api.example.invalid/search?access_token=***"
        ))
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}

#[tokio::test]
async fn http_tool_logging_decorator_redacts_error() {
    init_test_tool_call_logger();

    let tool = FailingFakeCoreTool {
        po: fake_tool_po(ToolProtocol::Http),
    };
    let decorated = LoggingDecorator::new(Box::new(tool));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let (_result, entry) = decorated
        .call_with_entry(ctx, json!({ "access_token": "placeholder-value" }))
        .await;

    let trace_text = serde_json::to_string(&entry).expect("trace entry should serialize");
    assert!(trace_text.contains("[REDACTED]"));
    assert!(
        !trace_text.contains("placeholder-value"),
        "HTTP trace leaked sensitive value: {trace_text}"
    );
    assert!(
        !trace_text.contains("api.example.invalid"),
        "HTTP trace leaked URL host: {trace_text}"
    );
}

#[tokio::test]
async fn http_tool_logging_decorator_redacts_input_and_output() {
    init_test_tool_call_logger();

    let sensitive_value = "placeholder-value";
    let tool = FakeCoreTool {
        po: fake_tool_po(ToolProtocol::Http),
        result: json!({
            "status": 200,
            "body": {
                "access_token": sensitive_value
            }
        }),
    };
    let decorated = LoggingDecorator::new(Box::new(tool));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let (_result, entry) = decorated
        .call_with_entry(
            ctx,
            json!({
                "query": "rust",
                "access_token": sensitive_value
            }),
        )
        .await;

    let trace_text = serde_json::to_string(&entry).expect("trace entry should serialize");
    assert!(trace_text.contains("[REDACTED]"));
    assert!(
        !trace_text.contains(sensitive_value),
        "HTTP trace leaked sensitive value: {trace_text}"
    );
    assert!(
        !trace_text.contains("access_token"),
        "HTTP trace leaked sensitive key: {trace_text}"
    );
}

#[tokio::test]
async fn mcp_tool_logging_decorator_redacts_error() {
    init_test_tool_call_logger();

    let tool = FailingFakeCoreTool {
        po: fake_tool_po(ToolProtocol::Mcp),
    };
    let decorated = LoggingDecorator::new(Box::new(tool));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let (_result, entry) = decorated
        .call_with_entry(ctx, json!({ "access_token": "placeholder-value" }))
        .await;

    let trace_text = serde_json::to_string(&entry).expect("trace entry should serialize");
    assert!(trace_text.contains("[REDACTED]"));
    assert!(
        !trace_text.contains("placeholder-value"),
        "MCP trace leaked sensitive value: {trace_text}"
    );
    assert!(
        !trace_text.contains("api.example.invalid"),
        "MCP trace leaked URL host: {trace_text}"
    );
}

#[tokio::test]
async fn mcp_tool_logging_decorator_redacts_input_and_output() {
    init_test_tool_call_logger();

    let sensitive_value = "placeholder-value";
    let tool = FakeCoreTool {
        po: fake_tool_po(ToolProtocol::Mcp),
        result: json!({
            "status": "ok",
            "payload": {
                "credential": sensitive_value
            }
        }),
    };
    let decorated = LoggingDecorator::new(Box::new(tool));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("test sqlite pool should be created");
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let (_result, entry) = decorated
        .call_with_entry(
            ctx,
            json!({
                "query": "rust",
                "credential": sensitive_value
            }),
        )
        .await;

    let trace_text = serde_json::to_string(&entry).expect("trace entry should serialize");
    assert!(trace_text.contains("[REDACTED]"));
    assert!(
        !trace_text.contains(sensitive_value),
        "MCP trace leaked sensitive value: {trace_text}"
    );
    assert!(
        !trace_text.contains("credential"),
        "MCP trace leaked sensitive key: {trace_text}"
    );
}

#[test]
fn test_read_nonexistent_entry_returns_error() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base_path);

    let tool_id = "test-tool-nonexistent";

    // Create directory structure by writing
    let writer = logger.writer_for_tool(tool_id);
    let _ = writer.append(&json!({}));

    // Try to read non-existent date file
    let result = logger.read_call(tool_id, "19990101", 1);
    assert!(result.is_err());
}

#[test]
fn test_different_tools_have_separate_directories() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base_path.clone());

    let tool1 = "tool-alpha";
    let tool2 = "tool-beta";

    // Create both writers and write something to create directories
    let writer1 = logger.writer_for_tool(tool1);
    let writer2 = logger.writer_for_tool(tool2);
    let _ = writer1.append(&json!({}));
    let _ = writer2.append(&json!({}));

    // Verify both directories exist
    let dir1 = base_path.join("tools").join(tool1).join("call_trace");
    let dir2 = base_path.join("tools").join(tool2).join("call_trace");

    assert!(dir1.exists());
    assert!(dir2.exists());
}
