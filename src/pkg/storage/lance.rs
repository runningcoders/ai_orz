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

/// 单行数据 → RecordBatch（upsert / update_payload 共用，payload 平铺列随行落库）
#[allow(clippy::too_many_arguments)]
fn row_to_batch(
    id: &str,
    vector: &[f32],
    content_hash: String,
    payload_hash: String,
    embedding_model: String,
    indexed_at: i64,
    expire_at: Option<i64>,
    payload: &VectorPayload,
) -> Result<RecordBatch> {
    let dimensions = vector.len() as i32;
    // FixedSizeListArray 需要 Vec<Option<f32>> 格式
    let vector_with_options: Vec<Option<f32>> = vector.iter().map(|&v| Some(v)).collect();
    let p = payload;
    RecordBatch::try_new(
        lance_schema(dimensions),
        vec![
            StdArc::new(StringArray::from(vec![id.to_string()])),
            StdArc::new(
                FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                    vec![Some(vector_with_options)],
                    dimensions,
                ),
            ),
            StdArc::new(StringArray::from(vec![content_hash])),
            StdArc::new(StringArray::from(vec![payload_hash])),
            StdArc::new(StringArray::from(vec![embedding_model])),
            StdArc::new(Int64Array::from(vec![indexed_at])),
            StdArc::new(Int64Array::from(vec![expire_at])),
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
    .map_err(|e| common::error::Error::internal(format!("Arrow record batch error: {}", e)))
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

        // payload 平铺列 + payload_hash 随向量一起落库
        let batch = row_to_batch(
            id,
            &params.vector,
            params.content_hash.clone(),
            params.payload_hash.clone(),
            params.embedding_model.clone(),
            now,
            params.expire_at,
            &params.payload,
        )?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], lance_schema(dimensions));

        table
            .add(batches)
            .execute()
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB add error: {}", e)))?;

        Ok(())
    }

    async fn update_payload(
        &self,
        collection: &str,
        id: &str,
        payload: &crate::models::vector::VectorPayload,
    ) -> Result<()> {
        let table = self.get_or_create_table(collection, 0).await?;
        let filter_sql = format!("id = '{}'", escape_sql_str(id));

        // 1. 取回旧行（含向量 + 元信息）
        let stream = table
            .query()
            .only_if(filter_sql.clone())
            .limit(1)
            .execute()
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB execute error: {}", e)))?;
        let batches: Vec<RecordBatch> = stream.try_collect().await.map_err(|e| {
            common::error::Error::internal(format!("LanceDB collect results error: {}", e))
        })?;

        let mut old: Option<(Vec<f32>, VectorMeta)> = None;
        for batch in batches {
            // vector 列：FixedSizeList<Float32>
            if let Some(vcol) = batch.column_by_name("vector")
                && let Some(va) = vcol.as_any().downcast_ref::<FixedSizeListArray>()
                && va.len() > 0
            {
                let flat = va.value(0);
                if let Some(f) = flat.as_any().downcast_ref::<Float32Array>() {
                    let vec: Vec<f32> = (0..f.len()).map(|i| f.value(i)).collect();
                    if let Some(rows) = parse_lance_batch(&batch)
                        && let Some((row, _)) = rows.into_iter().next()
                    {
                        old = Some((vec, row.meta));
                    }
                }
            }
        }
        let Some((vector, meta)) = old else {
            return Ok(()); // 行不存在：no-op
        };

        // 2. delete + 重写（仅 payload 与 payload_hash 变化）
        table
            .delete(&filter_sql)
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB delete error: {}", e)))?;

        let dimensions = vector.len() as i32;
        let now = chrono::Utc::now().timestamp();
        let batch = row_to_batch(
            id,
            &vector,
            meta.content_hash,
            payload.hash(),
            meta.embedding_model,
            now,
            meta.expire_at,
            payload,
        )?;
        let batches = RecordBatchIterator::new(vec![Ok(batch)], lance_schema(dimensions));
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
        filter: Option<&crate::models::vector::VectorFilter>,
    ) -> Result<Vec<VectorSearchHit>> {
        let table = self
            .get_or_create_table(collection, query_vector.len() as i32)
            .await?;

        // 执行向量搜索 - 0.26 API 使用 vector_search
        let mut query = table
            .vector_search(query_vector)
            .map_err(|e| {
                common::error::Error::internal(format!("LanceDB vector_search error: {}", e))
            })?
            .limit(top_k as usize);
        // 谓词下推：Top-K 在满足谓词的候选集内选取（pre-filter）
        if let Some(f) = filter {
            query = query.only_if(f.to_sql_expr(|field| field.column_name().to_string()));
        }
        let stream = query
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
            .delete(&format!("id = '{}'", escape_sql_str(id)))
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

#[cfg(test)]
mod prefilter_tests {
    use super::*;
    use crate::models::vector::{
        FilterValue, VectorField, VectorFilter, VectorIndexParams, VectorPayload,
    };
    use crate::pkg::storage::vector::VectorStore;

    fn params(agent: &str, published: bool, dim: usize) -> VectorIndexParams {
        let payload = VectorPayload {
            agent_id: Some(agent.to_string()),
            is_published: Some(published),
            ..Default::default()
        };
        VectorIndexParams {
            vector: vec![1.0; dim],
            content_hash: format!("h-{agent}-{published}"),
            payload_hash: payload.hash(),
            payload,
            model_provider_id: "p".into(),
            embedding_model: "m".into(),
            expire_at: None,
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_lance_search_prefilter() {
        let dir = std::env::temp_dir().join(format!("lance_pf_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = LanceVectorStore::new(&dir).unwrap();
        store.init_collection("pf", 4).await.unwrap();
        store
            .upsert("pf", "a1", &params("agent-1", false, 4))
            .await
            .unwrap();
        store
            .upsert("pf", "b1", &params("agent-2", true, 4))
            .await
            .unwrap();
        store
            .upsert("pf", "b2", &params("agent-2", false, 4))
            .await
            .unwrap();

        // 无过滤：3 条
        let all = store
            .search("pf", &[1.0, 1.0, 1.0, 1.0], 10, None)
            .await
            .unwrap();
        assert_eq!(all.len(), 3);

        // agent-1 视角：仅 1 条
        let f = VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into()));
        let hits = store
            .search("pf", &[1.0, 1.0, 1.0, 1.0], 10, Some(&f))
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].row.id, "a1");
        assert_eq!(hits[0].row.payload.agent_id.as_deref(), Some("agent-1"));

        // OR 可见性：agent-1 自己的 + 已发布的
        let vis = VectorFilter::Any(vec![
            VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into())),
            VectorFilter::Eq(VectorField::IsPublished, FilterValue::Bool(true)),
        ]);
        let hits = store
            .search("pf", &[1.0, 1.0, 1.0, 1.0], 10, Some(&vis))
            .await
            .unwrap();
        assert_eq!(hits.len(), 2); // a1 + b1
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_lance_update_payload() {
        let dir = std::env::temp_dir().join(format!("lance_up_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = LanceVectorStore::new(&dir).unwrap();
        store.init_collection("up", 4).await.unwrap();
        store
            .upsert("up", "a1", &params("agent-1", false, 4))
            .await
            .unwrap();

        let mut new_p = VectorPayload::default();
        new_p.agent_id = Some("agent-1".into());
        new_p.is_published = Some(true); // 发布翻转场景
        store.update_payload("up", "a1", &new_p).await.unwrap();

        let row = store.get("up", "a1").await.unwrap().unwrap();
        assert_eq!(row.payload.is_published, Some(true));
        assert_eq!(row.meta.payload_hash, new_p.hash());
    }
}
