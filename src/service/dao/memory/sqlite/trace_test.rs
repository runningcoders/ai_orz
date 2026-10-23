//! Memory DAO 记忆轨迹（trace）单元测试：JSONL 追加、批量写入与文件扫描查询

use crate::models::memory::{MemoryTrace, ShortTermMemoryIndexPo};
use crate::service::dao::memory::MemoryTraceQuery;
use crate::service::dao::memory::sqlite::MemoryDaoSqliteImpl;
use crate::service::dao::memory::sqlite::query_traces_at;
use chrono::TimeZone;
use common::enums::{MemoryRole, MemoryStatus};
use sqlx::SqlitePool;
use std::path::Path;

#[sqlx::test]
async fn test_append_trace_and_create_short_term_index(pool: SqlitePool) {
    // 初始化配置
    crate::config::init().unwrap();
    // 自动迁移已经由 sqlx::test 执行
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let trace = MemoryTrace::new(
        "test-agent-1".to_string(),
        "test-log-1".to_string(),
        "test-user".to_string(),
        "test-org".to_string(),
        MemoryRole::User,
        "这是一段测试内容".to_string(),
        None, // 测试不需要 task_id
    );

    // 阶段 1：append trace 到 daily jsonl
    let position = dao.append_trace(ctx.clone(), &trace).await.unwrap();
    assert_eq!(position.trace_id, trace.id);
    assert!(position.date_filename.ends_with(".jsonl"));

    // 阶段 2：创建 short-term index 关联 trace
    let now = chrono::Utc::now().timestamp();
    let index = ShortTermMemoryIndexPo {
        id: "st-1".to_string(),
        agent_id: "test-agent-1".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: "测试摘要".to_string(),
        tags: serde_json::to_string(&vec!["test", "memory"]).unwrap(),
        trace_ids: serde_json::to_string(&vec![&position.trace_id]).unwrap(),
        status: MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let result = dao.create_short_term_index(ctx, index).await;
    assert!(result.is_ok());
}

#[test]
fn test_memory_trace_id_is_trace_prefix() {
    // 验证 MemoryTrace 的 ID 格式是 trace-{agent_id}-{timestamp}
    let trace = MemoryTrace::new(
        "test-agent-1".to_string(),
        "log-1".to_string(),
        "user-1".to_string(),
        "org-1".to_string(),
        MemoryRole::User,
        "这是一段测试内容".to_string(),
        None,
    );

    // ID 应该以 trace- 开头
    assert!(trace.id.starts_with("trace-"));

    // 应该包含 agent_id
    assert!(trace.id.contains("test-agent-1"));

    // 最后一部分应该是数字（timestamp）
    let parts: Vec<&str> = trace.id.rsplitn(2, '-').collect();
    assert!(parts[0].parse::<u64>().is_ok());
}

#[test]
fn test_memory_trace_to_markdown() {
    // 验证 MemoryTrace 可以正确格式化为 markdown
    let trace = MemoryTrace::new(
        "test-agent-1".to_string(),
        "log-1".to_string(),
        "user-1".to_string(),
        "org-1".to_string(),
        MemoryRole::User,
        "你好，这是一个测试问题".to_string(),
        None,
    );

    let markdown = trace.to_markdown();
    assert!(markdown.contains(&trace.id));
    assert!(markdown.contains("User"));
    assert!(markdown.contains("你好，这是一个测试问题"));
}

#[sqlx::test]
async fn test_batch_append_traces(pool: SqlitePool) {
    // 初始化配置
    crate::config::init().unwrap();
    let dao = MemoryDaoSqliteImpl::new();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    let traces = vec![
        MemoryTrace::new(
            "test-agent-1".to_string(),
            "test-log-1".to_string(),
            "test-user".to_string(),
            "test-org".to_string(),
            MemoryRole::User,
            "第一条测试内容".to_string(),
            None,
        ),
        MemoryTrace::new(
            "test-agent-1".to_string(),
            "test-log-1".to_string(),
            "test-user".to_string(),
            "test-org".to_string(),
            MemoryRole::Assistant,
            "第二条测试内容".to_string(),
            None,
        ),
        MemoryTrace::new(
            "test-agent-1".to_string(),
            "test-log-1".to_string(),
            "test-user".to_string(),
            "test-org".to_string(),
            MemoryRole::User,
            "第三条测试内容".to_string(),
            None,
        ),
    ];

    // 批量追加
    let positions = dao.batch_append_traces(ctx, &traces).await.unwrap();
    assert_eq!(positions.len(), 3);
    assert_eq!(positions[0].trace_id, traces[0].id);
    assert_eq!(positions[1].trace_id, traces[1].id);
    assert_eq!(positions[2].trace_id, traces[2].id);
    assert!(positions[0].date_filename.ends_with(".jsonl"));

    // 批量写入只数一次行 → 位置必须严格连续（第 i 条 = 首行 + i），错一个就等于定位串位
    assert_eq!(positions[1].line_number, positions[0].line_number + 1);
    assert_eq!(positions[2].line_number, positions[0].line_number + 2);

    // 按 (date_filename, line_number) 读回，内容与写入顺序一一对应
    let trace_dir =
        crate::pkg::paths::agent_memory_dir(&crate::config::get().base_data_path(), "test-agent-1");
    let writer = crate::pkg::daily_jsonl::DailyJsonlWriter::new(trace_dir);
    for (position, expected) in positions.iter().zip(traces.iter()) {
        let date = position.date_filename.trim_end_matches(".jsonl");
        let read: MemoryTrace = writer
            .read_line_json(date, position.line_number as usize)
            .expect("position must resolve to a real line");
        assert_eq!(read.id, expected.id);
        assert_eq!(read.input, expected.input);
    }
}

/// 在临时目录下按写入侧布局落一份记忆轨迹分片
fn write_trace_shard(base: &Path, agent_id: &str, date: &str, traces: &[MemoryTrace]) {
    let dir = base.join("agents").join(agent_id).join("memory");
    std::fs::create_dir_all(&dir).unwrap();
    let mut content = String::new();
    for trace in traces {
        content.push_str(&serde_json::to_string(trace).unwrap());
        content.push('\n');
    }
    std::fs::write(dir.join(format!("{date}.jsonl")), content).unwrap();
}

fn trace_at(
    agent_id: &str,
    id: &str,
    log_id: &str,
    task_id: Option<&str>,
    created_at: i64,
) -> MemoryTrace {
    let mut trace = MemoryTrace::new(
        agent_id.to_string(),
        log_id.to_string(),
        "user-1".to_string(),
        "org-1".to_string(),
        MemoryRole::User,
        format!("input of {id}"),
        task_id.map(str::to_string),
    );
    trace.id = id.to_string();
    trace.created_at = created_at;
    trace
}

#[test]
fn test_query_traces_filters_by_agent_task_and_trace_id() {
    let dir = tempfile::tempdir().unwrap();
    let day = chrono::Local
        .with_ymd_and_hms(2026, 1, 1, 12, 0, 0)
        .unwrap()
        .timestamp();

    write_trace_shard(
        dir.path(),
        "agent-1",
        "20260101",
        &[
            trace_at("agent-1", "trace-a", "log-1", Some("task-1"), day),
            trace_at("agent-1", "trace-b", "log-2", None, day + 1),
        ],
    );
    write_trace_shard(
        dir.path(),
        "agent-2",
        "20260101",
        &[trace_at("agent-2", "trace-c", "log-1", None, day + 2)],
    );

    // 单 Agent 过滤
    let page = query_traces_at(
        dir.path(),
        &MemoryTraceQuery {
            agent_id: Some("agent-1".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.total, 2);
    // 最新在前
    assert_eq!(page.items[0].id, "trace-b");

    // task_id 过滤（精确）
    let page = query_traces_at(
        dir.path(),
        &MemoryTraceQuery {
            agent_id: Some("agent-1".to_string()),
            task_id: Some("task-1".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].id, "trace-a");

    // log_id 跨 Agent
    let page = query_traces_at(
        dir.path(),
        &MemoryTraceQuery {
            log_id: Some("log-1".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.total, 2);

    // trace_id 精确
    let page = query_traces_at(
        dir.path(),
        &MemoryTraceQuery {
            agent_id: Some("agent-2".to_string()),
            trace_id: Some("trace-c".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].agent_id, "agent-2");
}

#[test]
fn test_query_traces_paginates_and_date_filters() {
    let dir = tempfile::tempdir().unwrap();
    let jan1 = chrono::Local
        .with_ymd_and_hms(2026, 1, 1, 12, 0, 0)
        .unwrap()
        .timestamp();
    let jan2 = chrono::Local
        .with_ymd_and_hms(2026, 1, 2, 12, 0, 0)
        .unwrap()
        .timestamp();

    write_trace_shard(
        dir.path(),
        "agent-1",
        "20260101",
        &[
            trace_at("agent-1", "trace-1", "log", None, jan1),
            trace_at("agent-1", "trace-2", "log", None, jan1 + 1),
        ],
    );
    write_trace_shard(
        dir.path(),
        "agent-1",
        "20260102",
        &[trace_at("agent-1", "trace-3", "log", None, jan2)],
    );

    // 分页：total 为过滤后的总数，items 按 offset/limit 切
    let page = query_traces_at(
        dir.path(),
        &MemoryTraceQuery {
            agent_id: Some("agent-1".to_string()),
            pagination: common::api::PaginationParams {
                limit: Some(2),
                offset: Some(1),
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].id, "trace-2");
    assert_eq!(page.items[1].id, "trace-1");

    // 日期粗筛：只保留 1 月 2 日分片
    let page = query_traces_at(
        dir.path(),
        &MemoryTraceQuery {
            agent_id: Some("agent-1".to_string()),
            created_after: Some(jan2),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].id, "trace-3");

    // 超上限拒绝
    assert!(
        query_traces_at(
            dir.path(),
            &MemoryTraceQuery {
                pagination: common::api::PaginationParams {
                    limit: Some(crate::service::dao::memory::MAX_MEMORY_TRACE_QUERY_LIMIT + 1),
                    offset: None,
                },
                ..Default::default()
            },
        )
        .is_err()
    );
}

#[test]
fn test_query_traces_skips_corrupted_lines() {
    let dir = tempfile::tempdir().unwrap();
    let trace_dir = dir.path().join("agents").join("agent-1").join("memory");
    std::fs::create_dir_all(&trace_dir).unwrap();
    let trace = trace_at("agent-1", "trace-ok", "log", None, 0);
    let content = format!("not-json\n{}\n", serde_json::to_string(&trace).unwrap());
    std::fs::write(trace_dir.join("20260101.jsonl"), content).unwrap();

    let page = query_traces_at(
        dir.path(),
        &MemoryTraceQuery {
            agent_id: Some("agent-1".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].id, "trace-ok");
}
