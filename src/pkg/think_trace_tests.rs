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
        partial: false,
        partial_round_count: 0,
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

fn occurrence(node: &str, seq: usize, step: usize) -> ThinkTraceOccurrence {
    ThinkTraceOccurrence {
        node: node.to_string(),
        seq,
        step,
        signal: None,
    }
}

fn route(step: usize) -> ThinkTraceRoute {
    ThinkTraceRoute {
        step,
        candidates: Vec::new(),
        chosen: String::new(),
        via: "unique".to_string(),
        reason: None,
    }
}

fn round(round: usize) -> ThinkTraceRound {
    ThinkTraceRound {
        round,
        ..Default::default()
    }
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

#[test]
fn query_merges_reruns_of_same_run_into_one_entry() {
    // 新消息触发意图修订：同一 `intent-{main}` run 内的思考循环会被重跑，
    // 每次追加一行。查询侧必须合成一个动作，轮次/步号/节点序号连续。
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();

    let mut first = entry("run-1", "agent-a", 1_000);
    first.steps = 2;
    first.occurrences = vec![occurrence("intent", 1, 1), occurrence("poll_front", 1, 2)];
    first.routes = vec![route(1), route(2)];
    first.rounds = vec![round(1), round(2)];

    let mut second = entry("run-1", "agent-a", 2_000);
    second.steps = 2;
    second.occurrences = vec![occurrence("intent", 1, 1), occurrence("poll_front", 1, 2)];
    second.routes = vec![route(1), route(2)];
    second.rounds = vec![round(1), round(2)];

    write_entry(base, "agent-a", &date, &first);
    write_entry(base, "agent-a", &date, &second);

    let page = query(
        base,
        ThinkTraceQuery {
            agent_id: Some("agent-a".to_string()),
            ..Default::default()
        },
    );

    assert_eq!(page.total, 1, "同一 run 的重跑必须合成一条轨迹");
    let merged = &page.items[0];
    assert_eq!(merged.created_at, 1_000, "created_at 取 run 起点");
    assert_eq!(merged.steps, 4, "步数应累计");
    assert_eq!(
        merged.rounds.iter().map(|r| r.round).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "从 1 重新计数的轮次应接续为 3、4"
    );
    assert_eq!(
        merged
            .occurrences
            .iter()
            .map(|o| (o.node.as_str(), o.seq, o.step))
            .collect::<Vec<_>>(),
        vec![
            ("intent", 1, 1),
            ("poll_front", 1, 2),
            ("intent", 2, 3),
            ("poll_front", 2, 4),
        ],
        "节点序号与图步号必须跨片段连续"
    );
    assert_eq!(
        merged.routes.iter().map(|r| r.step).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert_eq!(
        merged.fingerprint, "intent#1>poll_front#1>intent#2>poll_front#2",
        "指纹应按合并后的 occurrence 序列重算"
    );
}

#[test]
fn query_keeps_continued_round_numbers_when_merging() {
    // 执行回退的思考循环已带唤醒级累计 `start_round`（3、4），合并时不得再次后移
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();

    let mut first = entry("run-1", "agent-a", 1_000);
    first.steps = 1;
    first.occurrences = vec![occurrence("think", 1, 1)];
    first.rounds = vec![round(1), round(2)];

    let mut second = entry("run-1", "agent-a", 2_000);
    second.steps = 1;
    second.occurrences = vec![occurrence("think", 1, 1)];
    second.rounds = vec![round(3), round(4)];

    write_entry(base, "agent-a", &date, &first);
    write_entry(base, "agent-a", &date, &second);

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(page.total, 1);
    assert_eq!(
        page.items[0]
            .rounds
            .iter()
            .map(|r| r.round)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "已是连续累计的轮次应原样保留"
    );
    assert_eq!(
        page.items[0]
            .occurrences
            .iter()
            .map(|o| o.seq)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

#[test]
fn in_progress_run_is_queryable_as_partial() {
    // 思考循环跑到一半：磁盘上只有逐轮增量行，没有收尾行。
    // 查询必须能拿到这个「半成品」，而不是等 run 结束后才可见。
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();

    let mut p1 = entry("run-1", "agent-a", 1_000);
    p1.steps = 0;
    p1.occurrences.clear();
    p1.routes.clear();
    p1.fingerprint.clear();
    p1.partial = true;
    p1.rounds = vec![round(1)];

    let mut p2 = entry("run-1", "agent-a", 2_000);
    p2.steps = 0;
    p2.occurrences.clear();
    p2.routes.clear();
    p2.fingerprint.clear();
    p2.partial = true;
    p2.rounds = vec![round(2)];

    write_entry(base, "agent-a", &date, &p1);
    write_entry(base, "agent-a", &date, &p2);

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(page.total, 1, "同一 run 的多条增量行必须合成一条");
    let merged = &page.items[0];
    assert_eq!(
        merged.rounds.iter().map(|r| r.round).collect::<Vec<_>>(),
        vec![1, 2],
        "运行中应能看到已完成轮次"
    );
    assert!(merged.partial, "无收尾行时仍是半成品");
    assert_eq!(merged.steps, 0);
}

#[test]
fn final_line_replaces_partial_rounds_without_duplication() {
    // 增量行（逐轮）+ 收尾行（全量）：收尾行的 rounds 必须替换本片段的增量行，
    // 不能与增量行拼接——否则每轮会出现两次。
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();

    let mut p1 = entry("run-1", "agent-a", 1_000);
    p1.steps = 0;
    p1.occurrences.clear();
    p1.routes.clear();
    p1.fingerprint.clear();
    p1.partial = true;
    p1.rounds = vec![round(1)];

    let mut p2 = entry("run-1", "agent-a", 2_000);
    p2.steps = 0;
    p2.occurrences.clear();
    p2.routes.clear();
    p2.fingerprint.clear();
    p2.partial = true;
    p2.rounds = vec![round(2)];

    let mut full = entry("run-1", "agent-a", 3_000);
    full.steps = 6;
    full.occurrences = vec![occurrence("think", 1, 1), occurrence("evaluate", 1, 2)];
    full.routes = vec![route(1), route(2)];
    full.rounds = vec![round(1), round(2)];
    full.partial = false;

    write_entry(base, "agent-a", &date, &p1);
    write_entry(base, "agent-a", &date, &p2);
    write_entry(base, "agent-a", &date, &full);

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(page.total, 1);
    let merged = &page.items[0];
    assert_eq!(
        merged.rounds.iter().map(|r| r.round).collect::<Vec<_>>(),
        vec![1, 2],
        "收尾行替换本片段增量行，轮次不得翻倍"
    );
    assert!(!merged.partial, "收尾后不再是半成品");
    assert_eq!(merged.steps, 6, "图执行信息只来自收尾行");
    assert_eq!(merged.occurrences.len(), 2);
    assert_eq!(merged.created_at, 1_000, "created_at 取 run 起点");
}

#[test]
fn rerun_fragments_keep_rounds_from_every_fragment() {
    // 一次唤醒内因意图修订重跑两次：真实落盘形态是每个片段各自
    // 「N 行增量 + 1 行收尾」。第二个片段的收尾行只能替换**本片段**的增量行，
    // 既不能把第一个片段的轮次一起冲掉，也不能让本片段轮次重复出现。
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();

    let partial = |ts: u64, r: usize| {
        let mut e = entry("run-1", "agent-a", ts);
        e.steps = 0;
        e.occurrences.clear();
        e.routes.clear();
        e.fingerprint.clear();
        e.partial = true;
        e.rounds = vec![round(r)];
        e
    };
    let final_line = |ts: u64, seq: usize| {
        let mut e = entry("run-1", "agent-a", ts);
        e.steps = 2;
        e.occurrences = vec![
            occurrence("think", seq, seq),
            occurrence("evaluate", seq, seq + 1),
        ];
        e.routes = vec![route(seq)];
        e.rounds = vec![round(1), round(2)];
        e.partial = false;
        e
    };

    // 片段 1：两轮增量 + 收尾
    write_entry(base, "agent-a", &date, &partial(1_000, 1));
    write_entry(base, "agent-a", &date, &partial(2_000, 2));
    write_entry(base, "agent-a", &date, &final_line(3_000, 1));
    // 片段 2：重跑，轮次从 1 重新计数
    write_entry(base, "agent-a", &date, &partial(4_000, 1));
    write_entry(base, "agent-a", &date, &partial(5_000, 2));
    write_entry(base, "agent-a", &date, &final_line(6_000, 2));

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(page.total, 1);
    let merged = &page.items[0];
    assert_eq!(
        merged.rounds.iter().map(|r| r.round).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "两个片段各 2 轮，必须连续且各出现一次"
    );
    assert_eq!(merged.steps, 4, "两个片段的图步数都要累计");
    assert_eq!(merged.occurrences.len(), 4, "两个片段的节点序列都要保留");
    assert!(!merged.partial, "收尾后不再是半成品");
}

#[test]
fn query_does_not_merge_same_run_id_across_scenes() {
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();

    let mut awaken = entry("run-1", "agent-a", 1_000);
    awaken.scene = "awaken".to_string();
    let mut settle = entry("run-1", "agent-a", 2_000);
    settle.scene = "settle".to_string();

    write_entry(base, "agent-a", &date, &awaken);
    write_entry(base, "agent-a", &date, &settle);

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(page.total, 2, "不同场景不是同一个动作，不能合并");
}
