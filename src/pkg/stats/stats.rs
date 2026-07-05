//! Top-level Stats struct implementation

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::any::TypeId;

use common::error::{Error, Result};
use common::models::{StatsInterval, TimeSeriesPoint, TokenSumResult};
use duckdb::{Connection, ToSql};
use duckdb::types::Value;
use serde_json::{self, Value as JsonValue};

use crate::pkg::request_context::RequestContext;
use super::erased::{ErasedBuffer, ErasedStatTable, ErasedWrapper};
use super::traits::{StatEvent, StatTable};
use super::default::{DefaultStatEvent, DefaultStatTable};
use super::model_call::{ModelCallEvent, ModelCallStatTable};
use super::tool_call::{ToolCallEvent, ToolCallStatTable};
use super::agent_awake::{AgentAwakeEvent, AgentAwakeStatTable};

/// 类型安全的 SQL 参数枚举（Send + Sync，替代 `dyn ToSql`）
///
/// 用于解决 `dyn ToSql` 不是 `Send`/`Sync` 导致 async Future 无法跨线程的问题。
/// 在 `query()` 内部转换为 `&dyn ToSql` 传给 duckdb。
#[derive(Debug, Clone)]
pub enum StatParam {
    /// 整数参数（时间戳等）
    Int(i64),
    /// 浮点数参数（范围过滤的 min/max）
    Double(f64),
    /// 字符串参数（JSON 值的字符串表示）
    Str(String),
}

impl ToSql for StatParam {
    fn to_sql(&self) -> duckdb::Result<duckdb::types::ToSqlOutput<'_>> {
        match self {
            StatParam::Int(v) => v.to_sql(),
            StatParam::Double(v) => v.to_sql(),
            StatParam::Str(v) => v.to_sql(),
        }
    }
}

/// Filter condition for querying statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum StatFilter {
    /// Equal match on a tag field
    Equals { key: String, value: JsonValue },
    /// Range match on a numeric field (timestamp or metric)
    Range { key: String, min: Option<f64>, max: Option<f64> },
}

/// Aggregation function to apply
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum StatAggregation {
    /// Count all matching events
    Count,
    /// Sum of a metric
    Sum(String),
    /// Average of a metric
    Avg(String),
}

/// One row of aggregation result
#[derive(Debug, Clone, serde::Serialize)]
pub struct AggregationRow {
    /// Group by key values
    pub groups: HashMap<String, JsonValue>,
    /// Aggregation results
    pub aggregations: HashMap<String, f64>,
}

/// Top-level Stats instance
///
/// Manages multiple statistic tables, provides automatic batching and flushing.
/// Each event type is bound to exactly one table.
///
/// 所有写操作通过内部可变性（`Mutex<HashMap>`）实现，
/// 因此 `record` / `register_table` / `flush_*` 等方法均只需 `&self`，
/// 可直接通过 `RequestContext::stats()` 返回的不可变引用调用。
#[derive(Debug)]
pub struct Stats {
    /// DuckDB connection
    conn: Mutex<Connection>,
    /// Batch size for automatic flush
    batch_size: usize,
    /// Registered tables: event TypeId → (table name, erased table, erased buffer)
    tables: Mutex<HashMap<TypeId, (String, Arc<dyn ErasedStatTable>, ErasedBuffer)>>,
    /// Registered tables by name: table name → erased table
    tables_by_name: Mutex<HashMap<String, Arc<dyn ErasedStatTable>>>,
}

impl Stats {
    /// Open a new Stats database
    pub async fn open(path: &str, batch_size: usize) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| Error::internal(format!("Failed to open DuckDB: {}", e)))?;

        Ok(Self {
            conn: Mutex::new(conn),
            batch_size,
            tables: Mutex::new(HashMap::new()),
            tables_by_name: Mutex::new(HashMap::new()),
        })
    }

    /// Initialize default tables for DefaultStatEvent, ModelCallEvent, and ToolCallEvent
    ///
    /// - default_events: 通用事件表，保留给灵活场景使用
    /// - model_call_events: 模型调用专用表
    /// - tool_call_events: 工具调用专用表
    /// - agent_awake_events: Agent 唤醒专用表
    pub fn initialize_default(&self) -> Result<()> {
        self.register_table(DefaultStatTable)?;
        self.register_table(ModelCallStatTable)?;
        self.register_table(ToolCallStatTable)?;
        self.register_table(AgentAwakeStatTable)?;
        Ok(())
    }

    /// Register a statistic table for a specific event type
    ///
    /// Each event type can be registered to exactly one table.
    /// If registered before, it will be replaced.
    pub fn register_table<E, T>(&self, table: T) -> Result<()>
    where
        E: StatEvent + 'static + Send + Sync,
        T: StatTable<E> + 'static,
    {
        let type_id = TypeId::of::<E>();
        let table_name = table.table_name().to_string();
        let erased: Arc<dyn ErasedStatTable> = Arc::new(ErasedWrapper {
            table,
            _marker: std::marker::PhantomData,
        });

        // Create table if not exists
        let mut conn_guard = self.conn.lock().map_err(|e| {
            Error::internal(format!("Failed to lock connection: {}", e))
        })?;
        erased.create_table(&mut conn_guard)?;

        let mut tables = self.tables.lock().map_err(|e| {
            Error::internal(format!("Failed to lock tables: {}", e))
        })?;
        tables.insert(
            type_id,
            (table_name.clone(), erased.clone(), ErasedBuffer::new())
        );

        let mut tables_by_name = self.tables_by_name.lock().map_err(|e| {
            Error::internal(format!("Failed to lock tables_by_name: {}", e))
        })?;
        tables_by_name.insert(table_name, erased);

        Ok(())
    }

    /// Get the registered table name for a specific event type
    ///
    /// Returns None if no table is registered for this event type.
    pub fn get_table_name<E>(&self) -> Option<String>
    where
        E: StatEvent + 'static + Send + Sync,
    {
        let type_id = TypeId::of::<E>();
        let tables = self.tables.lock().ok()?;
        tables.get(&type_id).map(|(name, _, _)| name.clone())
    }

    /// Get the registered table by table name
    ///
    /// Returns None if no table is registered with this name.
    pub fn get_table_by_name(&self, name: &str) -> Option<Arc<dyn ErasedStatTable>> {
        let tables_by_name = self.tables_by_name.lock().ok()?;
        tables_by_name.get(name).cloned()
    }

    /// Record a statistic event
    ///
    /// Automatically looks up the table registered for this event type.
    /// If no custom table registered, uses DefaultStatTable.
    /// Automatically flushes when buffer reaches batch_size.
    pub async fn record<E>(
        &self,
        _ctx: RequestContext,
        event: E,
    ) -> Result<()>
    where
        E: StatEvent + 'static + Send + Sync,
    {
        let type_id = TypeId::of::<E>();
        let need_flush = {
            let mut tables = self.tables.lock().map_err(|e| {
                Error::internal(format!("Failed to lock tables: {}", e))
            })?;
            let (_, _, buffer) = tables.get_mut(&type_id)
                .ok_or_else(|| Error::internal(format!("No table registered for event type {:?}", std::any::type_name::<E>())))?;

            buffer.push(event);
            buffer.len() >= self.batch_size
        };

        // Check if we need to flush
        if need_flush {
            self.flush_event_type::<E>()?;
        }

        Ok(())
    }

    /// Record with explicit table (backward compatibility / explicit override)
    pub async fn record_with_table<E, T>(
        &self,
        _ctx: RequestContext,
        _table: &T,
        event: E,
    ) -> Result<()>
    where
        E: StatEvent + 'static + Send + Sync,
        T: StatTable<E>,
    {
        self.record(_ctx, event).await
    }

    /// Flush all pending events in all tables
    pub async fn flush_all(
        &self,
        _ctx: RequestContext,
    ) -> Result<()> {
        // Get all type_ids first to avoid borrowing issue
        let type_ids: Vec<TypeId> = {
            let tables = self.tables.lock().map_err(|e| {
                Error::internal(format!("Failed to lock tables: {}", e))
            })?;
            tables.keys().cloned().collect()
        };
        for type_id in type_ids {
            self.flush_type_id(type_id)?;
        }
        Ok(())
    }

    /// Flush pending events for a specific event type
    fn flush_event_type<E>(&self) -> Result<()>
    where
        E: StatEvent + 'static,
    {
        let type_id = TypeId::of::<E>();
        self.flush_type_id(type_id)
    }

    /// Flush by type_id
    fn flush_type_id(&self, type_id: TypeId) -> Result<()> {
        // 持有 tables 锁期间完成 take events + 锁 conn + 批量插入。
        // 锁顺序固定为 tables → conn，避免死锁。
        let mut tables = self.tables.lock().map_err(|e| {
            Error::internal(format!("Failed to lock tables: {}", e))
        })?;
        let (_, erased, buffer) = tables.get_mut(&type_id)
            .ok_or_else(|| Error::internal(format!("No table registered for type id {:?}", type_id)))?;

        if buffer.is_empty() {
            return Ok(());
        }

        let events = buffer.take();

        let mut conn_guard = self.conn.lock().map_err(|e| {
            Error::internal(format!("Failed to lock connection: {}", e))
        })?;

        erased.bulk_insert_erased(&mut conn_guard, events)?;

        Ok(())
    }

    /// Get pending buffer length for an event type
    pub fn pending_buffer_len<E>(&self) -> usize
    where
        E: StatEvent + 'static,
    {
        let type_id = TypeId::of::<E>();
        self.tables.lock()
            .ok()
            .and_then(|tables| tables.get(&type_id).map(|(_, _, buf)| buf.len()))
            .unwrap_or(0)
    }

    /// Get number of registered event types (tables)
    pub fn registered_table_count(&self) -> usize {
        self.tables.lock()
            .map(|tables| tables.len())
            .unwrap_or(0)
    }

    /// Generic aggregation query with filters, grouping, and aggregations
    ///
    /// Works on the specified table where all model call events are stored
    /// with tags and metrics in JSON columns.
    /// If table_name is None, defaults to "default_events".
    pub async fn query_aggregation(
        &self,
        ctx: RequestContext,
        table_name: Option<&str>,
        filters: &[StatFilter],
        group_by: &[&str],
        aggregations: &[StatAggregation],
        time_range: Option<(i64, i64)>,
    ) -> Result<Vec<AggregationRow>> {
        let table = table_name.unwrap_or("default_events");
        let (sql, params) = self.build_aggregation_query(table, filters, group_by, aggregations, time_range)?;

        let json_rows = self.query(ctx, &sql, &params).await?;
        
        // Convert JSON rows to AggregationRow
        let mut result = Vec::with_capacity(json_rows.len());
        for json_row in json_rows {
            let obj = match json_row {
                JsonValue::Object(obj) => obj,
                _ => continue,
            };
            
            let mut groups = HashMap::new();
            let mut aggr_results = HashMap::new();
            
            for (key, value) in obj {
                if group_by.contains(&key.as_str()) {
                    groups.insert(key, value);
                } else {
                    // Convert to f64 for aggregation result
                    let f = match value {
                        JsonValue::Number(n) => n.as_f64().unwrap_or(0.0),
                        _ => 0.0,
                    };
                    aggr_results.insert(key, f);
                }
            }
            
            result.push(AggregationRow {
                groups,
                aggregations: aggr_results,
            });
        }
        
        Ok(result)
    }

    /// Build SQL query string and parameters from filters/grouping/aggregations
    fn build_aggregation_query(
        &self,
        table_name: &str,
        filters: &[StatFilter],
        group_by: &[&str],
        aggregations: &[StatAggregation],
        time_range: Option<(i64, i64)>,
    ) -> Result<(String, Vec<StatParam>)> {
        let table = self.get_table_by_name(table_name)
            .ok_or_else(|| Error::internal(format!("Table not found: {}", table_name)))?;
        let mut sql = String::from("SELECT ");
        let mut params: Vec<StatParam> = Vec::new();

        // Add group by columns
        for (i, col) in group_by.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!("{} AS {}", table.column_sql(col), col));
        }

        // Add aggregations
        if !group_by.is_empty() || !aggregations.is_empty() {
            if !group_by.is_empty() {
                sql.push_str(", ");
            }
        }
        for (i, agg) in aggregations.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            match agg {
                StatAggregation::Count => {
                    sql.push_str("COUNT(*) AS count");
                }
                StatAggregation::Sum(metric) => {
                    sql.push_str(&format!(
                        "COALESCE(SUM(CAST({} AS DOUBLE)), 0) AS {}",
                        table.metric_sql(metric), metric
                    ));
                }
                StatAggregation::Avg(metric) => {
                    sql.push_str(&format!(
                        "COALESCE(AVG(CAST({} AS DOUBLE)), 0) AS {}",
                        table.metric_sql(metric), metric
                    ));
                }
            }
        }

        // FROM clause
        sql.push_str(&format!(" FROM {} WHERE 1=1", table_name));

        // Add time range filter if provided
        if let Some((start, end)) = time_range {
            sql.push_str(" AND timestamp >= ? AND timestamp <= ?");
            params.push(StatParam::Int(start));
            params.push(StatParam::Int(end));
        }

        // Add other filters
        let mut sql = self.append_filters(sql, filters, &mut params, table.as_ref());

        // Add GROUP BY if needed
        if !group_by.is_empty() {
            sql.push_str(" GROUP BY ");
            for (i, col) in group_by.iter().enumerate() {
                if i > 0 {
                    sql.push_str(", ");
                }
                sql.push_str(col);
            }
            sql.push_str(" ORDER BY ");
            sql.push_str(group_by[0]);
        }

        Ok((sql, params))
    }

    /// Query time series data with specified interval
    ///
    /// If table_name is None, defaults to "default_events".
    pub async fn query_time_series(
        &self,
        ctx: RequestContext,
        table_name: Option<&str>,
        filters: &[StatFilter],
        interval: StatsInterval,
        time_range: (i64, i64),
    ) -> Result<Vec<TimeSeriesPoint>> {
        let table = table_name.unwrap_or("default_events");
        let table_meta = self.get_table_by_name(table)
            .ok_or_else(|| Error::internal(format!("Table not found: {}", table)))?;

        let truncate_func = match interval {
            StatsInterval::Hourly => "(timestamp / 3600000) * 3600000",
            StatsInterval::Daily => "(timestamp / 86400000) * 86400000",
        };

        let tokens_input_col = table_meta.metric_sql("tokens_input");
        let tokens_output_col = table_meta.metric_sql("tokens_output");

        let sql = format!(
            "SELECT
                {} AS interval_start,
                COALESCE(SUM(CAST({} AS DOUBLE)), 0) AS tokens_input,
                COALESCE(SUM(CAST({} AS DOUBLE)), 0) AS tokens_output,
                COUNT(*) AS call_count
             FROM {}
             WHERE timestamp >= ? AND timestamp <= ?",
            truncate_func, tokens_input_col, tokens_output_col, table
        );

        let mut params: Vec<StatParam> = vec![
            StatParam::Int(time_range.0),
            StatParam::Int(time_range.1),
        ];

        // Add additional filters
        let sql = self.append_filters(sql, filters, &mut params, table_meta.as_ref());

        // Group by interval
        let sql = format!("{} GROUP BY interval_start ORDER BY interval_start", sql);

        let json_rows = self.query(ctx, &sql, &params).await?;
        
        let mut result = Vec::with_capacity(json_rows.len());
        for json_row in json_rows {
            let obj = match json_row {
                JsonValue::Object(o) => o,
                _ => continue,
            };
            
            let interval_start = obj.get("interval_start")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let tokens_input = obj.get("tokens_input")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as u64;
            let tokens_output = obj.get("tokens_output")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as u64;
            let call_count = obj.get("call_count")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as u64;
            
            result.push(TimeSeriesPoint {
                interval_start,
                tokens_input,
                tokens_output,
                call_count,
            });
        }
        
        Ok(result)
    }

    /// Helper: append filter conditions to SQL and collect params
    fn append_filters(
        &self,
        mut sql: String,
        filters: &[StatFilter],
        params: &mut Vec<StatParam>,
        table: &dyn ErasedStatTable,
    ) -> String {
        for filter in filters {
            match filter {
                StatFilter::Equals { key, value } => {
                    sql.push_str(&format!(" AND {} = ?", table.filter_equals_sql(key)));
                    let s = match value {
                        JsonValue::String(s) => s.clone(),
                        _ => value.to_string(),
                    };
                    params.push(StatParam::Str(s));
                }
                StatFilter::Range { key, min, max } => {
                    if let Some(min_val) = min {
                        sql.push_str(&format!(
                            " AND CAST({} AS DOUBLE) >= ?",
                            table.filter_range_sql(key)
                        ));
                        params.push(StatParam::Double(*min_val));
                    }
                    if let Some(max_val) = max {
                        sql.push_str(&format!(
                            " AND CAST({} AS DOUBLE) <= ?",
                            table.filter_range_sql(key)
                        ));
                        params.push(StatParam::Double(*max_val));
                    }
                }
            }
        }
        sql
    }

    /// Execute a custom SQL query and return results as JSON rows
    pub async fn query(
        &self,
        _ctx: RequestContext,
        sql: &str,
        params: &[StatParam],
    ) -> Result<Vec<JsonValue>> {
        // 将类型安全的 StatParam 转换为 duckdb 需要的 &dyn ToSql
        // 此转换在同步作用域内完成，避免 dyn ToSql（非 Send）跨 .await 边界
        let param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();

        let mut conn_guard = self.conn.lock().map_err(|e| {
            Error::internal(format!("Failed to lock connection: {}", e))
        })?;

        let mut stmt = conn_guard.prepare(sql)
            .map_err(|e| Error::internal(format!("Failed to prepare query: {}", e)))?;

        // duckdb-rs 1.4: need to execute first before getting column info
        // collect rows in a block to drop rows before accessing column_count
        let raw_rows: Result<Vec<Vec<Option<Value>>>> = {
            let mut rows = match stmt.query(param_refs.as_slice()) {
                Ok(r) => r,
                Err(e) => return Err(Error::internal(format!("Failed to execute query: {}", e))),
            };

            let mut raw_rows = Vec::new();
            loop {
                match rows.next() {
                    Ok(None) => break,
                    Ok(Some(row)) => {
                        let mut raw_row = Vec::new();
                        for i in 0.. {
                            match row.get(i) {
                                Ok(cell) => raw_row.push(cell),
                                Err(_) => break,
                            }
                        }
                        raw_rows.push(raw_row);
                    }
                    Err(e) => return Err(Error::internal(format!("Failed to fetch row: {}", e))),
                }
            }

            Ok(raw_rows)
        };

        let raw_rows = raw_rows?;

        // Now rows are dropped, we can safely get column info because stmt has result set
        let column_count = stmt.column_count();
        let mut column_names = Vec::with_capacity(column_count);
        for i in 0..column_count {
            let name = stmt.column_name(i);
            let name = name.cloned().unwrap_or_default();
            column_names.push(name);
        }

        // Convert to JSON
        let mut json_rows = Vec::with_capacity(raw_rows.len());
        for raw_row in raw_rows {
            let mut json_row = JsonValue::Object(serde_json::Map::new());
            for (i, value) in raw_row.into_iter().enumerate() {
                let name = column_names[i].clone();
                let json_value = match value {
                    None => JsonValue::Null,
                    Some(v) => match v {
                        Value::Null => JsonValue::Null,
                        Value::Boolean(b) => JsonValue::Bool(b),
                        Value::TinyInt(i) => JsonValue::from(i as i64),
                        Value::SmallInt(i) => JsonValue::from(i as i64),
                        Value::Int(i) => JsonValue::from(i as i64),
                        Value::BigInt(i) => JsonValue::from(i),
                        Value::Float(f) => JsonValue::from(f as f64),
                        Value::Double(d) => JsonValue::from(d),
                        Value::Text(s) => JsonValue::String(s.clone()),
                        Value::Blob(_) => JsonValue::Null,
                        _ => JsonValue::Null,
                    },
                };
                json_row.as_object_mut().unwrap().insert(name, json_value);
            }
            json_rows.push(json_row);
        }

        Ok(json_rows)
    }
}