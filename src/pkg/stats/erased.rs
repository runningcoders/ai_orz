//! 类型擦除支持，内部使用

use std::fmt::Debug;
use std::marker::PhantomData;
use std::any::Any;

use common::error::{Error, Result};
use duckdb::Connection;

use crate::pkg::stats::traits::{StatEvent, StatTable};

/// 类型擦除的 StatTable
pub(crate) trait ErasedStatTable: Send + Sync + Debug {
    fn table_name(&self) -> &str;
    fn create_table(&self, conn: &mut Connection) -> Result<()>;
    /// 批量擦除事件插入，向下转换为具体类型后调用 table.bulk_insert_events
    fn bulk_insert_erased(&self, conn: &mut Connection, events: Vec<Box<dyn Any + Send + Sync>>) -> Result<()>;
    /// 是否是专用表结构
    fn is_dedicated_table(&self) -> bool;
    /// 获取标签/维度列的 SQL 引用方式
    fn column_sql(&self, column: &str) -> String;
    /// 获取指标列的 SQL 引用方式
    fn metric_sql(&self, metric: &str) -> String;
    /// 获取过滤条件（等于匹配）的 SQL 列引用方式
    fn filter_equals_sql(&self, column: &str) -> String;
    /// 获取过滤条件（范围匹配）的 SQL 列引用方式
    fn filter_range_sql(&self, column: &str) -> String;
}

// Instead of generic impl on T, wrap T in a newtype that holds the marker
/// Wrapper for type-erased stat table
#[derive(Debug)]
pub(crate) struct ErasedWrapper<E, T> {
    pub(crate) table: T,
    pub(crate) _marker: PhantomData<fn() -> E>,
}

impl<E, T> ErasedStatTable for ErasedWrapper<E, T>
where
    E: StatEvent + 'static,
    T: StatTable<E> + 'static,
{
    fn table_name(&self) -> &str {
        self.table.table_name()
    }

    fn create_table(&self, conn: &mut Connection) -> Result<()> {
        self.table.create_table(conn)
    }

    fn bulk_insert_erased(&self, conn: &mut Connection, events: Vec<Box<dyn Any + Send + Sync>>) -> Result<()> {
        // Convert Box<dyn Any> back to E
        let mut concrete_events = Vec::with_capacity(events.len());
        for erased in events {
            let concrete = erased.downcast::<E>().map(|b| *b);
            match concrete {
                Ok(e) => concrete_events.push(e),
                Err(_) => return Err(Error::internal("Failed to downcast erased event")),
            }
        }
        self.table.bulk_insert_events(conn, &concrete_events)
    }

    fn is_dedicated_table(&self) -> bool {
        self.table.is_dedicated_table()
    }

    fn column_sql(&self, column: &str) -> String {
        self.table.column_sql(column)
    }

    fn metric_sql(&self, metric: &str) -> String {
        self.table.metric_sql(metric)
    }

    fn filter_equals_sql(&self, column: &str) -> String {
        self.table.filter_equals_sql(column)
    }

    fn filter_range_sql(&self, column: &str) -> String {
        self.table.filter_range_sql(column)
    }
}

/// 类型擦除的事件缓冲
/// 保存为 Box<dyn Any + Send + Sync>, 运行时向下转换为具体类型
pub(crate) struct ErasedBuffer {
    buffer: Vec<Box<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for ErasedBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ErasedBuffer")
            .field("len", &self.buffer.len())
            .finish()
    }
}

impl ErasedBuffer {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn push<E: StatEvent + 'static + Send + Sync>(&mut self, event: E) {
        self.buffer.push(Box::new(event));
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn take(&mut self) -> Vec<Box<dyn Any + Send + Sync>> {
        std::mem::take(&mut self.buffer)
    }
}