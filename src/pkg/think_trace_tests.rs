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
    // 默认对话口径：只聚合「当前任务出现过的 Agent」，集合外的 Agent 一条都不能带出
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

// ---------- 小脑留痕缓冲：截断必须可计数（每 run 至多一次告警） ----------

fn cerebellum_item(i: usize) -> ThinkTraceCerebellum {
    ThinkTraceCerebellum {
        scene: format!("test_scene_{i}"),
        ..Default::default()
    }
}

#[test]
fn cerebellum_buffer_within_cap_drops_nothing() {
    let mut entry = CerebellumBufferEntry::default();
    for i in 0..CEREBELLUM_BUFFER_MAX_PER_RUN {
        entry.push(cerebellum_item(i));
    }
    let (items, dropped) = entry.drain();
    assert_eq!(items.len(), CEREBELLUM_BUFFER_MAX_PER_RUN);
    assert_eq!(dropped, 0, "未溢出时不应出现丢弃计数");
}

#[test]
fn cerebellum_buffer_overflow_counts_dropped_exactly_and_drains_once() {
    // 与 buffer 真数表同口径：最坏口径 58 条 - 上限 32 = 26 条被截断
    let overflow = 26;
    let mut entry = CerebellumBufferEntry::default();
    for i in 0..(CEREBELLUM_BUFFER_MAX_PER_RUN + overflow) {
        entry.push(cerebellum_item(i));
    }
    let (items, dropped) = entry.drain();
    assert_eq!(items.len(), CEREBELLUM_BUFFER_MAX_PER_RUN);
    assert_eq!(dropped, overflow, "丢弃条数必须精确可数，不能只说「满了」");
    // 告警挂在 drain 的 dropped>0 分支；drain 幂等清空 ⇒ 同一 run 不可能二次告警
    let (again, dropped_again) = entry.drain();
    assert!(again.is_empty());
    assert_eq!(dropped_again, 0);
}

#[test]
fn take_cerebellum_caps_items_and_clears_run_state() {
    // record_cerebellum 在落盘基址未注入时直接返回（无副作用设计），断言前先注入
    init(std::env::temp_dir().join("ai_orz_think_trace_buffer_test"));
    let run_id = "test-run-cerebellum-overflow";
    let _ = take_cerebellum(run_id); // 清场，避免跨用例串味
    for i in 0..(CEREBELLUM_BUFFER_MAX_PER_RUN + 5) {
        record_cerebellum(run_id, cerebellum_item(i));
    }
    assert_eq!(
        take_cerebellum(run_id).len(),
        CEREBELLUM_BUFFER_MAX_PER_RUN,
        "取出条数必须封顶在 CEREBELLUM_BUFFER_MAX_PER_RUN"
    );
    // 取走后本 run 状态被整体移除 ⇒ 第二次取空（也就不会有第二次告警）
    assert!(take_cerebellum(run_id).is_empty());
}

#[test]
fn cerebellum_buffer_capacity_is_64_after_raise() {
    // 锁死容量常量本身：31 处写入点 + 记账条目（放行/空输入/熔断开闸）共用
    // 同一批槽位，最坏口径 58 条 ⇒ 32 会稳定截断，64 覆盖并留 6 条余量。
    // 这条断言刻意写死字面量 64：它回答的是「AMan 拍板的 32→64 有没有真生效」，
    // 跟着常量走就永远为真、失去判据意义。
    assert_eq!(
        CEREBELLUM_BUFFER_MAX_PER_RUN, 64,
        "缓冲上限必须是 64（32→64 是拍板项，不得被无意改回）"
    );

    // 边界行为以 64 为界：64 条全留、第 65 条开始计丢弃
    let mut exactly_cap = CerebellumBufferEntry::default();
    for i in 0..64 {
        exactly_cap.push(cerebellum_item(i));
    }
    let (items, dropped) = exactly_cap.drain();
    assert_eq!(items.len(), 64, "恰满 64 条时一条都不许丢");
    assert_eq!(dropped, 0, "恰满不算溢出");

    let mut over_cap = CerebellumBufferEntry::default();
    for i in 0..65 {
        over_cap.push(cerebellum_item(i));
    }
    let (items, dropped) = over_cap.drain();
    assert_eq!(items.len(), 64, "第 65 条必须被拒之门外");
    assert_eq!(dropped, 1, "溢出 1 条就要能数出来 1 条");
}

#[test]
fn cerebellum_buffer_worst_case_58_fits_within_capacity() {
    // 回归护栏：32 时代的溢出量是 58-32=26；升到 64 后同一最坏口径必须零丢弃。
    // 这条是「为什么改 64」的可判定表达 —— 若将来有人把容量改回 32，它会红。
    let worst_case = 58;
    let mut entry = CerebellumBufferEntry::default();
    for i in 0..worst_case {
        entry.push(cerebellum_item(i));
    }
    let (items, dropped) = entry.drain();
    assert_eq!(items.len(), worst_case, "最坏口径 58 条必须全部保留");
    assert_eq!(dropped, 0, "64 的取值依据就是让 58 条不触顶");
}

// ---------- chain_reply 补丁行：run 收尾之后再追加的二段留痕 ----------

fn cerebellum(scene: &str, decision: &str) -> ThinkTraceCerebellum {
    ThinkTraceCerebellum {
        scene: scene.to_string(),
        decision: decision.to_string(),
        ..Default::default()
    }
}

#[test]
fn cerebellum_patch_line_merges_into_finished_run() {
    // 现缺陷的反事实：chain_reply 判定跑在 run 收尾之后，留痕落进一个永不
    // take 的键 ⇒ 结构上永远查不到。修法是「补一次 take + 追加补丁行」。
    // 本用例锁的是**查询侧那一半**：补丁行必须合并回原轨迹，且不搅乱轮次。
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();

    // ① 收尾行：run 级留痕 1 条 + 2 轮
    let mut final_line_entry = entry("run-chain", "agent-a", 1_000);
    final_line_entry.cerebellum = vec![cerebellum("thinking_strength.awaken", "70")];
    final_line_entry.rounds = vec![round(1), round(2)];
    final_line_entry.steps = 5;
    write_entry(base, "agent-a", &date, &final_line_entry);

    // ② 补丁行：partial=false、空载荷（rounds/occurrences/routes/steps 全零），
    //    只带 chain_reply 那一条留痕
    let mut patch = entry("run-chain", "agent-a", 2_000);
    patch.cerebellum = vec![cerebellum("chain_reply", "close")];
    patch.rounds = Vec::new();
    patch.occurrences = Vec::new();
    patch.routes = Vec::new();
    patch.steps = 0;
    patch.fingerprint = String::new();
    write_entry(base, "agent-a", &date, &patch);

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(
        page.total, 1,
        "补丁行必须按 (agent_id, run_id, scene) 合并回原轨迹，不能变成第二条卡片"
    );
    let merged = &page.items[0];
    assert_eq!(
        merged
            .cerebellum
            .iter()
            .map(|c| c.scene.as_str())
            .collect::<Vec<_>>(),
        vec!["thinking_strength.awaken", "chain_reply"],
        "两段留痕都要在：去掉 merge_into 里的 cerebellum extend，这条立刻变红（chain_reply 那条丢失）"
    );
    assert_eq!(
        merged.rounds.iter().map(|r| r.round).collect::<Vec<_>>(),
        vec![1, 2],
        "空载荷补丁行到达时不得清空（它 partial=false 会走「摘本片段增量」分支，         而 base 的 partial_round_count 已是 0 ⇒ 零摘除）也不得追加轮次"
    );
    assert_eq!(merged.steps, 5, "补丁行的 steps=0 不得把收尾行的步数冲掉");
    assert_eq!(
        merged.created_at, 1_000,
        "created_at 保留最早片段（run 起点语义），补丁行不得把时间推到后面"
    );
    assert!(!merged.partial, "收尾后不再是半成品");
}

#[test]
fn cerebellum_patch_line_alone_is_still_queryable() {
    // 防御性：若收尾行因写盘失败/进程中断缺失，补丁行自身必须是可查询的完整条目
    // —— 否则「补丁都补上了却还是看不到」会变成新的静默失败。
    let dir = tempdir().unwrap();
    let base = dir.path();
    let date = today();

    let mut patch = entry("run-orphan", "agent-a", 1_000);
    patch.cerebellum = vec![cerebellum("chain_reply", "close")];
    patch.rounds = Vec::new();
    patch.occurrences = Vec::new();
    patch.routes = Vec::new();
    patch.steps = 0;
    patch.fingerprint = String::new();
    write_entry(base, "agent-a", &date, &patch);

    let page = query(base, ThinkTraceQuery::default());
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].cerebellum.len(), 1, "孤立补丁行也要能被查到");
    assert_eq!(page.items[0].cerebellum[0].scene, "chain_reply");
}

// ---------- flush_cerebellum_patch：take + 追加补丁行 ----------

#[test]
fn flush_cerebellum_patch_is_noop_without_any_record() {
    // 「没有留痕就不写空行」：静态命中 / 无压力 / 未装配这些零调用路径，
    // 每次 handle_agent_message 都会走一次 flush，若它无条件落盘就会污染轨迹。
    let count = crate::service::domain::runtime::flush_cerebellum_patch(
        "agent-flush-none",
        "run-flush-none",
        "awaken",
        None,
        None,
    );
    assert_eq!(count, 0, "没有留痕时必须返回 0 且不落盘");

    // 空 run_id 直接短路（record_cerebellum 对空键同样只丢弃）
    let count = crate::service::domain::runtime::flush_cerebellum_patch(
        "agent-flush-none",
        "",
        "awaken",
        None,
        None,
    );
    assert_eq!(count, 0, "空 run_id 不得触发任何写入");
}

#[test]
fn flush_cerebellum_patch_drains_run_buffer_and_reports_count() {
    // 与上一条分工：上一条锁「没东西时别乱写」，本条锁「有东西时真的取走并报数」。
    // 落盘基址是全局 OnceLock（可能已被其它用例设成某个 temp 目录），这里只断言
    // 返回值与「取走后缓冲为空」——两者都不依赖落盘成功。
    init(std::env::temp_dir().join("ai_orz_think_trace_flush_test"));
    let run_id = "test-run-flush-cerebellum-patch";
    let _ = take_cerebellum(run_id); // 清场防串味

    record_cerebellum(run_id, cerebellum("chain_reply", "close"));
    record_cerebellum(run_id, cerebellum("chain_reply", "continue"));

    let count = crate::service::domain::runtime::flush_cerebellum_patch(
        "agent-flush",
        run_id,
        "awaken",
        None,
        None,
    );
    assert_eq!(count, 2, "补写条数必须等于本 run 缓冲里的条数");
    assert!(
        take_cerebellum(run_id).is_empty(),
        "flush 必须把缓冲取走（take 语义），否则同一条留痕会被后续 flush 重复落盘"
    );
    // 幂等：再 flush 一次拿不到东西、不再落盘
    let again = crate::service::domain::runtime::flush_cerebellum_patch(
        "agent-flush",
        run_id,
        "awaken",
        None,
        None,
    );
    assert_eq!(again, 0, "同一条留痕不得被补写两次");
}
