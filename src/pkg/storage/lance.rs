//! LanceDB 向量存储实现
//!
//! 基于 LanceDB 的高性能嵌入式向量数据库
//! - 纯 Rust 实现，零系统依赖
//! - 内置 HNSW 索引，支持百万级向量快速检索
//! - 持久化到磁盘，支持元数据过滤
//! - 单文件存储，跨平台完美支持

use crate::models::vector::{
    VectorIndexParams, VectorMeta, VectorPayload, VectorRow, VectorSearchHit,
};
use arrow_array::Array;
use arrow_array::types::Float32Type;
use arrow_array::{
    BooleanArray, FixedSizeListArray, Float32Array, Int64Array, RecordBatch, RecordBatchIterator,
    StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use common::error::Result;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{Connection, Table, connect};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::Arc as StdArc;
use tokio::sync::RwLock;

/// LanceDB 向量存储
///
/// 基于 LanceDB 的高性能向量数据库
#[derive(Clone)]
pub struct LanceVectorStore {
    db: Connection,
    tables: Arc<RwLock<HashMap<String, Arc<Table>>>>,
}

impl std::fmt::Debug for LanceVectorStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanceVectorStore")
            .field("tables", &self.tables)
            .finish()
    }
}

impl LanceVectorStore {
    /// 创建新的 LanceDB 向量存储
    pub fn new<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let base_path = base_path.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_path)?;

        // 使用 block_in_place 执行异步初始化
        let db = tokio::task::block_in_place(move || {
            tokio::runtime::Handle::current().block_on(async move {
                let path_str = base_path.to_str().unwrap_or_default();
                connect(path_str).execute().await
            })
        })
        .map_err(|e| common::error::Error::internal(format!("LanceDB connect error: {}", e)))?;

        Ok(Self {
            db,
            tables: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 将通用「collection 名」安全映射为 LanceDB 合法 table 名。
    ///
    /// 上层命名约定允许使用冒号作域分隔符（如 `memory:short_term`、`agent:profile`），
    /// 但 LanceDB 0.26 的表名只接受字母数字、下划线、连字符、点，且内部会 unwrap
    /// `InvalidTableName` 直接导致运行时 panic。这里在存储层做一次性过滤，避免
    /// 调用方在各 VectorStore 实现间感知差异（其它实现如 SQLite/HNSW 对冒号更宽容）。
    fn sanitize_table_name(collection: &str) -> String {
        let mut out = String::with_capacity(collection.len());
        for ch in collection.chars() {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') {
                out.push(ch);
            } else {
                out.push('_');
            }
        }
        // 保证至少有一个合法字符，避免后续 Lance 再炸
        if out.is_empty() {
            out.push('t');
        }
        out
    }

    /// 获取或创建表（懒加载模式）
    async fn get_or_create_table(&self, collection: &str, dimensions: i32) -> Result<Arc<Table>> {
        let table_name = Self::sanitize_table_name(collection);

        // 先检查缓存
        {
            let tables = self.tables.read().await;
            if let Some(table) = tables.get(&table_name) {
                return Ok(table.clone());
            }
        }

        // 检查表是否已存在
        let table_names = self.db.table_names().execute().await.map_err(|e| {
            common::error::Error::internal(format!("LanceDB table names error: {}", e))
        })?;

        let table = if table_names.iter().any(|t| t == &table_name) {
            // 打开已存在的表
            self.db
                .open_table(&table_name)
                .execute()
                .await
                .map_err(|e| {
                    common::error::Error::internal(format!("LanceDB open table error: {}", e))
                })?
        } else {
            // 创建新表（统一 schema：基础元信息列 + payload 平铺列）
            self.db
                .create_empty_table(&table_name, lance_schema(dimensions))
                .execute()
                .await
                .map_err(|e| {
                    common::error::Error::internal(format!("LanceDB create table error: {}", e))
                })?
        };

        let table_arc = Arc::new(table);

        // 缓存起来（key 使用清洗后的合法表名，与 clear_collection 同步）
        let mut tables = self.tables.write().await;
        tables.insert(table_name.clone(), table_arc.clone());

        Ok(table_arc)
    }
}

/// SQL 字符串字面量转义（单引号双写）
fn escape_sql_str(s: &str) -> String {
    s.replace('\'', "''")
}

/// 统一表 schema：基础元信息列 + payload 平铺列（全部 nullable，稀疏无成本）
fn lance_schema(dimensions: i32) -> StdArc<Schema> {
    let payload_columns = [
        ("org_id", DataType::Utf8),
        ("agent_id", DataType::Utf8),
        ("project_id", DataType::Utf8),
        ("task_id", DataType::Utf8),
        ("from_id", DataType::Utf8),
        ("to_id", DataType::Utf8),
        ("entity_type", DataType::Utf8),
        ("status", DataType::Utf8),
        ("is_published", DataType::Boolean),
        ("tags", DataType::Utf8),
    ];
    let mut fields = vec![
        Field::new("id", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                StdArc::new(Field::new("item", DataType::Float32, true)),
                dimensions,
            ),
            false,
        ),
        Field::new("content_hash", DataType::Utf8, false),
        Field::new("payload_hash", DataType::Utf8, false),
        Field::new("embedding_model", DataType::Utf8, false),
        Field::new("indexed_at", DataType::Int64, false),
        Field::new("expire_at", DataType::Int64, true),
    ];
    for (name, ty) in payload_columns {
        fields.push(Field::new(name, ty, true));
    }
    StdArc::new(Schema::new(fields))
}

/// 解析 Lance RecordBatch → (VectorRow, Option<distance>)
///
/// search/get 共用；`_distance` 列仅 vector_search 结果携带，普通 query 路径解析为 None
fn parse_lance_batch(batch: &RecordBatch) -> Option<Vec<(VectorRow, Option<f32>)>> {
    let str_col = |name: &str| -> Option<Vec<Option<String>>> {
        batch
            .column_by_name(name)
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .map(|a| {
                (0..a.len())
                    .map(|i| {
                        if a.is_null(i) {
                            None
                        } else {
                            Some(a.value(i).to_string())
                        }
                    })
                    .collect()
            })
    };
    let i64_col = |name: &str| -> Option<Vec<Option<i64>>> {
        batch
            .column_by_name(name)
            .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
            .map(|a| {
                (0..a.len())
                    .map(|i| if a.is_null(i) { None } else { Some(a.value(i)) })
                    .collect()
            })
    };

    let ids = str_col("id")?;
    let hashes = str_col("content_hash")?;
    let payload_hashes = str_col("payload_hash")?;
    let models = str_col("embedding_model")?;
    let indexed_ats = i64_col("indexed_at")?;
    let expire_ats = i64_col("expire_at")?;
    let distances: Vec<Option<f32>> = batch
        .column_by_name("_distance")
        .and_then(|c| c.as_any().downcast_ref::<Float32Array>())
        .map(|a| {
            (0..a.len())
                .map(|i| if a.is_null(i) { None } else { Some(a.value(i)) })
                .collect()
        })
        .unwrap_or_default();

    // payload 平铺列：先取出一次，避免循环内重复扫描
    let org_ids = str_col("org_id");
    let agent_ids = str_col("agent_id");
    let project_ids = str_col("project_id");
    let task_ids = str_col("task_id");
    let from_ids = str_col("from_id");
    let to_ids = str_col("to_id");
    let entity_types = str_col("entity_type");
    let statuses = str_col("status");
    let tags = str_col("tags");
    let is_published_col = batch
        .column_by_name("is_published")
        .and_then(|c| c.as_any().downcast_ref::<BooleanArray>());

    let mut rows = Vec::with_capacity(ids.len());
    for i in 0..ids.len() {
        let payload = VectorPayload {
            org_id: org_ids.as_ref().and_then(|c| c.get(i).cloned()).flatten(),
            agent_id: agent_ids.as_ref().and_then(|c| c.get(i).cloned()).flatten(),
            project_id: project_ids
                .as_ref()
                .and_then(|c| c.get(i).cloned())
                .flatten(),
            task_id: task_ids.as_ref().and_then(|c| c.get(i).cloned()).flatten(),
            from_id: from_ids.as_ref().and_then(|c| c.get(i).cloned()).flatten(),
            to_id: to_ids.as_ref().and_then(|c| c.get(i).cloned()).flatten(),
            entity_type: entity_types
                .as_ref()
                .and_then(|c| c.get(i).cloned())
                .flatten(),
            status: statuses.as_ref().and_then(|c| c.get(i).cloned()).flatten(),
            is_published: is_published_col
                .and_then(|a| if a.is_null(i) { None } else { Some(a.value(i)) }),
            tags: tags.as_ref().and_then(|c| c.get(i).cloned()).flatten(),
        };
        rows.push((
            VectorRow {
                id: ids[i].clone()?,
                vector: Vec::new(), // LanceDB 搜索结果不返回原始向量（保持现状语义）
                payload,
                meta: VectorMeta {
                    content_hash: hashes[i].clone()?,
                    payload_hash: payload_hashes[i].clone()?,
                    embedding_model: models[i].clone()?,
                    indexed_at: indexed_ats[i]?,
                    expire_at: expire_ats[i],
                },
            },
            distances.get(i).copied().flatten(),
        ));
    }
    Some(rows)
}

#[async_trait]
impl super::VectorStore for LanceVectorStore {
    async fn init_collection(&self, collection: &str, dimensions: i32) -> Result<()> {
        // 初始化表（如果不存在则创建）
        self.get_or_create_table(collection, dimensions).await?;
        Ok(())
    }

    async fn upsert(&self, collection: &str, id: &str, params: &VectorIndexParams) -> Result<()> {
        let dimensions = params.vector.len() as i32;
        let table = self.get_or_create_table(collection, dimensions).await?;

        let now = chrono::Utc::now().timestamp();

        // 先删除旧数据（id 单引号转义，防注入）
        table
            .delete(&format!("id = '{}'", escape_sql_str(id)))
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB delete error: {}", e)))?;

        // 创建 Arrow 记录批 - 使用 FixedSizeListArray 存储向量
        // FixedSizeListArray 需要 Vec<Option<f32>> 格式
        let vector_with_options: Vec<Option<f32>> =
            params.vector.iter().map(|&v| Some(v)).collect();

        // payload 平铺列 + payload_hash 随向量一起落库
        let p = &params.payload;
        let schema = lance_schema(dimensions);
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                StdArc::new(StringArray::from(vec![id.to_string()])),
                StdArc::new(
                    FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                        vec![Some(vector_with_options)],
                        dimensions,
                    ),
                ),
                StdArc::new(StringArray::from(vec![params.content_hash.clone()])),
                StdArc::new(StringArray::from(vec![params.payload_hash.clone()])),
                StdArc::new(StringArray::from(vec![params.embedding_model.clone()])),
                StdArc::new(Int64Array::from(vec![now])),
                StdArc::new(Int64Array::from(vec![params.expire_at])),
                StdArc::new(StringArray::from(vec![p.org_id.clone()])),
                StdArc::new(StringArray::from(vec![p.agent_id.clone()])),
                StdArc::new(StringArray::from(vec![p.project_id.clone()])),
                StdArc::new(StringArray::from(vec![p.task_id.clone()])),
                StdArc::new(StringArray::from(vec![p.from_id.clone()])),
                StdArc::new(StringArray::from(vec![p.to_id.clone()])),
                StdArc::new(StringArray::from(vec![p.entity_type.clone()])),
                StdArc::new(StringArray::from(vec![p.status.clone()])),
                StdArc::new(BooleanArray::from(vec![p.is_published])),
                StdArc::new(StringArray::from(vec![p.tags.clone()])),
            ],
        )
        .map_err(|e| common::error::Error::internal(format!("Arrow record batch error: {}", e)))?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);

        table
            .add(batches)
            .execute()
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB add error: {}", e)))?;

        Ok(())
    }

    async fn search(
        &self,
        collection: &str,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>> {
        let table = self
            .get_or_create_table(collection, query_vector.len() as i32)
            .await?;

        // 执行向量搜索 - 0.26 API 使用 vector_search
        let stream = table
            .vector_search(query_vector)
            .map_err(|e| {
                common::error::Error::internal(format!("LanceDB vector_search error: {}", e))
            })?
            .limit(top_k as usize)
            .execute()
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB execute error: {}", e)))?;

        let results: Vec<RecordBatch> = stream.try_collect().await.map_err(|e| {
            common::error::Error::internal(format!("LanceDB collect results error: {}", e))
        })?;

        let mut output = Vec::new();

        // 遍历结果，统一走 parse_lance_batch 解析（含 payload 平铺列）
        for batch in results {
            for (row, dist) in parse_lance_batch(&batch).unwrap_or_default() {
                if let Some(distance) = dist {
                    output.push(VectorSearchHit { row, distance });
                }
            }
        }

        Ok(output)
    }

    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRow>> {
        let table = self.get_or_create_table(collection, 0).await?;

        // 查询指定 id 的完整记录（id 单引号转义，防注入）
        let stream = table
            .query()
            .only_if(format!("id = '{}'", escape_sql_str(id)))
            .limit(1)
            .execute()
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB execute error: {}", e)))?;

        let results: Vec<RecordBatch> = stream.try_collect().await.map_err(|e| {
            common::error::Error::internal(format!("LanceDB collect results error: {}", e))
        })?;

        // 统一走 parse_lance_batch 解析（含 payload 平铺列），取首行
        for batch in results {
            if batch.num_rows() > 0
                && let Some(first) =
                    parse_lance_batch(&batch).and_then(|rows| rows.into_iter().next())
                && let (row, _) = first
            {
                return Ok(Some(row));
            }
        }

        Ok(None)
    }

    async fn delete(&self, collection: &str, id: &str) -> Result<()> {
        let table = self.get_or_create_table(collection, 0).await?;
        table
            .delete(&format!("id = '{}'", id))
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB delete error: {}", e)))?;
        Ok(())
    }

    async fn clear_collection(&self, collection: &str) -> Result<()> {
        let table = self.get_or_create_table(collection, 0).await?;
        table
            .delete("TRUE")
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB clear error: {}", e)))?;

        let table_name = Self::sanitize_table_name(collection);
        let mut tables = self.tables.write().await;
        tables.remove(&table_name);

        Ok(())
    }
}
