//! 定时任务/日程触发执行统计事件
//!
//! `TriggerExecEvent` 绑定 `trigger_exec_events` 表，按「一次 attempt」落一条：
//! 每次 `CronTriggerConsumer::on_event` 执行完（成功 / 跳过 / 失败）都记录。
//!
//! 与 `cron_triggers.total_runs`（SQLite 里的配额游标，仅计成功推进）语义不同：
//! 本表是**执行留痕**，失败/跳过也记，供管理端详情页展示「日程维度的执行记录」。
//! 刷盘沿用 Stats 的三重保障（攒满 batch_size / 周期定时 / 优雅退出），
//! 低频触发器由 30s 周期任务兜底可见。

use super::*;
use ai_orz_macros::StatsEvent;
use common::error::Result;
use duckdb::{Connection, ToSql};
use uuid::Uuid;

#[derive(Debug, Clone, Default, StatsEvent)]
#[event_type = "trigger_exec"]
pub struct TriggerExecEvent {
    #[timestamp]
    pub timestamp: i64,
    /// 触发器 ID（执行记录按此过滤）
    #[tag]
    pub trigger_id: String,
    #[tag]
    pub trigger_name: String,
    /// 业务动作标签（存量回落 payload.action 后的结果）
    #[tag]
    pub action: Option<String>,
    /// 行为模板名（SyncMaintenance / AsyncNotify / AsyncAgentWakeup）
    #[tag]
    pub behavior: String,
    /// 结果状态：success / skipped / failed
    #[tag]
    pub status: String,
    /// 跳过或失败原因（成功时为 None）
    #[tag]
    pub skip_reason: Option<String>,
    #[tag]
    pub org_id: Option<String>,
    #[tag]
    pub owner_user_id: Option<String>,
    #[tag]
    pub agent_id: Option<String>,
    /// 联邦调用方组织（审计维度）：由 Stats::record 从 ctx 自动注入
    #[tag]
    pub caller_organization_id: Option<String>,
    /// 每次 attempt 记 1，sum 即执行次数
    #[metric]
    pub call_count: u64,
    /// 本次 attempt 耗时（毫秒）
    #[metric]
    pub duration_ms: Option<u64>,
    /// 本次派发的下游动作数（通知 1 条消息 / 唤醒 1 个 Agent / 同步维护 1 个动作）
    #[metric]
    pub dispatched_count: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct TriggerExecStatTable;

impl StatTable<TriggerExecEvent> for TriggerExecStatTable {
    fn table_name(&self) -> &str {
        "trigger_exec_events"
    }

    fn is_dedicated_table(&self) -> bool {
        true
    }

    fn create_table(&self, conn: &mut Connection) -> Result<()> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS trigger_exec_events (
                id UUID PRIMARY KEY,
                timestamp BIGINT,
                trigger_id VARCHAR,
                trigger_name VARCHAR,
                action VARCHAR,
                behavior VARCHAR,
                status VARCHAR,
                skip_reason VARCHAR,
                org_id VARCHAR,
                owner_user_id VARCHAR,
                agent_id VARCHAR,
                caller_organization_id VARCHAR,
                call_count BIGINT,
                duration_ms BIGINT,
                dispatched_count BIGINT
            );
        "#;
        conn.execute(sql, []).map_err(|e| {
            common::error::Error::internal(format!(
                "Failed to create trigger_exec_events table: {}",
                e
            ))
        })?;
        // 兼容已有 DuckDB 文件：补加审计维度列（幂等）
        conn.execute(
            "ALTER TABLE trigger_exec_events ADD COLUMN IF NOT EXISTS caller_organization_id VARCHAR",
            [],
        )
        .map_err(|e| {
            common::error::Error::internal(format!(
                "Failed to migrate trigger_exec_events table: {}",
                e
            ))
        })?;
        // 明细/统计都按 trigger_id + 时间窗过滤，建 ART 索引避免全表扫（幂等）。
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_trigger_exec_events_trigger_time \
             ON trigger_exec_events (trigger_id, timestamp)",
            [],
        )
        .map_err(|e| {
            common::error::Error::internal(format!(
                "Failed to create trigger_exec_events index: {}",
                e
            ))
        })?;
        Ok(())
    }

    fn insert_event(&self, conn: &mut Connection, event: &TriggerExecEvent) -> Result<()> {
        let id = Uuid::now_v7();
        let sql = r#"
            INSERT INTO trigger_exec_events (
                id, timestamp, trigger_id, trigger_name, action, behavior,
                status, skip_reason, org_id, owner_user_id, agent_id,
                caller_organization_id, call_count, duration_ms, dispatched_count
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?);
        "#;
        conn.execute(
            sql,
            [
                &id.to_string() as &dyn ToSql,
                &event.timestamp as &dyn ToSql,
                &event.trigger_id as &dyn ToSql,
                &event.trigger_name as &dyn ToSql,
                &event.action as &dyn ToSql,
                &event.behavior as &dyn ToSql,
                &event.status as &dyn ToSql,
                &event.skip_reason as &dyn ToSql,
                &event.org_id as &dyn ToSql,
                &event.owner_user_id as &dyn ToSql,
                &event.agent_id as &dyn ToSql,
                &event.caller_organization_id as &dyn ToSql,
                &event.call_count as &dyn ToSql,
                &event.duration_ms as &dyn ToSql,
                &event.dispatched_count as &dyn ToSql,
            ],
        )
        .map_err(|e| {
            common::error::Error::internal(format!("Failed to insert trigger exec event: {}", e))
        })?;
        Ok(())
    }

    fn bulk_insert_events(&self, conn: &mut Connection, events: &[TriggerExecEvent]) -> Result<()> {
        for event in events {
            self.insert_event(conn, event)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::enums::TriggerExecStatus;

    #[test]
    fn trigger_exec_event_table_roundtrip() {
        let table = TriggerExecStatTable;
        let mut conn = Connection::open_in_memory().unwrap();
        table.create_table(&mut conn).unwrap();

        let event = TriggerExecEvent {
            timestamp: 1_700_000_000_000,
            trigger_id: "trig-1".to_string(),
            trigger_name: "每日提醒".to_string(),
            action: Some("notify".to_string()),
            behavior: "AsyncNotify".to_string(),
            status: TriggerExecStatus::Success.as_str().to_string(),
            skip_reason: None,
            org_id: Some("org-1".to_string()),
            owner_user_id: Some("u-1".to_string()),
            agent_id: None,
            caller_organization_id: None,
            call_count: 1,
            duration_ms: Some(12),
            dispatched_count: 1,
        };
        table.insert_event(&mut conn, &event).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM trigger_exec_events WHERE trigger_id = 'trig-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}

#[cfg(test)]
mod filter_tests {
    use super::*;
    use common::enums::TriggerExecStatus;

    fn event(trigger_id: &str, status: &str, org_id: Option<&str>, ts: i64) -> TriggerExecEvent {
        TriggerExecEvent {
            timestamp: ts,
            trigger_id: trigger_id.to_string(),
            trigger_name: "测试触发器".to_string(),
            action: Some("notify".to_string()),
            behavior: "AsyncNotify".to_string(),
            status: status.to_string(),
            skip_reason: (status == TriggerExecStatus::Skipped.as_str())
                .then(|| "missing owner".to_string()),
            org_id: org_id.map(str::to_string),
            owner_user_id: None,
            agent_id: None,
            caller_organization_id: None,
            call_count: 1,
            duration_ms: Some(5),
            dispatched_count: if status == TriggerExecStatus::Success.as_str() {
                1
            } else {
                0
            },
        }
    }

    #[test]
    fn trigger_exec_filters_by_status_time_and_org() {
        let table = TriggerExecStatTable;
        let mut conn = Connection::open_in_memory().unwrap();
        table.create_table(&mut conn).unwrap();

        for e in [
            event(
                "t-1",
                TriggerExecStatus::Success.as_str(),
                Some("org-1"),
                1000,
            ),
            event(
                "t-1",
                TriggerExecStatus::Skipped.as_str(),
                Some("org-1"),
                2000,
            ),
            event(
                "t-1",
                TriggerExecStatus::Failed.as_str(),
                Some("org-2"),
                3000,
            ),
            event(
                "t-2",
                TriggerExecStatus::Success.as_str(),
                Some("org-1"),
                4000,
            ),
        ] {
            table.insert_event(&mut conn, &e).unwrap();
        }

        // 三态按组织 + 时间窗过滤：org-1 的 t-1 只命中 success/skipped 两条
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM trigger_exec_events \
                 WHERE trigger_id = 't-1' AND org_id = 'org-1' \
                   AND timestamp >= 1000 AND timestamp <= 2500",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);

        let failed: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM trigger_exec_events WHERE status = 'failed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(failed, 1);

        let skipped_reason: String = conn
            .query_row(
                "SELECT skip_reason FROM trigger_exec_events WHERE status = 'skipped'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(skipped_reason, "missing owner");
    }
}
