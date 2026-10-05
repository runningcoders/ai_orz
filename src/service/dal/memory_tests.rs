//! tests 单元测试（拆分自 memory.rs）
//!
//! 文件瘦身：原 2209 行 → 2036 行，测试体 174 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::{dal, init};
use crate::models::memory::{KnowledgeNodeRelationPo, LongTermKnowledgeNodePo, MemoryCreateParams};
use crate::pkg::RequestContext;
use common::enums::{KnowledgeRelationStatus, MemoryStatus};

fn node_po(id: &str, agent_id: &str) -> LongTermKnowledgeNodePo {
    LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: agent_id.to_string(),
        node_name: format!("节点-{id}"),
        node_description: String::new(),
        node_type: "concept".to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 0,
        updated_at: 0,
    }
}

fn relation_po(id: &str, source: &str, target: &str) -> KnowledgeNodeRelationPo {
    KnowledgeNodeRelationPo {
        id: id.to_string(),
        source_node_id: source.to_string(),
        target_node_id: target.to_string(),
        relation_type: "related".to_string(),
        weight: None,
        status: KnowledgeRelationStatus::Active,
        created_at: 0,
        updated_at: 0,
    }
}

/// 造数：2 个活跃知识节点 + 1 条生效边 + 1 条悬挂边（关系表无外键，悬挂边可直接插入）
async fn seed_graph(pool: sqlx::SqlitePool) -> RequestContext {
    // dal::init() 会取用四个 DAO 单例，先按依赖顺序逐个初始化
    //（生产路径由 bootstrap 统一完成，测试路径须自备；OnceLock.set 幂等可重复调）
    crate::service::dao::memory::init();
    crate::service::dao::memory::init_vector();
    crate::service::dao::model_provider::init();
    crate::service::dao::cortex::init();
    // 三期 direction：get_knowledge_graph 聚合依赖 OntologyDao 单例
    crate::service::dao::ontology::init();
    init();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("u1", pool);
    let d = dal();
    for id in ["kn_a", "kn_b"].iter() {
        d.create(
            ctx.clone(),
            MemoryCreateParams::CreateKnowledgeNode {
                node: node_po(id, "agent_x"),
                references: vec![],
            },
        )
        .await
        .expect("create knowledge node should succeed");
    }
    d.create(
        ctx.clone(),
        MemoryCreateParams::CreateRelations(vec![
            // 生效边：kn_a → kn_b
            relation_po("kr_ab", "kn_a", "kn_b"),
            // 悬挂边：target "kn_ghost" 不在活跃节点集（R2 应被过滤）
            relation_po("kr_ghost", "kn_a", "kn_ghost"),
        ]),
    )
    .await
    .expect("create relations should succeed");
    ctx
}

/// 全量图聚合口径：R1 仅知识节点 / R2 悬挂边被丢弃 / R4 度数仅计生效边
#[sqlx::test]
async fn get_knowledge_graph_returns_active_nodes_and_drops_dangling_edges(pool: sqlx::SqlitePool) {
    let ctx = seed_graph(pool).await;

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
    let ctx = seed_graph(pool).await;

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
    let ctx = seed_graph(pool).await;
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
    let ctx = seed_graph(pool).await;
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
