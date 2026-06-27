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