//! Tool call logging 单元测试

use crate::pkg::tool_tracing::{ToolCallEntry, ToolCallLogger, ToolCallStatus};
use serde_json::json;
use tempfile::tempdir;

#[test]
fn test_tool_call_logger_basic() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    
    // Use direct base path, no need for AppConfig
    let logger = ToolCallLogger::with_base_path(temp_dir.path());
    
    // Log a tool call
    let entry = ToolCallEntry {
        call_id: "call-123".to_string(),
        tool_id: "test_tool".to_string(),
        tool_name: "Test Tool".to_string(),
        agent_id: None,
        task_id: None,
        project_id: None,
        started_at: chrono::Utc::now().timestamp_millis() as u64,
        finished_at: chrono::Utc::now().timestamp_millis() as u64 + 1234,
        duration_ms: 1234,
        input: json!({
            "prompt": "hello world",
            "temperature": 0.7
        }),
        output: Some(json!({
            "result": "success",
            "answer": "42"
        })),
        error: None,
        status: ToolCallStatus::Completed,
        metadata: json!({}),
    };
    
    let (date, line_number) = logger.log_call("test_tool", entry)?;
    
    // We should get back a valid date and line 0 (first entry)
    assert_eq!(line_number, 0);
    assert_eq!(date.len(), 8); // YYYYMMDD
    
    // Check that we can read it back
    let read_back = logger.read_call("test_tool", &date, 0)?;
    
    // Verify basic structure
    assert_eq!(read_back.call_id, "call-123");
    assert_eq!(read_back.tool_id, "test_tool");
    assert_eq!(read_back.duration_ms, 1234);
    assert_eq!(read_back.metadata, json!({}));
    
    Ok(())
}

#[test]
fn test_tool_call_logger_multiple_calls() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    
    let logger = ToolCallLogger::with_base_path(temp_dir.path());
    
    // Append multiple calls
    for i in 0..5 {
        let now = chrono::Utc::now().timestamp_millis() as u64;
        let entry = ToolCallEntry {
            call_id: format!("call-{}", i),
            tool_id: "multi_test".to_string(),
            tool_name: "Multiple Test".to_string(),
            agent_id: None,
            task_id: None,
            project_id: None,
            started_at: now,
            finished_at: now + 100 + i * 10,
            duration_ms: 100 + i * 10,
            input: json!({ "index": i }),
            output: Some(json!({ "result": i * 2 })),
            error: None,
            status: ToolCallStatus::Completed,
            metadata: json!({}),
        };
        let (_, line) = logger.log_call("multi_test", entry)?;
        assert_eq!(line, i as usize);
    }
    
    Ok(())
}

#[test]
fn test_tool_call_logger_different_tools_separate_paths() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    
    let logger = ToolCallLogger::with_base_path(temp_dir.path());
    let now = chrono::Utc::now().timestamp_millis() as u64;
    
    // Different tools should have separate directories
    let entry1 = ToolCallEntry {
        call_id: "call-1".to_string(),
        tool_id: "tool_a".to_string(),
        tool_name: "Tool A".to_string(),
        agent_id: None,
        task_id: None,
        project_id: None,
        started_at: now,
        finished_at: now + 100,
        duration_ms: 100,
        input: json!({}),
        output: None,
        error: None,
        status: ToolCallStatus::Completed,
        metadata: json!({}),
    };
    let (date1, _) = logger.log_call("tool_a", entry1)?;
    
    let entry2 = ToolCallEntry {
        call_id: "call-2".to_string(),
        tool_id: "tool_b".to_string(),
        tool_name: "Tool B".to_string(),
        agent_id: None,
        task_id: None,
        project_id: None,
        started_at: now,
        finished_at: now + 150,
        duration_ms: 150,
        input: json!({}),
        output: None,
        error: None,
        status: ToolCallStatus::Completed,
        metadata: json!({}),
    };
    let (date2, _) = logger.log_call("tool_b", entry2)?;
    
    // Verify directory structure matches spec: {base}/tools/{tool_id}/call_trace/{date}.jsonl
    let expected_path = temp_dir
        .path()
        .join("tools")
        .join("tool_a")
        .join("call_trace")
        .join(format!("{}.jsonl", date1));
    
    assert!(expected_path.exists());
    
    let expected_path_b = temp_dir
        .path()
        .join("tools")
        .join("tool_b")
        .join("call_trace")
        .join(format!("{}.jsonl", date2));
    
    assert!(expected_path_b.exists());
    
    Ok(())
}

#[test]
fn test_tool_call_failed_entry() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    
    let logger = ToolCallLogger::with_base_path(temp_dir.path());
    
    let now = chrono::Utc::now().timestamp_millis() as u64;
    let entry = ToolCallEntry {
        call_id: "fail-001".to_string(),
        tool_id: "error_test".to_string(),
        tool_name: "Error Test".to_string(),
        agent_id: None,
        task_id: None,
        project_id: None,
        started_at: now,
        finished_at: now + 500,
        duration_ms: 500,
        input: json!({ "param": "bad" }),
        output: None,
        error: Some("API rate limit exceeded".to_string()),
        status: ToolCallStatus::Failed,
        metadata: json!({ "retry_count": 1 }),
    };
    
    let (date, line) = logger.log_call("error_test", entry)?;
    
    let read = logger.read_call("error_test", &date, line)?;
    
    assert_eq!(read.call_id, "fail-001");
    assert_eq!(read.error, Some("API rate limit exceeded".to_string()));
    assert!(read.output.is_none());
    assert_eq!(read.metadata, json!({ "retry_count": 1 }));
    
    Ok(())
}

#[test]
fn test_tool_call_with_context_ids() -> anyhow::Result<()> {
    let temp_dir = tempdir()?;
    
    let logger = ToolCallLogger::with_base_path(temp_dir.path());
    let now = chrono::Utc::now().timestamp_millis() as u64;
    
    let entry = ToolCallEntry {
        call_id: "call-with-context".to_string(),
        tool_id: "context_test".to_string(),
        tool_name: "Context Test".to_string(),
        agent_id: Some("agent-abc".to_string()),
        task_id: Some("task-123".to_string()),
        project_id: Some("project-xyz".to_string()),
        started_at: now,
        finished_at: now + 200,
        duration_ms: 200,
        input: json!({ "query": "what is this" }),
        output: Some(json!({ "answer": "this is a test" })),
        error: None,
        status: ToolCallStatus::Completed,
        metadata: json!({}),
    };
    
    let (date, line) = logger.log_call("context_test", entry)?;
    let read_back = logger.read_call("context_test", &date, line)?;
    
    assert_eq!(read_back.agent_id, Some("agent-abc".to_string()));
    assert_eq!(read_back.task_id, Some("task-123".to_string()));
    assert_eq!(read_back.project_id, Some("project-xyz".to_string()));
    
    Ok(())
}