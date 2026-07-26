//! LanceDB 向量存储实现
//!
//! 基于 LanceDB 的高性能嵌入式向量数据库
//! - 纯 Rust 实现，零系统依赖
//! - 内置 HNSW 索引，支持百万级向量快速检索
//! - 持久化到磁盘，支持元数据过滤
//! - 单文件存储，跨平台完美支持

use crate::models::vector::{VectorIndexParams, VectorMeta, VectorRow, VectorSearchHit};
use arrow_array::types::Float32Type;
use arrow_array::{
    FixedSizeListArray, Float32Array, Int64Array, RecordBatch, RecordBatchIterator, StringArray,
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

    /// 获取或创建表（懒加载模式）
    async fn get_or_create_table(&self, collection: &str, dimensions: i32) -> Result<Arc<Table>> {
        // 先检查缓存
        {
            let tables = self.tables.read().await;
            if let Some(table) = tables.get(collection) {
                return Ok(table.clone());
            }
        }

        // 检查表是否已存在
        let table_names = self.db.table_names().execute().await.map_err(|e| {
            common::error::Error::internal(format!("LanceDB table names error: {}", e))
        })?;

        let table = if table_names.contains(&collection.to_string()) {
            // 打开已存在的表
            self.db
                .open_table(collection)
                .execute()
                .await
                .map_err(|e| {
                    common::error::Error::internal(format!("LanceDB open table error: {}", e))
                })?
        } else {
            // 创建新表 schema - 使用 FixedSizeListArray 替代 ListArray
            let schema = StdArc::new(Schema::new(vec![
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
                Field::new("embedding_model", DataType::Utf8, false),
                Field::new("indexed_at", DataType::Int64, false),
                Field::new("expire_at", DataType::Int64, true),
            ]));

            // 创建空表 - 新版 API 直接接受 schema
            self.db
                .create_empty_table(collection, schema.clone())
                .execute()
                .await
                .map_err(|e| {
                    common::error::Error::internal(format!("LanceDB create table error: {}", e))
                })?
        };

        let table_arc = Arc::new(table);

        // 缓存起来
        let mut tables = self.tables.write().await;
        tables.insert(collection.to_string(), table_arc.clone());

        Ok(table_arc)
    }
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

        // 先删除旧数据
        table
            .delete(&format!("id = '{}'", id))
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB delete error: {}", e)))?;

        // 创建 Arrow 记录批 - 使用 FixedSizeListArray 存储向量
        // FixedSizeListArray 需要 Vec<Option<f32>> 格式
        let vector_with_options: Vec<Option<f32>> =
            params.vector.iter().map(|&v| Some(v)).collect();

        let id_array = StringArray::from(vec![id.to_string()]);
        let vector_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            vec![Some(vector_with_options)],
            dimensions,
        );
        let hash_array = StringArray::from(vec![params.content_hash.clone()]);
        let model_array = StringArray::from(vec![params.embedding_model.clone()]);
        let indexed_at_array = Int64Array::from(vec![now]);
        let expire_at_array = Int64Array::from(vec![params.expire_at]);

        let schema = StdArc::new(Schema::new(vec![
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
            Field::new("embedding_model", DataType::Utf8, false),
            Field::new("indexed_at", DataType::Int64, false),
            Field::new("expire_at", DataType::Int64, true),
        ]));

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                StdArc::new(id_array),
                StdArc::new(vector_array),
                StdArc::new(hash_array),
                StdArc::new(model_array),
                StdArc::new(indexed_at_array),
                StdArc::new(expire_at_array),
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

        // 遍历结果，转换为 VectorSearchHit 标准结构
        for batch in results {
            // 获取所有列
            let id_col = batch.column_by_name("id");
            let hash_col = batch.column_by_name("content_hash");
            let model_col = batch.column_by_name("embedding_model");
            let indexed_at_col = batch.column_by_name("indexed_at");
            let expire_at_col = batch.column_by_name("expire_at");
            let dist_col = batch.column_by_name("_distance");

            if let (
                Some(id_col),
                Some(hash_col),
                Some(model_col),
                Some(indexed_at_col),
                Some(expire_at_col),
                Some(dist_col),
            ) = (
                id_col,
                hash_col,
                model_col,
                indexed_at_col,
                expire_at_col,
                dist_col,
            ) {
                let id_array = id_col.as_any().downcast_ref::<StringArray>().unwrap();
                let hash_array = hash_col.as_any().downcast_ref::<StringArray>().unwrap();
                let model_array = model_col.as_any().downcast_ref::<StringArray>().unwrap();
                let indexed_at_array = indexed_at_col
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .unwrap();
                let expire_at_array = expire_at_col.as_any().downcast_ref::<Int64Array>().unwrap();
                let dist_array = dist_col.as_any().downcast_ref::<Float32Array>().unwrap();

                for (i, expire_at_val) in expire_at_array.iter().enumerate() {
                    output.push(VectorSearchHit {
                        row: VectorRow {
                            id: id_array.value(i).to_string(),
                            vector: Vec::new(), // LanceDB 搜索结果不返回原始向量
                            meta: VectorMeta {
                                content_hash: hash_array.value(i).to_string(),
                                embedding_model: model_array.value(i).to_string(),
                                indexed_at: indexed_at_array.value(i),
                                expire_at: expire_at_val,
                            },
                        },
                        distance: dist_array.value(i),
                    });
                }
            }
        }

        Ok(output)
    }

    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRow>> {
        let table = self.get_or_create_table(collection, 0).await?;

        // 查询指定 id 的完整记录
        let stream = table
            .query()
            .only_if(&format!("id = '{}'", id))
            .limit(1)
            .execute()
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB execute error: {}", e)))?;

        let results: Vec<RecordBatch> = stream.try_collect().await.map_err(|e| {
            common::error::Error::internal(format!("LanceDB collect results error: {}", e))
        })?;

        for batch in results {
            if batch.num_rows() > 0 {
                if let (
                    Some(id_col),
                    Some(hash_col),
                    Some(model_col),
                    Some(indexed_at_col),
                    Some(expire_at_col),
                ) = (
                    batch.column_by_name("id"),
                    batch.column_by_name("content_hash"),
                    batch.column_by_name("embedding_model"),
                    batch.column_by_name("indexed_at"),
                    batch.column_by_name("expire_at"),
                ) {
                    let id_array = id_col.as_any().downcast_ref::<StringArray>().unwrap();
                    let hash_array = hash_col.as_any().downcast_ref::<StringArray>().unwrap();
                    let model_array = model_col.as_any().downcast_ref::<StringArray>().unwrap();
                    let indexed_at_array = indexed_at_col
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .unwrap();
                    let expire_at_array =
                        expire_at_col.as_any().downcast_ref::<Int64Array>().unwrap();

                    let expire_at_val = expire_at_array.iter().next().flatten();

                    return Ok(Some(VectorRow {
                        id: id_array.value(0).to_string(),
                        vector: Vec::new(), // 不返回原始向量（LanceDB 查询需要单独获取）
                        meta: VectorMeta {
                            content_hash: hash_array.value(0).to_string(),
                            embedding_model: model_array.value(0).to_string(),
                            indexed_at: indexed_at_array.value(0),
                            expire_at: expire_at_val,
                        },
                    }));
                }
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

        let mut tables = self.tables.write().await;
        tables.remove(collection);

        Ok(())
    }
}
