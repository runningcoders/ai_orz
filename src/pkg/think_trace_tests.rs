//! think_trace 查询单测（写入用真实 JSONL，查询走 `query_at` 避免全局单例）

use super::*;
use std::fs::OpenOptions;
use std::io::Write;
use tempfile::tempdir;

fn today() -> String {
    chrono::Local::now().format("%Y%m%d").to_string()
}

fn entry(run_id: &str, agent_id: &str, created_at: u64) -> ThinkTraceEntry {
    ThinkTraceEntry {
        run_id: run_id.to_string(),
        agent_id: agent_id.to_string(),
        scene: "awaken".to_string(),
        path: "think_round".to_string(),
        steps: 5,
        advisor_calls: 0,
        fingerprint: "fp".to_string(),
        occurrences: vec![ThinkTraceOccurrence {
            node: "think".to_string(),
            seq: 0,
            step: 0,
            signal: Some("done".to_string()),
        }],
        routes: vec![ThinkTraceRoute {
            step: 0,
            candidates: vec!["think".to_string()],
            chosen: "think".to_string(),
            via: "unique".to_string(),
            reason: None,
        }],
        created_at,
        project_id: None,
        task_id: None,
        rounds: Vec::new(),
    }
}

fn write_entry(base: &Path, agent_id: &str, file_date: &str, entry: &ThinkTraceEntry) {
    let dir = paths::agent_think_trace_dir(base, agent_id);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{file_date}.jsonl"));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    writeln!(file, "{}", serde_json::to_string(entry).unwrap()).unwrap();
}

fn query(base: &Path, query: ThinkTraceQuery) -> PagedResult<ThinkTraceEntry> {
    query_at(base, &query).unwrap()
}

#[test]
fn query_filters_by_agent_and_sorts_desc() {
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();
    write_entry(base, "agent-a", &date, &entry("run-1", "agent-a", 1_000));
    write_entry(base, "agent-a", &date, &entry("run-3", "agent-a", 3_000));
    write_entry(base, "agent-b", &date, &entry("run-2", "agent-b", 2_000));

    let page = query(
        base,
        ThinkTraceQuery {
            agent_id: Some("agent-a".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(page.total, 2);
    let ids: Vec<&str> = page.items.iter().map(|e| e.run_id.as_str()).collect();
    assert_eq!(ids, vec!["run-3", "run-1"]);
}

#[test]
fn query_scans_all_agents_without_agent_filter() {
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();
    write_entry(base, "agent-a", &date, &entry("run-1", "agent-a", 1_000));
    write_entry(base, "agent-b", &date, &entry("run-2", "agent-b", 2_000));

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(page.total, 2);
    let ids: Vec<&str> = page.items.iter().map(|e| e.run_id.as_str()).collect();
    assert_eq!(ids, vec!["run-2", "run-1"]);
}

#[test]
fn query_filters_project_and_scene() {
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();
    let mut p1_settle = entry("run-1", "agent-a", 1_000);
    p1_settle.project_id = Some("p1".to_string());
    p1_settle.scene = "settle".to_string();
    let mut p1_awaken = entry("run-2", "agent-a", 2_000);
    p1_awaken.project_id = Some("p1".to_string());
    let mut p2_settle = entry("run-3", "agent-b", 3_000);
    p2_settle.project_id = Some("p2".to_string());
    p2_settle.scene = "settle".to_string();
    write_entry(base, "agent-a", &date, &p1_settle);
    write_entry(base, "agent-a", &date, &p1_awaken);
    write_entry(base, "agent-b", &date, &p2_settle);

    let page = query(
        base,
        ThinkTraceQuery {
            project_id: Some("p1".to_string()),
            scene: Some("settle".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].run_id, "run-1");
}

#[test]
fn query_filters_by_run_id_exact_match() {
    // 记忆详情凭自身 id 反查思考过程：run_id 必须是精确匹配，不能前缀命中
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();
    write_entry(base, "agent-a", &date, &entry("trace-1", "agent-a", 1_000));
    write_entry(base, "agent-a", &date, &entry("trace-10", "agent-a", 2_000));

    let page = query(
        base,
        ThinkTraceQuery {
            run_id: Some("trace-1".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].run_id, "trace-1");
}

#[test]
fn query_paginates_with_total() {
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();
    for i in 0..5 {
        write_entry(
            base,
            "agent-a",
            &date,
            &entry(&format!("run-{i}"), "agent-a", i as u64 * 1_000),
        );
    }

    let page = query(
        base,
        ThinkTraceQuery {
            agent_id: Some("agent-a".to_string()),
            pagination: PaginationParams {
                limit: Some(2),
                offset: Some(2),
            },
            ..Default::default()
        },
    );
    assert_eq!(page.total, 5);
    let ids: Vec<&str> = page.items.iter().map(|e| e.run_id.as_str()).collect();
    assert_eq!(ids, vec!["run-2", "run-1"]);
}

#[test]
fn query_filters_by_created_at_window() {
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();
    write_entry(base, "agent-a", &date, &entry("old", "agent-a", 1_000));
    write_entry(base, "agent-a", &date, &entry("new", "agent-a", 5_000));

    let page = query(
        base,
        ThinkTraceQuery {
            agent_id: Some("agent-a".to_string()),
            started_after: Some(3_000),
            ..Default::default()
        },
    );
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].run_id, "new");
}

#[test]
fn query_rejects_limit_above_maximum() {
    let dir = tempdir().unwrap();
    let err = query_at(
        dir.path(),
        &ThinkTraceQuery {
            pagination: PaginationParams {
                limit: Some(MAX_THINK_TRACE_QUERY_LIMIT + 1),
                offset: None,
            },
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("exceeds maximum"));
}

#[test]
fn entry_parses_legacy_line_without_new_fields() {
    // 历史行没有 created_at / project_id / task_id / rounds，读取不能失败
    let line = r#"{"run_id":"r1","agent_id":"a1","scene":"awaken","path":"think_round","steps":1,"advisor_calls":0,"fingerprint":"fp","occurrences":[],"routes":[]}"#;
    let parsed: ThinkTraceEntry = serde_json::from_str(line).unwrap();
    assert_eq!(parsed.created_at, 0);
    assert!(parsed.project_id.is_none());
    assert!(parsed.task_id.is_none());
    assert!(parsed.rounds.is_empty());
}

#[test]
fn round_round_trips_thinking_intensity() {
    let mut trace = entry("run-2", "agent-a", 1_000);
    trace.rounds = vec![ThinkTraceRound {
        round: 1,
        thinking_intensity: Some(ThinkingIntensity::new(50)),
        ..Default::default()
    }];

    let json = serde_json::to_string(&trace).unwrap();
    // 扣在轮上而非 run 上：未来逐轮动态调整时存储结构无需变更
    assert!(json.contains("\"thinking_intensity\":50"), "json: {json}");
    let parsed: ThinkTraceEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed.rounds[0].thinking_intensity,
        Some(ThinkingIntensity::new(50))
    );
}

#[test]
fn entry_round_trips_round_details() {
    let mut trace = entry("run-1", "agent-a", 1_000);
    trace.rounds = vec![ThinkTraceRound {
        round: 1,
        assistant_text: Some("我先查一下".to_string()),
        reasoning: vec!["用户想要历史记录".to_string()],
        tool_calls: vec![ThinkTraceToolCall {
            call_id: Some("call-1".to_string()),
            name: "search_memory".to_string(),
            arguments: "{\"q\":\"历史\"}".to_string(),
            result: Some("命中 2 条".to_string()),
            status: "ok".to_string(),
        }],
        input_tokens: 120,
        output_tokens: 45,
        cached_tokens: 30,
        reasoning_tokens: 12,
        duration_ms: 900,
        decision: Some("continue".to_string()),
        thinking_intensity: Some(ThinkingIntensity::new(42)),
    }];

    let json = serde_json::to_string(&trace).unwrap();
    let parsed: ThinkTraceEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.rounds, trace.rounds);
}
