//! 待沉淀队列的取数口径：条数护栏 + 上下文预算 + 编号摘要
//!
//! 原先这段逻辑在 `handlers/hr/agent/settle_memory.rs`（Adapter 层），
//! 迁移到 Domain 的两个原因：
//!
//! 1. **分层红线**：`legacy_settle` 的 `prepare` 节点要直接取一批记忆，
//!    Domain 不能反过来依赖 Handler；
//! 2. **单一真相**：`PENDING_MAX_ITEMS` 同时被系统域（cron payload）与
//!    Handler（神经工具默认值）引用，只有一处的常量才不会被改漏。
//!
//! ## 批量策略（用户拍板）
//!
//! - **只按字符预算分批**：累计长度还在预算内就继续拼，不按条数切片；
//! - 条数上限 [`PENDING_MAX_ITEMS`] 只是 DB 防爆的硬护栏；
//! - 预算 = 模型「推荐/最佳上下文长度」的 40%；未知时用
//!   `max_context_length * 60%` 再乘 40%，完全未知用兜底常量；
//! - 排序**最早优先**：沉淀是队列语义，先进先出；按最近优先会让持续产生的新
//!   记忆把老记忆永远挤出窗口。

use crate::models::agent::Agent;
use crate::models::memory::{Memory, MemoryPo, ShortTermMemoryIndexPo};
use crate::pkg::RequestContext;
use crate::service::dao::memory::{MemoryQuery, MemorySortOrder};
use crate::service::domain::runtime::RuntimeMemory;
use common::enums::{MemoryStatus, MemoryType};
use common::error::Result;

/// 单批待沉淀记忆的条数上限（防呆，避免极端情况下无限拼接）
///
/// ⚠️ 这是**硬护栏**不是批量策略：真正的批量由上下文预算决定。
pub(crate) const PENDING_MAX_ITEMS: usize = 200;

/// 待沉淀列表最多占用「可用上下文」的比例
///
/// 沉淀 prompt 里还有 System（人设 + 技能）、通用上下文，以及模型输出与工具往返，
/// 因此待沉淀列表不能吃满整个窗口。
const PENDING_BUDGET_RATIO: f64 = 0.4;

/// 上下文长度未知时，`max_context_length` 的可用比例（与 think_loop 的压缩阈值同源）
const CONTEXT_USABLE_RATIO: f64 = 0.6;

/// 上下文长度完全未知时的待沉淀字符预算兜底
const DEFAULT_PENDING_BUDGET_CHARS: usize = 12_000;

/// 取出短期记忆索引（只读）
///
/// Domain 返回的是 `Memory` 业务实体而非 PO，短期记忆的具体字段在
/// `MemoryPo::ShortTerm` 变体内，这里统一收敛访问方式。
pub(crate) fn short_term_of(mem: &Memory) -> Option<&ShortTermMemoryIndexPo> {
    match &mem.po {
        MemoryPo::ShortTerm(index) => Some(index),
        _ => None,
    }
}

/// 计算待沉淀列表可用的字符预算
///
/// 优先级：模型推荐上下文长度 > `max_context_length * 60%` > 兜底常量，
/// 与 `run_think_loop` 的压缩阈值取法保持一致。
///
/// # 为什么按「字符」而不是「token」
///
/// 项目没有 tokenizer，硬造一个 risk 大于收益。这里保守假设
/// **1 字符 ≈ 1 token**（中文密集场景的上界）：对英文内容会明显偏保守，
/// 但绝不会把上下文撑爆 —— 沉淀场景宁可少处理几条，也不能劣化模型表现。
fn pending_budget_chars(agent: &Agent) -> usize {
    let Some(brain) = agent.brain.as_ref() else {
        return DEFAULT_PENDING_BUDGET_CHARS;
    };
    let Some(provider) = brain.model_provider() else {
        return DEFAULT_PENDING_BUDGET_CHARS;
    };

    let config = provider.config();
    let context_tokens = config
        .recommended_context_length
        .filter(|&v| v > 0)
        .map(|v| v as f64)
        .or_else(|| {
            config
                .max_context_length
                .filter(|&v| v > 0)
                .map(|v| v as f64 * CONTEXT_USABLE_RATIO)
        });

    match context_tokens {
        Some(tokens) => (tokens * PENDING_BUDGET_RATIO) as usize,
        None => DEFAULT_PENDING_BUDGET_CHARS,
    }
}

/// 构建待沉淀短期记忆的编号摘要（按上下文预算自适应批量）
///
/// 约束模板已内聚到 `PromptBuilder.build_sleep_prompt`，本函数只返回记忆摘要。
///
/// # 返回
/// - `Ok(None)` 表示无未沉淀记忆（调用方应跳过）
/// - `Ok(Some((summary, ids, truncated)))`：
///   - `summary` 编号摘要字符串（注入 prompt）
///   - `ids` 本批记忆 id，供沉淀成功后框架置状态使用
///   - `truncated` 是否因触达预算/上限而截断（true 表示还有剩余待沉淀）
pub(crate) async fn build_pending_memories_summary(
    ctx: &RequestContext,
    memory: &dyn RuntimeMemory,
    agent_id: &str,
    budget_chars: usize,
    max_items: usize,
) -> Result<Option<(String, Vec<String>, bool)>> {
    // 1. 取候选集：未沉淀（Active）、最早优先，最多 PENDING_MAX_ITEMS 条
    //
    // 不做分页：`MemoryQuery` 没有 offset，重复查询会一直拿到同一批
    // （本批此时还未置为 Settled）。单次取上限再由预算截断更简单也更省查询。
    let candidates = memory
        .query(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some(agent_id.to_string()),
                status: Some(MemoryStatus::Active),
                memory_type: Some(MemoryType::ShortTerm),
                limit: Some(max_items),
                order: MemorySortOrder::OldestFirst,
                ..Default::default()
            },
        )
        .await?;

    if candidates.is_empty() {
        return Ok(None);
    }

    // 2. 按预算累加（约束模板由 builder.build_sleep_prompt 注入）
    let budget = budget_chars.max(1);
    let mut summary = String::new();
    let mut ids = Vec::new();
    let mut truncated = false;

    for mem in &candidates {
        let Some(index) = short_term_of(mem) else {
            continue;
        };
        let line = format!("{}. [id={}] {}\n", ids.len() + 1, index.id, index.summary);

        // 预算用尽：停下，剩下的留给下一批
        if !summary.is_empty() && summary.chars().count() + line.chars().count() > budget {
            truncated = true;
            break;
        }
        // 条数上限：同样停下
        if ids.len() >= max_items {
            truncated = true;
            break;
        }

        summary.push_str(&line);
        ids.push(index.id.clone());
    }

    if ids.is_empty() {
        return Ok(None);
    }

    // 候选集本身被 limit 截断时，也算有剩余（无法确认后面还有没有，保守为 true）
    if candidates.len() >= max_items {
        truncated = true;
    }

    crate::log_info!(
        ctx,
        "legacy_settle",
        "agent_id={}, 候选 {} 条，本批纳入 {} 条，预算 {} 字符，仍有剩余={}",
        agent_id,
        candidates.len(),
        ids.len(),
        budget,
        truncated
    );

    Ok(Some((summary, ids, truncated)))
}

/// 由调用方的条数提示与硬上限取小，得到本批候选集上限
pub(crate) fn batch_item_cap(settle_limit: usize) -> usize {
    settle_limit.clamp(1, PENDING_MAX_ITEMS)
}

/// 计算一次 run 的字符预算（对外只暴露这一个入口）
pub(crate) fn settle_budget_chars(agent: &Agent) -> usize {
    pending_budget_chars(agent).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 预算取法：拿不到模型配置时回落到兜底常量，且恒为正
    #[test]
    fn pending_budget_chars_falls_back_without_brain() {
        let mut po = crate::models::agent::AgentPo::new(
            "Test".to_string(),
            vec!["assistant".to_string()],
            "desc".to_string(),
            vec![],
            "".to_string(),
            "provider-001".to_string(),
            "test-user".to_string(),
        );
        po.id = "agent-budget".to_string();
        let agent = Agent::from_po(po);

        // 无 brain → 兜底
        assert_eq!(pending_budget_chars(&agent), DEFAULT_PENDING_BUDGET_CHARS);
        assert!(pending_budget_chars(&agent) > 0);
    }

    /// 用户拍板：预算 = 推荐上下文长度的 40%
    #[test]
    fn pending_budget_is_forty_percent_of_recommended_context() {
        let mut po = crate::models::agent::AgentPo::new(
            "Test".to_string(),
            vec!["assistant".to_string()],
            "desc".to_string(),
            vec![],
            "".to_string(),
            "provider-001".to_string(),
            "test-user".to_string(),
        );
        po.id = "agent-budget-ratio".to_string();
        let mut agent = Agent::from_po(po);

        let provider = crate::models::model_provider::ModelProviderPo {
            id: "mock-provider".to_string(),
            name: "Mock Provider".to_string(),
            provider_type: common::enums::ProviderType::OpenAI,
            model_name: "gpt-4".to_string(),
            capability: common::enums::ModelCapability::Agent,
            api_key: "fake-key".to_string(),
            base_url: None,
            description: None,
            config: r#"{"recommended_context_length":1000}"#.to_string(),
            status: common::enums::ModelProviderStatus::Normal,
            created_by: "test-user".to_string(),
            modified_by: "test-user".to_string(),
            created_at: 0,
            updated_at: 0,
        };
        agent.brain = Some(crate::models::brain::Brain::new_local(
            agent.po.id.clone(),
            "Test Agent".to_string(),
            crate::models::agent::AgentRuntimeConfig::default(),
            provider,
            vec![],
        ));

        assert_eq!(pending_budget_chars(&agent), 400);
    }

    /// 条数提示与硬上限取小，且不小于 1
    #[test]
    fn batch_item_cap_respects_hard_limit() {
        assert_eq!(batch_item_cap(0), 1);
        assert_eq!(batch_item_cap(10), 10);
        assert_eq!(batch_item_cap(usize::MAX), PENDING_MAX_ITEMS);
    }
}
