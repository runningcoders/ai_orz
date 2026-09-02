//! Unit tests for tool call tracing module

use crate::models::tool::ToolPo;
use common::enums::{ToolProtocol, ToolStatus};
use serde_json::json;
use tempfile::tempdir;

use super::entry::{ToolCallEntry, ToolCallStatus};
use super::logger::ToolCallLogger;

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

// ==================== 脱敏函数测试 ====================
//
// 所有协议（Builtin / Http / Mcp）统一字段级脱敏：保留完整 JSON 结构，
// 仅敏感字段值替换为 `***`。不再按协议类型跳过任何工具。

#[test]
fn http_tool_field_level_redact_keeps_structure() {
    let po = fake_tool_po(ToolProtocol::Http);
    let (input, output, error) = super::entry::redact_trace_values_for_tool(
        &po,
        json!({ "query": "rust", "access_token": "placeholder-value" }),
        Some(
            json!({ "status": 200, "body": { "access_token": "placeholder-value", "count": 42 } }),
        ),
        None,
    );

    // 结构完整保留，敏感字段替换为 ***
    assert_eq!(input, json!({ "query": "rust", "access_token": "***" }));
    assert_eq!(
        output,
        Some(json!({
            "status": 200,
            "body": { "access_token": "***", "count": 42 }
        }))
    );
    assert!(error.is_none());
}

#[test]
fn http_tool_redacts_error_text_kv_pattern() {
    let po = fake_tool_po(ToolProtocol::Http);
    let (_input, output, error) = super::entry::redact_trace_values_for_tool(
        &po,
        json!({}),
        None,
        Some("http request failed, api_key=sk-123, retry next".to_string()),
    );

    assert!(output.is_none());
    // 错误文本中 api_key 的值被替换，结构保留
    assert_eq!(
        error.as_deref(),
        Some("http request failed, api_key=***, retry next")
    );
}

#[test]
fn mcp_tool_field_level_redact_keeps_structure() {
    let po = fake_tool_po(ToolProtocol::Mcp);
    let (input, output, error) = super::entry::redact_trace_values_for_tool(
        &po,
        json!({ "query": "rust", "credential": "placeholder-value" }),
        Some(json!({ "status": "ok", "payload": { "credential": "placeholder-value" } })),
        None,
    );

    assert_eq!(input, json!({ "query": "rust", "credential": "***" }));
    assert_eq!(
        output,
        Some(json!({
            "status": "ok",
            "payload": { "credential": "***" }
        }))
    );
    assert!(error.is_none());
}

#[test]
fn builtin_tool_also_field_level_redacts() {
    // Builtin 工具（如 shell_exec）也做统一字段级脱敏，不再特殊处理
    let po = fake_tool_po(ToolProtocol::Builtin);
    let (input, output, error) = super::entry::redact_trace_values_for_tool(
        &po,
        json!({ "command": "git push --token secret123", "env": { "password": "hunter2" } }),
        Some(json!({ "result": "ok", "token": "abcdef" })),
        None,
    );

    assert_eq!(
        input,
        json!({ "command": "git push --token ***", "env": { "password": "***" } })
    );
    assert_eq!(output, Some(json!({ "result": "ok", "token": "***" })));
    assert!(error.is_none());
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
