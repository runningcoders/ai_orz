//! Handler: 搜索记忆 - Neural Tool

use std::collections::HashSet;

use crate::models::memory::{Memory, MemoryPo};
use crate::pkg::RequestContext;
use crate::service::dal::memory::TraversalStrategy;
use crate::service::dao::memory::MemorySearch;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{MemoryResult, SearchMemoryParams, SearchMemoryResponse};
use common::enums::MemoryType;
use common::error::{Result, bail_err, err};

/// Search memory by keyword or semantic query
#[register_handler_tool(
    id = "search_memory",
    name = "Search Memory (Semantic)",
    description = "Search memories with a free-text query via hybrid keyword + vector semantic matching; results are ranked and annotated with match_type, vector_distance, and fts_rank. memory_type accepts short_term/knowledge_node/trace/relation/all; traversal_strategy accepts breadth_first/depth_first. Values outside those lists are rejected with invalid_request rather than silently defaulted. Knowledge nodes are hive-shared: every agent can read all of them, and agent_id is only an optional ownership filter on the seeds (omit it to search the whole hive). Pass seed_node_ids to expand through the knowledge graph instead of searching: the seed nodes themselves are always returned (the center node is never dropped), traversal_depth/breadth/strategy shape the expansion, and neighbors are never filtered by ownership. For structured field filtering use query_memory.",
    params = "common::api::SearchMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn search_memory(
    ctx: RequestContext,
    params: SearchMemoryParams,
) -> Result<SearchMemoryResponse> {
    let user_id = ctx.uid();
    if user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    // 归属筛选：**只认显式传入的 `params.agent_id`**（空串 = None = 不过滤）。
    // ⚠️ 刻意**不**回退 `ctx.agent_id()`：
    // - 知识节点是蜂巢共享资产，回退会让 Agent 的检索静默收窄成「只看自己沉淀的节点」，
    //   与「所有 Agent 都能看到所有知识节点」相反；
    // - 短期记忆（私有）需要的归属回退由 DAL 内部完成（`private_agent_scope`），
    //   那里才分得清「私有要作用域」和「共享不要门槛」。
    // 该参数只约束**起点**的选取，不约束沿图展开（见下方分支注释）。
    let agent_filter: Option<String> = params.agent_id.clone().filter(|s| !s.is_empty());

    // 类型/遍历策略统一走枚举自带的 SSOT 解析，非法值直接 400（不做 `_ =>` 兜底）。
    // ⚠️ 静默降级会让调用方拼错一个词就拿到全量结果 / 另一种形状的图，而响应看起来成功。
    let memory_type = match params.memory_type.as_deref() {
        None => MemoryType::All,
        Some(raw) => MemoryType::parse(raw).ok_or_else(|| {
            err!(
                InvalidRequest,
                "不支持的 memory_type: `{}`，合法取值：{}",
                raw,
                MemoryType::ACCEPTED_VALUES
            )
        })?,
    };

    let traversal_depth = params.traversal_depth.unwrap_or(0);
    let traversal_breadth = params.traversal_breadth.unwrap_or(0);
    let traversal_strategy = match params.traversal_strategy.as_deref() {
        None => TraversalStrategy::DEFAULT,
        Some(raw) => TraversalStrategy::parse(raw).ok_or_else(|| {
            err!(
                InvalidRequest,
                "不支持的 traversal_strategy: `{}`，合法取值：{}",
                raw,
                TraversalStrategy::ACCEPTED_VALUES
            )
        })?,
    };
    let seed_node_ids = params.seed_node_ids.clone().unwrap_or_default();

    let has_seeds = !seed_node_ids.is_empty();
    let do_traversal = traversal_depth > 0;

    let mut all_memories: Vec<Memory> = Vec::new();

    if has_seeds {
        // 🎯 点名种子（前端点击节点展开 / 调用方指定起点）：直接沿图展开，**不搜索**。
        // - 种子**必返回**（中心节点缺席的话整张展开图就没有意义了）；
        // - 展开不受归属筛选：知识节点蜂巢共享，邻居不该因为属于别人而被丢掉；
        // - `traversal_depth = 0` 表示「只看种子、不展开」，同样走这条路而不是回退到搜索。
        let traversed = runtime_domain()
            .memory()
            .traverse_graph(
                ctx.clone(),
                &seed_node_ids,
                traversal_depth,
                traversal_breadth,
                traversal_strategy,
            )
            .await?;
        all_memories.extend(traversed);
    } else if do_traversal {
        // 🔍 关键词搜索选起点（**这里**才受归属筛选：`agent_id` 决定从谁的节点起步）
        // → 再沿图展开（蜂巢全域，见 `has_seeds` 分支的说明）。
        let search = MemorySearch {
            keyword: Some(params.query.clone()),
            top_k: params.max_results,
            filters: crate::service::dao::memory::MemoryQuery {
                memory_type: Some(MemoryType::KnowledgeNode),
                limit: params.max_results.map(|l| l as usize),
                tags: params.tags.clone(),
                agent_id: agent_filter.clone(),
                ..Default::default()
            },
            ..Default::default()
        };

        let search_results = runtime_domain()
            .memory()
            .search(ctx.clone(), search)
            .await?;

        let seed_ids: Vec<String> = search_results
            .iter()
            .filter_map(|m| match &m.po {
                MemoryPo::KnowledgeNode(kn) => Some(kn.id.clone()),
                _ => None,
            })
            .collect();

        all_memories.extend(search_results);

        if !seed_ids.is_empty() {
            let traversed = runtime_domain()
                .memory()
                .traverse_graph(
                    ctx.clone(),
                    &seed_ids,
                    traversal_depth,
                    traversal_breadth,
                    traversal_strategy,
                )
                .await?;
            all_memories.extend(traversed);
        }
    } else {
        let search = MemorySearch {
            keyword: Some(params.query.clone()),
            top_k: params.max_results,
            filters: crate::service::dao::memory::MemoryQuery {
                memory_type: Some(memory_type),
                limit: params.max_results.map(|l| l as usize),
                tags: params.tags.clone(),
                task_id: params.task_id.clone(),
                agent_id: agent_filter,
                ..Default::default()
            },
            ..Default::default()
        };

        let search_results = runtime_domain().memory().search(ctx, search).await?;
        all_memories.extend(search_results);
    }

    let mut seen = HashSet::new();
    let mut unique_memories = Vec::new();
    for memory in all_memories {
        let id = memory_id(&memory);
        if seen.insert(id) {
            unique_memories.push(memory);
        }
    }

    let results = memories_to_results(unique_memories);

    Ok(SearchMemoryResponse { results })
}

fn memory_id(memory: &Memory) -> String {
    match &memory.po {
        MemoryPo::Trace(t) => t.id.clone(),
        MemoryPo::ShortTerm(st) => st.id.clone(),
        MemoryPo::KnowledgeNode(kn) => kn.id.clone(),
        MemoryPo::Relation(rel) => rel.id.clone(),
    }
}

fn memories_to_results(memories: Vec<Memory>) -> Vec<MemoryResult> {
    // 字段映射收敛在 `Memory::to_api_result`（与 query_memory 共用一份实现），
    // 这里只负责批量转换
    memories.iter().map(Memory::to_api_result).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::hr::agent::save_long_term_memory::save_long_term_memory;
    use crate::models::memory::{
        KnowledgeNodeRelationPo, LongTermKnowledgeNodePo, MemoryCreateParams,
    };
    use crate::service::dao::memory::{MemoryQuery, MemorySearch};
    use common::api::{KnowledgeRelationParam, SaveLongTermMemoryParams};
    use common::enums::MemoryStatus;

    fn init_env(pool: sqlx::SqlitePool) -> RequestContext {
        let _ = crate::config::init();
        let base_path = crate::config::get().base_data_path();
        crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);
        crate::service::dao::init_all();
        crate::service::dal::init_all();
        crate::service::domain::runtime::init();
        crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
    }

    async fn seed_node(ctx: &RequestContext, id: &str, name: &str, desc: &str, summary: &str) {
        let now = chrono::Utc::now().timestamp();
        let node = LongTermKnowledgeNodePo {
            id: id.to_string(),
            agent_id: "agent-kg".to_string(),
            node_name: name.to_string(),
            node_description: desc.to_string(),
            node_type: "general".to_string(),
            summary: summary.to_string(),
            tags: r#"["published"]"#.to_string(),
            status: MemoryStatus::Active,
            is_published: true,
            created_at: now,
            updated_at: now,
        };
        runtime_domain()
            .memory()
            .create(
                ctx.clone(),
                MemoryCreateParams::CreateKnowledgeNode {
                    node,
                    references: vec![],
                },
            )
            .await
            .unwrap();
    }

    /// 读链路端到端回归：知识图谱页拿到的字段必须是**人可读**的。
    ///
    /// 覆盖用户反馈的三条（「打分 / 摘要 / 内容都是错的、没有信息量」）：
    /// 1. 关系边的 content / relation_type 曾是 `format!("{:?}")` 的 Rust 变体名
    ///    （`"Causes"`），前端关系标签词表只认 `"causes"` → 连线标签退化成英文；
    /// 2. 关系边没有独立正文，至少要能读出中文关系名；
    /// 3. 遍历展开出来的邻居节点/关系边没有匹配过程，`score` 保持 `None`，
    ///    而不是伪造一个 0 让前端显示成「匹配度 0%」。
    #[sqlx::test]
    async fn knowledge_graph_payload_is_human_readable(pool: sqlx::SqlitePool) {
        let ctx = init_env(pool);

        let desc =
            "订单状态机描述了订单从创建到完成的完整状态流转：待支付、已支付、已发货、已完成。";
        // 模拟「调用方没给摘要」的写入：此处显式落空串，验证读侧会归一成 None
        seed_node(&ctx, "kn_a", "订单状态机", desc, "").await;
        seed_node(
            &ctx,
            "kn_b",
            "订单超时补偿",
            "订单超时补偿负责在订单超时后触发回滚。",
            "",
        )
        .await;

        let rel = KnowledgeNodeRelationPo {
            id: "kr_1".to_string(),
            source_node_id: "kn_a".to_string(),
            target_node_id: "kn_b".to_string(),
            relation_type: "causes".to_string(),
            // 强度要能一路穿过「迁移 → INSERT → SELECT → PO → DTO」，
            // 任何一层漏掉列，图谱上的线宽就又退化成统一粗细
            weight: Some(0.8),
            created_at: 0,
            updated_at: 0,
        };
        runtime_domain()
            .memory()
            .create(ctx.clone(), MemoryCreateParams::CreateRelations(vec![rel]))
            .await
            .unwrap();

        let search = MemorySearch {
            keyword: Some("订单状态机".to_string()),
            top_k: Some(50),
            filters: MemoryQuery {
                memory_type: Some(MemoryType::KnowledgeNode),
                limit: Some(50),
                // 不选 Agent = 全域（蜂巢：所有知识节点都可见）
                ..Default::default()
            },
            ..Default::default()
        };
        let hits = runtime_domain()
            .memory()
            .search(ctx.clone(), search)
            .await
            .unwrap();
        let seed_ids: Vec<String> = hits
            .iter()
            .filter_map(|m| match &m.po {
                MemoryPo::KnowledgeNode(kn) => Some(kn.id.clone()),
                _ => None,
            })
            .collect();

        let mut all = hits;
        all.extend(
            runtime_domain()
                .memory()
                .traverse_graph(
                    ctx.clone(),
                    &seed_ids,
                    1,
                    10,
                    TraversalStrategy::BreadthFirst,
                )
                .await
                .unwrap(),
        );

        let results = memories_to_results(all);

        let relation = results
            .iter()
            .find(|r| r.memory_type == "relation")
            .expect("应返回关系边");
        assert_eq!(
            relation.relation_type.as_deref(),
            Some("causes"),
            "关系类型必须是 Display 的 snake_case，前端关系标签词表以此为 key"
        );
        assert_eq!(relation.content, "导致", "关系边内容应为中文标签");
        assert_eq!(relation.name.as_deref(), Some("导致"));
        assert_eq!(relation.source_node_id.as_deref(), Some("kn_a"));
        assert_eq!(relation.target_node_id.as_deref(), Some("kn_b"));
        assert_eq!(
            relation.weight,
            Some(0.8),
            "关系强度要从库里读回来，否则图谱上的粗细与 hover 读数都是空的"
        );
        assert!(
            !relation.content.contains("Causes"),
            "不应再出现 Rust 变体名: {}",
            relation.content
        );

        let neighbor = results
            .iter()
            .find(|r| r.id == "kn_b")
            .expect("应返回邻居节点");
        assert!(
            neighbor.score.is_none(),
            "遍历展开的邻居没有匹配过程，不应有分值"
        );
        assert!(
            neighbor.summary.is_none(),
            "空摘要要归一成 None，否则前端会渲染一个空的摘要块"
        );
        assert_eq!(neighbor.name.as_deref(), Some("订单超时补偿"));
        assert!(!neighbor.content.is_empty(), "正文不能为空");
        assert!(
            neighbor.weight.is_none(),
            "强度只属于关系边，节点/记忆条目不应带值"
        );
        assert_eq!(
            neighbor.tags,
            Some(Vec::new()),
            "published 是重要性/影响力控制位，不该出现在业务标签里"
        );
    }

    /// 造一个可指定归属与共享状态的节点（私有 / 已发布）
    async fn seed_scoped(
        ctx: &RequestContext,
        id: &str,
        name: &str,
        agent_id: &str,
        published: bool,
    ) {
        let now = chrono::Utc::now().timestamp();
        let node = LongTermKnowledgeNodePo {
            id: id.to_string(),
            agent_id: agent_id.to_string(),
            node_name: name.to_string(),
            node_description: format!("{name}的正文"),
            node_type: "general".to_string(),
            summary: format!("{name}的摘要"),
            tags: if published {
                r#"["published"]"#.to_string()
            } else {
                "[]".to_string()
            },
            status: MemoryStatus::Active,
            is_published: published,
            created_at: now,
            updated_at: now,
        };
        runtime_domain()
            .memory()
            .create(
                ctx.clone(),
                MemoryCreateParams::CreateKnowledgeNode {
                    node,
                    references: vec![],
                },
            )
            .await
            .unwrap();
    }

    async fn link(ctx: &RequestContext, id: &str, src: &str, tgt: &str) {
        runtime_domain()
            .memory()
            .create(
                ctx.clone(),
                MemoryCreateParams::CreateRelations(vec![KnowledgeNodeRelationPo {
                    id: id.to_string(),
                    source_node_id: src.to_string(),
                    target_node_id: tgt.to_string(),
                    relation_type: "causes".to_string(),
                    weight: Some(0.8),
                    created_at: 0,
                    updated_at: 0,
                }]),
            )
            .await
            .unwrap();
    }

    /// 前端「点击节点展开」的真实请求形状：seed-only + depth=1，query 为空
    fn click_params(seed: &str, agent_id: Option<&str>) -> SearchMemoryParams {
        SearchMemoryParams {
            query: String::new(),
            max_results: Some(50),
            memory_type: None,
            traversal_depth: Some(1),
            traversal_breadth: Some(10),
            traversal_strategy: Some("breadth_first".to_string()),
            seed_node_ids: Some(vec![seed.to_string()]),
            tags: None,
            task_id: None,
            agent_id: agent_id.map(|s| s.to_string()),
        }
    }

    /// 回归（用户实测 ×2）：点击一个节点展开时 ——
    /// ① **中心节点（种子）必须回来**；② 边必须随其两端节点一起回来；③ 展开不受归属筛选。
    ///
    /// 前端图谱卡片的名称回退链是 `name → summary → content`，三者都拿不到才显示
    /// 「未命名节点」。旧实现里 traverse 取节点用 `ctx.agent_id()` 做可见性过滤
    /// （HTTP 场景 ctx 恒为空 → 目标 Agent 的私有节点全被滤掉），而拉边不受该约束，
    /// 于是「只回来一堆边、节点全丢」/「点开节点缺中心节点」。
    #[sqlx::test]
    async fn click_expand_returns_seed_node_with_edges(pool: sqlx::SqlitePool) {
        let ctx = init_env(pool);
        // Agent 自行沉淀的节点默认是私有的（未发布）——蜂巢语义下同样全域可见
        seed_scoped(&ctx, "kn_a", "订单状态机", "agent-kg", false).await;
        seed_scoped(&ctx, "kn_b", "订单超时补偿", "agent-kg", false).await;
        link(&ctx, "kr_1", "kn_a", "kn_b").await;

        // 不选 Agent（全局）、选定本 Agent、以及**选了一个不相干的 Agent**，
        // 三种情况都必须拿到同一份完整邻居图：种子 + 邻居 + 边。
        for agent in [None, Some("agent-kg"), Some("agent-other")] {
            let resp = search_memory(ctx.clone(), click_params("kn_a", agent))
                .await
                .unwrap();
            let ids: Vec<&str> = resp.results.iter().map(|r| r.id.as_str()).collect();
            assert!(
                ids.contains(&"kn_a"),
                "中心节点（种子）必须返回，否则点开之后图中心是空的: agent={agent:?} {ids:?}"
            );
            assert!(
                ids.contains(&"kn_b"),
                "邻居节点必须返回，否则前端只能画「未命名节点」: agent={agent:?} {ids:?}"
            );
            assert!(ids.contains(&"kr_1"), "边必须返回: agent={agent:?} {ids:?}");

            for node in resp
                .results
                .iter()
                .filter(|r| r.memory_type == "knowledge_node")
            {
                assert!(
                    node.name.as_deref().is_some_and(|n| !n.trim().is_empty()),
                    "图谱卡片第一行取自 name，不能为空: {node:?}"
                );
                assert!(!node.content.is_empty(), "hover 详情要展示正文，不能为空");
            }
        }
    }

    /// 写入方标注的**词表外**关系名必须逐字回到 DTO —— 不能被归一成 `custom`。
    ///
    /// 回归：写入路径曾做 `KnowledgeRelationType::from()`，凡没进那 16 个变体的
    /// 标注（「实现」「被测试覆盖」…）都会被塌成 `Custom`，于是这一类边在图上
    /// 一律显示「自定义」—— Agent 明明标了明确语义，用户却什么都看不出来。
    #[sqlx::test]
    async fn relation_type_outside_vocabulary_stays_verbatim(pool: sqlx::SqlitePool) {
        let ctx = init_env(pool);
        seed_scoped(&ctx, "kn_impl", "支付接口的实现", "agent-kg", false).await;
        seed_scoped(&ctx, "kn_spec", "支付接口契约", "agent-kg", false).await;

        // 走真实写入路径（归一化就发生在这个 handler 里），挂一条词表外的关系
        let saved = save_long_term_memory(
            ctx.clone(),
            SaveLongTermMemoryParams {
                node_name: "支付接口实现说明".to_string(),
                node_description: "记录实现与契约的对应关系".to_string(),
                node_type: "concept".to_string(),
                summary: Some("实现与契约的对应".to_string()),
                tags: None,
                relations: Some(vec![KnowledgeRelationParam {
                    source_node_id: "kn_impl".to_string(),
                    target_node_id: "kn_spec".to_string(),
                    relation_type: "实现".to_string(),
                    weight: None,
                }]),
                task_id: None,
            },
        )
        .await
        .unwrap();
        assert!(!saved.relation_ids.is_empty(), "关系应写入成功");

        let resp = search_memory(ctx.clone(), click_params("kn_impl", None))
            .await
            .unwrap();
        let edge = resp
            .results
            .iter()
            .find(|r| {
                r.memory_type == "relation"
                    && r.source_node_id.as_deref() == Some("kn_impl")
                    && r.target_node_id.as_deref() == Some("kn_spec")
            })
            .expect("应返回 kn_impl → kn_spec 这条边");

        assert_eq!(
            edge.relation_type.as_deref(),
            Some("实现"),
            "词表外的原文必须原样回传，不能被归一成 custom"
        );
        assert_eq!(edge.content, "实现", "没有中文映射可用时展示标签就是原文");
        assert_ne!(
            edge.content, "自定义",
            "匹配不上时绝不能替换成「自定义」—— 那会把 Agent 标注的语义抹掉"
        );
    }

    /// 软删除的端点必须**连边一起消失**（图批次不变式：边只随两端节点一起返回）。
    ///
    /// 可见性门槛已经不在归属维度上了（蜂巢共享），真正会让端点缺席的是
    /// `status = Forgotten`：节点被遗忘后，挂在它身上的边不能再单独漏出来，
    /// 否则前端会凭空长出一个「未命名节点」。
    #[sqlx::test]
    async fn forgotten_endpoint_drops_the_edge(pool: sqlx::SqlitePool) {
        let ctx = init_env(pool);
        seed_scoped(&ctx, "kn_live", "在库节点", "agent-kg", true).await;
        // 造一个已遗忘的节点（软删除），它是那条边的远端
        let now = chrono::Utc::now().timestamp();
        runtime_domain()
            .memory()
            .create(
                ctx.clone(),
                MemoryCreateParams::CreateKnowledgeNode {
                    node: LongTermKnowledgeNodePo {
                        id: "kn_gone".to_string(),
                        agent_id: "agent-kg".to_string(),
                        node_name: "已遗忘节点".to_string(),
                        node_description: "已遗忘节点的正文".to_string(),
                        node_type: "general".to_string(),
                        summary: "已遗忘节点的摘要".to_string(),
                        tags: "[]".to_string(),
                        status: MemoryStatus::Forgotten,
                        is_published: false,
                        created_at: now,
                        updated_at: now,
                    },
                    references: vec![],
                },
            )
            .await
            .unwrap();
        link(&ctx, "kr_cross", "kn_live", "kn_gone").await;

        let resp = search_memory(ctx.clone(), click_params("kn_live", None))
            .await
            .unwrap();
        let kinds: Vec<(&str, &str)> = resp
            .results
            .iter()
            .map(|r| (r.id.as_str(), r.memory_type.as_str()))
            .collect();

        assert!(
            kinds.contains(&("kn_live", "knowledge_node")),
            "在库节点必须可见: {kinds:?}"
        );
        assert!(
            !kinds.iter().any(|(id, _)| *id == "kn_gone"),
            "已遗忘节点不应出现在图上: {kinds:?}"
        );
        assert!(
            !kinds.iter().any(|(_, t)| *t == "relation"),
            "只有一端在批内的边不能返回，否则另一端会在图上变成「未命名节点」: {kinds:?}"
        );
    }

    /// 搜索路径（无种子、`traversal_depth=0`）同样守「边必须两端节点同在批内」。
    ///
    /// `search_relations_internal` 会把命中节点的**全部**入/出边一并带出来，远端节点没命中
    /// 时那条边就是半条信息（前端只能把远端画成「未命名节点」）—— 必须在这里丢掉。
    #[sqlx::test]
    async fn keyword_search_never_returns_dangling_edges(pool: sqlx::SqlitePool) {
        let ctx = init_env(pool);
        seed_scoped(&ctx, "kn_x", "苹果种植技术要点", "agent-kg", false).await;
        seed_scoped(&ctx, "kn_y", "香蕉冷链运输方案", "agent-kg", false).await;
        // 只有 kn_x 会被关键词命中；kn_y 是「远端未命中」的那一侧
        link(&ctx, "kr_xy", "kn_x", "kn_y").await;

        let resp = search_memory(
            ctx.clone(),
            SearchMemoryParams {
                query: "苹果种植".to_string(),
                max_results: Some(50),
                memory_type: None,
                traversal_depth: None,
                traversal_breadth: None,
                traversal_strategy: None,
                seed_node_ids: None,
                tags: None,
                task_id: None,
                agent_id: Some("agent-kg".to_string()),
            },
        )
        .await
        .unwrap();

        let node_ids: HashSet<&str> = resp
            .results
            .iter()
            .filter(|r| r.memory_type != "relation")
            .map(|r| r.id.as_str())
            .collect();
        assert!(
            node_ids.contains("kn_x"),
            "关键词命中的节点必须在结果里，否则本测试是空跑的: {:?}",
            resp.results
        );
        // 不变式：返回的每一条边，两端节点都要在结果里
        for rel in resp.results.iter().filter(|r| r.memory_type == "relation") {
            assert!(
                node_ids.contains(rel.source_node_id.as_deref().unwrap_or_default()),
                "边 {} 的源端不在结果里: {:?}",
                rel.id,
                resp.results
            );
            assert!(
                node_ids.contains(rel.target_node_id.as_deref().unwrap_or_default()),
                "边 {} 的目标端不在结果里: {:?}",
                rel.id,
                resp.results
            );
        }
        // 本场景下（FTS 只命中 kn_x）这条半条边必须已被丢弃
        assert!(
            !resp.results.iter().any(|r| r.id == "kr_xy"),
            "远端未命中的边不该出现在搜索结果里: {:?}",
            resp.results
        );
    }

    /// 非法 `memory_type` / `traversal_strategy` 必须报 400，**不能静默降级**。
    ///
    /// 回归背景：这两个字段曾用 `_ => MemoryType::All` / `_ => BreadthFirst` 兜底 ——
    /// 拼错一个词就拿到**全量结果**或**另一种形状的图**，而响应看起来完全成功。
    #[sqlx::test]
    async fn invalid_filter_values_are_rejected_instead_of_defaulted(pool: sqlx::SqlitePool) {
        let ctx = init_env(pool);

        let base = |memory_type: Option<&str>, strategy: Option<&str>| SearchMemoryParams {
            query: "任意关键词".to_string(),
            max_results: Some(5),
            memory_type: memory_type.map(|s| s.to_string()),
            traversal_depth: None,
            traversal_breadth: None,
            traversal_strategy: strategy.map(|s| s.to_string()),
            seed_node_ids: None,
            tags: None,
            task_id: None,
            agent_id: None,
        };

        // 拼错的 memory_type（少了 d）→ 400，而不是悄悄搜全部
        let err = search_memory(ctx.clone(), base(Some("knowlege_node"), None))
            .await
            .expect_err("拼错的 memory_type 必须报错");
        let msg = err.to_string();
        assert!(msg.contains("invalid_request"), "错误码不对: {msg}");
        assert!(msg.contains("memory_type"), "错误信息应指明字段: {msg}");
        assert!(
            msg.contains("knowledge_node"),
            "错误信息应列出合法取值供调用方纠正: {msg}"
        );

        // 拼错的 traversal_strategy → 400，而不是悄悄退化成 BFS
        let err = search_memory(ctx.clone(), base(None, Some("depth")))
            .await
            .expect_err("拼错的 traversal_strategy 必须报错");
        let msg = err.to_string();
        assert!(msg.contains("invalid_request"), "错误码不对: {msg}");
        assert!(
            msg.contains("traversal_strategy"),
            "错误信息应指明字段: {msg}"
        );

        // 合法值（含 PascalCase 与显式 all）照常放行
        for good in [
            Some("knowledge_node"),
            Some("KnowledgeNode"),
            Some("all"),
            None,
        ] {
            search_memory(ctx.clone(), base(good, Some("depth_first")))
                .await
                .unwrap_or_else(|e| panic!("合法 memory_type {good:?} 不该报错: {e}"));
        }
    }
}
