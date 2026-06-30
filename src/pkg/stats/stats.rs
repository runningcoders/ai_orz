//! Top-level Stats struct implementation

use std::collections::HashMap;
use std::sync::Mutex;
use std::any::TypeId;

use common::error::{Error, Result};
use duckdb::{Connection, ToSql};
use duckdb::types::Value;
use serde_json::{self, Value as JsonValue};

use crate::pkg::request_context::RequestContext;
use super::erased::{ErasedBuffer, ErasedStatTable, ErasedWrapper};
use super::traits::{StatEvent, StatTable};
use super::default::{DefaultStatEvent, DefaultStatTable};

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

/// Time series interval for grouping data
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub enum StatsInterval {
    /// Group by hour
    Hourly,
    /// Group by day
    Daily,
}

/// Time series data point
#[derive(Debug, Clone, serde::Serialize)]
pub struct TimeSeriesPoint {
    /// Start timestamp of this interval (millis)
    pub interval_start: i64,
    /// Total input tokens
    pub tokens_input: u64,
    /// Total output tokens
    pub tokens_output: u64,
    /// Number of calls
    pub call_count: u64,
}

/// Total token sum result
#[derive(Debug, Clone, serde::Serialize)]
pub struct TokenSumResult {
    /// Total input tokens
    pub total_tokens_input: u64,
    /// Total output tokens
    pub total_tokens_output: u64,
    /// Total number of calls
    pub total_calls: u64,
}

/// Top-level Stats instance
///
/// Manages multiple statistic tables, provides automatic batching and flushing.
/// Each event type is bound to exactly one table.
#[derive(Debug)]
pub struct Stats {
    /// DuckDB connection
    conn: Mutex<Connection>,
    /// Batch size for automatic flush
    batch_size: usize,
    /// Registered tables: event TypeId → (table name, erased table, erased buffer)
    tables: HashMap<TypeId, (String, Box<dyn ErasedStatTable>, ErasedBuffer)>,
}

impl Stats {
    /// Open a new Stats database
    pub async fn open(path: &str, batch_size: usize) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| Error::internal(format!("Failed to open DuckDB: {}", e)))?;

        Ok(Self {
            conn: Mutex::new(conn),
            batch_size,
            tables: HashMap::new(),
        })
    }

    /// Initialize default table for DefaultStatEvent
    pub fn initialize_default(&mut self) -> Result<()> {
        let table = DefaultStatTable;
        self.register_table(table)?;
        Ok(())
    }

    /// Register a statistic table for a specific event type
    ///
    /// Each event type can be registered to exactly one table.
    /// If registered before, it will be replaced.
    pub fn register_table<E, T>(&mut self, table: T) -> Result<()>
    where
        E: StatEvent + 'static + Send + Sync,
        T: StatTable<E> + 'static,
    {
        let type_id = TypeId::of::<E>();
        let table_name = table.table_name().to_string();
        let erased = ErasedWrapper {
            table,
            _marker: std::marker::PhantomData,
        };

        // Create table if not exists
        let mut conn_guard = self.conn.lock().map_err(|e| {
            Error::internal(format!("Failed to lock connection: {}", e))
        })?;
        erased.create_table(&mut conn_guard)?;

        self.tables.insert(
            type_id,
            (table_name, Box::new(erased), ErasedBuffer::new())
        );

        Ok(())
    }

    /// Record a statistic event
    ///
    /// Automatically looks up the table registered for this event type.
    /// If no custom table registered, uses DefaultStatTable.
    /// Automatically flushes when buffer reaches batch_size.
    pub async fn record<E>(
        &mut self,
        _ctx: RequestContext,
        event: E,
    ) -> Result<()>
    where
        E: StatEvent + 'static + Send + Sync,
    {
        let type_id = TypeId::of::<E>();
        let (_, _, buffer) = self.tables.get_mut(&type_id)
            .ok_or_else(|| Error::internal(format!("No table registered for event type {:?}", std::any::type_name::<E>())))?;

        buffer.push(event);

        // Check if we need to flush
        if buffer.len() >= self.batch_size {
            self.flush_event_type::<E>()?;
        }

        Ok(())
    }

    /// Record with explicit table (backward compatibility / explicit override)
    pub async fn record_with_table<E, T>(
        &mut self,
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
        &mut self,
        _ctx: RequestContext,
    ) -> Result<()> {
        // Get all type_ids first to avoid borrowing issue
        let type_ids: Vec<TypeId> = self.tables.keys().cloned().collect();
        for type_id in type_ids {
            self.flush_type_id(type_id)?;
        }
        Ok(())
    }

    /// Flush pending events for a specific event type
    fn flush_event_type<E>(&mut self) -> Result<()>
    where
        E: StatEvent + 'static,
    {
        let type_id = TypeId::of::<E>();
        self.flush_type_id(type_id)
    }

    /// Flush by type_id
    fn flush_type_id(&mut self, type_id: TypeId) -> Result<()> {
        let (_, erased, buffer) = self.tables.get_mut(&type_id)
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
        self.tables.get(&type_id)
            .map(|(_, _, buf)| buf.len())
            .unwrap_or(0)
    }

    /// Get number of registered event types (tables)
    pub fn registered_table_count(&self) -> usize {
        self.tables.len()
    }

    /// Generic aggregation query with filters, grouping, and aggregations
    ///
    /// Works on the default_events table where all model call events are stored
    /// with tags and metrics in JSON columns.
    pub async fn query_aggregation(
        &self,
        ctx: RequestContext,
        filters: &[StatFilter],
        group_by: &[&str],
        aggregations: &[StatAggregation],
        time_range: Option<(i64, i64)>,
    ) -> Result<Vec<AggregationRow>> {
        // Build SQL query
        let (sql, params) = self.build_aggregation_query(filters, group_by, aggregations, time_range);
        
        // Execute query using existing query method - convert Vec<Box<dyn ToSql>> to &[&dyn ToSql]
        let mut param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let json_rows = self.query(ctx, &sql, &param_refs).await?;
        
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
        filters: &[StatFilter],
        group_by: &[&str],
        aggregations: &[StatAggregation],
        time_range: Option<(i64, i64)>,
    ) -> (String, Vec<Box<dyn ToSql>>) {
        let mut sql = String::from("SELECT ");
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();
        
        // Add group by columns
        for (i, col) in group_by.iter().enumerate() {
            if i > 0 {
                sql.push_str(", ");
            }
            // All group by columns are from tags -> extract from JSON
            sql.push_str(&format!("json_extract(tags, '$.{}') AS {}", col, col));
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
                        "COALESCE(SUM(CAST(json_extract(metrics, '$.{}') AS DOUBLE)), 0) AS {}",
                        metric, metric
                    ));
                }
                StatAggregation::Avg(metric) => {
                    sql.push_str(&format!(
                        "COALESCE(AVG(CAST(json_extract(metrics, '$.{}') AS DOUBLE)), 0) AS {}",
                        metric, metric
                    ));
                }
            }
        }
        
        // FROM clause
        sql.push_str(" FROM default_events WHERE 1=1");
        
        // Add time range filter if provided
        if let Some((start, end)) = time_range {
            sql.push_str(" AND timestamp >= ? AND timestamp <= ?");
            params.push(Box::new(start));
            params.push(Box::new(end));
        }
        
        // Add other filters
        for filter in filters {
            match filter {
                StatFilter::Equals { key, value } => {
                    // For equality on JSON field
                    sql.push_str(&format!(
                        " AND json_extract(tags, '$.{}') = ?",
                        key
                    ));
                    // DuckDB needs string literal for JSON comparison
                    let json_str = value.to_string();
                    params.push(Box::new(json_str));
                }
                StatFilter::Range { key, min, max } => {
                    // Range comparison on JSON field
                    if min.is_some() {
                        sql.push_str(&format!(
                            " AND CAST(json_extract(tags, '$.{}') AS DOUBLE) >= ?",
                            key
                        ));
                        params.push(Box::new(min.unwrap()));
                    }
                    if max.is_some() {
                        sql.push_str(&format!(
                            " AND CAST(json_extract(tags, '$.{}') AS DOUBLE) <= ?",
                            key
                        ));
                        params.push(Box::new(max.unwrap()));
                    }
                }
            }
        }
        
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
            // Order by first group column
            sql.push_str(group_by[0]);
        }
        
        (sql, params)
    }

    /// Query time series data with specified interval
    pub async fn query_time_series(
        &self,
        ctx: RequestContext,
        filters: &[StatFilter],
        interval: StatsInterval,
        time_range: (i64, i64),
    ) -> Result<Vec<TimeSeriesPoint>> {
        // Truncate timestamp to interval boundary
        let truncate_func = match interval {
            StatsInterval::Hourly => "(timestamp / 3600000) * 3600000", // millis -> hour -> millis
            StatsInterval::Daily => "(timestamp / 86400000) * 86400000", // millis -> day -> millis
        };
        
        // We group by truncated timestamp and aggregate
        let sql = format!(
            "SELECT
                {} AS interval_start,
                COALESCE(SUM(CAST(json_extract(metrics, '$.tokens_input') AS DOUBLE)), 0) AS tokens_input,
                COALESCE(SUM(CAST(json_extract(metrics, '$.tokens_output') AS DOUBLE)), 0) AS tokens_output,
                COUNT(*) AS call_count
             FROM default_events
             WHERE timestamp >= ? AND timestamp <= ?",
            truncate_func
        );
        
        let mut params: Vec<Box<dyn ToSql>> = vec![
            Box::new(time_range.0),
            Box::new(time_range.1),
        ];
        
        // Add additional filters
        let sql = self.append_filters(sql, filters, &mut params);
        
        // Group by interval
        let sql = format!("{} GROUP BY interval_start ORDER BY interval_start", sql);
        
        // Convert Vec<Box<dyn ToSql>> to &[&dyn ToSql]
        let mut param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let json_rows = self.query(ctx, &sql, &param_refs).await?;
        
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
        params: &mut Vec<Box<dyn ToSql>>,
    ) -> String {
        for filter in filters {
            match filter {
                StatFilter::Equals { key, value } => {
                    sql.push_str(&format!(
                        " AND json_extract(tags, '$.{}') = ?",
                        key
                    ));
                    let json_str = value.to_string();
                    params.push(Box::new(json_str));
                }
                StatFilter::Range { key, min, max } => {
                    if min.is_some() {
                        sql.push_str(&format!(
                            " AND CAST(json_extract(tags, '$.{}') AS DOUBLE) >= ?",
                            key
                        ));
                        params.push(Box::new(min.unwrap()));
                    }
                    if max.is_some() {
                        sql.push_str(&format!(
                            " AND CAST(json_extract(tags, '$.{}') AS DOUBLE) <= ?",
                            key
                        ));
                        params.push(Box::new(max.unwrap()));
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
        params: &[&dyn ToSql],
    ) -> Result<Vec<JsonValue>> {
        let mut conn_guard = self.conn.lock().map_err(|e| {
            Error::internal(format!("Failed to lock connection: {}", e))
        })?;

        let mut stmt = conn_guard.prepare(sql)
            .map_err(|e| Error::internal(format!("Failed to prepare query: {}", e)))?;

        // duckdb-rs 1.4: need to execute first before getting column info
        // collect rows in a block to drop rows before accessing column_count
        let raw_rows: Result<Vec<Vec<Option<Value>>>> = {
            let mut rows = match stmt.query(params) {
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