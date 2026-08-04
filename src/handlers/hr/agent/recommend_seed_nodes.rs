//! Handler: 推荐知识图谱起点节点
//!
//! 按节点关联度数（入边 + 出边总数）倒序返回 Top N 知识节点。
//! 用于知识图谱页面"推荐起点"功能，帮助用户快速定位核心节点。
//!
//! 语义上属于 memory domain（知识图谱能力），agent_id 只是过滤条件之一。
//! 文件位置与 query_memory/search_memory 等 memory handler 一致，放在 agent/ 下。

use crate::models::memory::SeedNodeRecommendation;
use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::generate_http_handler;
use common::api::{
    RecommendSeedNodesParams, RecommendSeedNodesResponse,
    SeedNodeRecommendation as ApiSeedNodeRecommendation,
};
use common::error::Result;

/// 推荐知识图谱起点节点（按关联度数 Top N）
#[generate_http_handler]
pub async fn recommend_seed_nodes(
    ctx: RequestContext,
    params: RecommendSeedNodesParams,
) -> Result<RecommendSeedNodesResponse> {
    let limit = params.limit.unwrap_or(5).min(50);
    let recommendations = runtime_domain()
        .memory()
        .recommend_seed_nodes(ctx, params.agent_id, limit)
        .await?;

    let results = recommendations.into_iter().map(to_api).collect();
    Ok(RecommendSeedNodesResponse {
        recommendations: results,
    })
}

/// 解析 tags JSON 数组字符串为 Vec<String>，解析失败返回空 Vec
fn parse_tags_json(tags_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(tags_json).unwrap_or_default()
}

/// domain 层 SeedNodeRecommendation → API DTO
fn to_api(rec: SeedNodeRecommendation) -> ApiSeedNodeRecommendation {
    ApiSeedNodeRecommendation {
        node_id: rec.node.id.clone(),
        node_name: rec.node.node_name,
        node_description: rec.node.node_description,
        node_type: rec.node.node_type,
        summary: rec.node.summary,
        tags: parse_tags_json(&rec.node.tags),
        degree: rec.degree,
        incoming_count: rec.incoming_count,
        outgoing_count: rec.outgoing_count,
    }
}
