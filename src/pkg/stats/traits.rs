//! Stats 统计模块 — 核心类型定义
//!
//! 遵循设计文档：docs/stats_module_design.md

use std::fmt::Debug;

use async_trait::async_trait;
use common::error::Result;
use crate::pkg::request_context::RequestContext;
use serde::{Deserialize, Serialize};
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
}

/// 统计表 trait
///
/// 每个统计表对应 DuckDB 中的一张表，用户可以自定义。
/// E: 事件类型
pub trait StatTable<E: StatEvent>: Send + Sync + Debug {
    /// 表名
    fn table_name(&self) -> &str;

    /// 创建表（如果不存在），初始化 schema
    fn create_table(
        &self,
        conn: &mut duckdb::Connection,
    ) -> Result<()>;

    /// 插入单个事件
    fn insert_event(
        &self,
        conn: &mut duckdb::Connection,
        event: &E,
    ) -> Result<()>;

    /// 批量插入事件
    fn bulk_insert_events(
        &self,
        conn: &mut duckdb::Connection,
        events: &[E],
    ) -> Result<()>;
}
