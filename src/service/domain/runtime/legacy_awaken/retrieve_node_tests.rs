//! tests 单元测试（拆分自 retrieve_node.rs）
//!
//! 文件瘦身：原 1101 行 → 745 行，测试体 357 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::models::memory::{
    KnowledgeNodeRelationPo, LongTermKnowledgeNodePo, ShortTermMemoryIndexPo,
};
use common::enums::{KnowledgeRelationStatus, MemoryStatus, MemoryType};

fn node(id: &str, name: &str, desc: &str) -> Memory {
    Memory::new(MemoryPo::KnowledgeNode(LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: "agent-test".to_string(),
        node_name: name.to_string(),
        node_description: desc.to_string(),
        node_type: "concept".to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        user_tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 0,
        updated_at: 0,
    }))
}

fn relation(src: &str, dst: &str, kind: &str) -> Memory {
    relation_with_weight(src, dst, kind, None)
}

/// id 由「两端 + 关系类型」唯一决定：测试里要能构造同端点不同类型的两条边
fn relation_with_weight(src: &str, dst: &str, kind: &str, weight: Option<f32>) -> Memory {
    Memory::new(MemoryPo::Relation(KnowledgeNodeRelationPo {
        id: format!("rel-{src}-{dst}-{kind}"),
        source_node_id: src.to_string(),
        target_node_id: dst.to_string(),
        relation_type: kind.to_string(),
        weight,
        status: KnowledgeRelationStatus::Active,
        created_at: 0,
        updated_at: 0,
    }))
}

fn short_term(id: &str, summary: &str) -> Memory {
    Memory::new(MemoryPo::ShortTerm(ShortTermMemoryIndexPo {
        id: id.to_string(),
        agent_id: "agent-test".to_string(),
        task_id: None,
        role: "assistant".to_string(),
        summary: summary.to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: 0,
        updated_at: 0,
    }))
}

/// 护栏：长期知识与短期记忆必须是**两次独立查询**，不许改回 `All` 混检
///
/// `All` 会让两路共用 DAL 的「实体总数」limit —— 短期记忆排在前面时
/// 知识节点候选池会被挤空，正是这次拆查询要修的问题。
#[test]
fn long_and_short_term_recall_are_separate_queries() {
    let width = recall_width();
    let long_term = long_term_search("缓存 前缀", width);
    assert_eq!(
        long_term.filters.memory_type,
        Some(MemoryType::KnowledgeNode),
        "主召回只查长期知识节点"
    );
    assert_eq!(long_term.filters.limit, Some(width), "独立占满召回宽度");
    assert_eq!(long_term.top_k, Some(width as i32));
    assert_eq!(long_term.keyword.as_deref(), Some("缓存 前缀"));

    let st_width = SHORT_TERM_RECALL_WIDTH;
    let short_term = short_term_search("缓存 前缀", st_width);
    assert_eq!(
        short_term.filters.memory_type,
        Some(MemoryType::ShortTerm),
        "短期记忆单独查一次"
    );
    assert_eq!(
        short_term.filters.limit,
        Some(st_width),
        "短期记忆宽召回（不是直接只查 3 条）"
    );
    assert_eq!(short_term.top_k, Some(st_width as i32));
    assert_eq!(short_term.keyword.as_deref(), Some("缓存 前缀"));
    assert!(
        st_width > SHORT_TERM_TOP_K,
        "宽召回必须给精选留出取舍空间；没有小脑时按召回序取前 3 条"
    );

    assert_ne!(
        long_term.filters.memory_type, short_term.filters.memory_type,
        "两路的 memory_type 不得相同（相同 = 又混回一次查询）"
    );
    assert_eq!(SHORT_TERM_TOP_K, 3);
}

/// 渲染侧三份预算互相独立：5 个长期节点 + 3 条短期记忆 + 4 条边
#[test]
fn short_term_lines_have_their_own_render_budget() {
    let mut hits: Vec<Memory> = (0..9)
        .map(|i| node(&format!("k{i}"), &format!("节点{i}"), "描述"))
        .collect();
    hits.extend((0..10).map(|i| short_term(&format!("st{i}"), &format!("短期{i}"))));
    let relations: Vec<Memory> = (0..7)
        .map(|i| relation(&format!("k{}", i % 9), &format!("k{}", (i + 1) % 9), "rel"))
        .collect();

    let lines = build_knowledge_lines(&hits, &[], &[], &relations);

    assert_eq!(
        lines.len(),
        KNOWLEDGE_TOP_K + SHORT_TERM_TOP_K + RELATION_TOP_K
    );
    assert_eq!(
        lines.iter().filter(|l| l.contains("[短期记忆")).count(),
        SHORT_TERM_TOP_K
    );
    assert!(
        lines[..KNOWLEDGE_TOP_K]
            .iter()
            .all(|l| !l.contains("[短期记忆")),
        "短期记忆不能挤进长期知识的 5 个名额"
    );
    assert!(
        lines[KNOWLEDGE_TOP_K].contains("st0"),
        "短期记忆行带 id，供 query_memory(ids) 反查: {lines:?}"
    );
}

/// 召回面必须显著宽于渲染面，否则精选没有取舍空间
///
/// 并固定「召回宽度与精选开关无关」这条口径：`recall_width` 不接 deps，
/// 开不开精选看到的是同一个候选池。
#[test]
fn recall_width_is_a_multiple_of_render_budget() {
    let width = recall_width();
    assert!(
        width >= KNOWLEDGE_TOP_K * 2,
        "召回宽度 {width} 至少要 2 倍渲染预算 {}",
        KNOWLEDGE_TOP_K
    );
    // 候选池只有 1 条时精选会直接跳过（RankPolicy 不发单选项的题）
    assert!(width > 1);
}

/// 图谱扩展的核心价值：邻居节点只用来给边取名，正文不进 prompt
#[test]
fn relation_line_resolves_neighbor_name_without_dumping_its_body() {
    let hits = vec![node("k1", "订单状态机", "订单从创建到完成的流转")];
    let neighbors = vec![node("k2", "库存扣减", "这段邻居正文不应出现在 prompt")];
    let relations = vec![relation("k1", "k2", "depends_on")];

    let lines = build_knowledge_lines(&hits, &[], &neighbors, &relations);

    assert_eq!(
        lines[0],
        "[知识节点 k1] 订单状态机 · concept · 订单从创建到完成的流转"
    );
    assert_eq!(
        lines[1], "[关系 rel-k1-k2-depends_on] 订单状态机[k1] --depends_on--> 库存扣减[k2]",
        "边行带边 id 与两端节点 id，才能反查 / 继续沿图展开"
    );
    assert!(
        !lines.iter().any(|l| l.contains("邻居正文")),
        "邻居节点正文不应渲染: {lines:?}"
    );
}

/// ⚠️ 图谱扩展的种子必须覆盖「精选 + 索引」两层：只给 selected 会把
/// rest 里那些知识节点的边整段丢掉（分层渲染改造引入过的回归）。
#[test]
fn graph_seeds_cover_index_layer_nodes() {
    let selected: Vec<Memory> = (0..KNOWLEDGE_TOP_K)
        .map(|i| node(&format!("s{i}"), &format!("精选{i}"), "描述"))
        .collect();
    let rest: Vec<Memory> = (0..10)
        .map(|i| node(&format!("r{i}"), &format!("索引{i}"), "描述"))
        .collect();

    let seeds = graph_seeds(&selected, &rest);

    assert_eq!(seeds.len(), GRAPH_SEED_TOP_K, "整个候选池都该当种子");
    assert!(
        seeds.iter().any(|id| id == "r0"),
        "索引层节点必须进种子: {seeds:?}"
    );
    assert!(
        seeds.iter().any(|id| id == "r9"),
        "最末一条索引节点也不能漏: {seeds:?}"
    );
}

/// 边有自己的渲染预算：索引行再多也不能把图谱结构挤掉
#[test]
fn index_lines_do_not_consume_relation_budget() {
    let selected = vec![node("k1", "A", "a")];
    let rest: Vec<Memory> = (0..10)
        .map(|i| node(&format!("r{i}"), &format!("索引{i}"), "描述"))
        .collect();
    let relations = vec![relation("k1", "r0", "rel")];

    let lines = build_knowledge_lines(&selected, &rest, &[], &relations);

    assert_eq!(
        lines.iter().filter(|l| l.contains("[索引]")).count(),
        10,
        "索引行全部保留: {lines:?}"
    );
    assert_eq!(
        lines.iter().filter(|l| l.contains("[关系")).count(),
        1,
        "边仍占独立名额，且两端能按 selected + rest 取名: {lines:?}"
    );
    assert!(lines.last().is_some_and(|l| l.contains("--rel-->")));
}

/// 对端不在批内的悬空边跳过（否则 prompt 里会出现裸 id）
#[test]
fn dangling_relation_is_skipped() {
    let hits = vec![node("k1", "A", "a")];
    let relations = vec![relation("k1", "k9", "rel")];

    let lines = build_knowledge_lines(&hits, &[], &[], &relations);

    assert_eq!(lines, vec!["[知识节点 k1] A · concept · a".to_string()]);
}

/// 渲染预算：候选再多也只在 prompt 里占 5 个节点位
#[test]
fn node_and_relation_budgets_are_enforced() {
    let nodes: Vec<Memory> = (0..9)
        .map(|i| node(&format!("k{i}"), &format!("节点{i}"), "描述"))
        .collect();
    let relations: Vec<Memory> = (0..7)
        .map(|i| relation(&format!("k{}", i % 9), &format!("k{}", (i + 1) % 9), "rel"))
        .collect();

    let lines = build_knowledge_lines(&nodes, &[], &[], &relations);

    assert_eq!(
        lines.len(),
        KNOWLEDGE_TOP_K + RELATION_TOP_K,
        "应为 5 个节点 + 4 条边"
    );
    assert!(lines[KNOWLEDGE_TOP_K].contains("--rel-->"));
}

/// 边池 = 检索路 + 图谱路，同一条边只渲染一次
///
/// 两条路大范围重叠（都是命中节点的出入边），不去重会让 prompt 里
/// 同一条关系重复出现，白占边预算。
#[test]
fn edges_from_both_paths_are_merged_without_duplicates() {
    let edge = relation("k1", "k2", "depends_on");
    // 检索结果里带了这条边，图谱扩展又拉回来同一条
    let hits = vec![node("k1", "A", "a"), node("k2", "B", "b"), edge.clone()];
    let graph_edges = vec![edge];

    let lines = build_knowledge_lines(&hits, &[], &[], &graph_edges);

    let edge_lines: Vec<&String> = lines.iter().filter(|l| l.contains("-->")).collect();
    assert_eq!(edge_lines.len(), 1, "同一条边只渲染一次: {lines:?}");
}

/// 两端都命中的边优先于「weight 更高但远端没命中」的边
///
/// 「A 依赖 B」只有 A、B 都在本轮上下文里才闭环；远端缺席时读者拿到的是
/// 半条信息，weight 再高也不该挤掉完整的那条。
#[test]
fn edge_with_both_ends_hit_outranks_heavier_dangling_edge() {
    let hits = vec![node("k1", "A", "a"), node("k2", "B", "b")];
    // k9 只在邻居里（用于取名），不算命中
    let neighbors = vec![node("k9", "远端", "d")];
    let relations = vec![
        relation_with_weight("k1", "k9", "weak_link", Some(0.95)),
        relation_with_weight("k1", "k2", "strong_link", Some(0.10)),
    ];

    let lines = build_knowledge_lines(&hits, &[], &neighbors, &relations);

    assert!(lines[2].contains("strong_link"), "完整边应排前: {lines:?}");
    assert!(lines[3].contains("weak_link"));
}

/// 同样两端命中的两条边，按 weight 降序
#[test]
fn edges_of_equal_completeness_are_ordered_by_weight() {
    let hits = vec![node("k1", "A", "a"), node("k2", "B", "b")];
    let relations = vec![
        relation_with_weight("k1", "k2", "light", Some(0.1)),
        relation_with_weight("k1", "k2", "heavy", Some(0.9)),
    ];

    let lines = build_knowledge_lines(&hits, &[], &[], &relations);

    assert!(lines[2].contains("heavy"), "weight 高的排前: {lines:?}");
    assert!(lines[3].contains("light"));
}

/// 降级时按召回序前 keep 条入选，其余**原序保留**：宽召回不能白查
#[test]
fn rerank_fallback_keeps_unselected_candidates() {
    let entities: Vec<Memory> = (0..6)
        .map(|i| node(&format!("k{i}"), &format!("节点{i}"), "描述"))
        .collect();

    let out = split_by_recall_order(entities, KNOWLEDGE_TOP_K - 1);

    assert_eq!(
        out.selected
            .iter()
            .filter_map(memory_id)
            .collect::<Vec<_>>(),
        vec!["k0", "k1", "k2", "k3"]
    );
    assert_eq!(
        out.rest.iter().filter_map(memory_id).collect::<Vec<_>>(),
        vec!["k4", "k5"],
        "没入选的候选必须留作索引，不能被丢弃"
    );
}

/// 全文层给完整正文与 id，索引层只给元信息 + 摘要片段；两层都带 id
#[test]
fn full_and_index_layers_carry_ids_without_dumping_bodies() {
    let long_body = "缓存".repeat(60); // 120 字，必然触发摘要截断
    let selected = vec![node("k0", "前缀缓存", &long_body)];
    let rest = vec![
        node("k1", "缓存淘汰", &long_body),
        short_term("st0", &long_body),
    ];

    let lines = build_knowledge_lines(&selected, &rest, &[], &[]);

    assert_eq!(lines.len(), 3, "1 条全文 + 2 条索引");
    assert!(
        lines[0].contains("[知识节点 k0]"),
        "全文行带节点 id（seed_node_ids 查边的把手）: {lines:?}"
    );
    assert!(lines[0].contains(&long_body), "精选条目展示全文");
    assert!(lines[1].starts_with("[索引] [知识节点 k1]"));
    assert!(lines[2].starts_with("[索引] [st0]"));
    assert!(lines[2].contains("ts="), "短期记忆索引行带绝对时间戳");
    assert!(!lines[1].contains("ts="), "知识节点没有时间戳字段");
    assert!(lines[1].contains('…') && lines[2].contains('…'));
    assert!(
        lines[1].chars().count() < lines[0].chars().count(),
        "索引行必须比全文行短: {lines:?}"
    );
    assert!(
        lines.iter().all(|l| !l.starts_with("- ")),
        "行前缀由 builder 统一加，这里不能自带: {lines:?}"
    );
}
