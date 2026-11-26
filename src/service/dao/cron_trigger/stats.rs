//! CronTrigger 执行记录统计 DAO（DuckDB `trigger_exec_events`）
//!
//! 与 `sqlite.rs`（触发器配置/运行时列，SQLite）职责不同：本模块只读统计库，
//! 按「一次 attempt」的维度查询执行留痕，供管理端详情页展示。
//! 实例由 `dao::init_all` 注册，再由 DAL 组合根经构造注入使用（不在业务层取全局单例）。

use crate::pkg::RequestContext;
use crate::pkg::stats::StatParam;
use common::api::{TriggerExecBucket, TriggerExecRecord, TriggerExecSummary};
use common::enums::TriggerExecStatus;
use common::error::{Error, Result};
use serde_json::Value as JsonValue;
use std::sync::{Arc, OnceLock};

/// 执行记录查询条件（已由上层归一化时间窗与分页）
#[derive(Debug, Clone)]
pub struct TriggerExecQuery {
    pub trigger_id: String,
    /// 组织隔离：由上层从 trigger 的归属注入；None 表示系统级全局记录
    pub org_id: Option<String>,
    /// 毫秒闭区间；缺省由上层兜底
    pub time_range: (i64, i64),
    pub offset: usize,
    pub limit: usize,
}

/// 明细查询结果（概览走独立 stats 接口）
#[derive(Debug, Clone)]
pub struct TriggerExecQueryResult {
    pub records: Vec<TriggerExecRecord>,
    pub total: usize,
}

/// 统计查询条件（与明细共享时间窗 / 归属，额外带时序桶宽度）
#[derive(Debug, Clone)]
pub struct TriggerExecStatsQuery {
    pub trigger_id: String,
    pub org_id: Option<String>,
    pub time_range: (i64, i64),
    pub bucket_ms: i64,
}

/// 统计查询结果（概览 + 时序分布）
#[derive(Debug, Clone)]
pub struct TriggerExecStatsResult {
    pub summary: TriggerExecSummary,
    pub buckets: Vec<TriggerExecBucket>,
}

#[async_trait::async_trait]
pub trait TriggerExecStatsDao: Send + Sync {
    async fn query(
        &self,
        ctx: RequestContext,
        query: TriggerExecQuery,
    ) -> Result<TriggerExecQueryResult>;

    async fn query_stats(
        &self,
        ctx: RequestContext,
        query: TriggerExecStatsQuery,
    ) -> Result<TriggerExecStatsResult>;
}

static DAO: OnceLock<Arc<dyn TriggerExecStatsDao>> = OnceLock::new();

/// 取执行统计 DAO 单例（未初始化时为 None，由组合根在 `dao::init_all` 后注入 DAL）。
pub fn stats_dao() -> Option<Arc<dyn TriggerExecStatsDao>> {
    DAO.get().cloned()
}

pub fn stats_init() {
    let _ = DAO.set(Arc::new(TriggerExecStatsDaoDuckDb));
}

struct TriggerExecStatsDaoDuckDb;

/// 时序桶起点表达式（毫秒对齐，参数为桶宽）。
///
/// ⚠️ 必须用模减法而非 `timestamp / n`：DuckDB 的 `/` 是浮点除
/// （BIGINT / BIGINT → DOUBLE），浮点结果既会按事件逐条分组（失去 bucket 语义），
/// 又带精度误差，经 `Stats::query` 的 `as_i64()` 解析会整体落 0。
/// 与 `Stats::query_time_series` 的既有口径保持一致。
const BUCKET_START_EXPR: &str = "(timestamp - (timestamp % ?))";

/// 归属过滤：trigger_id + 组织 + 时间窗。组织隔离由上层从 trigger PO 注入，
/// 与 `Stats::query` 的 ctx 无关（后者不参与过滤）。
fn where_clause(with_org: bool) -> String {
    let mut sql = String::from(" WHERE trigger_id = ?");
    if with_org {
        sql.push_str(" AND org_id = ?");
    }
    sql.push_str(" AND timestamp >= ? AND timestamp <= ?");
    sql
}

fn range_params(query: &TriggerExecQuery, with_org: bool) -> Vec<StatParam> {
    let mut params = vec![StatParam::Str(query.trigger_id.clone())];
    if with_org {
        params.push(StatParam::Str(query.org_id.clone().unwrap_or_default()));
    }
    params.push(StatParam::Int(query.time_range.0));
    params.push(StatParam::Int(query.time_range.1));
    params
}

/// JSON 行 → u64（DuckDB COUNT 经 query() 转 JSON number）
fn row_u64(row: &JsonValue, key: &str) -> u64 {
    row.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0) as u64
}

#[async_trait::async_trait]
impl TriggerExecStatsDao for TriggerExecStatsDaoDuckDb {
    async fn query(
        &self,
        ctx: RequestContext,
        query: TriggerExecQuery,
    ) -> Result<TriggerExecQueryResult> {
        let stats = ctx.stats();
        let table = stats
            .get_table_name::<crate::pkg::stats::TriggerExecEvent>()
            .ok_or_else(|| Error::internal("trigger_exec_events table not registered"))?;
        // 系统级记录（org_id 为 None）不加组织条件，全局管理端可见；
        // 用户/组织日程必须命中同 org_id，避免跨租户读取执行明细。
        let with_org = query.org_id.is_some();
        let wc = where_clause(with_org);

        // 总数
        let count_sql = format!("SELECT COUNT(*) AS total FROM {table}{wc}");
        let count_rows = stats
            .query(ctx.clone(), &count_sql, &range_params(&query, with_org))
            .await?;
        let total = count_rows
            .first()
            .map(|r| row_u64(r, "total") as usize)
            .unwrap_or(0);

        // 明细：时间倒序分页（概览/分布走 query_stats 独立接口）
        let list_sql = format!(
            "SELECT timestamp, action, behavior, status, skip_reason, duration_ms, dispatched_count \
             FROM {table}{wc} ORDER BY timestamp DESC LIMIT ? OFFSET ?"
        );
        let mut params = range_params(&query, with_org);
        params.push(StatParam::Int(query.limit as i64));
        params.push(StatParam::Int(query.offset as i64));
        let rows = stats.query(ctx.clone(), &list_sql, &params).await?;

        let records = rows
            .iter()
            .map(|row| TriggerExecRecord {
                timestamp: row.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0),
                action: row
                    .get("action")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                behavior: row
                    .get("behavior")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                status: row
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                skip_reason: row
                    .get("skip_reason")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                duration_ms: row
                    .get("duration_ms")
                    .and_then(|v| v.as_f64())
                    .map(|v| v as u64),
                dispatched_count: row_u64(row, "dispatched_count"),
            })
            .collect();

        Ok(TriggerExecQueryResult { records, total })
    }

    async fn query_stats(
        &self,
        ctx: RequestContext,
        query: TriggerExecStatsQuery,
    ) -> Result<TriggerExecStatsResult> {
        let stats = ctx.stats();
        let table = stats
            .get_table_name::<crate::pkg::stats::TriggerExecEvent>()
            .ok_or_else(|| Error::internal("trigger_exec_events table not registered"))?;
        let with_org = query.org_id.is_some();
        let wc = where_clause(with_org);
        let mut base_params = vec![StatParam::Str(query.trigger_id.clone())];
        if with_org {
            base_params.push(StatParam::Str(query.org_id.clone().unwrap_or_default()));
        }
        base_params.push(StatParam::Int(query.time_range.0));
        base_params.push(StatParam::Int(query.time_range.1));

        // 总数 + 状态分布
        let count_sql = format!("SELECT COUNT(*) AS total FROM {table}{wc}");
        let count_rows = stats.query(ctx.clone(), &count_sql, &base_params).await?;
        let total = count_rows.first().map(|r| row_u64(r, "total")).unwrap_or(0);

        let dist_sql = format!("SELECT status, COUNT(*) AS cnt FROM {table}{wc} GROUP BY status");
        let dist_rows = stats.query(ctx.clone(), &dist_sql, &base_params).await?;
        let mut summary = TriggerExecSummary {
            total,
            ..Default::default()
        };
        for row in dist_rows {
            let cnt = row_u64(&row, "cnt");
            match TriggerExecStatus::from_value(
                row.get("status").and_then(|v| v.as_str()).unwrap_or(""),
            ) {
                Some(TriggerExecStatus::Success) => summary.success = cnt,
                Some(TriggerExecStatus::Skipped) => summary.skipped = cnt,
                Some(TriggerExecStatus::Failed) => summary.failed = cnt,
                None => {}
            }
        }

        let avg_sql =
            format!("SELECT AVG(CAST(duration_ms AS DOUBLE)) AS avg_duration_ms FROM {table}{wc}");
        let avg_rows = stats.query(ctx.clone(), &avg_sql, &base_params).await?;
        summary.avg_duration_ms = avg_rows
            .first()
            .and_then(|r| r.get("avg_duration_ms").and_then(|v| v.as_f64()));

        // 时序桶：按 bucket_ms 对齐（表达式必须用模减法，见 BUCKET_START_EXPR 注释）
        let success = TriggerExecStatus::Success.as_str();
        let skipped = TriggerExecStatus::Skipped.as_str();
        let failed = TriggerExecStatus::Failed.as_str();
        let bucket_sql = format!(
            "SELECT {BUCKET_START_EXPR} AS interval_start, \
                    COUNT(*) AS total, \
                    COUNT(*) FILTER (WHERE status = '{success}') AS success, \
                    COUNT(*) FILTER (WHERE status = '{skipped}') AS skipped, \
                    COUNT(*) FILTER (WHERE status = '{failed}') AS failed \
             FROM {table}{wc} GROUP BY interval_start ORDER BY interval_start"
        );
        let mut bucket_params = vec![StatParam::Int(query.bucket_ms)];
        bucket_params.extend(base_params);
        let bucket_rows = stats
            .query(ctx.clone(), &bucket_sql, &bucket_params)
            .await?;
        let buckets = bucket_rows
            .iter()
            .map(|row| TriggerExecBucket {
                interval_start: row
                    .get("interval_start")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0),
                total: row_u64(row, "total"),
                success: row_u64(row, "success"),
                skipped: row_u64(row, "skipped"),
                failed: row_u64(row, "failed"),
            })
            .collect();

        Ok(TriggerExecStatsResult { summary, buckets })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::stats::{StatTable, TriggerExecEvent, TriggerExecStatTable};
    use duckdb::Connection;

    fn event(timestamp: i64) -> TriggerExecEvent {
        TriggerExecEvent {
            timestamp,
            trigger_id: "trig-1".to_string(),
            trigger_name: "每日提醒".to_string(),
            behavior: "AsyncNotify".to_string(),
            status: "success".to_string(),
            call_count: 1,
            dispatched_count: 1,
            ..Default::default()
        }
    }

    /// 回归守卫：桶起点必须是对齐后的整数毫秒。
    /// 若退回 `(timestamp / ?) * ?`（DuckDB 浮点除），取 i64 会失败，
    /// 或是同桶事件被逐条分组、起点未对齐。
    #[test]
    fn bucket_start_is_aligned_integer_ms() {
        const BUCKET_MS: i64 = 3_600_000;
        let table = TriggerExecStatTable;
        let mut conn = Connection::open_in_memory().unwrap();
        table.create_table(&mut conn).unwrap();
        // 前两条落在同一小时桶，第三条进入下一个桶
        for ts in [1_700_000_000_123i64, 1_700_000_000_456, 1_700_003_600_000] {
            table.insert_event(&mut conn, &event(ts)).unwrap();
        }

        let sql = format!(
            "SELECT {BUCKET_START_EXPR} AS interval_start, COUNT(*) AS total \
             FROM trigger_exec_events GROUP BY interval_start ORDER BY interval_start"
        );
        let mut stmt = conn.prepare(&sql).unwrap();
        let rows: Vec<(i64, i64)> = stmt
            .query_map([BUCKET_MS], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(rows.len(), 2, "同一小时桶必须聚合，不能按事件逐条分组");
        assert_eq!(rows[0], (1_699_999_200_000, 2));
        assert_eq!(rows[1].0 % BUCKET_MS, 0);
    }
}
