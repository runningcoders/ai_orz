//! 附件混合搜索：向量语义召回 + FTS5 关键词召回 → 去重 → 三态标注 → 聚合排序。
//!
//! 向量路是增强路：provider 缺失 / embed 失败 / 向量检索失败一律 warn 降级为空集，
//! FTS5 关键词路照常返回（与 skill/memory 搜索同一口径）。

use super::AttachmentDalImpl;
use crate::models::attachment::Attachment;
use crate::models::vector::{MatchType, SearchMatchInfo, VectorSearchHit};
use crate::pkg::RequestContext;
use crate::service::dao::attachment::{AttachmentQuery, AttachmentSearch};
use common::error::Result;
use std::collections::{HashMap, HashSet};

/// 向量路默认/最大召回宽度（与 DAO search 的 LIMIT 对齐）。
const VECTOR_TOP_K_LIMIT: i32 = 20;
/// 余弦距离阈值（0~2，越小越相似）。
const DEFAULT_VECTOR_DISTANCE_THRESHOLD: f32 = 0.8;

impl AttachmentDalImpl {
    /// 混合搜索主入口（DAL trait 方法）。
    pub(super) async fn search_impl(
        &self,
        ctx: RequestContext,
        search: AttachmentSearch,
    ) -> Result<common::api::PagedResult<Attachment>> {
        let pagination = search.pagination.clone();
        let limit = pagination.limit.unwrap_or(20).clamp(1, 100);

        // 归属过滤由调用方强制注入，这里再兜一道：空 root_user_id 直接返回空，
        // 绝不执行无归属范围的搜索（越权红线，纵深防御）。
        if search
            .root_user_id
            .as_deref()
            .unwrap_or_default()
            .is_empty()
        {
            return Ok(common::api::PagedResult {
                items: Vec::new(),
                total: 0,
            });
        }

        // Step 1~2: 向量语义召回（增强路，失败降级为空）
        let threshold = DEFAULT_VECTOR_DISTANCE_THRESHOLD;
        let filtered_hits: Vec<(String, f32)> = self
            .vector_hits(&ctx, &search)
            .await
            .into_iter()
            .filter(|hit| hit.distance < threshold)
            .map(|hit| (hit.row.id, hit.distance))
            .collect();
        let vector_ids: HashSet<String> = filtered_hits.iter().map(|(id, _)| id.clone()).collect();
        let vector_scores: HashMap<String, f32> = filtered_hits.into_iter().collect();

        // Step 3: FTS5 关键词召回（DAO 返回 Vec<(Po, fts_rank)>）。
        // ⚠️ DAO 把 pagination.limit 当作 SQL 召回宽度（见 attachment/sqlite.rs），
        // 直接透传会让「limit=1 的第二页」只召回 1 条、total 失真、后续页恒空。
        // 这里把召回宽度放大到覆盖 offset+limit（并与向量路 top-K 对齐），
        // 真正的分页留到 Step 8 在完整召回集上做——召回宽度与返回分页彻底解耦。
        let recall_limit = pagination
            .offset
            .unwrap_or(0)
            .saturating_add(limit)
            .max(VECTOR_TOP_K_LIMIT as usize)
            .min(100);
        let mut recall_search = search.clone();
        recall_search.pagination.limit = Some(recall_limit);
        recall_search.pagination.offset = None; // 召回不偏移，偏移在 Step 8 内存切片
        let keyword_results = self
            .attachment_dao
            .search(ctx.clone(), recall_search)
            .await?;
        let mut fts_ranks: HashMap<String, f32> = HashMap::new();
        let mut keyword_ids: HashSet<String> = HashSet::new();
        let keyword_pos: Vec<crate::models::attachment::AttachmentPo> = keyword_results
            .into_iter()
            .map(|(po, rank)| {
                keyword_ids.insert(po.id.clone()); // MATCH 与 LIKE 命中都算关键词命中
                if let Some(r) = rank {
                    fts_ranks.insert(po.id.clone(), r);
                }
                po
            })
            .collect();

        // Step 4: 向量独有命中批量回填（沿用业务过滤，分页置空避免被截断）
        let mut all_pos = keyword_pos.clone();
        if !vector_ids.is_empty() {
            let mut ids_to_fetch: Vec<String> = vector_ids
                .into_iter()
                .filter(|id| !keyword_pos.iter().any(|po| po.id == *id))
                .collect();
            ids_to_fetch.sort();
            ids_to_fetch.dedup();
            for chunk in ids_to_fetch.chunks(20) {
                let chunk_query = AttachmentQuery {
                    ids: Some(chunk.to_vec()),
                    pagination: Default::default(),
                    root_user_id: search.root_user_id.clone(),
                    purpose: search.purpose.clone(),
                    file_type: search.file_type,
                };
                let chunk_page = self.attachment_dao.query(ctx.clone(), chunk_query).await?;
                all_pos.extend(chunk_page.items);
            }
        }

        // Step 5: 去重
        all_pos.sort_by(|a, b| a.id.cmp(&b.id));
        all_pos.dedup_by(|a, b| a.id == b.id);

        // Step 6: 三态匹配标注
        let mut attachments = Vec::with_capacity(all_pos.len());
        for po in all_pos {
            let has_vector = vector_scores.contains_key(&po.id);
            // ⚠️ 用 keyword_ids 而非 fts_ranks：LIKE 兜底路（短词 <3 字符）rank=NULL，
            // 不进 fts_ranks，但它仍是关键词命中，必须标注为 Keyword 并参与关键词排序。
            let has_keyword = keyword_ids.contains(&po.id);
            let match_info = if has_vector && has_keyword {
                Some(SearchMatchInfo {
                    match_type: MatchType::Hybrid,
                    vector_distance: vector_scores.get(&po.id).copied(),
                    fts_rank: fts_ranks.get(&po.id).copied(),
                    ..Default::default()
                })
            } else if has_vector {
                Some(SearchMatchInfo {
                    match_type: MatchType::Vector,
                    vector_distance: vector_scores.get(&po.id).copied(),
                    ..Default::default()
                })
            } else if has_keyword {
                Some(SearchMatchInfo {
                    match_type: MatchType::Keyword,
                    fts_rank: fts_ranks.get(&po.id).copied(),
                    ..Default::default()
                })
            } else {
                None
            };
            let mut attachment = Attachment::from_po(po);
            attachment.search_match = match_info;
            attachments.push(attachment);
        }

        // Step 7: 综合排序（Hybrid > Vector > Keyword/None；组内按距离/排名）
        attachments.sort_by(|a, b| {
            let at = a.search_match.as_ref().map(|m| m.match_type);
            let bt = b.search_match.as_ref().map(|m| m.match_type);
            let order = |t: Option<MatchType>| match t {
                Some(MatchType::Hybrid) => 0,
                Some(MatchType::Vector) => 1,
                _ => 2,
            };
            order(at).cmp(&order(bt)).then_with(|| match (at, bt) {
                (Some(MatchType::Hybrid), Some(MatchType::Hybrid))
                | (Some(MatchType::Vector), Some(MatchType::Vector)) => {
                    let ad = a
                        .search_match
                        .as_ref()
                        .and_then(|m| m.vector_distance)
                        .unwrap_or(f32::MAX);
                    let bd = b
                        .search_match
                        .as_ref()
                        .and_then(|m| m.vector_distance)
                        .unwrap_or(f32::MAX);
                    ad.partial_cmp(&bd).unwrap_or(std::cmp::Ordering::Equal)
                }
                _ => {
                    let ar = a
                        .search_match
                        .as_ref()
                        .and_then(|m| m.fts_rank)
                        .unwrap_or(f32::MAX);
                    let br = b
                        .search_match
                        .as_ref()
                        .and_then(|m| m.fts_rank)
                        .unwrap_or(f32::MAX);
                    ar.partial_cmp(&br).unwrap_or(std::cmp::Ordering::Equal)
                }
            })
        });

        // Step 8: 内存分页。total 取去重后的完整命中数，不是截断后的长度——
        // 否则 offset ≥ 召回上限时第二页起恒空、且 total 永远 ≤ 召回上限（分页语义错误）。
        // 召回宽度已由各路 LIMIT 控制，这里不再二次 truncate。
        let total = attachments.len();
        let offset = pagination.offset.unwrap_or(0);
        let items = attachments.into_iter().skip(offset).take(limit).collect();
        Ok(common::api::PagedResult { items, total })
    }

    /// 向量语义召回（增强路）：任一步失败只 warn 返回空集。
    async fn vector_hits(
        &self,
        ctx: &RequestContext,
        search: &AttachmentSearch,
    ) -> Vec<VectorSearchHit> {
        let Some(keyword) = search.keyword.as_deref() else {
            return Vec::new();
        };
        let (Some(vector_dao), Some(cortex_dao), Some(model_provider_dao)) = (
            &self.attachment_vector_dao,
            &self.cortex_dao,
            &self.model_provider_dao,
        ) else {
            return Vec::new();
        };
        let provider = match model_provider_dao
            .get_default_embedding_provider(ctx.clone())
            .await
        {
            Ok(Some(provider)) => provider,
            Ok(None) => return Vec::new(),
            Err(e) => {
                log_warn!(ctx, "vector_search", error = ?e, "读取 Embedding Provider 失败，附件搜索降级到关键词");
                return Vec::new();
            }
        };
        let query_vector = match cortex_dao
            .embed_text_for_search(ctx.clone(), &provider, keyword)
            .await
        {
            Ok(params) => params.vector,
            Err(e) => {
                log_warn!(ctx, "vector_search", error = ?e, "附件查询向量生成失败，降级到关键词");
                return Vec::new();
            }
        };
        // 归属过滤作为向量谓词下推，防越权召回。
        let filters = AttachmentQuery {
            root_user_id: search.root_user_id.clone(),
            purpose: search.purpose.clone(),
            file_type: search.file_type,
            ..Default::default()
        };
        match vector_dao
            .search_vector(ctx.clone(), &query_vector, VECTOR_TOP_K_LIMIT, &filters)
            .await
        {
            Ok(hits) => hits,
            Err(e) => {
                log_warn!(ctx, "vector_search", error = ?e, "附件向量搜索失败，降级到关键词");
                Vec::new()
            }
        }
    }
}
