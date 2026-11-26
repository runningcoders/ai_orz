//! 日程查询的领域逻辑。
//!
//! 查询能力属于 domain：DAO 只提供原始过滤基建（SQL 层面），DAL 封装单表访问；
//! 「哪个时间范围、哪些日程、展开成什么实例」是领域语义，放在这里才能被
//! handler、consumer 等上层复用，而不是各自去拼 DAO 的 `CronTriggerQuery`。
//!
//! 本文件只放纯函数与领域参数类型，不碰 IO；trait 方法在 `mod.rs` 编排
//! DA(L) 与这些纯函数。

use crate::models::cron_trigger::CronTriggerPo;
use common::api::{QueryScheduleResponse, ScheduleOccurrence};
use common::enums::{CreatorType, ScheduleDateTerm, TriggerBehavior, TriggerType};
use common::error::{Result, bail_err};
use common::schedule::{self, ScheduleRule, effective_type};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// 单次查询的最大规则数 / 每条规则的最大实例数 / 最终返回上限
pub(crate) const MAX_RULES: usize = 500;
const MAX_PER_RULE: usize = 400;
const MAX_ITEMS: usize = 500;

/// 领域级日程查询参数。
///
/// 之所以不直接复用 `dao::CronTriggerQuery`：日期词、显式区间、关键词都是 DAO
/// 实体表达不了的查询意图，展开成实例更是领域逻辑。能直接用 DAO Query 表达的
/// 简单查询（如 `list_triggers`）则直接复用，不为隔离而隔离。
///
/// 身份不在此结构里——`owner_user_id` 一律从 `RequestContext` 固化，
/// 调用方无法伪造，避免跨用户查询。
#[derive(Debug, Clone, Default)]
pub struct ScheduleQuery {
    pub date_term: ScheduleDateTerm,
    pub from: Option<i64>,
    pub to: Option<i64>,
    pub keyword: Option<String>,
    pub behavior: Option<TriggerBehavior>,
    pub is_enabled: Option<bool>,
    /// 返回条数上限，handler 层已按接口约定 clamp
    pub limit: usize,
}

/// 解析查询区间：显式 from/to 优先（必须成对且 `to > from`），
/// 否则把相对日期词按指定时区折算为本地日历日半开区间。
pub(crate) fn resolve_range(query: &ScheduleQuery, now: i64, timezone: &str) -> Result<(i64, i64)> {
    if let (Some(from), Some(to)) = (query.from, query.to) {
        if to <= from {
            bail_err!(InvalidRequest, "to 必须大于 from");
        }
        return Ok((from, to));
    }
    if query.from.is_some() || query.to.is_some() {
        bail_err!(InvalidRequest, "from 与 to 必须同时提供");
    }
    schedule::resolve_term_range(query.date_term, now, timezone)
}

fn rule_of(trigger: &CronTriggerPo) -> ScheduleRule {
    ScheduleRule {
        effective_type: effective_type(
            trigger.cron_expression.as_deref(),
            trigger.interval_seconds,
        ),
        cron_expression: trigger.cron_expression.clone(),
        interval_seconds: trigger.interval_seconds,
        run_at: trigger.run_at,
        next_run_at: trigger.next_run_at,
        created_at: trigger.created_at,
        start_at: trigger.start_at,
        end_at: trigger.end_at,
        max_runs: trigger.max_runs,
        total_runs: trigger.total_runs,
    }
}

fn matches_keyword(trigger: &CronTriggerPo, keyword: &str) -> bool {
    if keyword.is_empty() {
        return true;
    }
    [
        trigger.name.as_str(),
        trigger.description.as_str(),
        trigger.resolved_action().as_deref().unwrap_or(""),
    ]
    .iter()
    .any(|field| field.to_lowercase().contains(&keyword.to_lowercase()))
}

/// 把命中的规则展开成实例列表（领域核心算法编排）。
///
/// `now`/`timezone` 由上层注入，便于测试；时区口径与展开、前端分桶一致。
///
/// 实现是「逐规则展开 + k 路归并取前 `limit` 个」：`expand_rule` 的返回值本身
/// 升序，归并即可得到全局有序结果。**不能**用「凑够 limit 就 break」——后面
/// 规则完全可能有更早的实例，提前截断会让日历丢掉排在前面的日程。
pub(crate) fn collect_occurrences(
    triggers: &[CronTriggerPo],
    query: &ScheduleQuery,
    from: i64,
    to: i64,
    timezone: &str,
) -> Result<Vec<ScheduleOccurrence>> {
    let keyword = query.keyword.as_deref().unwrap_or("").trim();
    let limit = query.limit.clamp(1, MAX_ITEMS);

    let mut rules: Vec<(usize, ScheduleRule, Vec<i64>)> = Vec::new();
    for (idx, trigger) in triggers.iter().enumerate() {
        if !matches_keyword(trigger, keyword) {
            continue;
        }
        let rule = rule_of(trigger);
        let times = schedule::expand_rule(&rule, from, to, timezone, MAX_PER_RULE)?;
        if !times.is_empty() {
            rules.push((idx, rule, times));
        }
    }

    // 堆元素：(发生时间, 规则序号, 规则内下标)。时间相同时按规则原始顺序
    // 打破平手，与「先按时间稳定排序」的结果一致。
    let mut heap: BinaryHeap<Reverse<(i64, usize, usize)>> = BinaryHeap::new();
    for (pos, (_, _, times)) in rules.iter().enumerate() {
        heap.push(Reverse((times[0], pos, 0)));
    }

    let mut occurrences = Vec::with_capacity(limit);
    while let Some(Reverse((run_at, rule_pos, item_pos))) = heap.pop() {
        let (trigger_idx, rule, times) = &rules[rule_pos];
        let trigger = &triggers[*trigger_idx];
        occurrences.push(ScheduleOccurrence {
            trigger_id: trigger.id.clone(),
            run_at,
            name: trigger.name.clone(),
            description: trigger.description.clone(),
            action: trigger.resolved_action(),
            behavior: trigger.behavior,
            rule_kind: rule.effective_type,
            recurring: !matches!(rule.effective_type, TriggerType::Once),
            // 身份三字段随实例返回：日历/查询结果要能直接回答「谁安排的、
            // 算谁的、谁来干」，而不是让调用方再逐条查详情。
            creator_type: trigger.creator_type.map(CreatorType::from),
            creator_id: trigger.creator_id.clone(),
            agent_id: trigger.agent_id.clone(),
            owner_user_id: trigger.owner_user_id.clone(),
            is_enabled: trigger.is_enabled != 0,
        });
        if occurrences.len() >= limit {
            break;
        }
        if item_pos + 1 < times.len() {
            heap.push(Reverse((times[item_pos + 1], rule_pos, item_pos + 1)));
        }
    }

    Ok(occurrences)
}

/// 组装响应（区间与时区由解析阶段给出，保持一处来源）。
pub(crate) fn into_response(
    query: &ScheduleQuery,
    from: i64,
    to: i64,
    timezone: String,
    occurrences: Vec<ScheduleOccurrence>,
) -> QueryScheduleResponse {
    QueryScheduleResponse {
        date_term: query.date_term,
        timezone,
        from,
        to,
        occurrences,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query() -> ScheduleQuery {
        ScheduleQuery {
            limit: MAX_ITEMS,
            ..Default::default()
        }
    }

    #[test]
    fn range_prefers_explicit_from_to() {
        let mut q = query();
        q.from = Some(100);
        q.to = Some(200);
        assert_eq!(resolve_range(&q, 0, "UTC").unwrap(), (100, 200));
    }

    #[test]
    fn range_rejects_partial_explicit() {
        let mut q = query();
        q.from = Some(100);
        assert!(resolve_range(&q, 0, "UTC").is_err());
    }

    #[test]
    fn range_rejects_inverted_explicit() {
        let mut q = query();
        q.from = Some(200);
        q.to = Some(100);
        assert!(resolve_range(&q, 0, "UTC").is_err());
    }

    #[test]
    fn range_defaults_to_today() {
        let (from, to) = resolve_range(&query(), 1_785_705_600_000, "Asia/Shanghai").unwrap();
        assert_eq!(to - from, 86_400_000);
    }

    fn schedule_trigger(id: &str, cron: &str) -> CronTriggerPo {
        let mut t = CronTriggerPo::new(
            id.to_string(),
            id.to_string(),
            TriggerType::Schedule,
            0,
            None,
        );
        t.cron_expression = Some(cron.to_string());
        t.owner_user_id = Some("user-1".into());
        t.behavior = TriggerBehavior::AsyncAgentWakeup;
        t.agent_id = Some("agent-1".into());
        // 展开窗口必须晚于创建时间，否则 `within` 会把实例全部过滤掉
        t.created_at = 0;
        t.updated_at = 0;
        t
    }

    /// 多条规则必须做全局归并：limit 很小也不能因为「前一条规则凑够数」而丢掉
    /// 后一条规则里更早的实例。
    #[test]
    fn merge_keeps_earliest_across_rules() {
        // A 每 10 分钟、B 每 7 分钟；B 的 7 分钟排在 A 的 10 分钟之前
        let a = schedule_trigger("a", "*/10 * * * *");
        let b = schedule_trigger("b", "*/7 * * * *");
        let mut q = query();
        q.limit = 3;
        let from = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .timestamp_millis();

        // 0 点两条规则同时命中（按规则顺序 a 在前），第三早才是 B 的 7 分钟
        let got = collect_occurrences(&[a, b], &q, from, from + 3_600_000, "UTC").unwrap();
        let times: Vec<i64> = got.iter().map(|o| o.run_at).collect();
        assert_eq!(times, vec![from, from, from + 7 * 60_000]);
        assert_eq!(got[0].trigger_id, "a");
        assert_eq!(got[1].trigger_id, "b");
        assert_eq!(got[2].trigger_id, "b");
    }
}
