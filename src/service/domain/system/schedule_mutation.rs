//! Agent 修改日程的领域逻辑。
//!
//! 与 `schedule_query` 对称：handler 只做 DTO → 领域参数映射与身份固化，
//! 「谁能改、能改哪些字段」是领域语义，放这里才能被工具和其它上层复用。

use crate::models::cron_trigger::CronTriggerPo;
use common::enums::TriggerBehavior;
use common::error::{Result, bail_err};

/// Agent 更新日程的领域参数。
///
/// 身份刻意不在结构里——`owner_user_id` 一律从 `RequestContext` 固化，
/// 调用方无法伪造，避免跨用户修改他人日程。
#[derive(Debug, Clone, Default)]
pub struct ScheduleUpdate {
    pub trigger_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub cron_expression: Option<String>,
    pub interval_seconds: Option<i64>,
    pub run_at: Option<i64>,
    /// 三态：`None` 保持、`Some(None)` 清除、`Some(Some(v))` 覆盖
    pub start_at: Option<Option<i64>>,
    /// 三态：`None` 保持、`Some(None)` 清除、`Some(Some(v))` 覆盖
    pub end_at: Option<Option<i64>>,
    /// 三态：`None` 保持、`Some(None)` 清除、`Some(Some(v))` 覆盖
    pub max_runs: Option<Option<i64>>,
    /// 改派负责 Agent（空串视为未提供）
    pub agent_id: Option<String>,
}

/// 归属校验：只有「归属自己的异步日程」可改，系统维护模板一律不可经 Agent 触碰。
///
/// 判据是 `owner_user_id`（归属 / 受众），**不是** `creator_id`（谁安排的）：
/// creator 只用于创建时兜底（无负责 Agent 时由 Agent 创建者兜底、无归属时由
/// User 创建者兜底），不参与权限。同一用户下的多个 Agent 共享管理权，符合
/// 「Agent 代表用户行事」的模型；将来支持「替他人安排」时再引入 owner OR creator
/// 的可见范围，现在加会凭空放宽越权面。
///
/// 找不到 / 不属于自己 / 系统模板统一报 NotFound，不泄漏他人日程是否存在。
pub(crate) fn ensure_owned(trigger: &CronTriggerPo, uid: &str) -> Result<()> {
    let owned = trigger.owner_user_id.as_deref() == Some(uid);
    let async_template = trigger.behavior != TriggerBehavior::SyncMaintenance;
    if !owned || !async_template {
        bail_err!(NotFound, "日程不存在: {}", trigger.id);
    }
    Ok(())
}

/// 把允许的字段投影到 PO 上（原地修改），身份、行为、归属、运行统计一概不动。
///
/// 可改：标题 / 描述 / 时间三要素 / 窗口 / 次数 / 负责 Agent。
/// 不可改：`owner_user_id`（归属）、`creator_*`（创建者）、`behavior`、运行统计。
/// 窗口与次数用三态：不传保持、`null` 清除、传值覆盖。
///
/// 时间三选一是「整组替换」语义：只要给了三者中的任意一个，就把三处一并从参数
/// 覆盖（未给 = 清除）。否则「原来有 cron、这次只想换 interval」会被优先级
/// `cron > interval` 吃掉，表现为改了没生效——与更新接口的重算规则配套。
pub(crate) fn apply_allowed_update(target: &mut CronTriggerPo, update: &ScheduleUpdate) {
    if let Some(title) = update.title.as_deref().filter(|s| !s.trim().is_empty()) {
        target.name = title.to_string();
    }
    if let Some(description) = update.description.as_deref() {
        target.description = description.to_string();
    }
    if let Some(start) = update.start_at {
        target.start_at = start;
    }
    if let Some(end) = update.end_at {
        target.end_at = end;
    }
    if let Some(max_runs) = update.max_runs {
        target.max_runs = max_runs;
    }
    if let Some(agent_id) = update.agent_id.as_deref().filter(|s| !s.trim().is_empty()) {
        target.agent_id = Some(agent_id.to_string());
    }
    if update.cron_expression.is_some()
        || update.interval_seconds.is_some()
        || update.run_at.is_some()
    {
        target.cron_expression = update.cron_expression.clone();
        target.interval_seconds = update.interval_seconds;
        target.run_at = update.run_at;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::enums::TriggerType;

    fn own_schedule() -> CronTriggerPo {
        let mut t = CronTriggerPo::new(
            "sch-1".into(),
            "晨间简报".into(),
            TriggerType::Schedule,
            1_000,
            Some("agent-1".into()),
        );
        t.behavior = TriggerBehavior::AsyncAgentWakeup;
        t.owner_user_id = Some("user-1".into());
        t.cron_expression = Some("0 9 * * *".into());
        t
    }

    #[test]
    fn ownership_rejects_other_user_and_system_template() {
        let t = own_schedule();
        assert!(ensure_owned(&t, "user-1").is_ok());
        assert!(ensure_owned(&t, "user-2").is_err());
        let mut system = t.clone();
        system.behavior = TriggerBehavior::SyncMaintenance;
        assert!(ensure_owned(&system, "user-1").is_err());
    }

    #[test]
    fn update_applies_only_allowed_fields() {
        let mut t = own_schedule();
        let update = ScheduleUpdate {
            trigger_id: t.id.clone(),
            title: Some("新标题".into()),
            description: Some("新描述".into()),
            start_at: Some(Some(100)),
            max_runs: Some(Some(3)),
            ..Default::default()
        };
        apply_allowed_update(&mut t, &update);
        assert_eq!(t.name, "新标题");
        assert_eq!(t.description, "新描述");
        assert_eq!(t.start_at, Some(100));
        assert_eq!(t.max_runs, Some(3));
        // 身份/行为不动
        assert_eq!(t.owner_user_id.as_deref(), Some("user-1"));
        assert_eq!(t.behavior, TriggerBehavior::AsyncAgentWakeup);
        // 时间三选一未给 → 保持原样
        assert_eq!(t.cron_expression.as_deref(), Some("0 9 * * *"));
    }

    #[test]
    fn time_rule_is_full_replacement() {
        let mut t = own_schedule();
        let update = ScheduleUpdate {
            trigger_id: t.id.clone(),
            interval_seconds: Some(600),
            ..Default::default()
        };
        apply_allowed_update(&mut t, &update);
        // 新 interval 生效，旧 cron 被清掉，避免 cron > interval 优先级吃掉新值
        assert_eq!(t.cron_expression, None);
        assert_eq!(t.interval_seconds, Some(600));
    }

    #[test]
    fn tri_state_clears_window_and_runs() {
        let mut t = own_schedule();
        t.start_at = Some(100);
        t.end_at = Some(200);
        t.max_runs = Some(3);

        // 缺省 → 保持原值
        apply_allowed_update(&mut t, &ScheduleUpdate::default());
        assert_eq!(
            (t.start_at, t.end_at, t.max_runs),
            (Some(100), Some(200), Some(3))
        );

        // 显式 null → 清除限制
        apply_allowed_update(
            &mut t,
            &ScheduleUpdate {
                start_at: Some(None),
                end_at: Some(None),
                max_runs: Some(None),
                ..Default::default()
            },
        );
        assert_eq!((t.start_at, t.end_at, t.max_runs), (None, None, None));
    }

    #[test]
    fn agent_id_can_be_reassigned_but_identity_cannot() {
        let mut t = own_schedule();
        t.agent_id = Some("agent-1".into());
        apply_allowed_update(
            &mut t,
            &ScheduleUpdate {
                agent_id: Some("agent-2".into()),
                ..Default::default()
            },
        );
        assert_eq!(t.agent_id.as_deref(), Some("agent-2"));
        // 空串视为未提供，不清空负责 Agent
        apply_allowed_update(
            &mut t,
            &ScheduleUpdate {
                agent_id: Some("  ".into()),
                ..Default::default()
            },
        );
        assert_eq!(t.agent_id.as_deref(), Some("agent-2"));
        assert_eq!(t.owner_user_id.as_deref(), Some("user-1"));
    }
}
