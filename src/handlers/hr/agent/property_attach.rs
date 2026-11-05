//! 属性装配的**唯一实现**（`search_memory` / `query_memory` 共用）
//!
//! (d)「唯一装配源」：语义通道（关键词 / 向量 / 混合 / 图遍历）与结构化通道
//! 必须回显**同一份**属性集合。两条通道各自抄一份装配逻辑，编译器不会报错，
//! 只会让两个通道慢慢走岔（C 单 `legacy_tags_json` 同款教训：单点产出、
//! 两面共用）。所以装配逻辑全仓只允许有这一份。

use crate::pkg::RequestContext;
use crate::service::domain::runtime::domain as runtime_domain;
use common::api::{KnowledgePropertyItem, MemoryResult};
use common::enums::KnowledgeRelationStatus;
use common::enums::ontology::PropertyTargetType;
use common::error::Result;

/// 属性装配的读取口径
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AttachOptions {
    /// 是否带出历史版本（`status = 0`，已被取代的旧值）
    ///
    /// 默认 `false`：只回生效值。历史版本留在库里可追溯，但不进模型视野
    /// （避免模型对着旧值推理）。
    pub(crate) include_history: bool,
}

impl AttachOptions {
    /// 由请求参数派生：缺省 / `None` ⇒ 只回生效值
    pub(crate) fn from_param(include_history: Option<bool>) -> Self {
        Self {
            include_history: include_history.unwrap_or(false),
        }
    }
}

/// 把命中载体的属性回填进结果（无属性的载体保持 `None`，不是空数组）
///
/// `include_history = true` 走受控历史出口（`status != 2`：只出 1 生效 + 0 被取代，
/// 显式软删的 2 在任何情况下都不返回）；`is_current` 由 `status` 派生——
/// 生效行 `true`（序列化时因 `skip_serializing_if` 缺席）、历史行 `false`。
pub(crate) async fn attach_properties(
    ctx: &RequestContext,
    results: &mut [MemoryResult],
    opts: AttachOptions,
) -> Result<()> {
    let targets: Vec<(PropertyTargetType, String)> = results
        .iter()
        .filter(|r| r.memory_type == "knowledge_node")
        .map(|r| (PropertyTargetType::Node, r.id.clone()))
        .collect();
    if targets.is_empty() {
        return Ok(());
    }

    // 两条读取通道只在「要不要历史」上分叉，装配逻辑完全共用
    let map = if opts.include_history {
        runtime_domain()
            .memory()
            .list_entity_properties_batch_with_history(ctx.clone(), &targets)
            .await?
    } else {
        runtime_domain()
            .memory()
            .list_entity_properties_batch(ctx.clone(), &targets)
            .await?
    };

    let key_prefix = PropertyTargetType::Node.as_str();
    for r in results.iter_mut() {
        if r.memory_type != "knowledge_node" {
            continue;
        }
        let Some(pos) = map.get(&(key_prefix.to_string(), r.id.clone())) else {
            continue;
        };
        // 指派列缺失的行 = 脏数据，跳过而不是给一个错类型的值
        let items: Vec<KnowledgePropertyItem> = pos
            .iter()
            .filter_map(|po| {
                Some(KnowledgePropertyItem {
                    term_key: po.term_key.clone(),
                    value: po.value()?,
                    ordinal: po.ordinal,
                    // status=1 生效 / status=0 被取代（历史）
                    is_current: po.status == KnowledgeRelationStatus::Active,
                })
            })
            .collect();
        if !items.is_empty() {
            r.properties = Some(items);
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "property_attach_tests.rs"]
mod tests;
