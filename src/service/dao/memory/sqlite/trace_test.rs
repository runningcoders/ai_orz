//! Memory DAO 记忆轨迹（trace）单元测试：JSONL 追加、批量写入与文件扫描查询

use crate::models::memory::{MemoryTrace, ShortTermMemoryIndexPo};
use crate::service::dao::memory::MemoryTraceQuery;
use crate::service::dao::memory::sqlite::MemoryDaoSqliteImpl;
use crate::service::dao::memory::sqlite::{query_traces_at, resolve_trace_positions_at};
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
fn test_memory_trace_id_is_uuid_v7_simple() {
    // 验证 MemoryTrace 的 ID 格式是 trace_{uuid_v7_simple}（无横杠）
    let trace = MemoryTrace::new(
        "test-agent-1".to_string(),
        "log-1".to_string(),
        "user-1".to_string(),
        "org-1".to_string(),
        MemoryRole::User,
        "这是一段测试内容".to_string(),
        None,
    );

    let parsed = common::id::EntityId::parse(&trace.id)
        .unwrap_or_else(|e| panic!("trace ID 解析失败: {} ({e})", trace.id));
    assert_eq!(
        parsed.prefix(),
        &common::id::IdPrefix::Trace,
        "trace ID 前缀错误: {}",
        trace.id
    );
    assert_eq!(parsed.uuid().get_version_num(), 7, "trace ID 应使用 UUIDv7");
    assert!(!trace.id.contains('-'), "trace ID 不能带横杠: {}", trace.id);
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
    let writer = crate::pkg::jsonl::writer::DailyJsonlWriter::new(trace_dir);
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

/// 按 trace_id 定位物理坐标：命中回（日期文件, 行号），未命中缺席
///
/// 这是「知识节点 → 短期记忆 → trace」引用链能落地的前提：`knowledge_reference`
/// 有了 date_path + line_number 才能回读原始记录，而 `query_traces` **拿不到行号**
/// （`position` 带 `#[serde(skip)]`，只有写入侧回填）——本函数正是补这个缺口。
#[test]
fn test_resolve_trace_positions_locates_line_number() {
    let tmp = tempfile::tempdir().unwrap();
    let agent_id = "agent-pos-1";
    let dir = tmp.path().join("agents").join(agent_id).join("memory");
    std::fs::create_dir_all(&dir).unwrap();

    let make = |id: &str| {
        let mut trace = MemoryTrace::new(
            agent_id.to_string(),
            "log-1".to_string(),
            "user-1".to_string(),
            "org-1".to_string(),
            MemoryRole::User,
            format!("payload of {id}"),
            None,
        );
        trace.id = id.to_string();
        trace
    };

    // 同日 3 条 → 验证行号是文件内 0-based；次日 1 条 → 验证能跨文件命中
    let ids: Vec<String> = (0..3).map(|i| format!("trace-{i}")).collect();
    let body: Vec<String> = ids
        .iter()
        .map(|id| serde_json::to_string(&make(id)).unwrap())
        .collect();
    std::fs::write(dir.join("20260101.jsonl"), body.join("\n") + "\n").unwrap();
    std::fs::write(
        dir.join("20260102.jsonl"),
        serde_json::to_string(&make("trace-next-day")).unwrap() + "\n",
    )
    .unwrap();

    let found = resolve_trace_positions_at(tmp.path(), agent_id, &ids).unwrap();
    assert_eq!(found.len(), 3, "三条同日 trace 都应命中");
    assert_eq!(found[&ids[0]].date_filename, "20260101.jsonl");
    assert_eq!(found[&ids[0]].line_number, 0);
    assert_eq!(found[&ids[2]].line_number, 2, "行号是文件内 0-based");

    let cross =
        resolve_trace_positions_at(tmp.path(), agent_id, &["trace-next-day".to_string()]).unwrap();
    assert_eq!(cross["trace-next-day"].date_filename, "20260102.jsonl");
    assert_eq!(cross["trace-next-day"].line_number, 0);

    // 未命中缺席而非报错：引用关系本身仍成立，只是没有物理坐标
    let partial = resolve_trace_positions_at(
        tmp.path(),
        agent_id,
        &[ids[0].clone(), "trace-missing".to_string()],
    )
    .unwrap();
    assert_eq!(partial.len(), 1);
    assert!(partial.contains_key(&ids[0]));
    assert!(!partial.contains_key("trace-missing"));

    // 空入参直接返回空，不做无谓扫盘
    assert!(
        resolve_trace_positions_at(tmp.path(), agent_id, &[])
            .unwrap()
            .is_empty()
    );
}

// ==================== 旁车索引（sidecar index）====================

/// 索引轻量：不含 input 原文，但含查询字段 + 引用链行号
#[test]
fn sidecar_memory_index_is_lightweight_and_carries_line_number() {
    let dir = tempfile::tempdir().unwrap();
    let day = chrono::Local
        .with_ymd_and_hms(2026, 1, 1, 12, 0, 0)
        .unwrap()
        .timestamp();
    let secret = "UNIQUE_MEMORY_INPUT_SENTINEL_7c2e";
    let mut trace = trace_at("agent-idx", "mem-idx-1", "log-idx", Some("task-idx"), day);
    trace.input = secret.to_string();
    write_trace_shard(dir.path(), "agent-idx", "20260101", &[trace]);

    // 直接写原文（无索引）→ 查询触发惰性重建
    let page = query_traces_at(
        dir.path(),
        &MemoryTraceQuery {
            agent_id: Some("agent-idx".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].input, secret);

    let memory_dir = dir.path().join("agents").join("agent-idx").join("memory");
    let idx_files: Vec<_> = std::fs::read_dir(&memory_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "idx"))
        .collect();
    assert_eq!(idx_files.len(), 1, "应生成旁车索引");
    let idx_content = std::fs::read_to_string(&idx_files[0]).unwrap();
    assert!(
        !idx_content.contains(secret),
        "索引不应包含 input 原文（几十 KB 大字段）"
    );
    assert!(idx_content.contains("mem-idx-1"), "索引应含 trace_id");
    // 引用链行号由下方 sidecar_resolve_positions_keeps_physical_line_numbers 用行为验证，
    // 不断言序列化后的字段名（那是实现细节，改名不应让测试失败）。
}

/// 存量无索引文件的引用链解析：line_number 与物理行一致
#[test]
fn sidecar_resolve_positions_keeps_physical_line_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let day = chrono::Local
        .with_ymd_and_hms(2026, 1, 1, 12, 0, 0)
        .unwrap()
        .timestamp();
    write_trace_shard(
        dir.path(),
        "agent-pos",
        "20260101",
        &[
            trace_at("agent-pos", "pos-0", "log-0", None, day),
            trace_at("agent-pos", "pos-1", "log-1", None, day + 1),
            trace_at("agent-pos", "pos-2", "log-2", None, day + 2),
        ],
    );

    let found = resolve_trace_positions_at(
        dir.path(),
        "agent-pos",
        &["pos-0".to_string(), "pos-2".to_string()],
    )
    .unwrap();
    assert_eq!(found["pos-0"].line_number, 0);
    assert_eq!(found["pos-2"].line_number, 2);
    // 引用链解析本身也会建索引
    let memory_dir = dir.path().join("agents").join("agent-pos").join("memory");
    assert!(std::fs::read_dir(&memory_dir).unwrap().any(|e| {
        e.unwrap()
            .path()
            .extension()
            .is_some_and(|ext| ext == "idx")
    }));
}

/// 索引落后（原文新增但索引没跟上）时增量补齐，不漏新行
#[test]
fn sidecar_memory_backfills_lagging_index() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let day = chrono::Local
        .with_ymd_and_hms(2026, 1, 1, 12, 0, 0)
        .unwrap()
        .timestamp();
    write_trace_shard(
        dir.path(),
        "agent-lag",
        "20260101",
        &[trace_at("agent-lag", "lag-a", "log-a", None, day)],
    );

    // 首次查询建索引（只覆盖 lag-a）
    query_traces_at(
        dir.path(),
        &MemoryTraceQuery {
            agent_id: Some("agent-lag".to_string()),
            ..Default::default()
        },
    )
    .unwrap();

    // 直接追加一行原文，制造「原文有、索引无」的落后窗口
    let memory_dir = dir.path().join("agents").join("agent-lag").join("memory");
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(memory_dir.join("20260101.jsonl"))
            .unwrap();
        let mut t = trace_at("agent-lag", "lag-b", "log-b", None, day + 1);
        t.input = "later appended input".to_string();
        file.write_all(serde_json::to_string(&t).unwrap().as_bytes())
            .unwrap();
        file.write_all(b"\n").unwrap();
    }

    let page = query_traces_at(
        dir.path(),
        &MemoryTraceQuery {
            agent_id: Some("agent-lag".to_string()),
            trace_id: Some("lag-b".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(page.items.len(), 1, "落后索引应增量补齐，lag-b 不应丢");
    assert_eq!(page.items[0].id, "lag-b");
}

/// M1 回归：原文含坏行 + 索引落后时，增量补齐的物理行号必须按真实物理行算，
/// 不能用「合法索引记录数」近似——否则坏行会让后续行的 line_number 偏小，
/// 引用链据此 read_line 会读到另一条记忆。
#[test]
fn sidecar_line_number_stays_physical_with_corrupted_line_and_backfill() {
    use std::io::Write;

    let dir = tempfile::tempdir().unwrap();
    let agent_id = "agent-line-phys";
    let memory_dir = dir.path().join("agents").join(agent_id).join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();

    let day = chrono::Local
        .with_ymd_and_hms(2026, 1, 1, 12, 0, 0)
        .unwrap()
        .timestamp();
    // 物理行 0=好行 trace-0，1=坏行，2=好行 trace-2
    let good0 = serde_json::to_string(&trace_at(agent_id, "phys-0", "log", None, day)).unwrap();
    let good2 = serde_json::to_string(&trace_at(agent_id, "phys-2", "log", None, day + 2)).unwrap();
    std::fs::write(
        memory_dir.join("20260101.jsonl"),
        format!("{good0}\nNOT_VALID_JSON\n{good2}\n"),
    )
    .unwrap();

    // 首次查询建索引：坏行跳过，trace-0/2 的行号应分别是 0 和 2
    let found = resolve_trace_positions_at(
        dir.path(),
        agent_id,
        &["phys-0".to_string(), "phys-2".to_string()],
    )
    .unwrap();
    assert_eq!(found["phys-0"].line_number, 0);
    assert_eq!(found["phys-2"].line_number, 2, "坏行也占物理行号");

    // 旁路追加物理行 3=好行 phys-3，制造索引落后窗口
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(memory_dir.join("20260101.jsonl"))
            .unwrap();
        file.write_all(
            serde_json::to_string(&trace_at(agent_id, "phys-3", "log", None, day + 3))
                .unwrap()
                .as_bytes(),
        )
        .unwrap();
        file.write_all(b"\n").unwrap();
    }

    // 再次解析触发增量补齐：phys-3 的真实物理行号是 3，不是合法记录数 2
    let found = resolve_trace_positions_at(dir.path(), agent_id, &["phys-3".to_string()]).unwrap();
    assert_eq!(
        found["phys-3"].line_number, 3,
        "增量补齐行号必须按原文物理行算，坏行不能被忽略后把后续行号前移"
    );
}
