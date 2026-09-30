//! Handler: 获取知识图谱全量数据（全局点线视图）
//!
//! 一次返回全部活跃知识节点 + 双端活跃关系边 + 节点度数，
//! 供知识图谱页面默认装载全局点线视图（无任何筛选条件的默认视图）。
//!
//! 语义上属于 memory domain（知识图谱能力），agent_id 只是归属筛选条件之一，
//! 蜂巢共享语义与 recommend_seed_nodes / search_memory 一致。
//! 文件位置与 query_memory/search_memory 等 memory handler 一致，放在 agent/ 下。

use crate::models::memory::{KnowledgeGraphData, KnowledgeNodeRelationPo, LongTermKnowledgeNodePo};
use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain as runtime_domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{GetKnowledgeGraphParams, GetKnowledgeGraphResponse, GraphEdge, GraphNode};
use common::error::Result;

/// 获取知识图谱全量数据（全局点线视图）
#[register_handler_tool(
    id = "get_knowledge_graph",
    name = "Get Knowledge Graph",
    description = "Get the full knowledge-graph dataset for the global point-line view: all active knowledge nodes (R1: knowledge nodes + relation edges only), relation edges whose both endpoints are active (R2), and per-node degree counted from active edges only (R4). Fields are trimmed for the global view (no descriptions/summaries - node details go through the existing card-page channel). agent_id optionally filters by owner; omit it for the whole hive. Load this once for the default global view; use recommend_seed_nodes for cold-start entry points and search_memory for focused exploration.",
    params = "common::api::GetKnowledgeGraphParams",
    neural,
    tags = "memory"
)]
#[generate_http_handler]
pub async fn get_knowledge_graph(
    ctx: RequestContext,
    params: GetKnowledgeGraphParams,
) -> Result<GetKnowledgeGraphResponse> {
    let KnowledgeGraphData {
        nodes,
        edges,
        degrees,
    } = runtime_domain()
        .memory()
        .get_knowledge_graph(ctx, params.agent_id)
        .await?;

    let nodes = nodes
        .into_iter()
        .map(|node| {
            let (incoming_count, outgoing_count) = degrees.get(&node.id).copied().unwrap_or((0, 0));
            to_node_api(node, incoming_count, outgoing_count)
        })
        .collect();
    let edges = edges.into_iter().map(to_edge_api).collect();

    Ok(GetKnowledgeGraphResponse {
        nodes,
        edges,
        generated_at: chrono::Utc::now().timestamp_millis(),
    })
}

/// 解析 tags JSON 数组字符串为 Vec<String>，解析失败返回空 Vec
fn parse_tags_json(tags_json: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(tags_json).unwrap_or_default()
}

/// domain 层知识节点 PO → API 节点 DTO（字段裁剪口径：全局视图裁 node_description/summary/updated_at）
///
/// 裁剪依据：全局视角关注模块/区域/实体间关系而非具体细节——
/// 全局点线视图只需要「点（名称/类型/标签/归属/重要性）+ 度数 + 线（关系）」，
/// 细节字段在点击节点进入卡片页时经既有通道按需加载（字段裁剪清单见实施报告）。
fn to_node_api(
    node: LongTermKnowledgeNodePo,
    incoming_count: usize,
    outgoing_count: usize,
) -> GraphNode {
    GraphNode {
        id: node.id,
        node_name: node.node_name,
        node_type: node.node_type,
        tags: parse_tags_json(&node.tags),
        agent_id: node.agent_id,
        is_published: node.is_published,
        degree: incoming_count + outgoing_count,
        incoming_count,
        outgoing_count,
        created_at: node.created_at,
    }
}

/// domain 层关系 PO → API 边 DTO
fn to_edge_api(rel: KnowledgeNodeRelationPo) -> GraphEdge {
    GraphEdge {
        id: rel.id,
        source: rel.source_node_id,
        target: rel.target_node_id,
        relation_type: rel.relation_type,
        weight: rel.weight,
    }
}
