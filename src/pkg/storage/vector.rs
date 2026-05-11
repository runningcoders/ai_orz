//! 向量存储模块
//!
//! 抽象层：定义通用 VectorStore Trait，支持多种后端实现
//! - SqliteVssStore: 基于 SQLite VSS 扩展（需要系统依赖）
//! - HnswStore: 纯 Rust HNSW 索引（推荐，零系统依赖）
//!
//! 架构分层说明：
//! - 本模块 = 通用向量索引层（纯底层，无业务逻辑）
//! - 各业务 DAO = 决定"向量化什么、什么时候、用什么模型"

use async_trait::async_trait;
use crate::error::Result;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use std::sync::Arc;

/// 向量存储抽象 Trait
///
/// 所有向量存储后端都实现此 Trait，支持可插拔切换
#[async_trait]
pub trait VectorStore: Send + Sync + std::fmt::Debug {
    /// 初始化向量集合
    async fn init_collection(&self, collection: &str, dimensions: i32) -> Result<()>;
    
    /// 插入/更新向量
    /// 
    /// # 参数
    /// - collection: 集合名称（如 skills, memories）
    /// - id: 业务表 ID（如 skill_id, memory_id）
    /// - vector: 向量数据
    /// - content_hash: 原始内容的哈希（用于判断是否需要重索引）
    /// - embedding_model: 使用的 Embedding 模型名称
    /// - expire_at: 过期时间戳（秒），None 表示永不过期
    async fn upsert(
        &self, 
        collection: &str, 
        id: &str, 
        vector: &[f32],
        content_hash: &str,
        embedding_model: &str,
        expire_at: Option<i64>,
    ) -> Result<()>;
    
    /// 语义搜索
    /// 
    /// 返回: Vec<(source_id, distance)>
    async fn search(
        &self, 
        collection: &str, 
        query_vector: &[f32], 
        top_k: i32,
    ) -> Result<Vec<(String, f32)>>;
    
    /// 获取指定文档的内容哈希（用于增量索引判断）
    async fn get_content_hash(
        &self,
        collection: &str,
        id: &str,
    ) -> Result<Option<String>>;
    
    /// 删除向量
    async fn delete(&self, collection: &str, id: &str) -> Result<()>;
}

/// SQLite VSS 向量存储
/// 基于 SQLite vss0 扩展，支持高效向量相似性搜索
#[derive(Clone, Debug)]
pub struct SqliteVssStore {
    pool: Arc<SqlitePool>,
}

#[async_trait]
impl VectorStore for SqliteVssStore {
    async fn init_collection(&self, collection: &str, dimensions: i32) -> Result<()> {
        let sql = format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vss_{} USING vss0(embedding({}));",
            collection, dimensions
        );
        sqlx::query(&sql).execute(&*self.pool).await?;
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
        // 1. 先存到元数据表（id -> rowid 映射）
        let (rowid,): (i64,) = sqlx::query_as(
            "INSERT OR REPLACE INTO vector_metadata 
             (collection, source_id, content_hash, model, dimensions, expire_at) 
             VALUES (?, ?, ?, ?, ?, ?) 
             RETURNING rowid"
        )
        .bind(collection)
        .bind(id)
        .bind(content_hash)
        .bind(embedding_model)
        .bind(vector.len() as i32)
        .bind(expire_at)
        .fetch_one(&*self.pool)
        .await?;
        
        // 2. 存到 vss 虚拟表
        let vector_json = serde_json::to_string(vector)?;
        let sql = format!("INSERT OR REPLACE INTO vss_{}(rowid, embedding) VALUES (?, json(?));", collection);
        sqlx::query(&sql).bind(rowid).bind(vector_json).execute(&*self.pool).await?;
        
        Ok(())
    }

    async fn search(
        &self, 
        collection: &str, 
        query_vector: &[f32], 
        top_k: i32,
    ) -> Result<Vec<(String, f32)>> {
        let vector_json = serde_json::to_string(query_vector)?;
        let sql = format!(
            "SELECT m.source_id, v.distance 
             FROM vss_{} v
             JOIN vector_metadata m ON v.rowid = m.rowid
             WHERE v.embedding MATCH json(?)
               AND (m.expire_at IS NULL OR m.expire_at > unixepoch())
             ORDER BY v.distance
             LIMIT ?;",
            collection
        );
        
        let results = sqlx::query_as::<_, (String, f32)>(&sql)
            .bind(vector_json)
            .bind(top_k)
            .fetch_all(&*self.pool)
            .await?;
        
        Ok(results)
    }

    async fn get_content_hash(
        &self,
        collection: &str,
        id: &str,
    ) -> Result<Option<String>> {
        let result: Option<(String,)> = sqlx::query_as(
            "SELECT content_hash FROM vector_metadata WHERE collection = ? AND source_id = ?"
        )
        .bind(collection)
        .bind(id)
        .fetch_optional(&*self.pool)
        .await?;
        
        Ok(result.map(|(h,)| h))
    }

    async fn delete(&self, collection: &str, id: &str) -> Result<()> {
        // 1. 从元数据表获取 rowid
        let rowid: Option<(i64,)> = sqlx::query_as(
            "SELECT rowid FROM vector_metadata WHERE collection = ? AND source_id = ?"
        )
        .bind(collection)
        .bind(id)
        .fetch_optional(&*self.pool)
        .await?;
        
        if let Some((rowid,)) = rowid {
            // 2. 从 vss 虚拟表删除
            let sql = format!("DELETE FROM vss_{} WHERE rowid = ?;", collection);
            sqlx::query(&sql).bind(rowid).execute(&*self.pool).await?;
            
            // 3. 从元数据表删除
            sqlx::query("DELETE FROM vector_metadata WHERE collection = ? AND source_id = ?")
                .bind(collection)
                .bind(id)
                .execute(&*self.pool)
                .await?;
        }
        
        Ok(())
    }
}

impl SqliteVssStore {
    /// 从已有的 pool 创建（测试专用，保证数据隔离）
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool: Arc::new(pool) }
    }
    
    /// 创建向量存储实例
    pub async fn new(db_path: &str) -> Result<Self> {
        let connection_url = format!("sqlite:{}", db_path);
        
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&connection_url)
            .await?;
            
        // 尝试加载 vss 扩展（失败不影响，降级到内存计算模式）
        let _ = sqlx::query("SELECT load_extension('vss0')").execute(&pool).await;
        
        Ok(Self { pool: Arc::new(pool) })
    }
    
    /// 创建向量集合（按领域分表，如 skills, memories, tasks）
    /// 注意：幂等操作，已存在则跳过
    pub async fn create_collection(&self, collection: &str, dimensions: i32) -> Result<()> {
        self.init_collection(collection, dimensions).await
    }
    
    /// 检查是否需要重索引（内容哈希变更）
    pub async fn needs_reindex(
        &self,
        collection: &str,
        source_id: &str,
        current_content_hash: &str,
    ) -> Result<bool> {
        let result: Option<(String,)> = sqlx::query_as(
            "SELECT content_hash FROM vector_metadata WHERE collection = ? AND source_id = ?"
        )
        .bind(collection)
        .bind(source_id)
        .fetch_optional(&*self.pool)
        .await?;
        
        Ok(match result {
            Some((stored_hash,)) => stored_hash != current_content_hash,
            None => true,
        })
    }
}
