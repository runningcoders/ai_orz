//! 工作台场景投影：语义层的纯推导
//!
//! 取数由 handler 完成（沿用 `workspace_metrics` 的编排模式），本模块不做 IO、
//! 不持有 DAL；输出直接是 `common::api::scene` 的 DTO。
//!
//! 判定函数（`category_of` / `attention_of` / `rank`）同时服务 v1 的全量 `derive`
//! 与 v2 的增量 `apply`，保证口径只有一份。

use common::api::scene::{
    BlockReason, PresenceDims, SceneAttention, SceneCategory, SceneEntity, SceneEntityKind,
    SceneProjectionResponse, SceneStats, StateLabel,
};
use common::enums::AgentRuntimeState;
use std::cmp::Reverse;
use std::collections::HashMap;

/// 停滞阈值：Busy 且超过该时长状态未变 → `Stalled`（仅 Info）
pub const STALLED_THRESHOLD_MS: i64 = 30 * 60 * 1000;
/// 缺省工位数
pub const DEFAULT_SEAT_LIMIT: u32 = 8;
/// 工位数硬上限：再多就是停车场，超出部分只能用聚合表达
pub const MAX_SEAT_LIMIT: u32 = 8;

/// 一次投影所需的全部事实（handler 取好后传入）
#[derive(Debug, Clone, Default)]
pub struct SceneInput {
    pub now: i64,
    pub seat_limit: u32,
    pub queue_backlog: u64,
    /// 在役 Agent 的 `id → 展示名`（HR 档案口径，已排除 Deleted）
    ///
    /// 同时作为**membership 白名单**：runtime 注册表可能残留已删除 Agent 的状态，
    /// 但只有档案里仍在役的 Agent 才有资格上席位。展示名也从这里取，不再回退 id。
    pub active_agents: HashMap<String, String>,
    /// 只包含运行时已注册的 Agent；未注册的不进入投影
    pub facts: Vec<PresenceFacts>,
}

/// 单个 Agent 的判定事实
///
/// `last_agent_message_at` / `last_user_message_at` / `is_task_context` 三者共同
/// 决定是否「在等用户回话」，由 handler 的消息窗口查询提供。
#[derive(Debug, Clone, Default)]
pub struct PresenceFacts {
    pub agent_id: String,
    pub runtime_state: AgentRuntimeState,
    /// 当前状态的起始时间（毫秒）
    pub state_started_at: i64,
    pub last_agent_message_at: Option<i64>,
    pub last_user_message_at: Option<i64>,
    /// 最近一条待回复消息是否属于任务类或被 @
    pub is_task_context: bool,
}

/// 全量推导：v1 的输入源是轮询快照，v2 换成事件流后本函数不变
pub fn derive(input: SceneInput, seq: u64) -> SceneProjectionResponse {
    let limit = input.seat_limit.clamp(1, MAX_SEAT_LIMIT);

    // membership 由 HR 档案决定，runtime 只提供状态：档案里不在役的（如已删除）
    // 即使 runtime 仍残留状态也不进入投影，避免席位数与右侧 Agent 列表对不上。
    let active: Vec<(&PresenceFacts, &String)> = input
        .facts
        .iter()
        .filter_map(|facts| {
            input
                .active_agents
                .get(&facts.agent_id)
                .map(|name| (facts, name))
        })
        .collect();

    let mut stats = SceneStats {
        agent_total: active.len() as u32,
        queue_backlog: input.queue_backlog,
        ..Default::default()
    };

    let mut entities: Vec<SceneEntity> = active
        .into_iter()
        .map(|(facts, name)| entity_of(facts, name, input.now))
        .collect();

    for entity in &entities {
        match entity.dims.category {
            SceneCategory::Working => stats.working += 1,
            SceneCategory::Idle => stats.idle += 1,
            SceneCategory::Resting => stats.resting += 1,
            SceneCategory::Waiting => stats.waiting += 1,
            SceneCategory::Stalled => stats.stalled += 1,
            SceneCategory::Offline => {}
        }
    }

    let attention_total = entities
        .iter()
        .filter(|entity| entity.attention == SceneAttention::NeedsYou)
        .count() as u32;

    entities.sort_by_key(rank);
    entities.truncate(limit as usize);

    SceneProjectionResponse {
        generated_at: input.now,
        seq,
        entities,
        attention_total,
        stats,
    }
}

/// 单个 Agent 的类别判定
///
/// 优先级：忙碌（含停滞）优先于「等你回话」——正在干活的 Agent 不该被算成在等你，
/// 否则注意力队列会被误报淹没。
pub fn category_of(facts: &PresenceFacts, now: i64) -> SceneCategory {
    match facts.runtime_state {
        AgentRuntimeState::Busy => {
            if now.saturating_sub(facts.state_started_at) >= STALLED_THRESHOLD_MS {
                SceneCategory::Stalled
            } else {
                SceneCategory::Working
            }
        }
        AgentRuntimeState::Resting => {
            if owes_reply(facts) {
                SceneCategory::Waiting
            } else {
                SceneCategory::Resting
            }
        }
        AgentRuntimeState::Idle => {
            if owes_reply(facts) {
                SceneCategory::Waiting
            } else {
                SceneCategory::Idle
            }
        }
    }
}

/// 注意力判定：只有「等你回话」进 NeedsYou，停滞仅作 Info
pub fn attention_of(category: SceneCategory) -> SceneAttention {
    match category {
        SceneCategory::Waiting => SceneAttention::NeedsYou,
        SceneCategory::Stalled => SceneAttention::Info,
        _ => SceneAttention::None,
    }
}

fn owes_reply(facts: &PresenceFacts) -> bool {
    if !facts.is_task_context {
        return false;
    }
    let Some(last_agent) = facts.last_agent_message_at else {
        return false;
    };
    facts
        .last_user_message_at
        .is_none_or(|last_user| last_agent > last_user)
}

fn entity_of(facts: &PresenceFacts, display_name: &str, now: i64) -> SceneEntity {
    let category = category_of(facts, now);
    let attention = attention_of(category);
    let last_activity_at = [
        Some(facts.state_started_at),
        facts.last_agent_message_at,
        facts.last_user_message_at,
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(facts.state_started_at);

    SceneEntity {
        id: facts.agent_id.clone(),
        kind: SceneEntityKind::Agent,
        display_name: display_name.to_string(),
        dims: PresenceDims {
            interruptible: !matches!(
                category,
                SceneCategory::Resting | SceneCategory::Stalled | SceneCategory::Offline
            ),
            category,
            blocked_on: match category {
                SceneCategory::Waiting => Some(BlockReason::UserReply),
                _ => None,
            },
            since: if category == SceneCategory::Waiting {
                facts
                    .last_agent_message_at
                    .unwrap_or(facts.state_started_at)
            } else {
                facts.state_started_at
            },
        },
        // v1 只给类别级标签；think_round / tool_call 等细粒度标签随事件流接入
        labels: Vec::<StateLabel>::new(),
        attention,
        last_activity_at,
    }
}

/// 排序：先按 attention，再按类别紧急度，最后按最近活动
fn rank(entity: &SceneEntity) -> (u8, u8, Reverse<i64>) {
    let attention_rank = match entity.attention {
        SceneAttention::NeedsYou => 0,
        SceneAttention::Info => 1,
        SceneAttention::None => 2,
    };
    let category_rank = match entity.dims.category {
        SceneCategory::Waiting => 0,
        SceneCategory::Stalled => 1,
        SceneCategory::Working => 2,
        SceneCategory::Resting => 3,
        SceneCategory::Idle => 4,
        SceneCategory::Offline => 5,
    };
    (
        attention_rank,
        category_rank,
        Reverse(entity.last_activity_at),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000_000_000;

    fn facts(state: AgentRuntimeState, since: i64) -> PresenceFacts {
        PresenceFacts {
            agent_id: "agent-1".to_string(),
            runtime_state: state,
            state_started_at: since,
            ..Default::default()
        }
    }

    #[test]
    fn busy_is_working_and_interruptible() {
        let entity = entity_of(&facts(AgentRuntimeState::Busy, NOW - 1_000), "A", NOW);
        assert_eq!(entity.dims.category, SceneCategory::Working);
        assert!(entity.dims.interruptible);
        assert_eq!(entity.attention, SceneAttention::None);
    }

    #[test]
    fn resting_is_not_interruptible() {
        let entity = entity_of(&facts(AgentRuntimeState::Resting, NOW - 1_000), "A", NOW);
        assert_eq!(entity.dims.category, SceneCategory::Resting);
        assert!(!entity.dims.interruptible);
    }

    #[test]
    fn busy_over_threshold_is_stalled_info_only() {
        let entity = entity_of(
            &facts(AgentRuntimeState::Busy, NOW - STALLED_THRESHOLD_MS),
            "A",
            NOW,
        );
        assert_eq!(entity.dims.category, SceneCategory::Stalled);
        assert_eq!(entity.attention, SceneAttention::Info);
    }

    #[test]
    fn idle_with_agent_message_after_user_is_waiting() {
        let mut f = facts(AgentRuntimeState::Idle, NOW - 1_000);
        f.is_task_context = true;
        f.last_agent_message_at = Some(NOW - 500);
        f.last_user_message_at = Some(NOW - 900);
        let entity = entity_of(&f, "A", NOW);
        assert_eq!(entity.dims.category, SceneCategory::Waiting);
        assert_eq!(entity.attention, SceneAttention::NeedsYou);
        assert_eq!(entity.dims.blocked_on, Some(BlockReason::UserReply));
    }

    #[test]
    fn user_replied_clears_waiting() {
        let mut f = facts(AgentRuntimeState::Idle, NOW - 1_000);
        f.is_task_context = true;
        f.last_agent_message_at = Some(NOW - 900);
        f.last_user_message_at = Some(NOW - 500);
        assert_eq!(category_of(&f, NOW), SceneCategory::Idle);
    }

    #[test]
    fn non_task_message_does_not_raise_attention() {
        let mut f = facts(AgentRuntimeState::Idle, NOW - 1_000);
        f.last_agent_message_at = Some(NOW - 500);
        f.last_user_message_at = Some(NOW - 900);
        assert_eq!(category_of(&f, NOW), SceneCategory::Idle);
    }

    #[test]
    fn truncates_entities_but_stats_cover_all() {
        let mut input = SceneInput {
            now: NOW,
            seat_limit: 2,
            queue_backlog: 3,
            active_agents: HashMap::new(),
            facts: Vec::new(),
        };
        for index in 0..5 {
            let mut f = facts(AgentRuntimeState::Idle, NOW - 1_000);
            f.agent_id = format!("agent-{index}");
            input
                .active_agents
                .insert(f.agent_id.clone(), format!("A{index}"));
            input.facts.push(f);
        }
        let projection = derive(input, 7);
        assert_eq!(projection.entities.len(), 2);
        assert_eq!(projection.stats.agent_total, 5);
        assert_eq!(projection.stats.idle, 5);
        assert_eq!(projection.seq, 7);
        assert_eq!(projection.generated_at, NOW);
    }

    #[test]
    fn needs_you_sorts_first() {
        let mut input = SceneInput {
            now: NOW,
            seat_limit: 8,
            queue_backlog: 0,
            active_agents: HashMap::new(),
            facts: Vec::new(),
        };
        let mut working = facts(AgentRuntimeState::Busy, NOW - 1_000);
        working.agent_id = "working".to_string();
        input
            .active_agents
            .insert(working.agent_id.clone(), "Working".to_string());
        let mut waiting = facts(AgentRuntimeState::Idle, NOW - 1_000);
        waiting.agent_id = "waiting".to_string();
        input
            .active_agents
            .insert(waiting.agent_id.clone(), "Waiting".to_string());
        waiting.is_task_context = true;
        waiting.last_agent_message_at = Some(NOW - 500);
        input.facts.push(working);
        input.facts.push(waiting);

        let projection = derive(input, 0);
        assert_eq!(projection.entities[0].id, "waiting");
        assert_eq!(projection.attention_total, 1);
    }

    #[test]
    fn deleted_agent_still_in_runtime_does_not_take_a_seat() {
        let mut input = SceneInput {
            now: NOW,
            seat_limit: 8,
            queue_backlog: 0,
            active_agents: HashMap::new(),
            facts: Vec::new(),
        };
        let mut active = facts(AgentRuntimeState::Idle, NOW - 1_000);
        active.agent_id = "active".to_string();
        input
            .active_agents
            .insert(active.agent_id.clone(), "在役".to_string());
        // 已删除但 runtime 注册表仍残留的 Agent：有状态、无档案
        let mut deleted = facts(AgentRuntimeState::Busy, NOW - 2_000);
        deleted.agent_id = "000000006a91aaf2ab8d94aa".to_string();
        input.facts.push(active);
        input.facts.push(deleted);

        let projection = derive(input, 0);
        assert_eq!(projection.entities.len(), 1);
        assert_eq!(projection.entities[0].id, "active");
        assert_eq!(projection.entities[0].display_name, "在役");
        assert_eq!(projection.stats.agent_total, 1);
    }
}
