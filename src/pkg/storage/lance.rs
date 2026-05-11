//! LanceDB 向量存储实现
//!
//! 基于 LanceDB 的高性能嵌入式向量数据库
//! - 纯 Rust 实现，零系统依赖
//! - 内置 HNSW 索引，支持百万级向量快速检索
//! - 持久化到磁盘，支持元数据过滤
//! - 单文件存储，跨平台完美支持

use async_trait::async_trait;
use crate::error::Result;
use lancedb::{connect, Table};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use arrow_array::{RecordBatchIterator, RecordBatch, Float32Array, StringArray, Int64Array};
use arrow_schema::{Schema, Field, DataType};
use std::sync::Arc as StdArc;

/// LanceDB 向量存储
///
/// 基于 LanceDB 的高性能向量数据库
#[derive(Clone, Debug)]
pub struct LanceVectorStore {
    base_path: PathBuf,
    tables: Arc<RwLock<HashMap<String, Arc<Table>>>>,
}

impl LanceVectorStore {
    /// 创建新的 LanceDB 向量存储
    pub fn new<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let base_path = base_path.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_path)?;
        
        Ok(Self {
            base_path,
            tables: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 获取表路径
    fn table_path(&self, collection: &str) -> PathBuf {
        self.base_path.join(collection)
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
        
        // 创建或打开表
        let table_path = self.table_path(collection);
        
        let table = if table_path.exists() {
            // 打开已存在的表
            connect(&table_path).await?.open().await?
        } else {
            // 创建新表 schema
            let schema = StdArc::new(Schema::new(vec![
                Field::new("id", DataType::Utf8, false),
                Field::new("vector", DataType::List(StdArc::new(Field::new("item", DataType::Float32, false)), false),
                Field::new("content_hash", DataType::Utf8, false),
                Field::new("embedding_model", DataType::Utf8, false),
                Field::new("indexed_at", DataType::Int64, false),
                Field::new("expire_at", DataType::Int64, true),
            ]));
            
            // 创建空表（需要至少一批空数据
            let batches = RecordBatchIterator::new(
                vec![], schema.clone());
            
            connect(&table_path).await?.create_empty(collection, batches).await?
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

    async fn upsert(
        &self, 
        collection: &str, 
        id: &str, 
        vector: &[f32],
        content_hash: &str,
        embedding_model: &str,
        expire_at: Option<i64>,
    ) -> Result<()> {
        let table = self.get_or_create_table(collection, vector.len() as i32).await?;
        
        let now = chrono::Utc::now().timestamp();
        
        // 先删除旧数据
        let _ = table.delete(&format!("id = '{}'", id)).await;
        
        // 创建 Arrow 记录批
        let id_array = StringArray::from(vec![id.to_string()]);
        let vector_array = Float32Array::from(vec![vector.to_vec()]);
        let hash_array = StringArray::from(vec![content_hash.to_string()]);
        let model_array = StringArray::from(vec![embedding_model.to_string()]);
        let indexed_at_array = Int64Array::from(vec![now]);
        let expire_at_array = Int64Array::from(vec![expire_at]);
        
        let schema = StdArc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("vector", DataType::List(StdArc::new(Field::new("item", DataType::Float32, false))), false),
            Field::new("content_hash", DataType::Utf8, false),
            Field::new("embedding_model", DataType::Utf8, false),
            Field::new("indexed_at", DataType::Int64, false),
            Field::new("expire_at", DataType::Int64, true),
        ]));
        
        let batch = RecordBatch::try_new(
            schema,
            vec![
                StdArc::new(id_array),
                StdArc::new(vector_array),
                StdArc::new(hash_array),
                StdArc::new(model_array),
                StdArc::new(indexed_at_array),
                StdArc::new(expire_at_array),
            ],
        )?;
        
        let batches = RecordBatchIterator::new(vec![batch.schema().clone()], batch.schema());
        
        table.add(batches).await?;
        
        Ok(())
    }

    async fn search(
        &self, 
        collection: &str, 
        query_vector: &[f32], 
        top_k: i32,
    ) -> Result<Vec<(String, f32)>> {
        let table = self.get_or_create_table(collection, query_vector.len() as i32).await?;
        
        // 执行向量搜索
        let results = table
            .vector_search(query_vector)?
            .limit(Some(top_k as usize))
            .execute()
            .await?;
        
        let mut output = Vec::new();
        
        // 遍历结果
        for batch in results {
            let batch = batch?;
            
            // 获取 id 列
            if let Some(id_col) = batch.column_by_name("id") {
                let id_array = id_col.as_any().downcast_ref::<StringArray>().unwrap();
                
                // 获取距离列（LanceDB 返回的距离列名是 "_distance"
                if let Some(dist_col) = batch.column_by_name("_distance") {
                    let dist_array = dist_col.as_any().downcast_ref::<Float32Array>().unwrap();
                    
                    for i in 0..batch.num_rows() {
                        output.push((
                            id_array.value(i).to_string(),
                            dist_array.value(i),
                        ));
                    }
                }
            }
        }
        
        Ok(output)
    }

    async fn get_content_hash(
        &self,
        collection: &str,
        id: &str,
    ) -> Result<Option<String>> {
        let table = self.get_or_create_table(collection, 0).await?;
        
        // 查询指定 id 的记录
        let results = table
            .query()
            .filter(&format!("id = '{}'", id))
            .select(&["content_hash"])
            .limit(1)
            .execute()
            .await?;
        
        for batch in results {
            let batch = batch?;
            if batch.num_rows() > 0 {
                if let Some(hash_col) = batch.column_by_name("content_hash") {
                    let hash_array = hash_col.as_any().downcast_ref::<StringArray>().unwrap();
                    return Ok(Some(hash_array.value(0).to_string()));
                }
            }
        }
        
        Ok(None)
    }

    async fn delete(&self, collection: &str, id: &str) -> Result<()> {
        let table = self.get_or_create_table(collection, 0).await?;
        table.delete(&format!("id = '{}'", id)).await?;
        Ok(())
    }
}
