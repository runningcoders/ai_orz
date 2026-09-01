//! Agent 关联全景装配（跨领域编排）
//!
//! ## 职责边界
//!
//! 后端只负责产出「去重后的实体 ID 全集 / 实体列表」，分组（按 installed pack tag）
//! 交给前端。本模块把 Agent 的工具/技能实体查询 + 打包（DTO）交给专业领域方法：
//!
//! | 环节 | 归属 | 说明 |
//! |------|------|------|
//! | 工具实体查询 | finance domain | `query_tools`（按 ID 批量，一次查完） |
//! | 运行时就绪探测 | runtime domain | `probe_runtime_ready`（带 TTL 缓存） |
//! | 工具打包 | `finance::tool::response` | `to_list_item`（含 `runtime_ready`） |
//! | 技能实体查询 + 打包 | hr `skill_manage` + `hr::skill::response` | `to_list_item` |
//!
//! 这样做的好处：domain 层不重复实现 DTO 转换，专业领域的全部逻辑
//! （尤其是 `runtime_ready` 就绪判定）被完整复用，不会退化成硬编码 `Unknown`。

use crate::handlers::finance::tool::response::{
    probe_runtime_ready, to_list_item as tool_to_list_item,
};
use crate::handlers::hr::skill::response::to_list_item as skill_to_list_item;
use crate::pkg::RequestContext;
use crate::service::dao::tool::ToolQuery;
use crate::service::domain::finance::domain as finance_domain;
use crate::service::domain::hr::domain as hr_domain;
use common::api::{SkillListItem, ToolListItem};
use common::error::Result;

/// 按 ID 装配工具列表（扁平；调用方已保证 ID 唯一，此处按 id 排序稳定输出）
pub(crate) async fn build_flat_tools(
    ctx: RequestContext,
    tool_ids: Vec<String>,
) -> Result<Vec<ToolListItem>> {
    if tool_ids.is_empty() {
        return Ok(Vec::new());
    }

    let page = finance_domain()
        .tool_provider_manage()
        .query_tools(
            ctx.clone(),
            ToolQuery {
                ids: Some(tool_ids),
                ..Default::default()
            },
        )
        .await?;

    let ready = probe_runtime_ready(&ctx, &page.items).await;

    let mut items: Vec<ToolListItem> = page
        .items
        .iter()
        .map(|t| {
            let runtime_ready = ready.get(&t.po.id).cloned().unwrap_or_default();
            tool_to_list_item(t, runtime_ready)
        })
        .collect();
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(items)
}

/// 装配 Agent 自身目录下的技能列表（扁平；按 id 排序稳定输出）
pub(crate) async fn build_flat_skills(
    ctx: RequestContext,
    agent_id: &str,
) -> Result<Vec<SkillListItem>> {
    let skills = hr_domain()
        .skill_manage()
        .list_for_agent(ctx.clone(), agent_id)
        .await?;

    let mut items: Vec<SkillListItem> = skills.iter().map(skill_to_list_item).collect();
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(items)
}
