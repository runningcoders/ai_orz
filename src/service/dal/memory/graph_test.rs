//! Memory DAL 知识图谱测试：BFS/DFS 遍历（含限宽与 IN 分块）、
//! 遍历策略解析纯函数、全量图聚合的节点/边/度数/方向口径。

use super::dal;
use super::test_support::*;
use crate::models::memory::{KnowledgeNodeRelationPo, MemoryCreateParams, MemoryPo};
use common::enums::KnowledgeRelationStatus;
use common::error::Result;
use sqlx::SqlitePool;

// ========== 知识图谱遍历测试（BFS/DFS + IN 分块） ==========

/// BFS 菱形图分层语义：A→B/C→D
#[sqlx::test]
async fn test_traverse_bfs_layered(pool: SqlitePool) -> Result<()> {
    use crate::service::dal::memory::TraversalStrategy;
    use std::collections::HashSet;

    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let mut ctx = create_test_ctx(pool.clone());
    ctx.agent_id = Some("agent-001".to_string());
    let now = chrono::Utc::now().timestamp();

    seed_graph(
        &dal,
        &ctx,
        ["tA", "tB", "tC", "tD"]
            .iter()
            .map(|id| traverse_node(id, now))
            .collect(),
        vec![
            traverse_rel("r-ab", "tA", "tB", now),
            traverse_rel("r-ac", "tA", "tC", now + 1),
            traverse_rel("r-bd", "tB", "tD", now + 2),
            traverse_rel("r-cd", "tC", "tD", now + 3),
        ],
    )
    .await?;

    // depth=1：只展开第一层，D 不可达
    let results = dal
        .traverse_knowledge_graph(
            ctx.clone(),
            &["tA".to_string()],
            1,
            0,
            TraversalStrategy::BreadthFirst,
        )
        .await?;
    assert_eq!(
        node_ids_of(&results),
        HashSet::from(["tA".to_string(), "tB".to_string(), "tC".to_string()]),
        "depth=1 应只含第一层节点"
    );
    assert_eq!(
        rel_ids_of(&results),
        HashSet::from(["r-ab".to_string(), "r-ac".to_string()])
    );

    // depth=2：全图可达，菱形汇合点 D 只出现一次（visited 去重）
    let results = dal
        .traverse_knowledge_graph(
            ctx.clone(),
            &["tA".to_string()],
            2,
            0,
            TraversalStrategy::BreadthFirst,
        )
        .await?;
    assert_eq!(
        results
            .iter()
            .filter(|m| matches!(m.po, MemoryPo::KnowledgeNode(_)))
            .count(),
        4
    );
    assert_eq!(rel_ids_of(&results).len(), 4, "四条边应全部收集");

    Ok(())
}

/// DFS 分支图 + max_breadth 限宽：守护预取缓存后的选边语义不变
///（按 created_at ASC take，重复边不占限宽名额）
#[sqlx::test]
async fn test_traverse_dfs_branching_with_breadth_limit(pool: SqlitePool) -> Result<()> {
    use crate::service::dal::memory::TraversalStrategy;
    use std::collections::HashSet;

    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let mut ctx = create_test_ctx(pool.clone());
    ctx.agent_id = Some("agent-001".to_string());
    let now = chrono::Utc::now().timestamp();

    seed_graph(
        &dal,
        &ctx,
        ["dA", "dB", "dC", "dD"]
            .iter()
            .map(|id| traverse_node(id, now))
            .collect(),
        vec![
            traverse_rel("dr-ab", "dA", "dB", now), // 早创建，限宽 1 时应被选中
            traverse_rel("dr-ac", "dA", "dC", now + 10), // 晚创建，应被限宽丢弃
            traverse_rel("dr-bd", "dB", "dD", now - 10), // dB 最早的边：限宽 1 时应被选中
                                                    //（dr-ab 作为双端点边会同时挂在 dA/dB 缓存名下，若去重失效会抢占此名额）
        ],
    )
    .await?;

    let results = dal
        .traverse_knowledge_graph(
            ctx.clone(),
            &["dA".to_string()],
            3,
            1,
            TraversalStrategy::DepthFirst,
        )
        .await?;

    assert_eq!(
        node_ids_of(&results),
        HashSet::from(["dA".to_string(), "dB".to_string(), "dD".to_string()]),
        "限宽 1 时 DFS 应沿最早创建的边深入，C 不可达"
    );
    assert_eq!(
        rel_ids_of(&results),
        HashSet::from(["dr-ab".to_string(), "dr-bd".to_string()])
    );

    Ok(())
}

/// DFS 链图：守护预取缓存不漏边（链上每次批量只有单节点，等价旧逐节点查询路径）
#[sqlx::test]
async fn test_traverse_dfs_chain(pool: SqlitePool) -> Result<()> {
    use crate::service::dal::memory::TraversalStrategy;
    use std::collections::HashSet;

    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let mut ctx = create_test_ctx(pool.clone());
    ctx.agent_id = Some("agent-001".to_string());
    let now = chrono::Utc::now().timestamp();

    seed_graph(
        &dal,
        &ctx,
        ["cA", "cB", "cC", "cD"]
            .iter()
            .map(|id| traverse_node(id, now))
            .collect(),
        vec![
            traverse_rel("cr-1", "cA", "cB", now),
            traverse_rel("cr-2", "cB", "cC", now + 1),
            traverse_rel("cr-3", "cC", "cD", now + 2),
        ],
    )
    .await?;

    let results = dal
        .traverse_knowledge_graph(
            ctx.clone(),
            &["cA".to_string()],
            3,
            0,
            TraversalStrategy::DepthFirst,
        )
        .await?;

    assert_eq!(
        node_ids_of(&results),
        HashSet::from([
            "cA".to_string(),
            "cB".to_string(),
            "cC".to_string(),
            "cD".to_string()
        ]),
        "链图 depth=3 应全链可达"
    );
    assert_eq!(rel_ids_of(&results).len(), 3);

    Ok(())
}

/// 宽星型图 BFS：visited 451 个节点 > IN_CLAUSE_CHUNK(400)，
/// 覆盖 fetch_nodes_by_ids 的分块路径
#[sqlx::test]
async fn test_traverse_wide_star_fetch_chunking(pool: SqlitePool) -> Result<()> {
    use crate::service::dal::memory::TraversalStrategy;

    init_test_tables(&pool).await;
    let dal = init_test(pool.clone()).await;
    let mut ctx = create_test_ctx(pool.clone());
    ctx.agent_id = Some("agent-001".to_string());
    let now = chrono::Utc::now().timestamp();

    let leaf_count = 450usize;
    let mut nodes = vec![traverse_node("star-center", now)];
    let mut relations = Vec::with_capacity(leaf_count);
    for i in 0..leaf_count {
        let leaf_id = format!("star-leaf-{:03}", i);
        nodes.push(traverse_node(&leaf_id, now));
        relations.push(traverse_rel(
            &format!("star-rel-{:03}", i),
            "star-center",
            &leaf_id,
            now + i as i64 + 1,
        ));
    }
    seed_graph(&dal, &ctx, nodes, relations).await?;

    let results = dal
        .traverse_knowledge_graph(
            ctx.clone(),
            &["star-center".to_string()],
            1,
            0,
            TraversalStrategy::BreadthFirst,
        )
        .await?;

    assert_eq!(
        node_ids_of(&results).len(),
        leaf_count + 1,
        "451 个节点应全部返回（跨分块拼接完整）"
    );
    assert_eq!(rel_ids_of(&results).len(), leaf_count, "450 条边应全部收集");

    Ok(())
}

// ========== 纯函数：遍历策略字符串解析 ==========

/// 遍历策略解析：兼容 snake_case / PascalCase，且**拒绝非法值**。
///
/// 回归背景：`search_memory` 曾用 `_ => BreadthFirst` 兜底 ——
/// 调用方把 `depth_first` 拼错会**静默换成宽度优先**，图的结构换了一套却不报错。
#[test]
fn traversal_strategy_parse_accepts_both_spellings_and_rejects_garbage() {
    use crate::service::dal::memory::TraversalStrategy;

    // ACCEPTED_VALUES 必须与实际可解析集合一致，否则错误提示会误导调用方
    for token in TraversalStrategy::ACCEPTED_VALUES.split(',') {
        let token = token.trim();
        assert!(
            TraversalStrategy::parse(token).is_some(),
            "ACCEPTED_VALUES 里的 `{token}` 必须可解析"
        );
    }
    for raw in [" depth_first ", "DepthFirst", "DEPTH_FIRST"] {
        assert_eq!(
            TraversalStrategy::parse(raw),
            Some(TraversalStrategy::DepthFirst),
            "`{raw}` 应解析成 DepthFirst"
        );
    }
    // 拼错必须返回 None（由 handler 报 400），绝不能悄悄退化成 BFS
    for bad in ["depth", "dfs", "depth-first", ""] {
        assert_eq!(
            TraversalStrategy::parse(bad),
            None,
            "`{bad}` 是非法值，必须被拒绝而不是兜底"
        );
    }
    assert_eq!(
        TraversalStrategy::parse("breadth_first"),
        Some(TraversalStrategy::DEFAULT)
    );
}

// ========== 全量图聚合 ==========

/// 全量图聚合口径：R1 仅知识节点 / R2 悬挂边被丢弃 / R4 度数仅计生效边
#[sqlx::test]
async fn get_knowledge_graph_returns_active_nodes_and_drops_dangling_edges(pool: sqlx::SqlitePool) {
    let ctx = seed_graph_ctx(pool).await;

    let data = dal()
        .get_knowledge_graph(ctx, None)
        .await
        .expect("get_knowledge_graph should succeed");

    // R1：仅活跃知识节点，不含短期记忆/trace
    assert_eq!(data.nodes.len(), 2);
    let mut ids: Vec<&str> = data.nodes.iter().map(|n| n.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["kn_a", "kn_b"]);

    // R2：悬挂边（target 不在活跃节点集）被严格双端活跃过滤
    assert_eq!(data.edges.len(), 1);
    assert_eq!(data.edges[0].id, "kr_ab");

    // R4：度数 = in + out，仅计生效边（悬挂边不计入）
    assert_eq!(data.degrees.get("kn_a"), Some(&(0, 1)));
    assert_eq!(data.degrees.get("kn_b"), Some(&(1, 0)));
    assert!(!data.degrees.contains_key("kn_ghost"));
}

/// agent_id 归属筛选：无归属匹配节点 → 返回空图
#[sqlx::test]
async fn get_knowledge_graph_filters_by_agent_id(pool: sqlx::SqlitePool) {
    let ctx = seed_graph_ctx(pool).await;

    let data = dal()
        .get_knowledge_graph(ctx, Some("agent_other".to_string()))
        .await
        .expect("get_knowledge_graph should succeed");

    assert!(data.nodes.is_empty());
    assert!(data.edges.is_empty());
    assert!(data.degrees.is_empty());
}

/// 三期方案 a′：边方向服务端按词表 resolve 带出（edge_directions 与 edges 键集对齐；
/// 词表外自拟词兜底 "undirected"，前端零词表映射不变式维持）
#[sqlx::test]
async fn get_knowledge_graph_resolves_edge_directions(pool: sqlx::SqlitePool) {
    let ctx = seed_graph_ctx(pool).await;
    // 词表注册 "related" = undirected（DDL 回填口径），"kr_ab" 边 relation_type="related"
    let data = dal()
        .get_knowledge_graph(ctx, None)
        .await
        .expect("get_knowledge_graph should succeed");
    assert_eq!(data.edges.len(), 1);
    assert_eq!(
        data.edge_directions.get("kr_ab").map(String::as_str),
        Some("undirected")
    );
    // 键集与生效边一一对应，零缺零余
    let mut dir_keys: Vec<&str> = data.edge_directions.keys().map(String::as_str).collect();
    dir_keys.sort_unstable();
    let mut edge_keys: Vec<&str> = data.edges.iter().map(|e| e.id.as_str()).collect();
    edge_keys.sort_unstable();
    assert_eq!(dir_keys, edge_keys);
}

/// 三期方案 a′：directed 词带出 "directed"（contains 属 10 有逆词回填口径）
#[sqlx::test]
async fn get_knowledge_graph_carries_directed_direction_for_lexicon_hit(pool: sqlx::SqlitePool) {
    let ctx = seed_graph_ctx(pool).await;
    let d = dal();
    d.create(
        ctx.clone(),
        MemoryCreateParams::CreateRelations(vec![KnowledgeNodeRelationPo {
            id: "kr_dir".to_string(),
            source_node_id: "kn_a".to_string(),
            target_node_id: "kn_b".to_string(),
            relation_type: "CONTAINS".to_string(), // 书写变体：经同义/归一命中词表
            weight: None,
            status: KnowledgeRelationStatus::Active,
            created_at: 0,
            updated_at: 0,
        }]),
    )
    .await
    .expect("create directed edge should succeed");

    let data = d
        .get_knowledge_graph(ctx, None)
        .await
        .expect("get_knowledge_graph should succeed");
    // seed_graph 预置词表不含 contains——本用例词表为空，CONTAINS 应判 Drift 兜底 undirected；
    // directed 命中路径由 common/src/ontology.rs resolve 单测覆盖（resolve_relation_carries_lexicon_direction）
    assert_eq!(
        data.edge_directions.get("kr_dir").map(String::as_str),
        Some("undirected")
    );
}
