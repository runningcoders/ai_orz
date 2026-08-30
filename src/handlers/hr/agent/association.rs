//! Agent 关联全景装配（跨领域编排）
//!
//! ## 职责边界
//!
//! Hr domain 的 `get_agent_association_groups` 只产出「谁属于哪一组」的 **ID 分组**
//! （neural → bound → pack / neural → pack → standalone 的业务规则归它）。
//! 本模块负责把 ID 换成实体、并调用**专业领域的打包方法**转成 DTO：
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

use std::collections::HashMap;

use crate::handlers::finance::tool::response::{
    probe_runtime_ready, to_list_item as tool_to_list_item,
};
use crate::handlers::hr::skill::response::to_list_item as skill_to_list_item;
use crate::pkg::RequestContext;
use crate::service::dao::tool::ToolQuery;
use crate::service::domain::finance::domain as finance_domain;
use crate::service::domain::hr::{self, domain as hr_domain};
use common::api::{
    AgentSkillPackGroup, AgentSkillsOverview, AgentToolPackGroup, AgentToolsOverview,
    SkillListItem, ToolListItem,
};
use common::error::Result;

/// 按 ID 分组装配工具全景（含 runtime_ready 就绪探测）
pub(crate) async fn build_tools_overview(
    ctx: RequestContext,
    groups: hr::AgentToolGroups,
) -> Result<AgentToolsOverview> {
    // 1. 汇总全部 ID（去重），一次性批量查询，避免按组多次往返
    let mut all_ids: Vec<String> = Vec::with_capacity(
        groups.neural_ids.len() + groups.bound_ids.len() + groups.pack_groups.len(),
    );
    all_ids.extend(groups.neural_ids.iter().cloned());
    all_ids.extend(groups.bound_ids.iter().cloned());
    for g in &groups.pack_groups {
        all_ids.extend(g.tool_ids.iter().cloned());
    }
    all_ids.sort();
    all_ids.dedup();

    // 空分组直接返回，避免无谓查询
    if all_ids.is_empty() {
        return Ok(AgentToolsOverview {
            neural_tools: Vec::new(),
            bound_tools: Vec::new(),
            pack_groups: groups
                .pack_groups
                .iter()
                .map(|g| AgentToolPackGroup {
                    tag: g.tag.clone(),
                    tools: Vec::new(),
                })
                .collect(),
        });
    }

    // 2. 专业领域：工具实体查询（finance domain）
    let page = finance_domain()
        .tool_provider_manage()
        .query_tools(
            ctx.clone(),
            ToolQuery {
                ids: Some(all_ids),
                // 不传 limit → DAO 侧 LIMIT -1，返回全部命中项
                ..Default::default()
            },
        )
        .await?;

    // 3. 专业领域：运行时就绪探测（runtime domain）
    let ready = probe_runtime_ready(&ctx, &page.items).await;

    // 4. 专业领域：打包成 DTO
    let by_id: HashMap<String, ToolListItem> = page
        .items
        .iter()
        .map(|t| {
            let runtime_ready = ready.get(&t.po.id).cloned().unwrap_or_default();
            (t.po.id.clone(), tool_to_list_item(t, runtime_ready))
        })
        .collect();

    let pick = |ids: &[String]| -> Vec<ToolListItem> {
        ids.iter().filter_map(|id| by_id.get(id).cloned()).collect()
    };

    Ok(AgentToolsOverview {
        neural_tools: pick(&groups.neural_ids),
        bound_tools: pick(&groups.bound_ids),
        pack_groups: groups
            .pack_groups
            .iter()
            .map(|g| AgentToolPackGroup {
                tag: g.tag.clone(),
                tools: pick(&g.tool_ids),
            })
            .collect(),
    })
}

/// 按 ID 分组装配技能全景
pub(crate) async fn build_skills_overview(
    ctx: RequestContext,
    agent_id: &str,
    groups: hr::AgentSkillGroups,
) -> Result<AgentSkillsOverview> {
    // 1. 专业领域：Agent 自身目录下的技能副本（hr skill_manage）
    let skills = hr_domain()
        .skill_manage()
        .list_for_agent(ctx.clone(), agent_id)
        .await?;

    // 2. 专业领域：打包成 DTO
    let by_id: HashMap<String, SkillListItem> = skills
        .iter()
        .map(|s| (s.po.id.clone(), skill_to_list_item(s)))
        .collect();

    let pick = |ids: &[String]| -> Vec<SkillListItem> {
        ids.iter().filter_map(|id| by_id.get(id).cloned()).collect()
    };

    Ok(AgentSkillsOverview {
        neural_skills: pick(&groups.neural_ids),
        pack_groups: groups
            .pack_groups
            .iter()
            .map(|g| AgentSkillPackGroup {
                tag: g.tag.clone(),
                skills: pick(&g.skill_ids),
            })
            .collect(),
        standalone_skills: pick(&groups.standalone_ids),
    })
}
