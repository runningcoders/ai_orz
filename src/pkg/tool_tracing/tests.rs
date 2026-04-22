//! Unit tests for tool call tracing module

use tempfile::tempdir;
use serde_json::json;

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
    assert!(read_result.is_ok(), "Reading should succeed: {:?}", read_result);

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
    assert_eq!(read_entry.error, Some("Parameter validation failed: bad_param is invalid".to_string()));
    assert!(read_entry.output.is_none());
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
