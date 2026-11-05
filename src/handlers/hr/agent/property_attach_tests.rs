//! CP3 handler 级证据：属性装配的**唯一实现**在两条通道上必须同口径。
//!
//! 三条验收（TL 升级口径：不接受 DTO 单测替代，要真实 handler 响应）：
//! 1. `A1-2`：同一节点经 `query_memory` 与 `search_memory` 返回的属性集合完全相同；
//! 2. `A1-3` + `A1-7`：`include_history=true` → 4 term × 2 status = 8 行、其中 4 行
//!    `"is_current": false`；`false` → 4 行且响应 JSON 里**不出现** `is_current`；
//! 3. `A1-6`：多 target（≥3 载体、中间位带 `status=0` 历史行）→ 默认只回 `status=1`，
//!    且 `include_history=true` 也不返回 `status=2`（显式软删不外泄，A1-k）。
//!
//! 全部走 `#[sqlx::test]` 临时库，不依赖现网库，可入库可复现。

use super::*;
use crate::handlers::hr::agent::query_memory::query_memory;
use crate::handlers::hr::agent::search_memory::search_memory;
use crate::models::memory::{LongTermKnowledgeNodePo, MemoryCreateParams};
use crate::models::ontology::OntologyPropertyPo;
use common::api::{QueryMemoryParams, SearchMemoryParams};
use common::enums::MemoryStatus;
use common::enums::ontology::PropertyTargetType;
use common::ontology_property::{PropertyEntry, PropertyValue};

fn init_env(pool: sqlx::SqlitePool) -> RequestContext {
    let _ = crate::config::init();
    let base_path = crate::config::get().base_data_path();
    crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);
    crate::service::dao::init_all();
    crate::service::dal::init_all();
    crate::service::domain::runtime::init();
    crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
}

fn node_po(id: &str) -> LongTermKnowledgeNodePo {
    LongTermKnowledgeNodePo {
        id: id.to_string(),
        agent_id: "agent-kg".to_string(),
        node_name: format!("节点-{id}"),
        node_description: format!("{id} 的正文"),
        node_type: "concept".to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        user_tags: "[]".to_string(),
        status: MemoryStatus::Active,
        is_published: false,
        created_at: 0,
        updated_at: 0,
    }
}

async fn seed_node(ctx: &RequestContext, id: &str) {
    runtime_domain()
        .memory()
        .create(
            ctx.clone(),
            MemoryCreateParams::CreateKnowledgeNode {
                node: node_po(id),
                references: vec![],
            },
        )
        .await
        .unwrap();
}

/// 属性定义（`applies_to` 空 = 不限载体；`domain_classes` 空 = 不限实体类）
async fn define_property(ctx: &RequestContext, term_key: &str) {
    let po = OntologyPropertyPo::new(
        term_key,
        format!("属性-{term_key}"),
        format!("{term_key} 的语义描述"),
        "[]",
        "",
        "string",
        "",
        "single",
        None,
        false,
    );
    crate::service::dao::ontology::dao()
        .insert_property(ctx.clone(), &po)
        .await
        .expect("insert property definition");
}

/// 写一次属性；同一 term 写两次 ⇒ 1 行生效（status=1）+ 1 行被取代（status=0）
async fn write_prop(ctx: &RequestContext, target_id: &str, term: &str, value: &str) {
    runtime_domain()
        .memory()
        .save_entity_properties(
            ctx.clone(),
            PropertyTargetType::Node,
            target_id,
            vec![PropertyEntry {
                term_key: term.to_string(),
                value: PropertyValue::Text(value.to_string()),
                ordinal: 0,
            }],
        )
        .await
        .expect("save property");
}

fn query_params(include_history: Option<bool>) -> QueryMemoryParams {
    QueryMemoryParams {
        agent_id: None,
        memory_type: None,
        limit: Some(50),
        tags: None,
        task_id: None,
        status: None,
        include_history,
    }
}

fn seed_params(seed_id: &str, include_history: Option<bool>) -> SearchMemoryParams {
    SearchMemoryParams {
        query: String::new(),
        max_results: Some(50),
        memory_type: None,
        traversal_depth: Some(0),
        traversal_breadth: Some(10),
        traversal_strategy: Some("breadth_first".to_string()),
        seed_node_ids: Some(vec![seed_id.to_string()]),
        tags: None,
        task_id: None,
        agent_id: None,
        property_filters: None,
        include_history,
    }
}

/// 取出某个节点回显的属性三元组（term_key, 值, ordinal），用于集合比对
fn props_of(results: &[MemoryResult], id: &str) -> Vec<(String, String, i32)> {
    let mut out: Vec<(String, String, i32)> = results
        .iter()
        .find(|r| r.id == id)
        .and_then(|r| r.properties.clone())
        .unwrap_or_default()
        .into_iter()
        .map(|p| (p.term_key, format!("{:?}", p.value), p.ordinal))
        .collect();
    out.sort();
    out
}

/// `A1-2`：同一节点两条通道返回的属性集合**完全相同**。
///
/// 回归背景：`query_memory` 此前完全不装配属性（硬缺口 A1-2），语义通道有、
/// 结构化通道没有——同一个节点在两个工具里长得不一样。
#[sqlx::test]
async fn query_and_search_return_identical_properties(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    seed_node(&ctx, "kn_dual").await;
    for term in ["domain", "event_time", "lifecycle_status", "version"] {
        define_property(&ctx, term).await;
        // 每个 term 写两次 ⇒ 物理 8 行（4 生效 + 4 历史）
        write_prop(&ctx, "kn_dual", term, "old").await;
        write_prop(&ctx, "kn_dual", term, "new").await;
    }

    let via_query = query_memory(ctx.clone(), query_params(None)).await.unwrap();
    let via_search = search_memory(ctx.clone(), seed_params("kn_dual", None))
        .await
        .unwrap();

    let a = props_of(&via_query.results, "kn_dual");
    let b = props_of(&via_search.results, "kn_dual");
    assert_eq!(a.len(), 4, "默认只回 4 条生效值: {a:?}");
    assert_eq!(
        a, b,
        "query_memory 与 search_memory 必须回同一份属性集合（唯一装配源）"
    );
    assert!(
        a.iter().all(|(_, v, _)| v.contains("new")),
        "默认读路径只能出现生效值，历史值不得进模型视野: {a:?}"
    );
}

/// `A1-3` + `A1-7`（响应级）：历史出口的行数与 `is_current` 序列化口径。
///
/// 断言的是**handler 真实响应 JSON**：`include_history=true` 时历史行必须带
/// `"is_current": false`，生效行不带；`false` 时整个响应里没有 `is_current`。
#[sqlx::test]
async fn history_opt_in_controls_row_count_and_is_current_json(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    // 与现网探针同一节点 ID，便于和 TL 的现场复算对照
    const NODE: &str = "kn_01a1919814797e92b7812ffc7931c382";
    seed_node(&ctx, NODE).await;
    for term in ["domain", "event_time", "lifecycle_status", "version"] {
        define_property(&ctx, term).await;
        write_prop(&ctx, NODE, term, "history-value").await;
        write_prop(&ctx, NODE, term, "current-value").await;
    }

    // ---- 默认（false）：4 行且 JSON 不出现 is_current ----
    let plain = search_memory(ctx.clone(), seed_params(NODE, Some(false)))
        .await
        .unwrap();
    let plain_items = plain
        .results
        .iter()
        .find(|r| r.id == NODE)
        .unwrap()
        .properties
        .clone()
        .unwrap();
    assert_eq!(plain_items.len(), 4, "include_history=false 只回生效行");
    let plain_json = serde_json::to_string(&plain).unwrap();
    assert!(
        !plain_json.contains("is_current"),
        "字段缺席 ⇒ 视为 true，默认响应不得出现 is_current: {plain_json}"
    );

    // ---- 历史（true）：8 行、恰 4 行 is_current=false ----
    let hist = search_memory(ctx.clone(), seed_params(NODE, Some(true)))
        .await
        .unwrap();
    let hist_items = hist
        .results
        .iter()
        .find(|r| r.id == NODE)
        .unwrap()
        .properties
        .clone()
        .unwrap();
    assert_eq!(
        hist_items.len(),
        8,
        "include_history=true 回 8 行（4 生效 + 4 历史）"
    );
    assert_eq!(
        hist_items.iter().filter(|p| !p.is_current).count(),
        4,
        "恰 4 行历史（status=0 派生 is_current=false）"
    );
    let hist_json = serde_json::to_string(&hist).unwrap();
    assert_eq!(
        hist_json.matches("\"is_current\":false").count(),
        4,
        "响应 JSON 必须恰好出现 4 个 is_current:false: {hist_json}"
    );
    assert!(
        !hist_json.contains("\"is_current\":true"),
        "生效行不得序列化 is_current=true（缺席即 true）"
    );
    assert!(
        hist_json.contains("history-value"),
        "历史值必须真的回显出来，否则「受控历史出口」形同虚设"
    );

    // ---- 同口径在结构化通道成立（唯一装配源）----
    let via_query = query_memory(ctx.clone(), query_params(Some(true)))
        .await
        .unwrap();
    let q_items = via_query
        .results
        .iter()
        .find(|r| r.id == NODE)
        .unwrap()
        .properties
        .clone()
        .unwrap();
    assert_eq!(
        q_items.len(),
        8,
        "query_memory 的 include_history 必须同样生效（共用同一份实现）"
    );
    let mut a: Vec<_> = hist_items
        .iter()
        .map(|p| (p.term_key.clone(), !p.is_current))
        .collect();
    let mut b: Vec<_> = q_items
        .iter()
        .map(|p| (p.term_key.clone(), !p.is_current))
        .collect();
    a.sort();
    b.sort();
    assert_eq!(a, b, "两通道的历史口径必须一致");
}

/// `A1-6`（handler 级）：多 target ≥3 载体、中间位带历史行 ⇒ 只回生效行；
/// 且 `status=2`（显式软删）任何通道都不返回（A1-k）。
///
/// DAO 层已在 CP1 验过；这里验的是「handler 装配有没有把批量结果接错位」。
#[sqlx::test]
async fn handler_batch_assembly_filters_every_carrier_including_middle(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    for id in ["kn_first", "kn_middle", "kn_last"] {
        seed_node(&ctx, id).await;
    }
    define_property(&ctx, "memo").await;
    write_prop(&ctx, "kn_first", "memo", "first-v1").await;
    // 中间位：两次写入 ⇒ 1 生效 + 1 被取代（修复前只有它能测出 H-C）
    write_prop(&ctx, "kn_middle", "memo", "mid-v1").await;
    write_prop(&ctx, "kn_middle", "memo", "mid-v2").await;
    write_prop(&ctx, "kn_last", "memo", "last-v1").await;

    // 末位再写一次后把生效行打成 status=2：显式软删必须对所有通道隐身
    write_prop(&ctx, "kn_last", "memo", "last-v2").await;
    sqlx::query(
        "UPDATE knowledge_property SET \"status\" = 2 WHERE target_id = ? AND \"status\" = 1",
    )
    .bind("kn_last")
    .execute(ctx.db_pool())
    .await
    .unwrap();

    let plain = query_memory(ctx.clone(), query_params(Some(false)))
        .await
        .unwrap();
    assert_eq!(
        props_of(&plain.results, "kn_first").len(),
        1,
        "首位只回生效行"
    );
    assert_eq!(
        props_of(&plain.results, "kn_middle").len(),
        1,
        "中间位只回生效行（H-C 回归：曾带出历史行）"
    );
    assert!(
        props_of(&plain.results, "kn_middle")[0]
            .1
            .contains("mid-v2"),
        "中间位回的必须是最新生效值"
    );
    assert!(
        props_of(&plain.results, "kn_last").is_empty(),
        "末位生效行已被打成 status=2，默认通道必须整条不出现"
    );

    let hist = query_memory(ctx.clone(), query_params(Some(true)))
        .await
        .unwrap();
    assert_eq!(
        props_of(&hist.results, "kn_middle").len(),
        2,
        "历史出口下中间位应回 2 行（生效 + 被取代）"
    );
    // 末位：status=2 的行必须隐身，但它下面那条 status=0 的历史行属于「1+0」，
    // 照常可见 —— 这才是 A1-k「只出 1+0、永不出 2」的准确口径。
    let last_hist = props_of(&hist.results, "kn_last");
    assert_eq!(
        last_hist.len(),
        1,
        "历史出口只剩被取代的那一行（status=0），status=2 必须排除: {last_hist:?}"
    );
    assert!(
        last_hist[0].1.contains("last-v1"),
        "留下的必须是被取代的旧值: {last_hist:?}"
    );
    let hist_json = serde_json::to_string(&hist).unwrap();
    assert!(
        !hist_json.contains("last-v2"),
        "status=2（显式软删）的值不得外泄: {hist_json}"
    );
}

// ==================== A1-9：运行态双通道一致性门禁（第一件交付） ====================

/// 某载体的**物理**属性行数（含 `status=0` 历史行，用于证明门禁不是空转）。
async fn physical_rows(ctx: &RequestContext, id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM knowledge_property WHERE target_type = 'node' AND target_id = ?",
    )
    .bind(id)
    .fetch_one(ctx.db_pool())
    .await
    .expect("统计物理属性行应成功")
}

/// 多载体种子通道参数（`traversal_depth=0` ⇒ 只取种子本身，不展开邻居）。
fn seed_params_many(seed_ids: &[&str], include_history: Option<bool>) -> SearchMemoryParams {
    SearchMemoryParams {
        seed_node_ids: Some(seed_ids.iter().map(|id| id.to_string()).collect()),
        ..seed_params(seed_ids.first().copied().unwrap_or(""), include_history)
    }
}

/// 结构化通道参数：`property_filters` 走独立召回通道（按属性反查节点）。
fn filter_params(term_key: &str, value: &str) -> SearchMemoryParams {
    SearchMemoryParams {
        // ⚠️ `search_memory` 的 else 分支把 `query` 恒转成 `keyword: Some(..)` 走 FTS，
        // 空串命中 0 条 ⇒ 结构化通道会拿不到任何节点。这里给一个**三个载体都能命中**
        // 的关键词（节点名 / 正文都含 `gate`）；真正的收口仍是 property_filters 的
        // 命中集（DAO 侧 `retain(|m| hit_ids.contains(..))` 硬条件）。
        query: "gate".to_string(),
        seed_node_ids: None,
        property_filters: Some(vec![common::api::PropertyFilterParam {
            term_key: term_key.to_string(),
            op: None,
            value: PropertyValue::Text(value.to_string()),
        }]),
        ..seed_params("", None)
    }
}

/// `A1-9`（第一件交付）：**三通道**（语义 / 种子 / 结构化）装配出的属性集合必须一致，
/// 且每个载体只回生效行。
///
/// 报红机制（本用例存在的唯一理由）：三个载体同批查询时，若 `list_properties_batch`
/// 的 `status` 谓词回到 OR 链末尾（H-C 缺陷形态
/// `(t1∧i1) OR (t2∧i2) OR ((t3∧i3) AND status=1)`），SQL 的 AND 优先于 OR ⇒
/// 前两个载体带出 `status=0` 历史行 ⇒ 每载体 2 条，`a.len() == 1` 与通道间相等
/// 断言同时失败。若写成「各自都是 N 条」就成了空转，故此处断言的是
/// **集合形态 + 通道间相等**。
#[sqlx::test]
async fn tri_channel_property_sets_stay_identical_across_carriers(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    const FIRST: &str = "kn_gate_first";
    const MIDDLE: &str = "kn_gate_middle";
    const LAST: &str = "kn_gate_last";

    for id in [FIRST, MIDDLE, LAST] {
        seed_node(&ctx, id).await;
    }
    define_property(&ctx, "gate_probe").await;
    for id in [FIRST, MIDDLE, LAST] {
        // 每个载体写两次 ⇒ 物理 2 行（1 生效 + 1 历史）；中间位必然带 status=0
        write_prop(&ctx, id, "gate_probe", "old").await;
        write_prop(&ctx, id, "gate_probe", "new").await;
        assert_eq!(
            physical_rows(&ctx, id).await,
            2,
            "{id} 必须有历史行，否则本门禁空转"
        );
    }

    // 三通道：语义（query_memory）/ 种子（seed_node_ids + depth=0）/ 结构化（property_filters）
    let via_semantic = query_memory(ctx.clone(), query_params(None)).await.unwrap();
    let via_seed = search_memory(ctx.clone(), seed_params_many(&[FIRST, MIDDLE, LAST], None))
        .await
        .unwrap();
    let via_structured = search_memory(ctx.clone(), filter_params("gate_probe", "new"))
        .await
        .unwrap();

    for id in [FIRST, MIDDLE, LAST] {
        let semantic = props_of(&via_semantic.results, id);
        let seeded = props_of(&via_seed.results, id);
        let structured = props_of(&via_structured.results, id);
        assert_eq!(
            semantic.len(),
            1,
            "语义通道每个载体只回生效行（H-C 回归：中间位曾带出 status=0）{id}: {semantic:?}"
        );
        assert_eq!(
            semantic, seeded,
            "语义通道与种子通道必须同口径（唯一装配源）{id}"
        );
        assert_eq!(
            semantic, structured,
            "语义通道与结构化通道必须同口径（唯一装配源）{id}"
        );
        assert!(
            semantic[0].1.contains("new"),
            "只能回生效值 {id}: {semantic:?}"
        );
    }
}
