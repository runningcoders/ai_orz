//! 技能混合搜索：向量语义召回 + FTS5 关键词召回 → 去重 → 三态标注 → 聚合排序。
//!
//! 两条召回路的宽度**必须同宽**：向量路读 `SkillSearch.top_k`，FTS 路读
//! `filters.pagination.limit`。调用方声明「宽召回 6 条」时两个字段都要给 6，
//! 否则向量路按缺省 20 条塞进来，聚合后把关键词命中挤到分页之外。

use super::SkillDalImpl;
use crate::models::skill::{Skill, SkillPo};
use crate::models::vector::{MatchType, SearchMatchInfo, VectorSearchHit};
use crate::pkg::request_context::RequestContext;
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use common::error::Result;

/// 向量路默认 / 最大召回宽度：与 DAO `search` 的 `LIMIT 20` 上限对齐。
const SKILL_VECTOR_TOP_K_LIMIT: i32 = 20;

/// 向量距离默认阈值（余弦距离 0~2，越小越相似）。
const DEFAULT_SKILL_VECTOR_DISTANCE_THRESHOLD: f32 = 0.8;

/// 向量路召回宽度，与 `MemorySearch.top_k` 同口径：
/// `SkillSearch.top_k` 只管向量路，FTS 路由 `filters.pagination.limit` 管。
///
/// ⚠️ 此前 `top_k` 字段在 DAL 里根本没被读取，向量路恒按 20 条召回；调用方
/// 即便声明「宽召回 6 条」，也会拿到 20 条向量命中，聚合排序后把 6 条 FTS
/// 命中挤到分页之外。缺省仍回落 20，保证未声明 `top_k` 的既有调用方行为不变；
/// 夹紧到 1~20 是因为向量 Store 不接受非正数，且 DAO 侧上限就是 20。
pub(crate) fn vector_search_top_k(search: &SkillSearch) -> i32 {
    search
        .top_k
        .unwrap_or(SKILL_VECTOR_TOP_K_LIMIT)
        .clamp(1, SKILL_VECTOR_TOP_K_LIMIT)
}

impl SkillDalImpl {
    pub(super) async fn search(
        &self,
        ctx: RequestContext,
        search: SkillSearch,
    ) -> Result<common::api::PagedResult<Skill>> {
        // 提前捕获 pagination，避免 search 被 move 后无法访问
        let pagination = search.filters.pagination.clone();

        // Step 1~2: 向量语义召回（增强路）。任一步失败由 [`Self::vector_hits`]
        // 内部 warn 降级为空集，FTS5 关键词路照常返回 —— 向量不是闸门。
        // 只保留距离小于阈值的结果（余弦距离 0~2，0 是完全相同）。
        let vector_distance_threshold = search
            .vector_distance_threshold
            .unwrap_or(DEFAULT_SKILL_VECTOR_DISTANCE_THRESHOLD);
        let filtered_hits: Vec<(String, f32)> = self
            .vector_hits(&ctx, &search)
            .await
            .into_iter()
            .filter(|hit| hit.distance < vector_distance_threshold)
            .map(|hit| (hit.row.id, hit.distance))
            .collect();
        let vector_skill_ids: std::collections::HashSet<String> =
            filtered_hits.iter().map(|(id, _)| id.clone()).collect();
        let vector_scores: std::collections::HashMap<String, f32> =
            filtered_hits.into_iter().collect();

        // Step 3: 执行 FTS5 关键词搜索（DAO 返回 Vec<(Po, fts_rank)>）
        let keyword_results = self.skill_dao.search(ctx.clone(), search.clone()).await?;

        // 提取 fts_rank 并转换为 Vec<Po> 便于聚合
        let mut fts_ranks: std::collections::HashMap<String, f32> =
            std::collections::HashMap::new();
        let keyword_pos: Vec<SkillPo> = keyword_results
            .into_iter()
            .map(|(po, rank)| {
                if let Some(r) = rank {
                    fts_ranks.insert(po.id.clone(), r);
                }
                po
            })
            .collect();

        // Step 4: 聚合结果（如果有向量结果，用通用 query 批量获取，避免 N+1）
        let mut all_pos = keyword_pos.clone();

        if !vector_skill_ids.is_empty() {
            let ids_to_fetch: Vec<String> = vector_skill_ids
                .into_iter()
                .filter(|id| !keyword_pos.iter().any(|po| po.id == *id))
                .collect();

            if !ids_to_fetch.is_empty() {
                // 分批获取（避免 SQL 太长）
                let mut ids_to_fetch = ids_to_fetch;
                ids_to_fetch.sort();
                ids_to_fetch.dedup();

                for chunk in ids_to_fetch.chunks(20) {
                    let chunk_ids: Vec<String> = chunk.to_vec();
                    // ⚠️ 向量独有命中必须沿用调用方的业务过滤条件（status / exclude_status /
                    // author_id / author_type / category / tags / parent_skill_id 等）。
                    // 此前用 ..Default::default() 会把条件全部丢弃，导致已过期技能
                    // （exclude_status = Expired）仍以 MatchType::Vector 返回。
                    // pagination 置空：此处按 id 精确回填，不能被调用方 limit 截断。
                    let chunk_query = SkillQuery {
                        ids: Some(chunk_ids),
                        pagination: Default::default(),
                        ..search.filters.clone()
                    };
                    let chunk_pos = self.skill_dao.query(ctx.clone(), chunk_query).await?;
                    all_pos.extend(chunk_pos.items);
                }
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 构建 Skill 对象，附加 SearchMatchInfo（三态匹配）
        let mut skills = Vec::with_capacity(all_pos.len());
        for po in all_pos {
            let files = self.skill_dao.list_files(&po)?;
            let has_vector = vector_scores.contains_key(&po.id);
            let has_keyword = fts_ranks.contains_key(&po.id);
            let match_info = if has_vector && has_keyword {
                // 双命中：向量 + 关键词
                Some(SearchMatchInfo {
                    match_type: MatchType::Hybrid,
                    vector_distance: vector_scores.get(&po.id).copied(),
                    fts_rank: fts_ranks.get(&po.id).copied(),
                    ..Default::default()
                })
            } else if has_vector {
                // 仅向量命中
                Some(SearchMatchInfo {
                    match_type: MatchType::Vector,
                    vector_distance: vector_scores.get(&po.id).copied(),
                    ..Default::default()
                })
            } else if has_keyword {
                // 仅关键词命中
                Some(SearchMatchInfo {
                    match_type: MatchType::Keyword,
                    fts_rank: fts_ranks.get(&po.id).copied(),
                    ..Default::default()
                })
            } else {
                None
            };
            skills.push(Skill {
                po,
                files,
                search_match: match_info,
            });
        }

        // Step 7: 综合排序（Hybrid 优先 → Vector 次之 → Keyword/None 最后）
        //    组内排序：Hybrid/Vector 按向量距离升序，Keyword 按 fts_rank 升序（BM25 越小越相关）
        skills.sort_by(|a, b| {
            let a_type = a.search_match.as_ref().map(|m| m.match_type);
            let b_type = b.search_match.as_ref().map(|m| m.match_type);
            let order_a = match a_type {
                Some(MatchType::Hybrid) => 0,
                Some(MatchType::Vector) => 1,
                _ => 2,
            };
            let order_b = match b_type {
                Some(MatchType::Hybrid) => 0,
                Some(MatchType::Vector) => 1,
                _ => 2,
            };
            order_a.cmp(&order_b).then_with(|| match (a_type, b_type) {
                (Some(MatchType::Hybrid), Some(MatchType::Hybrid))
                | (Some(MatchType::Vector), Some(MatchType::Vector)) => {
                    let a_dist = a
                        .search_match
                        .as_ref()
                        .and_then(|m| m.vector_distance)
                        .unwrap_or(f32::MAX);
                    let b_dist = b
                        .search_match
                        .as_ref()
                        .and_then(|m| m.vector_distance)
                        .unwrap_or(f32::MAX);
                    a_dist
                        .partial_cmp(&b_dist)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
                _ => {
                    let a_rank = a
                        .search_match
                        .as_ref()
                        .and_then(|m| m.fts_rank)
                        .unwrap_or(f32::MAX);
                    let b_rank = b
                        .search_match
                        .as_ref()
                        .and_then(|m| m.fts_rank)
                        .unwrap_or(f32::MAX);
                    a_rank
                        .partial_cmp(&b_rank)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
            })
        });

        // Step 8: 应用聚合上限（与 DAO LIMIT 20 对齐，避免内存聚合后结果过多）
        skills.truncate(SKILL_VECTOR_TOP_K_LIMIT as usize);

        // Step 9: 内存分页（DAO 已按 LIMIT 20 截断，这里基于聚合后的全量做 offset/limit 分页）
        let total = skills.len();
        let offset = pagination.offset.unwrap_or(0);
        let limit = pagination.limit.unwrap_or(20);
        let items = skills.into_iter().skip(offset).take(limit).collect();

        Ok(common::api::PagedResult { items, total })
    }

    /// 向量语义召回（增强路）：任何一步失败都只 `warn` 并返回空集，由外层
    /// 继续跑 FTS5 关键词搜索。
    ///
    /// ⚠️ 此前「生成查询向量」用 `?`，会把整个 `search` 打成 `Err`，与
    /// C-知识「向量搜索失败时自动降级为关键词搜索，不影响可用性」的口径不符：
    /// 生产 11-10 出现过一次 embedding 500，技能召回会整路退回 1 条 tag 技能，
    /// 小脑精选又变成恒定空转。provider 读取 / 向量生成 / 向量检索三步
    /// 全部按同一降级口径处理 —— 向量是增强，不是闸门。
    async fn vector_hits(
        &self,
        ctx: &RequestContext,
        search: &SkillSearch,
    ) -> Vec<VectorSearchHit> {
        let Some(keyword) = search.keyword.as_deref() else {
            return Vec::new();
        };
        let provider = match self
            .model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await
        {
            Ok(Some(provider)) => provider,
            Ok(None) => return Vec::new(),
            Err(e) => {
                log_warn!(
                    ctx,
                    "vector_search",
                    "读取 Embedding Provider 失败，降级到关键词搜索: {}",
                    e
                );
                return Vec::new();
            }
        };
        let query_vector = match self
            .cortex_dao
            .embed_text_for_search(ctx.clone(), &provider, keyword)
            .await
        {
            Ok(params) => params.vector,
            Err(e) => {
                log_warn!(
                    ctx,
                    "vector_search",
                    "查询向量生成失败，降级到关键词搜索: {}",
                    e
                );
                return Vec::new();
            }
        };
        // pre-filter：category 在 DAO 内转译为向量谓词下推；其余过滤条件
        // 由 Step 4 的「向量独有命中回填」沿用调用方 filters 兜底。
        let top_k = vector_search_top_k(search);
        match self
            .skill_vector_dao
            .search_vector(ctx.clone(), &query_vector, top_k, &search.filters)
            .await
        {
            Ok(hits) => hits,
            Err(e) => {
                log_warn!(
                    ctx,
                    "vector_search",
                    "技能向量搜索失败，降级到关键词搜索: {}",
                    e
                );
                Vec::new()
            }
        }
    }
}
