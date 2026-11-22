//! Unit tests for tool call tracing module

use serde_json::json;
use tempfile::tempdir;

use super::entry::{ToolCallEntry, ToolCallStatus};
use super::logger::ToolCallLogger;

#[test]
fn test_logger_creates_flat_trace_directory() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base_path.clone());

    // 单一共享 writer —— 不再按 tool 取 writer，目录在首次写入时创建
    let writer = logger.writer();

    // Do an empty write to create directory
    let _ = writer.append_no_position(&json!({}));

    // 拉平后：{base}/tools/call_trace/（不再有 {tool_id} 这一层）
    let expected_dir = base_path.join("tools").join("call_trace");
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

    // Log the entry（生产路径 log_call 不返行号；这里要验证 read_call，直接用 writer 取坐标）
    let (date, line_number) = logger
        .writer()
        .append(&entry)
        .expect("append should succeed");

    // Read it back
    let read_result = logger.read_call(&date, line_number);
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

    logger.log_call(old_entry).unwrap();
    logger.log_call(latest_entry.clone()).unwrap();
    logger.log_call(other_agent_entry).unwrap();

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
fn query_calls_filters_by_agent_ids_set() {
    // 默认对话口径：只聚合「当前任务出现过的 Agent」，集合外的 Agent 一条都不能带出
    let temp_dir = tempdir().unwrap();
    let logger = ToolCallLogger::new(temp_dir.path().to_path_buf());

    let base = ToolCallEntry {
        call_id: "call-a".to_string(),
        tool_id: "tool-a".to_string(),
        tool_name: "Tool A".to_string(),
        agent_id: Some("agent-a".to_string()),
        task_id: None,
        project_id: None,
        started_at: 1_000,
        finished_at: 1_100,
        duration_ms: 100,
        input: json!({}),
        output: None,
        error: None,
        status: ToolCallStatus::Completed,
        metadata: json!(null),
    };
    logger.log_call(base.clone()).unwrap();
    logger
        .log_call(ToolCallEntry {
            call_id: "call-b".to_string(),
            agent_id: Some("agent-b".to_string()),
            started_at: 2_000,
            ..base.clone()
        })
        .unwrap();
    logger
        .log_call(ToolCallEntry {
            call_id: "call-c".to_string(),
            agent_id: Some("agent-c".to_string()),
            started_at: 3_000,
            ..base
        })
        .unwrap();

    let results = logger
        .query_calls(super::logger::ToolCallQuery {
            agent_ids: Some(vec!["agent-a".to_string(), "agent-c".to_string()]),
            limit: Some(10),
            ..Default::default()
        })
        .expect("query should succeed");

    let ids: Vec<&str> = results.iter().map(|e| e.call_id.as_str()).collect();
    assert_eq!(ids, vec!["call-c", "call-a"]);
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

    logger.log_call(entry_a).unwrap();
    logger.log_call(entry_b.clone()).unwrap();

    let by_call_id = logger
        .read_call_by_id("target-call")
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
fn query_calls_without_project_excludes_scoped_entries() {
    // 默认对话「工具」Tab：只看未归属项目的游离调用，项目内调用一条都不能带出
    let temp_dir = tempdir().unwrap();
    let logger = ToolCallLogger::new(temp_dir.path().to_path_buf());

    let free = ToolCallEntry {
        call_id: "free-call".to_string(),
        tool_id: "tool-a".to_string(),
        tool_name: "Tool A".to_string(),
        agent_id: Some("agent-1".to_string()),
        task_id: None,
        project_id: None,
        started_at: 1000,
        finished_at: 1100,
        duration_ms: 100,
        input: json!({}),
        output: None,
        error: None,
        status: ToolCallStatus::Completed,
        metadata: json!(null),
    };
    let scoped = ToolCallEntry {
        call_id: "scoped-call".to_string(),
        project_id: Some("project-1".to_string()),
        started_at: 2000,
        finished_at: 2100,
        ..free.clone()
    };
    logger.log_call(free).unwrap();
    logger.log_call(scoped).unwrap();

    let results = logger
        .query_calls(super::logger::ToolCallQuery {
            without_project: true,
            limit: Some(10),
            ..Default::default()
        })
        .expect("query should succeed");

    let ids: Vec<&str> = results.iter().map(|e| e.call_id.as_str()).collect();
    assert_eq!(ids, vec!["free-call"]);
}

#[test]
fn query_calls_page_applies_offset_and_reports_total() {
    // 分页：total 始终是命中总数（不受 limit/offset 影响），items 只取当前窗口
    let temp_dir = tempdir().unwrap();
    let logger = ToolCallLogger::new(temp_dir.path().to_path_buf());
    let entry = |call_id: &str, started_at: u64| ToolCallEntry {
        call_id: call_id.to_string(),
        tool_id: "tool-page".to_string(),
        tool_name: "Tool Page".to_string(),
        agent_id: Some("agent-1".to_string()),
        task_id: None,
        project_id: None,
        started_at,
        finished_at: started_at + 100,
        duration_ms: 100,
        input: json!({}),
        output: None,
        error: None,
        status: ToolCallStatus::Completed,
        metadata: json!(null),
    };
    for (i, call_id) in ["c1", "c2", "c3"].iter().enumerate() {
        logger
            .log_call(entry(call_id, 1_000 + i as u64 * 100))
            .unwrap();
    }

    let page = logger
        .query_calls_page(super::logger::ToolCallQuery {
            limit: Some(1),
            offset: 1,
            ..Default::default()
        })
        .expect("paged query should succeed");

    assert_eq!(page.total, 3);
    assert_eq!(page.items.len(), 1);
    // 最新优先排序：c3, c2, c1 → offset=1 命中 c2
    assert_eq!(page.items[0].call_id, "c2");
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
        let (date, line) = logger
            .writer()
            .append(&entry)
            .expect("append should succeed");
        line_numbers.push(line);
        dates.push(date);
        entries.push(entry);
    }

    // All entries should have same date
    let first_date = &dates[0];
    assert!(dates.iter().all(|d| d == first_date));

    // Read back each entry and verify
    for (i, (line, expected)) in line_numbers.iter().zip(entries.iter()).enumerate() {
        let read_result = logger.read_call(&dates[0], *line);
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

    let (date, line) = logger
        .writer()
        .append(&entry)
        .expect("append should succeed");
    let read_entry = logger.read_call(&date, line).unwrap();

    assert_eq!(read_entry.status, ToolCallStatus::Failed);
    assert_eq!(
        read_entry.error,
        Some("Parameter validation failed: bad_param is invalid".to_string())
    );
    assert!(read_entry.output.is_none());
}

#[test]
fn test_read_nonexistent_entry_returns_error() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base_path);

    // Create directory structure by writing
    let writer = logger.writer();
    let _ = writer.append_no_position(&json!({}));

    // Try to read non-existent date file
    let result = logger.read_call("19990101", 1);
    assert!(result.is_err());
}

/// 拉平后所有工具共用同一目录 —— `tool_id` 只是 entry 的字段（过滤维度），不是地址维度。
#[test]
fn test_all_tools_share_one_flat_trace_dir() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base_path.clone());

    let writer = logger.writer();
    let _ = writer.append_no_position(&json!({}));

    let dir = base_path.join("tools").join("call_trace");
    assert!(dir.exists());
    // 不存在任何 per-tool 子目录
    assert!(!dir.join("tool-alpha").exists());
    assert!(!dir.join("tool-beta").exists());
}

/// 按 `call_id` 查询**不按 tool_id 收窄**：同一个 call_id 落在别的 tool_id 下也必须查得到
/// （历史脏数据正是这种形态；按 tool_id 收窄会直接查不到，等于把幂等做成半个）。
#[test]
fn read_call_by_id_is_not_scoped_by_tool_id() {
    let temp_dir = tempdir().unwrap();
    let logger = ToolCallLogger::new(temp_dir.path().to_path_buf());

    let older = ToolCallEntry {
        call_id: "cross-tool-call".to_string(),
        tool_id: "tool-a".to_string(),
        tool_name: "Tool A".to_string(),
        agent_id: Some("agent-1".to_string()),
        task_id: None,
        project_id: None,
        started_at: 1000,
        finished_at: 1100,
        duration_ms: 100,
        input: json!({}),
        output: Some(json!({"ok": true})),
        error: None,
        status: ToolCallStatus::Completed,
        metadata: json!(null),
    };
    let newer = ToolCallEntry {
        tool_id: "tool-b".to_string(),
        tool_name: "Tool B".to_string(),
        started_at: 2000,
        finished_at: 2100,
        ..older.clone()
    };

    logger.log_call(older).unwrap();
    logger.log_call(newer.clone()).unwrap();

    let found = logger
        .read_call_by_id("cross-tool-call")
        .expect("lookup should succeed")
        .expect("cross-tool call_id must be findable without tool_id");
    assert_eq!(found.tool_id, "tool-b");
    assert_eq!(found.started_at, newer.started_at);
}

/// `log_call` 走「不数行」的追加路径：不返回坐标，但每行都完整落盘且可按 `call_id` 检索。
#[test]
fn log_call_appends_without_line_number_and_stays_queryable() {
    let temp_dir = tempdir().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base_path.clone());

    for i in 0..5 {
        logger
            .log_call(ToolCallEntry {
                call_id: format!("no-position-{i}"),
                tool_id: "tool-x".to_string(),
                tool_name: "Tool X".to_string(),
                agent_id: None,
                task_id: None,
                project_id: None,
                started_at: 1000 + i as u64,
                finished_at: 1100 + i as u64,
                duration_ms: 100,
                input: json!({ "index": i }),
                output: Some(json!({ "ok": true })),
                error: None,
                status: ToolCallStatus::Completed,
                metadata: json!(null),
            })
            .expect("log_call should succeed");
    }

    for i in 0..5 {
        let found = logger
            .read_call_by_id(&format!("no-position-{i}"))
            .expect("lookup should succeed")
            .unwrap_or_else(|| panic!("entry no-position-{i} must be queryable"));
        assert_eq!(found.input, json!({ "index": i }));
    }

    // 不数行 ≠ 少写：5 条都进了当日文件
    let trace_dir = base_path.join("tools").join("call_trace");
    let mut total_lines = 0;
    for file in std::fs::read_dir(&trace_dir).expect("trace dir should exist") {
        let path = file.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "jsonl") {
            total_lines += std::fs::read_to_string(path).unwrap().lines().count();
        }
    }
    assert_eq!(total_lines, 5);
}

// ==================== 旁车索引（sidecar index）====================

fn has_sidecar_index(trace_dir: &std::path::Path) -> bool {
    std::fs::read_dir(trace_dir).unwrap().any(|entry| {
        entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|ext| ext == "idx")
    })
}

fn sidecar_test_entry(call_id: &str, started_at: u64) -> ToolCallEntry {
    ToolCallEntry {
        call_id: call_id.to_string(),
        tool_id: "idx-tool".to_string(),
        tool_name: "Idx Tool".to_string(),
        agent_id: Some("idx-agent".to_string()),
        task_id: None,
        project_id: None,
        started_at,
        finished_at: started_at + 100,
        duration_ms: 100,
        input: json!({}),
        output: Some(json!({"ok": true})),
        error: None,
        status: ToolCallStatus::Completed,
        metadata: json!(null),
    }
}

/// 索引自包含过滤字段 + 字节坐标，且不含 input/output 原文（轻量性）
#[test]
fn sidecar_index_is_lightweight_and_queryable() {
    let temp_dir = tempdir().unwrap();
    let base = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base.clone());
    let secret = "UNIQUE_PAYLOAD_SENTINEL_9f3a";

    logger
        .log_call(ToolCallEntry {
            input: json!({ "blob": secret }),
            output: Some(json!({ "blob": secret })),
            ..sidecar_test_entry("idx-call", 5000)
        })
        .unwrap();

    let trace_dir = base.join("tools").join("call_trace");
    let idx_files: Vec<_> = std::fs::read_dir(&trace_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "idx"))
        .collect();
    assert_eq!(idx_files.len(), 1, "应为原文生成旁车索引");
    let idx_content = std::fs::read_to_string(&idx_files[0]).unwrap();
    assert!(
        !idx_content.contains(secret),
        "索引不应包含 input/output 原文"
    );
    assert!(
        idx_content.contains("idx-call"),
        "索引应包含过滤字段 call_id"
    );

    let found = logger.read_call_by_id("idx-call").unwrap().unwrap();
    assert_eq!(found.input, json!({ "blob": secret }));
}

/// 存量无索引文件（绕过 log_call 直接写原文）查询时惰性重建索引
#[test]
fn query_rebuilds_index_for_legacy_jsonl_without_index() {
    let temp_dir = tempdir().unwrap();
    let base = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base.clone());
    let entry = sidecar_test_entry("legacy-call", 1000);

    // 绕过 log_call，模拟历史数据 / 其它写入方直接落原文
    logger.writer().append_no_position(&entry).unwrap();

    let trace_dir = base.join("tools").join("call_trace");
    assert!(!has_sidecar_index(&trace_dir), "初始应无索引");

    let found = logger.read_call_by_id("legacy-call").unwrap().unwrap();
    assert_eq!(found.call_id, "legacy-call");
    assert!(has_sidecar_index(&trace_dir), "查询后应重建出索引");
}

/// 索引落后（原文新增但索引没跟上）时增量补齐，不漏新行
#[test]
fn query_backfills_lagging_index_incrementally() {
    let temp_dir = tempdir().unwrap();
    let base = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base.clone());

    logger
        .log_call(sidecar_test_entry("lagged-a", 1000))
        .unwrap();
    logger
        .log_call(sidecar_test_entry("lagged-b", 2000))
        .unwrap();
    // 直接写原文，制造「原文有、索引无」的落后窗口
    logger
        .writer()
        .append_no_position(&sidecar_test_entry("lagged-c", 3000))
        .unwrap();

    let results = logger
        .query_calls(super::logger::ToolCallQuery {
            agent_id: Some("idx-agent".to_string()),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
    let ids: Vec<&str> = results.iter().map(|e| e.call_id.as_str()).collect();
    assert_eq!(ids, vec!["lagged-c", "lagged-b", "lagged-a"]);
}

/// 索引含坏行时全量重建，不吞掉任何一条记录
#[test]
fn query_rebuilds_corrupted_index_from_scratch() {
    use std::io::Write;

    let temp_dir = tempdir().unwrap();
    let base = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base.clone());

    logger
        .log_call(sidecar_test_entry("corrupt-a", 1000))
        .unwrap();
    logger
        .log_call(sidecar_test_entry("corrupt-b", 2000))
        .unwrap();

    let trace_dir = base.join("tools").join("call_trace");
    let idx_path = std::fs::read_dir(&trace_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|ext| ext == "idx"))
        .unwrap();
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&idx_path)
            .unwrap();
        file.write_all(b"NOT_VALID_JSON\n").unwrap();
    }

    let found = logger.read_call_by_id("corrupt-a").unwrap().unwrap();
    assert_eq!(found.call_id, "corrupt-a");
    let found_b = logger.read_call_by_id("corrupt-b").unwrap().unwrap();
    assert_eq!(found_b.call_id, "corrupt-b");
}

/// 并发写入：原文与索引在同一把锁内追加，索引行顺序/数量必须与原文严格对应，
/// 不丢不重（watermark 不回退）。这是 append_with_index 的核心不变量。
#[test]
fn sidecar_concurrent_writes_keep_index_in_sync() {
    use std::sync::Arc;

    let temp_dir = tempdir().unwrap();
    let base = temp_dir.path().to_path_buf();
    let logger = Arc::new(ToolCallLogger::new(base.clone()));
    let threads = 8;
    let per_thread = 12; // 总数 96 < MAX_TOOL_CALL_QUERY_LIMIT(100)，一次查询可全取回

    let mut handles = Vec::new();
    for t in 0..threads {
        let logger = Arc::clone(&logger);
        handles.push(std::thread::spawn(move || {
            for i in 0..per_thread {
                let id = format!("conc-{t}-{i}");
                logger
                    .log_call(sidecar_test_entry(&id, (t * per_thread + i) as u64))
                    .unwrap();
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    let expected = threads * per_thread;
    let results = logger
        .query_calls(super::logger::ToolCallQuery {
            agent_id: Some("idx-agent".to_string()),
            limit: Some(100),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        results.len(),
        expected,
        "并发写入后查询应拿回全部记录，不丢不重"
    );

    let trace_dir = base.join("tools").join("call_trace");
    let idx_path = std::fs::read_dir(&trace_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|ext| ext == "idx"))
        .unwrap();
    let idx_lines = std::fs::read_to_string(idx_path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    assert_eq!(idx_lines, expected, "索引行数必须与原文行数严格一致");
}

/// 跨天分片：两个日期的原文各自重建索引，查询应合并全部命中并按 started_at 排序
#[test]
fn sidecar_query_merges_across_date_shards() {
    let temp_dir = tempdir().unwrap();
    let base = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base.clone());
    let trace_dir = base.join("tools").join("call_trace");
    std::fs::create_dir_all(&trace_dir).unwrap();

    // 直接写两个日期的原文（绕过 log_call 的「当天」约束），触发查询侧惰性重建
    for (date, id, ts) in [
        ("20260101", "day1-old", 100u64),
        ("20260102", "day2-new", 200u64),
    ] {
        let entry = sidecar_test_entry(id, ts);
        std::fs::write(
            trace_dir.join(format!("{date}.jsonl")),
            format!("{}\n", serde_json::to_string(&entry).unwrap()),
        )
        .unwrap();
    }

    let results = logger
        .query_calls(super::logger::ToolCallQuery {
            agent_id: Some("idx-agent".to_string()),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
    let ids: Vec<&str> = results.iter().map(|e| e.call_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["day2-new", "day1-old"],
        "跨天应按 started_at 倒序合并"
    );
}

/// limit=0 / 无命中：边界返回空，不 panic、不误建索引数据
#[test]
fn sidecar_query_zero_limit_and_empty_result() {
    let temp_dir = tempdir().unwrap();
    let base = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base.clone());
    logger
        .log_call(sidecar_test_entry("some-call", 1000))
        .unwrap();

    let zero = logger
        .query_calls(super::logger::ToolCallQuery {
            agent_id: Some("idx-agent".to_string()),
            limit: Some(0),
            ..Default::default()
        })
        .unwrap();
    assert!(zero.is_empty(), "limit=0 应返回空");

    let no_match = logger
        .query_calls(super::logger::ToolCallQuery {
            call_id: Some("nonexistent".to_string()),
            limit: Some(10),
            ..Default::default()
        })
        .unwrap();
    assert!(no_match.is_empty(), "无命中应返回空");
}

/// 索引文件被删后：下次查询自动重建，行为与从未有过索引一致
#[test]
fn sidecar_rebuilds_after_index_file_deleted() {
    let temp_dir = tempdir().unwrap();
    let base = temp_dir.path().to_path_buf();
    let logger = ToolCallLogger::new(base.clone());
    logger
        .log_call(sidecar_test_entry("persist-call", 1000))
        .unwrap();
    assert!(logger.read_call_by_id("persist-call").unwrap().is_some());

    let trace_dir = base.join("tools").join("call_trace");
    for entry in std::fs::read_dir(&trace_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "idx") {
            std::fs::remove_file(&path).unwrap();
        }
    }
    assert!(!has_sidecar_index(&trace_dir), "索引应已被删除");

    let found = logger.read_call_by_id("persist-call").unwrap().unwrap();
    assert_eq!(found.call_id, "persist-call");
    assert!(has_sidecar_index(&trace_dir), "查询后应自动重建索引");
}
