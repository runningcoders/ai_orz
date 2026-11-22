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
        cerebellum: Vec::new(),
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
        kind: TraceLineKind::Legacy,
        fact: None,
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
fn query_filters_by_agent_ids_set() {
    // 按 Agent 集合圈定范围，集合外的 Agent 一条都不能带出
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();
    write_entry(base, "agent-a", &date, &entry("run-1", "agent-a", 1_000));
    write_entry(base, "agent-b", &date, &entry("run-2", "agent-b", 2_000));
    write_entry(base, "agent-c", &date, &entry("run-3", "agent-c", 3_000));

    let page = query(
        base,
        ThinkTraceQuery {
            agent_ids: Some(vec!["agent-a".to_string(), "agent-c".to_string()]),
            ..Default::default()
        },
    );
    assert_eq!(page.total, 2);
    let ids: Vec<&str> = page.items.iter().map(|e| e.run_id.as_str()).collect();
    assert_eq!(ids, vec!["run-3", "run-1"]);
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
fn query_without_project_excludes_project_scoped_runs() {
    // 默认对话「思考」Tab：只看未归属项目的游离轨迹
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();
    let mut scoped = entry("run-scoped", "agent-a", 1_000);
    scoped.project_id = Some("p1".to_string());
    write_entry(base, "agent-a", &date, &entry("run-free", "agent-a", 2_000));
    write_entry(base, "agent-a", &date, &scoped);

    let page = query(
        base,
        ThinkTraceQuery {
            without_project: true,
            ..Default::default()
        },
    );
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].run_id, "run-free");
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
            intent: Some("查历史记录用于比对".to_string()),
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

// ---------- 写穿观测事实：行形态是扩展契约 ----------

fn cerebellum(scene: &str, decision: &str) -> ThinkTraceCerebellum {
    ThinkTraceCerebellum {
        scene: scene.to_string(),
        decision: decision.to_string(),
        ..Default::default()
    }
}

fn scope() -> TraceSink {
    TraceSink::new("agent-a", "run-a", "awaken").with_scope(Some("proj-1"), Some("task-1"))
}

#[test]
fn fact_entry_carries_identity_and_flattens_payload() {
    let entry = scope()
        .fact_entry(
            "cerebellum",
            serde_json::json!({ "cerebellum": [cerebellum("chain_reply", "close")] }),
        )
        .expect("完整身份必须可写");

    assert_eq!(entry["kind"], "fact");
    assert_eq!(entry["fact"], "cerebellum");
    // 身份字段全带：查询侧靠 (agent_id, run_id, scene) 归并回同一条轨迹
    assert_eq!(entry["agent_id"], "agent-a");
    assert_eq!(entry["run_id"], "run-a");
    assert_eq!(entry["scene"], "awaken");
    assert_eq!(entry["project_id"], "proj-1");
    assert_eq!(entry["task_id"], "task-1");
    // 载荷平铺到顶层（而不是塞进 payload），便于按字段名检索
    assert_eq!(entry["cerebellum"].as_array().unwrap().len(), 1);
    assert!(entry.get("payload").is_none());
}

#[test]
fn fact_entry_ignores_reserved_keys_and_requires_identity() {
    // 载荷试图覆盖身份字段 ⇒ 一律忽略，轨迹的归属不能被载荷改写
    let entry = scope()
        .fact_entry(
            "custom",
            serde_json::json!({
                "run_id": "hijacked",
                "kind": "close",
                // 合并语义字段：若被载荷写入，前端会把半成品当已完成
                "partial": false,
                "rounds": [{ "round": 99 }],
                "payload_value": 7,
            }),
        )
        .expect("完整身份必须可写");
    assert_eq!(entry["run_id"], "run-a", "身份字段不许被载荷覆盖");
    assert_eq!(entry["kind"], "fact", "行种类不许被载荷覆盖");
    assert!(
        entry.get("partial").is_none(),
        "合并语义字段不许被载荷覆盖（否则轨迹的收尾状态会被改写）"
    );
    assert!(entry.get("rounds").is_none(), "轮次字段不许被载荷覆盖");
    assert_eq!(entry["payload_value"], 7, "非保留键正常平铺");

    // 非对象载荷收进 payload，不破坏行结构
    let scalar = scope()
        .fact_entry("custom", serde_json::json!(42))
        .expect("完整身份必须可写");
    assert_eq!(scalar["payload"], 42);

    // 空 run_id ⇒ 没有可归属的轨迹，直接丢弃（与旧 record 的「空键丢弃」同义）
    assert!(
        TraceSink::new("agent-a", "", "awaken")
            .fact_entry("x", serde_json::json!({}))
            .is_none(),
        "空 run_id 不得产生任何行"
    );
}

#[test]
fn fact_line_after_close_keeps_run_finished() {
    // 反事实：chain_reply 的判定跑在 awaken 返回之后，留痕必然落在收尾行**之后**。
    // 写穿方案下它是独立一行（kind=fact, partial=true），合并时既不能把轮次
    // 冲掉，也不能把已完成的轨迹重新标成「半成品」。
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();

    // ① 收尾行：1 条 run 级留痕 + 2 轮
    let mut close_line = entry("run-chain", "agent-a", 1_000);
    close_line.kind = TraceLineKind::Close;
    close_line.cerebellum = vec![cerebellum("thinking_strength.awaken", "70")];
    close_line.rounds = vec![round(1), round(2)];
    close_line.steps = 5;
    write_entry(base, "agent-a", &date, &close_line);

    // ② 写穿的事实行：partial=true、只带自己的载荷
    let mut fact_line = entry("run-chain", "agent-a", 2_000);
    fact_line.kind = TraceLineKind::Fact;
    fact_line.partial = true;
    fact_line.cerebellum = vec![cerebellum("chain_reply", "close")];
    fact_line.rounds = Vec::new();
    fact_line.occurrences = Vec::new();
    fact_line.routes = Vec::new();
    fact_line.steps = 0;
    fact_line.fingerprint = String::new();
    write_entry(base, "agent-a", &date, &fact_line);

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(page.total, 1, "事实行必须合并回原轨迹，不能变成第二条卡片");
    let merged = &page.items[0];
    assert_eq!(
        merged
            .cerebellum
            .iter()
            .map(|c| c.scene.as_str())
            .collect::<Vec<_>>(),
        vec!["thinking_strength.awaken", "chain_reply"],
        "两段留痕都要在：去掉 extend 这条立刻变红"
    );
    assert_eq!(
        merged.rounds.iter().map(|r| r.round).collect::<Vec<_>>(),
        vec![1, 2],
        "事实行不得清空或追加轮次"
    );
    assert_eq!(merged.steps, 5, "事实行的 steps=0 不得把收尾行的步数冲掉");
    assert!(
        !merged.partial,
        "收尾之后再来事实行，仍然必须是「已完成」——否则前端会永远显示在跑"
    );
}

#[test]
fn fact_line_first_does_not_leave_path_empty() {
    // 入口门的小脑留痕发生在第一轮之前 ⇒ 事实行可能比任何轮次行都早。
    // 合并结果必须补上收尾行才有的图执行信息（path / steps / occurrences）。
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();

    let mut early = entry("run-early-fact", "agent-a", 1_000);
    early.kind = TraceLineKind::Fact;
    // 注意：这里刻意**不**设 partial —— 写穿的事实行落盘时不含该字段
    // （`TraceSink::fact` 只写 kind/fact/载荷），反序列化得到 false
    early.partial = false;
    early.path = String::new();
    early.steps = 0;
    early.occurrences = Vec::new();
    early.routes = Vec::new();
    early.cerebellum = vec![cerebellum("entry_gate", "proceed")];
    write_entry(base, "agent-a", &date, &early);

    let mut close_line = entry("run-early-fact", "agent-a", 2_000);
    close_line.kind = TraceLineKind::Close;
    close_line.rounds = vec![round(1)];
    close_line.steps = 3;
    write_entry(base, "agent-a", &date, &close_line);

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(page.total, 1);
    let merged = &page.items[0];
    assert_eq!(
        merged.path, "think_round",
        "首个片段是事实行时，path 必须从收尾行补上（否则面板拿不到蓝图 ID）"
    );
    assert_eq!(merged.steps, 3, "图步数按收尾行累计");
    assert_eq!(merged.rounds.len(), 1);
    assert_eq!(merged.cerebellum.len(), 1);
    assert!(!merged.partial);
}

#[test]
fn fact_line_alone_is_queryable_as_in_progress() {
    // 只有事实行（run 尚未收尾）也必须能查到：写穿的意义就是「不用等收尾」
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();

    let mut fact_line = entry("run-early", "agent-a", 1_000);
    fact_line.kind = TraceLineKind::Fact;
    // 真实落盘形态：事实行不带 partial（反序列化默认 false），
    // 「未收尾」必须由 merge 侧按 kind 判出来
    fact_line.partial = false;
    fact_line.cerebellum = vec![cerebellum("entry_gate", "proceed")];
    fact_line.occurrences = Vec::new();
    fact_line.routes = Vec::new();
    fact_line.steps = 0;
    write_entry(base, "agent-a", &date, &fact_line);

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].cerebellum.len(), 1);
    assert_eq!(page.items[0].cerebellum[0].scene, "entry_gate");
    assert!(page.items[0].partial, "没有收尾行时仍是半成品");
}

#[test]
fn finish_guard_closes_early_exit_run_without_duplicate() {
    // 小脑 Direct / 预算耗尽 / 超时等早退路径不经过 `persist_trace`：
    // 没有 guard 就只剩 fact 行，查询侧会永久显示「进行中」。
    let scope = TraceSink::memory("agent-a", "run-direct", "awaken");
    scope.cerebellum(cerebellum("entry_gate", "direct"));
    {
        let mut guard = TraceFinishGuard::new(scope.clone(), "finished");
        guard.set_reason("cerebellum_direct");
    }
    let lines = scope.recorded();
    assert_eq!(lines.len(), 2, "一条事实行 + 一行兜底 close");
    assert_eq!(lines[1]["kind"], "close");
    assert_eq!(lines[1]["close_reason"], "cerebellum_direct");
    assert_eq!(lines[1]["partial"], false);

    // 图正常跑完时 persist_trace 先 mark_closed，guard 不得再写第二行 close
    let ok_scope = TraceSink::memory("agent-a", "run-ok", "awaken");
    ok_scope.mark_closed();
    drop(TraceFinishGuard::new(ok_scope.clone(), "finished"));
    assert!(ok_scope.recorded().is_empty(), "收尾行必须只写一次");

    // 端到端：真实落盘字节（fact + guard close）查询后必须是「已结束」
    let dir = tempdir().unwrap();
    let base = dir.path();
    let trace_dir = paths::agent_think_trace_dir(base, "agent-a");
    fs::create_dir_all(&trace_dir).unwrap();
    let mut raw = lines.clone();
    raw[0]["created_at"] = 1_000.into();
    raw[1]["created_at"] = 2_000.into();
    let body = raw
        .iter()
        .map(|line| serde_json::to_string(line).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        trace_dir.join(format!("{}.jsonl", today())),
        format!("{body}\n"),
    )
    .unwrap();

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(page.total, 1);
    assert!(
        !page.items[0].partial,
        "早退路径也必须关闭半成品标记，不能永远显示进行中"
    );
    assert_eq!(page.items[0].cerebellum.len(), 1);
}

#[test]
fn write_through_fact_line_has_no_partial_field() {
    // 契约：`TraceSink` 写穿的事实行**不含** `partial` 字段。
    // `merge_same_run` 因此必须按 `kind` 判首行的半成品状态，
    // 不能照搬字段值（否则入口门留痕在前时 run 会显示已完成）。
    let scope = TraceSink::memory("agent-a", "run-fact-shape", "awaken");
    scope.cerebellum(cerebellum("wake_gate", "proceed"));
    let lines = scope.recorded();
    assert_eq!(lines.len(), 1, "一条留痕 = 一行");
    assert_eq!(lines[0]["kind"], "fact");
    assert!(
        lines[0].get("partial").is_none(),
        "事实行不带 partial，收尾语义只由 kind 决定"
    );

    // 端到端：把真实落盘字节写进临时 JSONL，查询后必须是「半成品」
    let dir = tempdir().unwrap();
    let base = dir.path();
    let trace_dir = paths::agent_think_trace_dir(base, "agent-a");
    fs::create_dir_all(&trace_dir).unwrap();
    let mut raw = lines[0].clone();
    raw["created_at"] = 1_000.into();
    let file = trace_dir.join(format!("{}.jsonl", today()));
    fs::write(&file, format!("{}\n", serde_json::to_string(&raw).unwrap())).unwrap();

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(page.total, 1);
    assert_eq!(
        page.items[0].kind,
        TraceLineKind::Fact,
        "读回来的行种类必须是 fact（否则合并语义无法判定）"
    );
    assert!(
        page.items[0].partial,
        "只有事实行、收尾行未到时，合并结果必须是半成品"
    );
    assert_eq!(page.items[0].cerebellum.len(), 1);

    // 收尾行到达 ⇒ 关闭半成品标记
    let mut close_line = entry("run-fact-shape", "agent-a", 2_000);
    close_line.kind = TraceLineKind::Close;
    write_entry(base, "agent-a", &today(), &close_line);
    let page = query(base, ThinkTraceQuery::default());
    assert!(!page.items[0].partial, "收尾行必须关闭半成品标记");
}

// ==================== 旁车索引（sidecar index）====================

/// 索引轻量：不含 rounds/reasoning 大载荷，但含过滤字段
#[test]
fn sidecar_think_index_is_lightweight_and_queryable() {
    let base = tempdir().unwrap();
    let date = today();
    let mut e = entry("think-idx-run", "agent-idx", 1_000);
    e.rounds = vec![ThinkTraceRound {
        assistant_text: Some("UNIQUE_THINK_ROUND_SENTINEL_5b1f".to_string()),
        reasoning: vec!["deep reasoning payload".to_string()],
        ..Default::default()
    }];
    write_entry(base.path(), "agent-idx", &date, &e);

    // 直接写原文（无索引）→ 查询触发惰性重建
    let result = query(
        base.path(),
        ThinkTraceQuery {
            agent_id: Some("agent-idx".to_string()),
            run_id: Some("think-idx-run".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(result.items.len(), 1);
    assert_eq!(
        result.items[0].rounds[0].assistant_text.as_deref(),
        Some("UNIQUE_THINK_ROUND_SENTINEL_5b1f")
    );

    let trace_dir = paths::agent_think_trace_dir(base.path(), "agent-idx");
    let idx_files: Vec<_> = fs::read_dir(&trace_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "idx"))
        .collect();
    assert_eq!(idx_files.len(), 1);
    let idx_content = fs::read_to_string(&idx_files[0]).unwrap();
    assert!(
        !idx_content.contains("UNIQUE_THINK_ROUND_SENTINEL_5b1f"),
        "索引不应包含 rounds/reasoning 大载荷"
    );
    assert!(idx_content.contains("think-idx-run"), "索引应含 run_id");
}

/// 索引落后时增量补齐 + merge 语义不变（partial + final 合并成一整条）
#[test]
fn sidecar_think_backfills_and_preserves_merge() {
    let base = tempdir().unwrap();
    let date = today();

    // 先写 partial（无索引）→ 查询建索引
    let mut partial = entry("think-lag-run", "agent-lag", 1_000);
    partial.partial = true;
    partial.rounds = vec![ThinkTraceRound {
        round: 0,
        assistant_text: Some("round-0".to_string()),
        ..Default::default()
    }];
    write_entry(base.path(), "agent-lag", &date, &partial);
    let first = query(
        base.path(),
        ThinkTraceQuery {
            agent_id: Some("agent-lag".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(first.items.len(), 1);

    // 直接追加 final 行（制造索引落后），含本片段 rounds
    let mut final_line = entry("think-lag-run", "agent-lag", 2_000);
    final_line.partial = false;
    final_line.rounds = vec![ThinkTraceRound {
        round: 1,
        assistant_text: Some("round-1".to_string()),
        ..Default::default()
    }];
    write_entry(base.path(), "agent-lag", &date, &final_line);

    let result = query(
        base.path(),
        ThinkTraceQuery {
            agent_id: Some("agent-lag".to_string()),
            run_id: Some("think-lag-run".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(result.total, 1, "同一 run 的 partial/final 应合并为一条");
    assert_eq!(result.items.len(), 1);
    // think 的既定语义：final 行替换本片段 partial 的 rounds（避免重复计数），
    // 所以这里验证「落后补齐的 final 行生效」，而非 partial + final 轮次叠加。
    let texts: Vec<_> = result.items[0]
        .rounds
        .iter()
        .filter_map(|r| r.assistant_text.as_deref())
        .collect();
    assert!(texts.contains(&"round-1"), "落后补齐的 final 行应生效");
}

/// 索引坏行时全量重建，不吞记录
#[test]
fn sidecar_think_rebuilds_corrupted_index() {
    use std::io::Write;

    let base = tempdir().unwrap();
    let date = today();
    write_entry(
        base.path(),
        "agent-corrupt",
        &date,
        &entry("corrupt-run", "agent-corrupt", 1_000),
    );
    // 建索引
    query(
        base.path(),
        ThinkTraceQuery {
            agent_id: Some("agent-corrupt".to_string()),
            ..Default::default()
        },
    );

    // 往索引追加坏行
    let trace_dir = paths::agent_think_trace_dir(base.path(), "agent-corrupt");
    let idx_path = fs::read_dir(&trace_dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|ext| ext == "idx"))
        .unwrap();
    {
        let mut f = OpenOptions::new().append(true).open(&idx_path).unwrap();
        f.write_all(b"NOT_VALID_JSON\n").unwrap();
    }

    let result = query(
        base.path(),
        ThinkTraceQuery {
            agent_id: Some("agent-corrupt".to_string()),
            run_id: Some("corrupt-run".to_string()),
            ..Default::default()
        },
    );
    assert_eq!(result.items.len(), 1, "坏行应触发全量重建，不吞记录");
    assert_eq!(result.items[0].run_id, "corrupt-run");
}
