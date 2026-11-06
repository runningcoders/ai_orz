//! Runtime Memory 具体实现

use crate::models::memory::{
    KnowledgePropertyPo, Memory, MemoryCreateParams, MemoryTrace, MemoryTracePosition,
};
use crate::pkg::request_context::RequestContext;
use crate::service::dal::memory::TraversalStrategy;
use crate::service::dao::memory::{MemoryQuery, MemorySearch};
use crate::service::domain::runtime::{RuntimeDomainImpl, RuntimeMemory};
use common::enums::ontology::PropertyTargetType;
use common::error::{Result, err};
use common::ontology_property::{PropertyEntry, PropertyFilter, PropertySaveReport};

impl RuntimeDomainImpl {
    /// **按需翻页**取「记忆索引候选集」：从 T0 往过去一页页查，预算装满就停，
    /// 不够才查下一页（不再一次性预取固定上限的候选集）。
    ///
    /// 每取满一页，用与生产装配**完全相同**的预算 / T0 / `estimate_tokens`
    /// 重算一次 `MemoryIndexPlan`；`needs_more() == false` 表示最后一档配额
    /// 已经耗尽，再翻更早的页也不可能多留一行，立刻收手。预算没装满则继续
    /// 翻页 —— 大窗口模型不会因固定页数被截断，小窗口模型也不白读一行。
    ///
    /// 返回**时间正序**（旧 → 新）。已过滤 `created_at > T0`（未来记忆 ⇒ 同一次
    /// 唤醒链内索引块逐字节冻结）。时间**不参与分档**、也不设遗忘地平线：
    /// 保留多少条始终由 `MemoryIndexPlan::build` 按字符预算从新往旧累加裁决。
    /// 页数硬上限只防病态数据下的失控扫描，**不是保留条数上限**。
    pub(crate) async fn load_memory_index_window(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        anchor_ts: i64,
        budget_tokens: usize,
    ) -> Result<Vec<Memory>> {
        use crate::models::memory::{
            MEMORY_INDEX_MAX_PAGES, MEMORY_INDEX_PAGE_SIZE, MemoryIndexPlan, MemoryPo,
            normalize_timestamp_secs,
        };
        use crate::service::dao::memory::MemorySortOrder;
        use common::enums::MemoryType;

        let scoped = ctx.to_builder().agent_id(agent_id).build();
        let anchor = normalize_timestamp_secs(anchor_ts);
        // `created_at DESC, id ASC` 的分页次序在 DAO 里已固定，跨页相对次序确定，
        // 不会因同秒写入而漏行（见 short_term.rs 的 ORDER BY 注释）。
        let mut picked: Vec<Memory> = Vec::new(); // DESC 页序

        for page in 0..MEMORY_INDEX_MAX_PAGES {
            let mut rows = crate::service::dal::memory::dal()
                .query(
                    scoped.clone(),
                    MemoryQuery {
                        agent_id: Some(agent_id.to_string()),
                        memory_type: Some(MemoryType::ShortTerm),
                        limit: Some(MEMORY_INDEX_PAGE_SIZE),
                        offset: Some(page * MEMORY_INDEX_PAGE_SIZE),
                        order: MemorySortOrder::RecentFirst,
                        ..Default::default()
                    },
                )
                .await?;
            let page_len = rows.len();
            rows.retain(|memory| match &memory.po {
                MemoryPo::ShortTerm(st) => normalize_timestamp_secs(st.created_at) <= anchor,
                _ => false,
            });
            picked.extend(rows);
            if page_len < MEMORY_INDEX_PAGE_SIZE {
                break; // DB 已翻到头，再查也是空
            }
            // 计划要求时间正序；这里翻成旧→新视图再算。候选集最多 1000 行，
            // 每页一次纯内存重算的代价远小于多查一页 DB。
            let ascending: Vec<Memory> = picked.iter().rev().cloned().collect();
            let plan = MemoryIndexPlan::build(
                &ascending,
                Some(anchor),
                budget_tokens,
                crate::service::domain::runtime::compaction::estimate_tokens,
            );
            if !plan.needs_more() {
                break;
            }
        }

        picked.reverse(); // DESC 页序 → 时间正序：新增记忆落在纯尾部追加
        Ok(picked)
    }
}

#[async_trait::async_trait]
impl RuntimeMemory for RuntimeDomainImpl {
    // === 内部使用方法 ===

    async fn get_recent_context(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        let ctx = ctx.to_builder().agent_id(agent_id).build();
        use crate::service::dal::memory::dal;
        // DB 侧取的是「最近 N 条」的倒序（created_at DESC），这里翻转成时间正序。
        // 【历史对话】的渲染顺序直接决定前缀缓存命中：倒序时新增一条记忆会插到
        // 整块最前面，令其后的所有区块从第一个字符起失配；正序在窗口未满时是纯
        // 尾部追加，整块前缀可继续命中（满了以后按窗口滑动）。
        let mut memories = dal()
            .query(
                ctx,
                MemoryQuery {
                    agent_id: Some(agent_id.to_string()),
                    memory_type: Some(common::enums::MemoryType::ShortTerm),
                    limit: Some(limit),
                    ..Default::default()
                },
            )
            .await?;
        memories.reverse();
        Ok(memories)
    }

    /// 写入思考 Trace
    ///
    /// 直接接收外部构造的 MemoryTrace，内部只负责调用 DAL 写入
    /// 调用方提前构造 trace 可以拿到 trace_id 注入 Prompt
    async fn write_thinking_trace(
        &self,
        ctx: RequestContext,
        mut trace: MemoryTrace,
    ) -> Result<Memory> {
        use crate::service::dal::memory::dal;

        // 可选：内部统一补充信息（如果缺失）
        if trace.log_id.is_empty() {
            trace.log_id = ctx.log_id.clone();
        }
        if trace.user_id.is_empty() {
            trace.user_id = ctx.uid();
        }
        if trace.organization_id.is_empty() {
            trace.organization_id = ctx.organization_id.clone().unwrap_or_default();
        }
        if trace.task_id.is_none() {
            trace.task_id = ctx.task_id().cloned();
        }

        let mut results = dal()
            .create(ctx, MemoryCreateParams::AppendTraces(vec![trace]))
            .await?;

        results
            .pop()
            .ok_or_else(|| err!(Internal, "Write trace failed, no memory returned"))
    }

    // === 公开方法（供 Handler/神经工具调用） ===

    async fn search(&self, ctx: RequestContext, search: MemorySearch) -> Result<Vec<Memory>> {
        use crate::service::dal::memory::dal;
        dal().search(ctx, search).await
    }

    async fn query(&self, ctx: RequestContext, query: MemoryQuery) -> Result<Vec<Memory>> {
        use crate::service::dal::memory::dal;
        dal().query(ctx, query).await
    }

    async fn resolve_trace_positions(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        trace_ids: &[String],
    ) -> Result<std::collections::HashMap<String, MemoryTracePosition>> {
        use crate::service::dal::memory::dal;
        dal()
            .resolve_trace_positions(ctx, agent_id, trace_ids)
            .await
    }

    async fn mark_short_term_settled(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        memory_ids: &[String],
    ) -> Result<usize> {
        use crate::service::dal::memory::dal;
        dal()
            .mark_short_term_settled(ctx, agent_id, memory_ids)
            .await
    }

    async fn recommend_seed_nodes(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<crate::models::memory::SeedNodeRecommendation>> {
        use crate::service::dal::memory::dal;
        dal().recommend_seed_nodes(ctx, agent_id, limit).await
    }

    async fn get_knowledge_graph(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
    ) -> Result<crate::models::memory::KnowledgeGraphData> {
        use crate::service::dal::memory::dal;
        dal().get_knowledge_graph(ctx, agent_id).await
    }

    async fn create(&self, ctx: RequestContext, params: MemoryCreateParams) -> Result<Vec<Memory>> {
        use crate::service::dal::memory::dal;

        // 写路径单管道（design 决策 #16）：原文落库成功后的唯一动作是 publish
        // 词条事件——不打点、不解析、不调 domain/dal，认证与漂移记账全在消费侧
        let written = extract_written_terms(&ctx, &params);
        // 骨架软门禁需要写边两端节点的类型，落库前先留一份边清单（写入后校验）
        let edges_for_skeleton = match &params {
            MemoryCreateParams::CreateRelations(relations) => Some(relations.clone()),
            _ => None,
        };
        let results = dal().create(ctx.clone(), params).await?;

        if let Some(edges) = edges_for_skeleton {
            // 软门禁：类型不匹配只留痕（log + 漂移打点），绝不阻断/回滚写入
            apply_skeleton_soft_gate(&ctx, &edges).await;
        }

        if let Some((agent_id, terms)) = written {
            // publish 返回 ()：投递失败已在 registry 内 sys_error! 兜底落日志，无需调用侧处理
            let _ = crate::pkg::aop::publish(
                &ctx,
                crate::models::events::MemoryTermsWrittenEvent::new(agent_id, terms),
            )
            .await;
        }

        Ok(results)
    }

    async fn update(&self, ctx: RequestContext, memory: Memory) -> Result<Memory> {
        use crate::service::dal::memory::dal;
        dal().update(ctx, memory).await
    }

    async fn delete(&self, ctx: RequestContext, memory: Memory) -> Result<()> {
        use crate::service::dal::memory::dal;
        dal().delete(ctx, memory).await
    }

    async fn traverse_graph(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        strategy: TraversalStrategy,
        include_retired: bool,
    ) -> Result<Vec<Memory>> {
        use crate::service::dal::memory::dal;
        dal()
            .traverse_knowledge_graph(
                ctx,
                seed_node_ids,
                max_depth,
                max_breadth,
                strategy,
                include_retired,
            )
            .await
    }

    // ==================== 实体属性（薄编排：门闩与事务都在 DAL） ====================

    async fn save_entity_properties(
        &self,
        ctx: RequestContext,
        target_type: PropertyTargetType,
        target_id: &str,
        entries: Vec<PropertyEntry>,
    ) -> Result<PropertySaveReport> {
        use crate::service::dal::memory::dal;
        dal()
            .save_entity_properties(ctx, target_type, target_id, entries)
            .await
    }

    async fn list_entity_properties(
        &self,
        ctx: RequestContext,
        target_type: PropertyTargetType,
        target_id: &str,
    ) -> Result<Vec<KnowledgePropertyPo>> {
        use crate::service::dal::memory::dal;
        dal()
            .list_entity_properties(ctx, target_type, target_id)
            .await
    }

    async fn list_entity_properties_batch(
        &self,
        ctx: RequestContext,
        targets: &[(PropertyTargetType, String)],
    ) -> Result<std::collections::HashMap<(String, String), Vec<KnowledgePropertyPo>>> {
        use crate::service::dal::memory::dal;
        dal().list_entity_properties_batch(ctx, targets).await
    }

    async fn list_entity_properties_batch_with_history(
        &self,
        ctx: RequestContext,
        targets: &[(PropertyTargetType, String)],
    ) -> Result<std::collections::HashMap<(String, String), Vec<KnowledgePropertyPo>>> {
        use crate::service::dal::memory::dal;
        dal()
            .list_entity_properties_batch_with_history(ctx, targets)
            .await
    }

    async fn query_entities_by_properties(
        &self,
        ctx: RequestContext,
        filters: &[PropertyFilter],
    ) -> Result<Vec<String>> {
        use crate::service::dal::memory::dal;
        dal().query_entities_by_properties(ctx, filters).await
    }
}

/// 骨架软门禁：对本次写入的知识边按「类-关系-类」骨架做类型校验
///
/// 设计契约：
/// - **软门禁**：Mismatch 只留痕（warn 日志 + `ontology_drift` 打点 kind=`skeleton`），
///   绝不阻断或回滚已落库的边——骨架是治理信号，不是写入门闩
/// - 无任何三元组约束时 `check` 返回 `Unconstrained`，不产生噪声
/// - 两端节点缺失（并发删除等）时跳过；骨架加载失败只降级为一条 warn
/// - 只做大小写/空白归一（`normalize`），不做同义映射（同义归并走异步认证链路）
async fn apply_skeleton_soft_gate(
    ctx: &RequestContext,
    edges: &[crate::models::memory::KnowledgeNodeRelationPo],
) {
    use crate::models::memory::MemoryPo;
    use common::ontology::{SkeletonVerdict, normalize};

    if edges.is_empty() {
        return;
    }

    let node_ids: Vec<String> = edges
        .iter()
        .flat_map(|e| [e.source_node_id.clone(), e.target_node_id.clone()])
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let nodes = match crate::service::dal::memory::dal()
        .query(
            ctx.clone(),
            MemoryQuery {
                ids: Some(node_ids),
                ..Default::default()
            },
        )
        .await
    {
        Ok(nodes) => nodes,
        Err(e) => {
            crate::log_warn!(
                ctx,
                "memory_skeleton_gate",
                "加载边端点失败，跳过校验: {}",
                e
            );
            return;
        }
    };

    let node_types: std::collections::HashMap<String, String> = nodes
        .into_iter()
        .filter_map(|m| match m.po {
            MemoryPo::KnowledgeNode(node) => Some((node.id, normalize(&node.node_type))),
            _ => None,
        })
        .collect();

    let skeleton = match crate::service::dal::ontology::dal()
        .load_skeleton(ctx.clone())
        .await
    {
        Ok(skeleton) => skeleton,
        Err(e) => {
            crate::log_warn!(
                ctx,
                "memory_skeleton_gate",
                "加载本体骨架失败，跳过校验: {}",
                e
            );
            return;
        }
    };

    for edge in edges {
        let (Some(subject), Some(object)) = (
            node_types.get(&edge.source_node_id),
            node_types.get(&edge.target_node_id),
        ) else {
            continue;
        };
        let relation = normalize(&edge.relation_type);
        if skeleton.check(&relation, subject, object) != SkeletonVerdict::Mismatch {
            continue;
        }

        let triple = format!("{subject} -[{relation}]-> {object}");
        crate::log_warn!(
            ctx,
            "memory_skeleton_gate",
            "骨架类型不匹配（软门禁，写入保留）: {}",
            triple
        );

        // 漂移打点失败不阻塞主流程（与词元认证消费者同一兜底口径）
        if let Some(stats) = crate::pkg::stats::global_stats() {
            let stats_event =
                crate::pkg::stats::OntologyDriftEvent::new(chrono::Utc::now().timestamp())
                    .with_agent_id(ctx.agent_id().cloned().unwrap_or_default())
                    .with_kind("skeleton".to_string())
                    .with_raw_term(triple);
            if let Err(e) = stats.record(ctx.clone(), stats_event).await {
                crate::log_warn!(ctx, "memory_skeleton_gate", "骨架漂移打点失败: {}", e);
            }
        }
    }
}

/// 从写入参数提取本次落库的词条清单（写路径单管道，design 决策 #16）
///
/// - `CreateKnowledgeNode` → node_type（class 类词元；agent_id 取 node.agent_id）
/// - `CreateRelations` → relation_type（relation 类词元）
/// - 其余变体无词条 → None；原文 trim 后为空的词条跳过
/// - agent_id 优先取数据自带事实（node.agent_id），回退请求上下文
fn extract_written_terms(
    ctx: &RequestContext,
    params: &MemoryCreateParams,
) -> Option<(Option<String>, Vec<crate::models::events::WrittenTerm>)> {
    use common::ontology::TermKind;

    let (node_agent, raw_terms): (Option<String>, Vec<(TermKind, String)>) = match params {
        MemoryCreateParams::CreateKnowledgeNode { node, .. } => (
            Some(node.agent_id.clone()).filter(|s| !s.is_empty()),
            vec![(TermKind::Class, node.node_type.clone())],
        ),
        MemoryCreateParams::CreateRelations(relations) => (
            None,
            relations
                .iter()
                .map(|r| (TermKind::Relation, r.relation_type.clone()))
                .collect(),
        ),
        _ => return None,
    };

    let terms: Vec<crate::models::events::WrittenTerm> = raw_terms
        .into_iter()
        .map(|(kind, raw)| crate::models::events::WrittenTerm {
            kind,
            raw_term: raw.trim().to_string(),
        })
        .filter(|t| !t.raw_term.is_empty())
        .collect();

    if terms.is_empty() {
        return None;
    }

    let agent_id = node_agent
        .or_else(|| ctx.agent_id().cloned())
        .filter(|s| !s.is_empty());
    Some((agent_id, terms))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::memory::{KnowledgeNodeRelationPo, LongTermKnowledgeNodePo};
    use common::enums::MemoryStatus;
    use common::ontology::TermKind;

    fn node_po(agent_id: &str, node_type: &str) -> LongTermKnowledgeNodePo {
        LongTermKnowledgeNodePo {
            id: "kn_test".to_string(),
            agent_id: agent_id.to_string(),
            node_name: "测试节点".to_string(),
            node_description: String::new(),
            node_type: node_type.to_string(),
            summary: String::new(),
            tags: "[]".to_string(),
            user_tags: "[]".to_string(),
            status: MemoryStatus::Active,
            is_published: false,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn relation_po(relation_type: &str) -> KnowledgeNodeRelationPo {
        KnowledgeNodeRelationPo {
            id: "kr_test".to_string(),
            source_node_id: "kn_a".to_string(),
            target_node_id: "kn_b".to_string(),
            relation_type: relation_type.to_string(),
            weight: None,
            status: common::enums::KnowledgeRelationStatus::Active,
            created_at: 0,
            updated_at: 0,
        }
    }

    /// 知识节点 → class 词条，agent_id 取 node 自带事实
    #[sqlx::test]
    async fn extract_knowledge_node_yields_class_term(pool: sqlx::SqlitePool) {
        let ctx = crate::pkg::request_context_test_support::new_test_ctx("u1", pool)
            .to_builder()
            .agent_id("agent_node")
            .build();
        let params = MemoryCreateParams::CreateKnowledgeNode {
            node: node_po("agent_from_node", "  preference  "),
            references: vec![],
        };

        let (agent_id, terms) = extract_written_terms(&ctx, &params).expect("应有词条");

        assert_eq!(agent_id.as_deref(), Some("agent_from_node"));
        assert_eq!(terms.len(), 1);
        assert!(matches!(terms[0].kind, TermKind::Class));
        assert_eq!(terms[0].raw_term, "preference");
    }

    /// 关系批次 → relation 词条（保留原文空白归一），agent_id 回退 ctx
    #[sqlx::test]
    async fn extract_relations_yields_relation_terms_with_ctx_agent(pool: sqlx::SqlitePool) {
        let ctx = crate::pkg::request_context_test_support::new_test_ctx("u1", pool)
            .to_builder()
            .agent_id("agent_ctx")
            .build();
        let params = MemoryCreateParams::CreateRelations(vec![
            relation_po("管理"),
            relation_po("  implements  "),
        ]);

        let (agent_id, terms) = extract_written_terms(&ctx, &params).expect("应有词条");

        assert_eq!(agent_id.as_deref(), Some("agent_ctx"));
        assert_eq!(terms.len(), 2);
        assert!(terms.iter().all(|t| matches!(t.kind, TermKind::Relation)));
        assert_eq!(terms[0].raw_term, "管理");
        assert_eq!(terms[1].raw_term, "implements");
    }

    /// AppendTraces 变体无词条 → None
    #[sqlx::test]
    async fn extract_traces_yields_none(pool: sqlx::SqlitePool) {
        let ctx = crate::pkg::request_context_test_support::new_test_ctx("u1", pool);
        let params = MemoryCreateParams::AppendTraces(vec![MemoryTrace::new(
            "agent_1".to_string(),
            "log_1".to_string(),
            "u1".to_string(),
            String::new(),
            common::enums::MemoryRole::User,
            "内容".to_string(),
            None,
        )]);

        assert!(extract_written_terms(&ctx, &params).is_none());
    }

    /// node_type 全空白 → 词条全被过滤 → None
    #[sqlx::test]
    async fn extract_blank_terms_yields_none(pool: sqlx::SqlitePool) {
        let ctx = crate::pkg::request_context_test_support::new_test_ctx("u1", pool)
            .to_builder()
            .agent_id("agent_ctx")
            .build();
        let params = MemoryCreateParams::CreateKnowledgeNode {
            node: node_po("agent_node", "   "),
            references: vec![],
        };

        assert!(extract_written_terms(&ctx, &params).is_none());
    }
}
