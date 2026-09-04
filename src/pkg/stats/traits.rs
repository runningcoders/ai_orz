//! Stats 统计模块 — 核心类型定义
//!
//! 遵循设计文档：docs/stats_module_design.md

use std::fmt::Debug;

use common::error::Result;
use serde_json::Value;

/// 统计事件 trait
///
/// 所有统计事件都需要实现这个 trait。
/// 默认方法已经提供，用户只需要实现必须的方法。
pub trait StatEvent: Send + Sync + Debug {
    /// 获取时间戳（毫秒），必须实现
    fn timestamp(&self) -> i64;

    /// 获取事件类型名称，默认使用 type_name
    fn event_type(&self) -> &str {
        std::any::type_name::<Self>()
    }

    /// 获取标签引用（可选）
    fn tags(&self) -> Option<&Value> {
        None
    }

    /// 获取标签 JSON（如果自定义组装，默认使用 tags()）
    fn tags_json(&self) -> Option<Value> {
        self.tags().cloned()
    }

    /// 获取指标引用（可选）
    fn metrics(&self) -> Option<&Value> {
        None
    }

    /// 获取指标 JSON（如果自定义组装，默认使用 metrics()）
    fn metrics_json(&self) -> Option<Value> {
        self.metrics().cloned()
    }

    /// 注入联邦调用方组织（审计维度，见 docs/plan/跨组织业务调用方案.md §八）
    ///
    /// 由 `Stats::record` 统一从 `RequestContext.caller_organization_id` 调用；
    /// 事件结构体声明了 `caller_organization_id` 字段（derive 宏自动生成 override）
    /// 才会实际写入，否则 no-op（如通用 JSON 表事件）。
    fn apply_caller_organization(&mut self, _caller_org: Option<String>) {}
}

/// 统计表 trait
///
/// 每个统计表对应 DuckDB 中的一张表，用户可以自定义。
/// E: 事件类型
pub trait StatTable<E: StatEvent>: Send + Sync + Debug {
    /// 表名
    fn table_name(&self) -> &str;

    /// 创建表（如果不存在），初始化 schema
    fn create_table(&self, conn: &mut duckdb::Connection) -> Result<()>;

    /// 插入单个事件
    fn insert_event(&self, conn: &mut duckdb::Connection, event: &E) -> Result<()>;

    /// 批量插入事件
    fn bulk_insert_events(&self, conn: &mut duckdb::Connection, events: &[E]) -> Result<()>;

    /// 是否是专用表结构（有独立字段，而非 tags/metrics JSON）
    ///
    /// 默认表（如 default_events）使用 JSON 列存储 tags 和 metrics，
    /// 专用表（如 model_call_events）使用独立字段。
    fn is_dedicated_table(&self) -> bool {
        false
    }

    /// 获取标签/维度列的 SQL 引用方式
    ///
    /// 默认表：json_extract(tags, '$.column')
    /// 专用表：直接字段名
    fn column_sql(&self, column: &str) -> String {
        if self.is_dedicated_table() {
            column.to_string()
        } else {
            format!("json_extract_string(tags, '${}')", column)
        }
    }

    /// 获取指标列的 SQL 引用方式
    ///
    /// 默认表：json_extract(metrics, '$.metric')
    /// 专用表：直接字段名
    fn metric_sql(&self, metric: &str) -> String {
        if self.is_dedicated_table() {
            metric.to_string()
        } else {
            format!("json_extract(metrics, '${}')", metric)
        }
    }

    /// 获取过滤条件（等于匹配）的 SQL 列引用方式
    ///
    /// 默认实现使用 `column_sql`，适用于大多数场景。
    /// 默认表：json_extract_string(tags, '$.column')
    /// 专用表：直接字段名
    fn filter_equals_sql(&self, column: &str) -> String {
        self.column_sql(column)
    }

    /// 获取过滤条件（范围匹配）的 SQL 列引用方式
    ///
    /// 默认表：json_extract(tags, '$.column')（返回 JSON 值，便于 CAST 成数值）
    /// 专用表：直接字段名
    fn filter_range_sql(&self, column: &str) -> String {
        if self.is_dedicated_table() {
            column.to_string()
        } else {
            format!("json_extract(tags, '${}')", column)
        }
    }
}
