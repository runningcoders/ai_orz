//! tests 单元测试（拆分自 update_memory.rs）
//!
//! 文件瘦身：原 400 行 → 174 行，测试体 227 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::handlers::hr::agent::save_long_term_memory::save_long_term_memory;
use crate::handlers::hr::agent::save_short_term_memory::save_short_term_memory;
use crate::models::ontology::OntologyPropertyPo;
use crate::service::dao::memory::MemoryQuery;
use common::api::{
    KnowledgePropertyParam, KnowledgeRelationParam, RelationWriteStatus, SaveLongTermMemoryParams,
    SaveShortTermMemoryParams, UpdateMemoryParams,
};
use common::enums::ontology::PropertyTargetType;
use common::ontology_property::PropertyValue;

fn init_env(pool: sqlx::SqlitePool) -> RequestContext {
    let _ = crate::config::init();
    let base_path = crate::config::get().base_data_path();
    crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);
    crate::service::dao::init_all();
    crate::service::dal::init_all();
    crate::service::domain::runtime::init();
    crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
}

/// 造一条真实的短期记忆（走真实写入路径，不手搓 PO）。
async fn seed_short_term(ctx: &RequestContext) -> String {
    save_short_term_memory(
        ctx.clone(),
        SaveShortTermMemoryParams {
            summary: "待更新的短期记忆".to_string(),
            tags: None,
            task_id: None,
            content: None,
            trace_ids: None,
        },
    )
    .await
    .expect("保存短期记忆应成功")
    .memory_id
}

/// 断言指定 id 的短期记忆当前状态为 `expected`。
///
/// ⚠️ 用「ids + **显式** status」查询：通用查询在 status 未指定时会默认排除
/// Forgotten（`status != 0`，软删除语义），而本测试恰恰要读取被遗忘的记忆；
/// 按 id 直取的 `get_short_term_index` 同样带 `status != 0` 过滤，两者都会让
/// Forgotten 行不可见。显式指定 status 才能覆盖全部状态。
async fn assert_status_is(ctx: &RequestContext, id: &str, expected: MemoryStatus) {
    let pos = crate::service::dao::memory::dao()
        .query_short_term(
            ctx.clone(),
            MemoryQuery {
                ids: Some(vec![id.to_string()]),
                status: Some(expected),
                ..Default::default()
            },
        )
        .await
        .expect("查询应成功");
    assert_eq!(
        pos.len(),
        1,
        "应恰好查到 1 条状态为 {expected:?} 的记忆，实际: {pos:?}"
    );
    assert_eq!(pos[0].status, expected);
}

fn update_params(memory_id: &str, status: Option<&str>) -> UpdateMemoryParams {
    UpdateMemoryParams {
        memory_id: memory_id.to_string(),
        content: None,
        summary: None,
        tags: None,
        is_published: None,
        status: status.map(|s| s.to_string()),
        node_tags: None,
        relations: None,
        properties: None,
    }
}

/// 非法 `status` 必须报 400，且**绝不能顺手把记忆写成 Active**。
///
/// 回归背景：原 `_ => MemoryStatus::Active` 兜底把「拼错状态」变成一次**静默写入** ——
/// 调用方以为只是参数无效被忽略，实际记忆的活跃度已经被改掉了。
#[sqlx::test]
async fn invalid_status_is_rejected_and_never_silently_written(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let id = seed_short_term(&ctx).await;
    assert_status_is(&ctx, &id, MemoryStatus::Active).await;

    let e = update_memory(ctx.clone(), update_params(&id, Some("actived")))
        .await
        .expect_err("拼错的 status 必须报错");
    let msg = e.to_string();
    assert!(msg.contains("invalid_request"), "错误码不对: {msg}");
    assert!(msg.contains("status"), "错误信息应指明字段: {msg}");
    assert!(
        msg.contains("forgotten"),
        "应列出合法取值供调用方纠正: {msg}"
    );
    // 状态原封不动（既没被兜底成别的值，也没被写坏）
    assert_status_is(&ctx, &id, MemoryStatus::Active).await;

    // 判别值字符串仍等价于名称（在遗忘之前做：被遗忘的记忆对
    // update_memory 的默认取数不可见（软删除语义），无法再被更新）
    update_memory(ctx.clone(), update_params(&id, Some("2")))
        .await
        .expect("判别值 \"2\" 应等价于 settled");
    assert_status_is(&ctx, &id, MemoryStatus::Settled).await;

    // 合法值是生效的：forgotten 必须真的落库（而不是被兜底成 Active）
    update_memory(ctx.clone(), update_params(&id, Some("forgotten")))
        .await
        .expect("合法 status 应更新成功");
    assert_status_is(&ctx, &id, MemoryStatus::Forgotten).await;
}

/// 短期记忆传 relations 必须报 400：静默忽略会让调用方以为边已建立，
/// 沉淀联想悄悄丢失（正是「正文隐形边」问题的工具侧防线）。
#[sqlx::test]
async fn relations_on_short_term_memory_are_rejected(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let id = seed_short_term(&ctx).await;

    let mut params = update_params(&id, None);
    params.relations = Some(vec![KnowledgeRelationParam {
        source_node_id: id.clone(),
        target_node_id: "kn_other".to_string(),
        relation_type: "related".to_string(),
        weight: None,
    }]);

    let e = update_memory(ctx.clone(), params)
        .await
        .expect_err("短期记忆建边必须报错");
    let msg = e.to_string();
    assert!(msg.contains("invalid_request"), "错误码不对: {msg}");
    assert!(msg.contains("relations"), "错误信息应指明字段: {msg}");
}

/// 既有知识节点用 update_memory 补边：沉淀联想落成显式关系边的唯一路径。
/// 覆盖：词表外关系名原样保留、weight 归一化穿透、内容更新与建边同调用生效。
#[sqlx::test]
async fn relations_create_edges_on_existing_knowledge_node(pool: sqlx::SqlitePool) {
    use crate::handlers::hr::agent::save_long_term_memory::save_long_term_memory;
    use crate::handlers::hr::agent::search_memory::search_memory;
    use common::api::{SaveLongTermMemoryParams, SearchMemoryParams};

    let ctx = init_env(pool);

    let node_a = save_long_term_memory(
        ctx.clone(),
        SaveLongTermMemoryParams {
            node_name: "飞书消息入站".to_string(),
            node_description: "旧描述".to_string(),
            node_type: "concept".to_string(),
            summary: None,
            tags: None,
            is_published: None,
            relations: None,
            properties: None,
            task_id: None,
            source_short_term_ids: None,
        },
    )
    .await
    .expect("建节点 A 应成功")
    .node_id;
    let node_b = save_long_term_memory(
        ctx.clone(),
        SaveLongTermMemoryParams {
            node_name: "异步回调骨架".to_string(),
            node_description: "五类出站渠道骨架".to_string(),
            node_type: "concept".to_string(),
            summary: None,
            tags: None,
            is_published: None,
            relations: None,
            properties: None,
            task_id: None,
            source_short_term_ids: None,
        },
    )
    .await
    .expect("建节点 B 应成功")
    .node_id;

    // 更新内容 + 补边一次调用完成：词表外关系名「实现」必须原样落库
    let mut params = update_params(&node_a, None);
    params.content = Some("新描述：入站后走两阶段唤醒".to_string());
    params.relations = Some(vec![KnowledgeRelationParam {
        source_node_id: node_a.clone(),
        target_node_id: node_b.clone(),
        relation_type: "实现".to_string(),
        weight: Some(0.8),
    }]);
    update_memory(ctx.clone(), params)
        .await
        .expect("更新节点 + 建边应成功");

    // 用纯图谱遍历验证边真实存在（正文里引用是查不到的——遍历只认关系表）
    let resp = search_memory(
        ctx.clone(),
        SearchMemoryParams {
            query: String::new(),
            max_results: Some(50),
            memory_type: None,
            traversal_depth: Some(1),
            traversal_breadth: Some(10),
            traversal_strategy: Some("breadth_first".to_string()),
            seed_node_ids: Some(vec![node_a.clone()]),
            tags: None,
            task_id: None,
            agent_id: None,
            property_filters: None,
        },
    )
    .await
    .expect("遍历应成功");

    let relation = resp
        .results
        .iter()
        .find(|r| r.memory_type == "relation")
        .expect("更新建立的边必须对遍历可见");
    assert_eq!(relation.source_node_id.as_deref(), Some(node_a.as_str()));
    assert_eq!(relation.target_node_id.as_deref(), Some(node_b.as_str()));
    assert_eq!(
        relation.relation_type.as_deref(),
        Some("实现"),
        "词表外关系名必须原样落库（与 save_long_term_memory 一致）"
    );
    assert_eq!(relation.weight, Some(0.8), "weight 必须穿透到遍历结果");

    let node = resp
        .results
        .iter()
        .find(|r| r.id == node_a)
        .expect("种子节点必须在结果里");
    assert_eq!(
        node.content, "新描述：入站后走两阶段唤醒",
        "内容更新与建边应同调用生效"
    );
}

// ==================== 结构化属性（既有节点的补 / 改入口） ====================

/// 造一条本体属性定义（写侧门闩的输入；**不**定义则走「未定义属性兜底写入」）
async fn define_property(ctx: &RequestContext, term_key: &str, data_type: &str) {
    crate::service::dao::ontology::dao()
        .insert_property(
            ctx.clone(),
            &OntologyPropertyPo::new(
                term_key,
                format!("属性-{term_key}"),
                format!("{term_key} 的语义描述"),
                "[]",
                "node",
                data_type,
                "",
                "single",
                None,
                false,
            ),
        )
        .await
        .expect("属性定义应插入成功");
}

/// 读回某节点的生效属性（走 domain 直读，验证「真的落库」而不是只看回执说成功）
async fn active_properties(ctx: &RequestContext, node_id: &str) -> Vec<(String, PropertyValue)> {
    crate::service::domain::runtime::domain()
        .memory()
        .list_entity_properties(ctx.clone(), PropertyTargetType::Node, node_id)
        .await
        .expect("读属性应成功")
        .into_iter()
        .filter_map(|po| Some((po.term_key.clone(), po.value()?)))
        .collect()
}

/// 既有节点补属性 + 软取代更正：`update_memory` 的 `properties` 是唯一路径
///
/// 沉淀链路命中既有节点时走的就是这条路——没有它，存量节点永远挂不上属性。
#[sqlx::test]
async fn update_memory_attaches_and_supersedes_properties(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    define_property(&ctx, "confidence", "number").await;

    let node_id = save_long_term_memory(
        ctx.clone(),
        SaveLongTermMemoryParams {
            node_name: "入站去重策略".to_string(),
            node_description: "同一条消息重复入站时按幂等键丢弃".to_string(),
            node_type: "concept".to_string(),
            summary: None,
            tags: None,
            is_published: None,
            relations: None,
            properties: None,
            task_id: None,
            source_short_term_ids: None,
        },
    )
    .await
    .expect("建节点应成功")
    .node_id;

    let write = |v: f64| {
        update_memory(
            ctx.clone(),
            UpdateMemoryParams {
                memory_id: node_id.clone(),
                content: None,
                summary: None,
                tags: None,
                is_published: None,
                status: None,
                node_tags: None,
                relations: None,
                properties: Some(vec![KnowledgePropertyParam {
                    term_key: "confidence".to_string(),
                    value: PropertyValue::Number(v),
                    ordinal: 0,
                }]),
            },
        )
    };

    let resp = write(0.4).await.expect("补属性应成功");
    assert!(
        resp.property_issues.is_empty(),
        "类型对齐的属性应被接受，实际回执: {:?}",
        resp.property_issues
    );
    let rows = active_properties(&ctx, &node_id).await;
    assert_eq!(rows.len(), 1, "属性应真的落库");
    assert_eq!(rows[0].0, "confidence");
    assert!(
        matches!(rows[0].1, PropertyValue::Number(v) if (v - 0.4).abs() < 1e-6),
        "首次写入值不对: {:?}",
        rows[0].1
    );

    // 软取代：同一个单值属性再写一次 = 更正（不是冲突、不需要先删后建）
    let resp = write(0.9).await.expect("更正属性应成功");
    assert!(
        resp.property_issues.is_empty(),
        "单值属性重复写是软取代而非冲突，实际回执: {:?}",
        resp.property_issues
    );
    let rows = active_properties(&ctx, &node_id).await;
    assert_eq!(rows.len(), 1, "读侧仍只见一条生效值");
    assert!(
        matches!(rows[0].1, PropertyValue::Number(v) if (v - 0.9).abs() < 1e-6),
        "软取代后应读到新值: {:?}",
        rows[0].1
    );
}

/// 短期记忆挂属性 → 400：属性挂在图谱载体上，短期记忆没有载体语义
#[sqlx::test]
async fn properties_are_rejected_for_short_term_memory(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let memory_id = seed_short_term(&ctx).await;

    let err = update_memory(
        ctx.clone(),
        UpdateMemoryParams {
            memory_id,
            content: None,
            summary: None,
            tags: None,
            is_published: None,
            status: None,
            node_tags: None,
            relations: None,
            properties: Some(vec![KnowledgePropertyParam {
                term_key: "confidence".to_string(),
                value: PropertyValue::Number(0.5),
                ordinal: 0,
            }]),
        },
    )
    .await
    .expect_err("短期记忆挂属性必须被拒");
    assert!(
        err.to_string().contains("properties"),
        "错误信息应点明是 properties 字段: {err}"
    );
}

// ==================== 建边 keyed 回执（relations_result / 假成功修复回归） ====================

/// 造一个最小知识节点（回执测试的端点素材）。
async fn seed_node(ctx: &RequestContext, name: &str) -> String {
    save_long_term_memory(
        ctx.clone(),
        SaveLongTermMemoryParams {
            node_name: name.to_string(),
            node_description: format!("{name} 的描述"),
            node_type: "concept".to_string(),
            summary: None,
            tags: None,
            is_published: None,
            relations: None,
            properties: None,
            task_id: None,
            source_short_term_ids: None,
        },
    )
    .await
    .expect("建节点应成功")
    .node_id
}

/// 混合批次：逐项有 key、成功项 relation_id = **实际落库** ID（删后实证）、
/// 占位符 / 缺失端点 / 批内重复逐项 failed、顶层 relation_ids 只收成功项。
///
/// 覆盖调研单要求的「混合成败」「占位符」「重复边」与语义自测①③。
#[sqlx::test]
async fn mixed_batch_relations_receipt_has_keys_and_real_ids(pool: sqlx::SqlitePool) {
    use crate::handlers::hr::agent::delete_memory::delete_memory;
    use common::api::DeleteMemoryParams;

    let ctx = init_env(pool);
    let node_a = seed_node(&ctx, "回执端点A").await;
    let node_b = seed_node(&ctx, "回执端点B").await;

    let resp = save_long_term_memory(
        ctx.clone(),
        SaveLongTermMemoryParams {
            node_name: "混合建边批次".to_string(),
            node_description: "1 成功 + 3 失败：整批必须 200".to_string(),
            node_type: "concept".to_string(),
            summary: None,
            tags: None,
            is_published: None,
            relations: Some(vec![
                // 0：两端都存在 → 成功
                KnowledgeRelationParam {
                    source_node_id: node_a.clone(),
                    target_node_id: node_b.clone(),
                    relation_type: "related".to_string(),
                    weight: None,
                },
                // 1：占位符 → placeholder_node_id_rejected
                KnowledgeRelationParam {
                    source_node_id: "SELF".to_string(),
                    target_node_id: node_b.clone(),
                    relation_type: "related".to_string(),
                    weight: None,
                },
                // 2：目标不存在 → target_node_not_found
                KnowledgeRelationParam {
                    source_node_id: node_b.clone(),
                    target_node_id: "kn_missing_node".to_string(),
                    relation_type: "related".to_string(),
                    weight: None,
                },
                // 3：批内同键重复 → duplicate_active_edge
                KnowledgeRelationParam {
                    source_node_id: node_a.clone(),
                    target_node_id: node_b.clone(),
                    relation_type: "related".to_string(),
                    weight: None,
                },
            ]),
            properties: None,
            task_id: None,
            source_short_term_ids: None,
        },
    )
    .await
    .expect("混合批次整批必须 200 + 逐项回执（拍板①）");

    let results = resp
        .relations_result
        .expect("携带 relations 必须有 keyed 回执");
    assert_eq!(results.len(), 4, "逐项回执：4 条 4 项");

    // 语义自测①：逐项有 key（index 为主 + 复合串辅助）
    assert_eq!(results[0].key, format!("0:{node_a}->{node_b}"));
    assert_eq!(results[1].key, format!("1:SELF->{node_b}"));
    assert_eq!(results[2].key, format!("2:{node_b}->kn_missing_node"));
    assert_eq!(results[3].key, format!("3:{node_a}->{node_b}"));

    assert_eq!(results[0].status, RelationWriteStatus::Success);
    let real_id = results[0]
        .relation_id
        .clone()
        .expect("成功项必须带 relation_id");
    assert!(
        real_id.starts_with("kr_"),
        "落库边 ID 应为 kr_ 前缀: {real_id}"
    );

    assert_eq!(results[1].status, RelationWriteStatus::Failed);
    assert_eq!(
        results[1].error_code.as_deref(),
        Some("placeholder_node_id_rejected")
    );
    assert!(results[1].relation_id.is_none());

    assert_eq!(results[2].status, RelationWriteStatus::Failed);
    assert_eq!(
        results[2].error_code.as_deref(),
        Some("target_node_not_found")
    );

    assert_eq!(results[3].status, RelationWriteStatus::Failed);
    assert_eq!(
        results[3].error_code.as_deref(),
        Some("duplicate_active_edge"),
        "批内同键重复必须逐项拒绝"
    );

    // 顶层 relation_ids 收紧：恰为实际落库成功边 ID（与 results 成功项严格一致）
    assert_eq!(
        resp.relation_ids,
        vec![real_id.clone()],
        "relation_ids 只收实际落库成功项（假成功修复）"
    );

    // 语义自测③：回执 relation_id = 实际落库 ID —— 删除成功证明它真的在库里
    delete_memory(
        ctx.clone(),
        DeleteMemoryParams {
            memory_id: real_id.clone(),
        },
    )
    .await
    .expect("回执返回的边 ID 必须真实落库（可删除）");
    // 二次删除必须报错：证明第一次删除真的消费了这条边（不是预生成假 ID 的幻影成功）
    let again = delete_memory(ctx.clone(), DeleteMemoryParams { memory_id: real_id })
        .await
        .expect_err("已删除的边二次删除必须报错");
    assert!(
        again.to_string().contains("not_found"),
        "二次删除错误应指明不存在: {again}"
    );
}

/// 占位符批次：逐项 failed + 绝不落半悬空边（图谱遍历证伪）。
/// 覆盖语义自测②与拍板⑤。
#[sqlx::test]
async fn placeholder_relations_fail_per_item_without_any_edge(pool: sqlx::SqlitePool) {
    use crate::handlers::hr::agent::search_memory::search_memory;
    use common::api::SearchMemoryParams;

    let ctx = init_env(pool);
    let node = seed_node(&ctx, "占位符批次宿主").await;

    let resp = save_long_term_memory(
        ctx.clone(),
        SaveLongTermMemoryParams {
            node_name: "占位符批次".to_string(),
            node_description: "两条全占位符".to_string(),
            node_type: "concept".to_string(),
            summary: None,
            tags: None,
            is_published: None,
            relations: Some(vec![
                KnowledgeRelationParam {
                    source_node_id: "SELF".to_string(),
                    target_node_id: node.clone(),
                    relation_type: "related".to_string(),
                    weight: None,
                },
                KnowledgeRelationParam {
                    source_node_id: node.clone(),
                    target_node_id: "kn_new_self".to_string(),
                    relation_type: "related".to_string(),
                    weight: None,
                },
            ]),
            properties: None,
            task_id: None,
            source_short_term_ids: None,
        },
    )
    .await
    .expect("全失败仍必须 200 + 回执（拍板①）");

    let results = resp.relations_result.expect("必须有 keyed 回执");
    assert_eq!(results.len(), 2);
    assert!(
        results
            .iter()
            .all(|r| r.status == RelationWriteStatus::Failed),
        "占位符条目必须全 failed"
    );
    assert!(
        results
            .iter()
            .all(|r| r.error_code.as_deref() == Some("placeholder_node_id_rejected")),
        "占位符错误码必须逐项标注: {:?}",
        results
    );
    assert!(
        resp.relation_ids.is_empty(),
        "占位符绝不落半悬空边（relation_ids 必空）"
    );

    // 图谱遍历证伪：种子节点周边不存在任何边
    let graph = search_memory(
        ctx.clone(),
        SearchMemoryParams {
            query: String::new(),
            max_results: Some(50),
            memory_type: None,
            traversal_depth: Some(1),
            traversal_breadth: Some(10),
            traversal_strategy: Some("breadth_first".to_string()),
            seed_node_ids: Some(vec![node]),
            tags: None,
            task_id: None,
            agent_id: None,
            property_filters: None,
        },
    )
    .await
    .expect("遍历应成功");
    assert!(
        !graph.results.iter().any(|r| r.memory_type == "relation"),
        "占位符批次不得留下任何边: {:?}",
        graph.results
    );
}

/// 全失败批次：update 本体更新照常成功，HTTP 语义成功 + 全 failed 逐项回执。
/// 覆盖语义自测④与拍板①。
#[sqlx::test]
async fn all_failed_relations_return_ok_with_full_failed_receipt(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let node_a = seed_node(&ctx, "全失败宿主").await;
    let node_b = seed_node(&ctx, "全失败对端").await;

    let mut params = update_params(&node_a, None);
    params.relations = Some(vec![
        // 占位符
        KnowledgeRelationParam {
            source_node_id: "SELF".to_string(),
            target_node_id: "SELF".to_string(),
            relation_type: "related".to_string(),
            weight: None,
        },
        // 目标不存在
        KnowledgeRelationParam {
            source_node_id: node_b.clone(),
            target_node_id: "kn_ghost".to_string(),
            relation_type: "related".to_string(),
            weight: None,
        },
        // 源格式非法
        KnowledgeRelationParam {
            source_node_id: "not_a_kn_id".to_string(),
            target_node_id: node_b.clone(),
            relation_type: "related".to_string(),
            weight: None,
        },
    ]);

    let resp = update_memory(ctx.clone(), params)
        .await
        .expect("全失败批次不得整体报错（拍板①）");
    let results = resp.relations_result.expect("必须有 keyed 回执");
    assert_eq!(results.len(), 3);
    let codes: Vec<_> = results
        .iter()
        .map(|r| r.error_code.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(
        codes,
        vec![
            "placeholder_node_id_rejected",
            "target_node_not_found",
            "invalid_node_id",
        ],
        "逐项错误码必须按入参序标注: {:?}",
        codes
    );
    assert!(results.iter().all(|r| r.relation_id.is_none()));
}

/// property_issues V2 结构化：code 取自 error 词表、field 至少携带 term_key、
/// 类型不符另带 expected；V1 串过渡期并存。
/// 覆盖语义自测⑤与拍板③。
#[sqlx::test]
async fn property_issues_v2_structured_with_error_codes(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    define_property(&ctx, "confidence", "number").await;

    let resp = save_long_term_memory(
        ctx.clone(),
        SaveLongTermMemoryParams {
            node_name: "V2 结构化回执".to_string(),
            node_description: "类型不符 + 未定义属性".to_string(),
            node_type: "concept".to_string(),
            summary: None,
            tags: None,
            is_published: None,
            relations: None,
            properties: Some(vec![
                // 类型不符（number 声明 vs 文本值）→ 硬拒
                KnowledgePropertyParam {
                    term_key: "confidence".to_string(),
                    value: PropertyValue::Text("oops".to_string()),
                    ordinal: 0,
                },
                // 本体未定义 → 兜底写入（软门禁）
                KnowledgePropertyParam {
                    term_key: "brand_new_key".to_string(),
                    value: PropertyValue::Number(0.5),
                    ordinal: 0,
                },
            ]),
            task_id: None,
            source_short_term_ids: None,
        },
    )
    .await
    .expect("带属性的保存应成功（部分成功语义）");

    // V1 串保留（过渡并存）
    assert!(
        resp.property_issues
            .iter()
            .any(|s| s.starts_with("confidence: ")),
        "V1 串必须并存: {:?}",
        resp.property_issues
    );

    // V2 结构化
    let v2 = resp.property_issues_v2.expect("V2 回执必须存在");
    assert_eq!(v2.len(), 2, "两条问题各一条 V2 条目");

    let mismatch = v2
        .iter()
        .find(|e| e.field["term_key"] == "confidence")
        .expect("类型不符条目必须在场");
    assert_eq!(
        mismatch.code, "property_type_mismatch",
        "code 取自 error 词表"
    );
    assert_eq!(
        mismatch.field["expected"], "number",
        "类型不符必须把声明类型结构化带出"
    );
    assert!(!mismatch.message.is_empty(), "message 必须有可读原因");

    let undefined = v2
        .iter()
        .find(|e| e.field["term_key"] == "brand_new_key")
        .expect("未定义属性条目必须在场");
    assert_eq!(undefined.code, "property_undefined");
    assert_eq!(undefined.field["term_key"], "brand_new_key");
}

/// 无问题属性：V1 空数组、V2 缺省（None，序列化省略）。
#[sqlx::test]
async fn clean_properties_yield_empty_v1_and_omitted_v2(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    define_property(&ctx, "confidence", "number").await;

    let resp = save_long_term_memory(
        ctx.clone(),
        SaveLongTermMemoryParams {
            node_name: "干净属性".to_string(),
            node_description: "类型对齐应全过".to_string(),
            node_type: "concept".to_string(),
            summary: None,
            tags: None,
            is_published: None,
            relations: None,
            properties: Some(vec![KnowledgePropertyParam {
                term_key: "confidence".to_string(),
                value: PropertyValue::Number(0.8),
                ordinal: 0,
            }]),
            task_id: None,
            source_short_term_ids: None,
        },
    )
    .await
    .expect("类型对齐属性应全过");

    assert!(resp.property_issues.is_empty());
    assert!(resp.property_issues_v2.is_none(), "无问题不得有 V2 条目");
    assert!(
        resp.relations_result.is_none(),
        "未带 relations 不得有建边回执"
    );
}

// ========== C 单：tags 与发布标记拆分（user_tags / is_published / system_flags）==========

fn split_save_params(name: &str, tags: Option<Vec<&str>>) -> SaveLongTermMemoryParams {
    SaveLongTermMemoryParams {
        node_name: name.to_string(),
        node_description: format!("{name} 的正文"),
        node_type: "concept".to_string(),
        summary: None,
        tags: tags.map(|v| v.into_iter().map(|t| t.to_string()).collect()),
        is_published: None,
        relations: None,
        source_short_term_ids: None,
        properties: None,
        task_id: None,
    }
}

fn split_update_params(memory_id: &str) -> UpdateMemoryParams {
    UpdateMemoryParams {
        memory_id: memory_id.to_string(),
        content: None,
        summary: None,
        tags: None,
        status: None,
        node_tags: None,
        is_published: None,
        relations: None,
        properties: None,
    }
}

fn split_tags(json: &str) -> Vec<String> {
    crate::models::memory::parse_tags_json(json)
}

fn split_has(tags: &[String], tag: &str) -> bool {
    tags.iter().any(|t| t == tag)
}

async fn split_read_node(
    ctx: &RequestContext,
    id: &str,
) -> crate::models::memory::LongTermKnowledgeNodePo {
    crate::service::dao::memory::dao()
        .get_knowledge_node(ctx.clone(), id)
        .await
        .expect("查询应成功")
        .expect("节点应存在")
}

fn split_po(is_published: bool) -> crate::models::memory::LongTermKnowledgeNodePo {
    crate::models::memory::LongTermKnowledgeNodePo {
        id: "kn_split".to_string(),
        agent_id: "agent-x".to_string(),
        node_name: "拆分节点".to_string(),
        node_description: String::new(),
        node_type: "concept".to_string(),
        summary: String::new(),
        tags: "[]".to_string(),
        user_tags: r#"["架构"]"#.to_string(),
        status: MemoryStatus::Active,
        is_published,
        created_at: 0,
        updated_at: 0,
    }
}

/// **C-2 核心回归**：脏节点「读全字段 → 原样回写」不得静默清掉发布标记。
///
/// 存量形态：节点 `tags` 里带 `published`（迁移搬列时 `user_tags` 被填成同值，
/// 故也是「脏」的）。调用方读完回显（`tags` 只剩业务标签，`published` 出现在
/// `system_flags` 里），把回显的业务标签原样写回 `node_tags`、不碰 `is_published`。
/// 拆分前这一步会让 `is_published` 从 true 翻成 false —— 这就是被切断的损坏路径。
#[sqlx::test]
async fn split_roundtrip_write_never_clears_published(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let saved = save_long_term_memory(
        ctx.clone(),
        split_save_params("存量脏节点", Some(vec!["published", "架构"])),
    )
    .await
    .expect("保存应成功");

    let before = split_read_node(&ctx, &saved.node_id).await;
    assert!(before.is_published, "前置：带 published 标签应置为已发布");
    assert!(
        !split_has(&split_tags(&before.user_tags), "published"),
        "published 是系统标记，不得落进 user_tags：{}",
        before.user_tags
    );

    // 读侧回显 → 原样回写（只回写业务标签，is_published 不传 = 保持不变）
    let mut params = split_update_params(&saved.node_id);
    params.node_tags = Some(vec!["架构".to_string()]);
    update_memory(ctx.clone(), params)
        .await
        .expect("更新应成功");

    let after = split_read_node(&ctx, &saved.node_id).await;
    assert!(
        after.is_published,
        "C-2 回归：原样回写业务标签不得清掉发布标记"
    );
    assert_eq!(split_tags(&after.user_tags), vec!["架构".to_string()]);
    assert!(
        split_has(&split_tags(&after.tags), "published"),
        "旧列双写必须保留 published（FTS5 触发器与旧读侧零回归）：{}",
        after.tags
    );
}

/// 只写业务标签不得触碰发布状态（`is_published` 与业务标签彻底解耦）。
#[sqlx::test]
async fn split_business_tags_do_not_change_publish_state(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let saved = save_long_term_memory(ctx.clone(), split_save_params("普通节点", Some(vec!["a"])))
        .await
        .expect("保存应成功");
    assert!(!split_read_node(&ctx, &saved.node_id).await.is_published);

    let mut params = split_update_params(&saved.node_id);
    params.node_tags = Some(vec!["a".to_string(), "b".to_string()]);
    update_memory(ctx.clone(), params)
        .await
        .expect("更新应成功");

    let after = split_read_node(&ctx, &saved.node_id).await;
    assert!(!after.is_published, "只写业务标签不得改动发布状态");
    assert_eq!(
        split_tags(&after.user_tags),
        vec!["a".to_string(), "b".to_string()]
    );
}

/// `node_tags` 里塞 `published` → **语义收编**：折算成 `is_published = true`，
/// 且绝不写进 `user_tags`（旧行为会把它当业务标签落库）。
#[sqlx::test]
async fn split_published_in_node_tags_is_captured_not_stored(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let saved = save_long_term_memory(
        ctx.clone(),
        split_save_params("待发布节点", Some(vec!["真实标签"])),
    )
    .await
    .expect("保存应成功");

    let mut params = split_update_params(&saved.node_id);
    params.node_tags = Some(vec!["published".to_string(), "另一标签".to_string()]);
    update_memory(ctx.clone(), params)
        .await
        .expect("更新应成功");

    let after = split_read_node(&ctx, &saved.node_id).await;
    assert!(after.is_published, "node_tags 含 published 应折算成已发布");
    let user_tags = split_tags(&after.user_tags);
    assert!(
        !split_has(&user_tags, "published"),
        "published 不得污染 user_tags：{user_tags:?}"
    );
    assert_eq!(user_tags, vec!["另一标签".to_string()]);
    assert!(
        split_has(&split_tags(&after.tags), "published"),
        "旧列双写应补上 published"
    );
}

/// `system_flags` 与 `is_published` 恒自洽（知识节点恒有该字段，未发布为空数组）。
#[test]
fn split_system_flags_mirror_is_published() {
    use crate::models::memory::{Memory, MemoryPo};

    let published = Memory::new(MemoryPo::KnowledgeNode(split_po(true))).to_api_result();
    assert_eq!(published.system_flags, Some(vec!["published".to_string()]));
    assert_eq!(published.tags, Some(vec!["架构".to_string()]));

    let unpublished = Memory::new(MemoryPo::KnowledgeNode(split_po(false))).to_api_result();
    assert_eq!(
        unpublished.system_flags,
        Some(Vec::new()),
        "未发布 → 空数组（不是 None：知识节点恒有该字段）"
    );
    assert_eq!(
        unpublished.tags,
        Some(vec!["架构".to_string()]),
        "业务标签不受发布状态影响"
    );
}

/// **口径回归**：按 `published` 过滤 = 活跃（`status != 0`）∩ `is_published = 1`。
///
/// 三条否定断言直接钉死三个错误口径：
///   * 不是「全表已发布」（退役节点必须排除）
///   * 不是「tags 里带 published 的脏节点数」（脏值不承载语义）
///   * 不为 0（别再退化成「published 查不到」）
#[sqlx::test]
async fn split_published_filter_uses_is_published_column(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);

    // A：活跃 + 已发布，且**没有**任何 published 脏值 → 唯一应命中的
    let a = save_long_term_memory(ctx.clone(), split_save_params("A", Some(vec!["业务"])))
        .await
        .expect("保存 A 应成功");
    let mut p = split_update_params(&a.node_id);
    p.is_published = Some(true);
    update_memory(ctx.clone(), p).await.expect("发布 A 应成功");

    // B：活跃 + 未发布
    let _b = save_long_term_memory(ctx.clone(), split_save_params("B", Some(vec!["业务"])))
        .await
        .expect("保存 B 应成功");

    // D：活跃 + 未发布，但旧列被手工塞了 published 脏值 → 必须**不**命中
    let dirty_id = "kn_split_dirty";
    sqlx::query(
        r#"INSERT INTO long_term_knowledge_node
           (id, agent_id, node_name, node_description, node_type, summary, tags, user_tags,
            status, is_published, created_at, updated_at)
           VALUES (?, 'agent-x', 'D', '', 'concept', '', '["published"]', '["published"]', 1, 0, 0, 0)"#,
    )
    .bind(dirty_id)
    .execute(ctx.db_pool())
    .await
    .expect("造脏节点应成功");

    let hits = crate::service::dao::memory::dao()
        .query_knowledge_nodes(
            ctx.clone(),
            MemoryQuery {
                tags: Some(vec!["published".to_string()]),
                ..Default::default()
            },
        )
        .await
        .expect("查询应成功");

    let ids: Vec<&str> = hits.iter().map(|n| n.id.as_str()).collect();
    assert_ne!(ids.len(), 0, "published 过滤不得为 0（口径退化）");
    assert_eq!(
        ids,
        vec![a.node_id.as_str()],
        "published 过滤必须只命中「活跃 ∩ is_published=1」，实际: {ids:?}"
    );
    assert!(
        !ids.contains(&dirty_id),
        "tags 里的 published 脏值不得让节点命中（不许靠脏值兜）"
    );

    // 退役（Forgotten）节点即使已发布也不得命中
    let mut p = split_update_params(&a.node_id);
    p.status = Some("forgotten".to_string());
    update_memory(ctx.clone(), p).await.expect("遗忘 A 应成功");
    let after = crate::service::dao::memory::dao()
        .query_knowledge_nodes(
            ctx.clone(),
            MemoryQuery {
                tags: Some(vec!["published".to_string()]),
                ..Default::default()
            },
        )
        .await
        .expect("查询应成功");
    assert!(
        after.is_empty(),
        "退役节点不得计入 published 口径（≠全表口径）"
    );
}

/// `is_published = Some(false)` 能取消发布，且旧列 `tags` 里的 `published` 同步移除。
#[sqlx::test]
async fn split_is_published_false_unpublishes(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let saved = save_long_term_memory(
        ctx.clone(),
        split_save_params("已发布节点", Some(vec!["published", "业务"])),
    )
    .await
    .expect("保存应成功");
    assert!(split_read_node(&ctx, &saved.node_id).await.is_published);

    let mut params = split_update_params(&saved.node_id);
    params.is_published = Some(false);
    update_memory(ctx.clone(), params)
        .await
        .expect("取消发布应成功");

    let after = split_read_node(&ctx, &saved.node_id).await;
    assert!(!after.is_published, "is_published=Some(false) 应取消发布");
    assert!(
        !split_has(&split_tags(&after.tags), "published"),
        "旧列双写必须跟随取消发布（否则 FTS/旧读侧与真相源脱节）：{}",
        after.tags
    );

    use crate::models::memory::{Memory, MemoryPo};
    let dto = Memory::new(MemoryPo::KnowledgeNode(after)).to_api_result();
    assert_eq!(
        dto.system_flags,
        Some(Vec::new()),
        "取消发布后派生 system_flags 应变空数组"
    );
    assert_eq!(dto.tags, Some(vec!["业务".to_string()]));
}

/// **独立性断言**：`user_tags` 单独可检索，不依赖过渡期双写。
///
/// 造一个 `user_tags = ["a"]` 而旧列 `tags = '[]'` 的节点（模拟 D 单停写 tags
/// 之后的形态）—— 若过滤只看旧列，这里会立刻查不到。
#[sqlx::test]
async fn split_user_tags_is_searchable_without_legacy_column(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    sqlx::query(
        r#"INSERT INTO long_term_knowledge_node
           (id, agent_id, node_name, node_description, node_type, summary, tags, user_tags,
            status, is_published, created_at, updated_at)
           VALUES ('kn_only_user_tags', 'agent-x', '仅新列', '', 'concept', '',
                   '[]', '["a"]', 1, 0, 0, 0)"#,
    )
    .execute(ctx.db_pool())
    .await
    .expect("造节点应成功");

    let hits = crate::service::dao::memory::dao()
        .query_knowledge_nodes(
            ctx.clone(),
            MemoryQuery {
                tags: Some(vec!["a".to_string()]),
                ..Default::default()
            },
        )
        .await
        .expect("查询应成功");
    assert_eq!(
        hits.len(),
        1,
        "user_tags 必须独立可检索（D 单停写 tags 后标签检索不能失灵）"
    );

    // 反向：只写在旧列里的标签仍应命中（过渡期双写兼容）
    sqlx::query(
        r#"INSERT INTO long_term_knowledge_node
           (id, agent_id, node_name, node_description, node_type, summary, tags, user_tags,
            status, is_published, created_at, updated_at)
           VALUES ('kn_only_legacy_tags', 'agent-x', '仅旧列', '', 'concept', '',
                   '["legacy_only"]', '[]', 1, 0, 0, 0)"#,
    )
    .execute(ctx.db_pool())
    .await
    .expect("造节点应成功");
    let hits = crate::service::dao::memory::dao()
        .query_knowledge_nodes(
            ctx.clone(),
            MemoryQuery {
                tags: Some(vec!["legacy_only".to_string()]),
                ..Default::default()
            },
        )
        .await
        .expect("查询应成功");
    assert_eq!(hits.len(), 1, "旧列标签过渡期仍需可检索");
}
