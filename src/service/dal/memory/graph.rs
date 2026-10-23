//! 知识图谱聚合与遍历（推荐起点 / 全局点线视图 / BFS·DFS 展开）

use super::{MemoryDalImpl, TraversalStrategy, TraverseState, drop_dangling_relations};
use crate::models::memory::{
    KnowledgeGraphData, KnowledgeNodeRelationPo, LongTermKnowledgeNodePo, Memory, MemoryPo,
    SeedNodeRecommendation,
};
use crate::pkg::RequestContext;
use crate::service::dao::memory::MemoryQuery;
use common::enums::{MemoryStatus, MemoryType};
use common::error::Result;
use std::collections::{HashMap, HashSet, VecDeque};

impl MemoryDalImpl {
    pub(super) async fn recommend_seed_nodes(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
        limit: usize,
    ) -> Result<Vec<SeedNodeRecommendation>> {
        // 1. 拉取知识节点
        //
        // ⚠️ 蜂巢语义：不选 Agent 时（`agent_id = None`）池子是**全部** Agent 的知识节点，
        // 不再是「只挑 published」。`published` 降级为「重要性/影响力」信号，只在
        // 连接度持平时作为决胜项（见下方排序），不再决定可见性/入池资格。
        let query = MemoryQuery {
            memory_type: Some(MemoryType::KnowledgeNode),
            agent_id: agent_id.clone().filter(|s| !s.is_empty()),
            status: Some(MemoryStatus::Active),
            exclude_status: Some(MemoryStatus::Forgotten),
            limit: Some(500), // 上限保护，避免节点过多拖慢统计
            ..Default::default()
        };
        let nodes = self
            .memory_dao
            .query_knowledge_nodes(ctx.clone(), query)
            .await?;

        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        // 2. 批量查询这批节点的所有关系
        let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let relations = self.memory_dao.list_relations_batch(ctx, &node_ids).await?;

        // 3. 应用层统计每个节点的度数
        let mut degree_map: HashMap<String, (usize, usize)> = HashMap::new();
        for rel in &relations {
            // 出边：rel.source_node_id 指向 rel.target_node_id
            degree_map.entry(rel.source_node_id.clone()).or_default().1 += 1;
            // 入边：rel.target_node_id 被 rel.source_node_id 引用
            degree_map.entry(rel.target_node_id.clone()).or_default().0 += 1;
        }

        // 4. 组装推荐列表：连接度倒序；度数持平用 `is_published`（重要性/影响力）决胜
        //    —— 这是该标记在新语义下唯一的用途：不是门槛，只是「同等连接度时更值得当起点」。
        let mut recommendations: Vec<SeedNodeRecommendation> = nodes
            .into_iter()
            .map(|node| {
                let (incoming, outgoing) = degree_map.get(&node.id).copied().unwrap_or((0, 0));
                SeedNodeRecommendation {
                    degree: incoming + outgoing,
                    incoming_count: incoming,
                    outgoing_count: outgoing,
                    node,
                }
            })
            .collect();
        recommendations.sort_by_key(|r| {
            (
                std::cmp::Reverse(r.degree),
                std::cmp::Reverse(r.node.is_published),
            )
        });

        // 5. 截断到 limit
        recommendations.truncate(limit);
        Ok(recommendations)
    }

    pub(super) async fn get_knowledge_graph(
        &self,
        ctx: RequestContext,
        agent_id: Option<String>,
    ) -> Result<KnowledgeGraphData> {
        // 1. 全量拉取活跃知识节点（R1：仅知识节点；蜂巢语义与 recommend_seed_nodes 一致）
        let query = MemoryQuery {
            memory_type: Some(MemoryType::KnowledgeNode),
            agent_id: agent_id.clone().filter(|s| !s.is_empty()),
            status: Some(MemoryStatus::Active),
            exclude_status: Some(MemoryStatus::Forgotten),
            // 全量：limit/offset 均为 None 时 SQL 不带 LIMIT 子句
            limit: None,
            ..Default::default()
        };
        let nodes = self
            .memory_dao
            .query_knowledge_nodes(ctx.clone(), query)
            .await?;

        if nodes.is_empty() {
            return Ok(KnowledgeGraphData {
                nodes: Vec::new(),
                edges: Vec::new(),
                degrees: std::collections::HashMap::new(),
                edge_directions: std::collections::HashMap::new(),
            });
        }

        // 2. 全量拉取关系（list_relations_batch 的 SQL 仅返回 Active 边）
        let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let all_relations = self
            .memory_dao
            .list_relations_batch(ctx.clone(), &node_ids)
            .await?;

        // 3. R2：严格双端活跃——只保留两端节点都在活跃节点集内的边
        let active_ids: std::collections::HashSet<&str> =
            nodes.iter().map(|n| n.id.as_str()).collect();
        let edges: Vec<crate::models::memory::KnowledgeNodeRelationPo> = all_relations
            .into_iter()
            .filter(|rel| {
                active_ids.contains(rel.source_node_id.as_str())
                    && active_ids.contains(rel.target_node_id.as_str())
            })
            .collect();

        // 4. R4：度数只按生效边统计（in + out）
        let mut degrees: std::collections::HashMap<String, (usize, usize)> =
            std::collections::HashMap::new();
        for rel in &edges {
            // 出边：rel.source_node_id 指向 rel.target_node_id
            degrees.entry(rel.source_node_id.clone()).or_default().1 += 1;
            // 入边：rel.target_node_id 被 rel.source_node_id 引用
            degrees.entry(rel.target_node_id.clone()).or_default().0 += 1;
        }

        // 5. 三期方案 a′：按本体词表 resolve 逐边带出方向（服务端聚合，
        //    前端零词表映射）。词表三表 try_join 并行拉取，图聚合不串行化。
        //    纪律：DAL 之间禁止互引——此处直接走 OntologyDao 原始行装配词表，
        //    不调用 dal::ontology::load_lexicon（同构口径见 OntologyDalImpl）。
        let (classes, relation_types, synonyms) = tokio::try_join!(
            self.ontology_dao.list_all_classes(ctx.clone()),
            self.ontology_dao.list_all_relation_types(ctx.clone()),
            self.ontology_dao.list_all_synonyms(ctx)
        )?;
        let lexicon = common::ontology::OntologyLexicon {
            class_keys: classes
                .into_iter()
                .map(|po| common::ontology::normalize(&po.term_key))
                .collect(),
            relation_keys: relation_types
                .iter()
                .map(|po| common::ontology::normalize(&po.term_key))
                .collect(),
            relation_directions: relation_types
                .iter()
                .map(|po| {
                    (
                        common::ontology::normalize(&po.term_key),
                        common::ontology::Direction::parse_or_default(&po.direction),
                    )
                })
                .collect(),
            synonyms: synonyms
                .into_iter()
                .filter_map(|po| {
                    // target_kind 解析失败（脏数据）的映射不进词表
                    let kind = po.target_kind.parse().ok()?;
                    Some((
                        (common::ontology::normalize(&po.raw_term), kind),
                        po.target_key,
                    ))
                })
                .collect(),
        };
        let edge_directions: HashMap<String, String> = edges
            .iter()
            .map(|rel| {
                let direction = match common::ontology::resolve(
                    &lexicon,
                    common::ontology::TermKind::Relation,
                    &rel.relation_type,
                ) {
                    common::ontology::ResolvedTerm::Canonical { direction, .. }
                    | common::ontology::ResolvedTerm::ViaSynonym { direction, .. } => direction,
                    common::ontology::ResolvedTerm::Drift { .. } => {
                        common::ontology::Direction::Undirected
                    }
                };
                (rel.id.clone(), direction.as_str().to_string())
            })
            .collect();

        Ok(KnowledgeGraphData {
            nodes,
            edges,
            degrees,
            edge_directions,
        })
    }

    pub(super) async fn traverse_knowledge_graph(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        strategy: TraversalStrategy,
    ) -> Result<Vec<Memory>> {
        if seed_node_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut visited_nodes: HashSet<String> = HashSet::new();
        let mut visited_relations: HashSet<String> = HashSet::new();
        let mut result_relations: Vec<KnowledgeNodeRelationPo> = Vec::new();

        // 不变量一：种子是被点名的节点，先无条件放入（取回时不施加归属筛选）
        for id in seed_node_ids {
            visited_nodes.insert(id.clone());
        }

        if max_depth <= 0 {
            let nodes = self.fetch_nodes_by_ids(ctx.clone(), &visited_nodes).await?;
            return Ok(self.build_graph_memories(nodes, result_relations));
        }

        match strategy {
            TraversalStrategy::BreadthFirst => {
                self.traverse_bfs(
                    ctx.clone(),
                    seed_node_ids,
                    max_depth,
                    max_breadth,
                    TraverseState {
                        visited_nodes: &mut visited_nodes,
                        visited_relations: &mut visited_relations,
                        result_relations: &mut result_relations,
                    },
                )
                .await?;
            }
            TraversalStrategy::DepthFirst => {
                self.traverse_dfs(
                    ctx.clone(),
                    seed_node_ids,
                    max_depth,
                    max_breadth,
                    TraverseState {
                        visited_nodes: &mut visited_nodes,
                        visited_relations: &mut visited_relations,
                        result_relations: &mut result_relations,
                    },
                )
                .await?;
            }
        }

        let nodes = self.fetch_nodes_by_ids(ctx.clone(), &visited_nodes).await?;
        Ok(self.build_graph_memories(nodes, result_relations))
    }

    async fn traverse_bfs(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        state: TraverseState<'_>,
    ) -> Result<()> {
        let TraverseState {
            visited_nodes,
            visited_relations,
            result_relations,
        } = state;
        let mut queue: VecDeque<(String, i32)> = VecDeque::new();
        for id in seed_node_ids {
            queue.push_back((id.clone(), 0));
        }

        let mut current_depth = 0;
        while current_depth < max_depth && !queue.is_empty() {
            let mut current_level_nodes: Vec<String> = Vec::new();
            while let Some((node_id, depth)) = queue.front() {
                if *depth != current_depth {
                    break;
                }
                current_level_nodes.push(node_id.clone());
                queue.pop_front();
            }

            if current_level_nodes.is_empty() {
                break;
            }

            let all_relations = self
                .memory_dao
                .list_relations_batch(ctx.clone(), &current_level_nodes)
                .await?;

            let mut relations_by_node: HashMap<String, Vec<KnowledgeNodeRelationPo>> =
                HashMap::new();
            for rel in &all_relations {
                relations_by_node
                    .entry(rel.source_node_id.clone())
                    .or_default()
                    .push(rel.clone());
                relations_by_node
                    .entry(rel.target_node_id.clone())
                    .or_default()
                    .push(rel.clone());
            }

            for node_id in &current_level_nodes {
                let node_relations = relations_by_node.get(node_id).cloned().unwrap_or_default();
                let limited_relations = if max_breadth > 0 {
                    node_relations
                        .into_iter()
                        .take(max_breadth as usize)
                        .collect::<Vec<_>>()
                } else {
                    node_relations
                };

                for rel in limited_relations {
                    if !visited_relations.insert(rel.id.clone()) {
                        continue;
                    }
                    result_relations.push(rel.clone());

                    let neighbor_id = if rel.source_node_id == *node_id {
                        rel.target_node_id.clone()
                    } else {
                        rel.source_node_id.clone()
                    };

                    if visited_nodes.insert(neighbor_id.clone()) {
                        queue.push_back((neighbor_id, current_depth + 1));
                    }
                }
            }

            current_depth += 1;
        }

        Ok(())
    }

    async fn traverse_dfs(
        &self,
        ctx: RequestContext,
        seed_node_ids: &[String],
        max_depth: i32,
        max_breadth: i32,
        state: TraverseState<'_>,
    ) -> Result<()> {
        let TraverseState {
            visited_nodes,
            visited_relations,
            result_relations,
        } = state;
        let mut stack: Vec<(String, i32)> = Vec::new();
        for id in seed_node_ids.iter().rev() {
            stack.push((id.clone(), 0));
        }

        // 边预取缓存，避免逐节点查询的 N+1：
        // - fetched：已完整拉取过边的节点；batch 预取后所有 batch_ids 均有缓存条目（含空条目），
        //   故 fetched 命中即代表边已拉全，可跳过查询
        // - edge_cache：节点 → 其边（按双端点分组缓存）；注意仅作为边端点出现的节点也会获得
        //   部分边条目，这类节点不在 fetched 中，pop 时仍需预取其自身全部边
        let mut edge_cache: HashMap<String, Vec<KnowledgeNodeRelationPo>> = HashMap::new();
        let mut fetched: HashSet<String> = HashSet::new();

        while let Some((node_id, depth)) = stack.pop() {
            if depth >= max_depth {
                continue;
            }

            if !fetched.contains(&node_id) {
                // 未完整拉取过：当前节点 + 栈上未拉取的待展开节点一次批量预取
                let mut batch_ids: Vec<String> = vec![node_id.clone()];
                let mut seen: HashSet<&str> = HashSet::new();
                seen.insert(node_id.as_str());
                for (id, d) in &stack {
                    if *d < max_depth && !fetched.contains(id) && seen.insert(id.as_str()) {
                        batch_ids.push(id.clone());
                    }
                }
                let batch_relations = self
                    .memory_dao
                    .list_relations_batch(ctx.clone(), &batch_ids)
                    .await?;
                for id in &batch_ids {
                    fetched.insert(id.clone());
                }
                // 每条边挂到双端点名下的缓存；batch 内未被任何边引用的节点补空条目，
                // 保证 fetched 节点必有缓存条目，pop 时不会误判为「未拉取」重复查询
                for rel in batch_relations {
                    edge_cache
                        .entry(rel.source_node_id.clone())
                        .or_default()
                        .push(rel.clone());
                    edge_cache
                        .entry(rel.target_node_id.clone())
                        .or_default()
                        .push(rel);
                }
                for id in &batch_ids {
                    edge_cache.entry(id.clone()).or_default();
                }
            }

            // batch 内两节点之间的边会挂到双方名下，pop 时可能取到重复边：
            // 按 id 去重，避免重复边占用 take(max_breadth) 名额（旧实现单查无重复）
            let mut node_relations = edge_cache.remove(&node_id).unwrap_or_default();
            node_relations.sort_by_key(|rel| rel.created_at);
            node_relations.dedup_by(|a, b| a.id == b.id);

            let limited_relations: Vec<KnowledgeNodeRelationPo> = if max_breadth > 0 {
                node_relations
                    .into_iter()
                    .take(max_breadth as usize)
                    .collect()
            } else {
                node_relations
            };

            let mut neighbors: Vec<(String, KnowledgeNodeRelationPo)> = Vec::new();
            for rel in limited_relations {
                if !visited_relations.insert(rel.id.clone()) {
                    continue;
                }

                let neighbor_id = if rel.source_node_id == node_id {
                    rel.target_node_id.clone()
                } else {
                    rel.source_node_id.clone()
                };

                neighbors.push((neighbor_id, rel));
            }

            for (neighbor_id, rel) in neighbors.iter().rev() {
                result_relations.push(rel.clone());
                if visited_nodes.insert(neighbor_id.clone()) {
                    stack.push((neighbor_id.clone(), depth + 1));
                }
            }
        }

        Ok(())
    }

    /// 按 ID 批量取回知识节点（遍历用）。
    ///
    /// ⚠️ **不施加归属筛选** —— 这是「种子恒返回 + 蜂巢可见性」两条不变量的落点：
    /// - 种子是调用方点名的节点，无论归属哪个 Agent 都必须取回（否则点击展开缺中心节点）；
    /// - 邻居也是共享知识，不因归属被丢弃。
    ///
    /// 仍然受软删除约束：DAO 默认排除 `status = Forgotten`（遗忘的节点不该再出现在图上）。
    async fn fetch_nodes_by_ids(
        &self,
        ctx: RequestContext,
        node_ids: &HashSet<String>,
    ) -> Result<Vec<LongTermKnowledgeNodePo>> {
        if node_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = node_ids.iter().cloned().collect();
        // 分块：SQLite 绑定参数上限 999，遍历的 visited 集合可能很大
        let mut nodes: Vec<LongTermKnowledgeNodePo> = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(crate::service::dao::memory::sqlite::IN_CLAUSE_CHUNK) {
            let query = MemoryQuery {
                ids: Some(chunk.to_vec()),
                ..Default::default()
            };
            nodes.extend(
                self.memory_dao
                    .query_knowledge_nodes(ctx.clone(), query)
                    .await?,
            );
        }
        Ok(nodes)
    }

    fn build_memories(
        &self,
        nodes: Vec<LongTermKnowledgeNodePo>,
        relations: Vec<KnowledgeNodeRelationPo>,
    ) -> Vec<Memory> {
        let mut memories = Vec::with_capacity(nodes.len() + relations.len());
        for node in nodes {
            memories.push(Memory {
                po: MemoryPo::KnowledgeNode(node),
                search_match: None,
            });
        }
        for rel in relations {
            memories.push(Memory {
                po: MemoryPo::Relation(rel),
                search_match: None,
            });
        }
        memories
    }

    /// 组装图谱批次（遍历路径），保证**边随其两端节点一起返回**。
    ///
    /// 走的是与 `search` 路径同一道闸 `drop_dangling_relations`：端点没通过可见性过滤
    /// （或没被遍历到）时整条边丢弃，绝不让半条边进入结果。
    fn build_graph_memories(
        &self,
        nodes: Vec<LongTermKnowledgeNodePo>,
        relations: Vec<KnowledgeNodeRelationPo>,
    ) -> Vec<Memory> {
        let mut memories = self.build_memories(nodes, relations);
        drop_dangling_relations(&mut memories);
        memories
    }
}
