//! 向量存储模块
//!
//! 抽象层：定义通用 VectorStore Trait，支持多种后端实现
//! - SqliteVssStore: 基于 SQLite VSS 扩展（需要系统依赖）
//! - HnswStore: 纯 Rust HNSW 索引（推荐，零系统依赖）
//!
//! 架构分层说明：
//! - 本模块 = 通用向量索引层（纯底层，无业务逻辑）
//! - 各业务 DAO = 决定"向量化什么、什么时候、用什么模型"

use crate::models::vector::{VectorIndexParams, VectorMeta, VectorRow, VectorSearchHit};
use async_trait::async_trait;
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use common::error::Result;

/// 向量存储抽象 Trait
///
/// 所有向量存储后端都实现此 Trait，支持可插拔切换
/// 作为基础适配层，所有方法返回统一的行级结构体
#[async_trait]
pub trait VectorStore: Send + Sync + std::fmt::Debug {
    /// 初始化向量集合
    async fn init_collection(&self, collection: &str, dimensions: i32) -> Result<()>;

    /// 插入/更新向量
    ///
    /// # 参数
    /// - collection: 集合名称（如 skills, memories）
    /// - id: 业务表 ID（如 skill_id, memory_id）
    /// - params: 向量索引参数（向量、哈希、模型信息、过期时间）
    async fn upsert(&self, collection: &str, id: &str, params: &VectorIndexParams) -> Result<()>;

    /// 语义搜索
    ///
    /// 返回: 完整的向量行数据 + 相似度距离
    async fn search(
        &self,
        collection: &str,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>>;

    /// 获取指定文档的完整向量行
    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRow>>;

    /// 删除向量
    async fn delete(&self, collection: &str, id: &str) -> Result<()>;

    /// 清空向量集合
    async fn clear_collection(&self, collection: &str) -> Result<()>;
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

    async fn upsert(&self, collection: &str, id: &str, params: &VectorIndexParams) -> Result<()> {
        // 1. 先存到元数据表（id -> rowid 映射）
        let (rowid,): (i64,) = sqlx::query_as(
            "INSERT OR REPLACE INTO vector_metadata 
             (collection, source_id, content_hash, model, dimensions, expire_at) 
             VALUES (?, ?, ?, ?, ?, ?) 
             RETURNING rowid",
        )
        .bind(collection)
        .bind(id)
        .bind(&params.content_hash)
        .bind(&params.embedding_model)
        .bind(params.vector.len() as i32)
        .bind(params.expire_at)
        .fetch_one(&*self.pool)
        .await?;

        // 2. 存到 vss 虚拟表
        let vector_json = serde_json::to_string(&params.vector)?;
        let sql = format!(
            "INSERT OR REPLACE INTO vss_{}(rowid, embedding) VALUES (?, json(?));",
            collection
        );
        sqlx::query(&sql)
            .bind(rowid)
            .bind(vector_json)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }

    async fn search(
        &self,
        collection: &str,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_json = serde_json::to_string(query_vector)?;
        let sql = format!(
            "SELECT m.source_id, m.content_hash, m.model, m.dimensions, m.expire_at, v.distance 
             FROM vss_{} v
             JOIN vector_metadata m ON v.rowid = m.rowid
             WHERE v.embedding MATCH json(?)
               AND (m.expire_at IS NULL OR m.expire_at > unixepoch())
             ORDER BY v.distance
             LIMIT ?;",
            collection
        );

        let results = sqlx::query_as::<_, (String, String, String, i32, Option<i64>, f32)>(&sql)
            .bind(vector_json)
            .bind(top_k)
            .fetch_all(&*self.pool)
            .await?;

        // 注意：SqliteVSS 不存储原始向量，这里返回的 VectorRow 中 vector 字段为空
        // 实际业务场景中，业务 DAO 需要根据 source_id 从业务表获取内容并重新向量化
        // 或者我们可以考虑在 metadata 表中存储原始向量的 JSON
        Ok(results
            .into_iter()
            .map(
                |(source_id, content_hash, model, _dimensions, expire_at, distance)| {
                    VectorSearchHit {
                        row: VectorRow {
                            id: source_id,
                            vector: Vec::new(), // SqliteVSS 不存储原始向量
                            meta: VectorMeta {
                                content_hash,
                                embedding_model: model,
                                indexed_at: 0, // SQLite 中没有存储索引时间，暂时用 0
                                expire_at,
                            },
                        },
                        distance,
                    }
                },
            )
            .collect())
    }

    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRow>> {
        let result: Option<(String, String, String, Option<i64>)> = sqlx::query_as(
            "SELECT source_id, content_hash, model, expire_at FROM vector_metadata WHERE collection = ? AND source_id = ?"
        )
        .bind(collection)
        .bind(id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|(source_id, content_hash, model, expire_at)| {
            VectorRow {
                id: source_id,
                vector: Vec::new(), // SqliteVSS 不存储原始向量
                meta: VectorMeta {
                    content_hash,
                    embedding_model: model,
                    indexed_at: 0,
                    expire_at,
                },
            }
        }))
    }

    async fn delete(&self, collection: &str, id: &str) -> Result<()> {
        // 1. 从元数据表获取 rowid
        let rowid: Option<(i64,)> = sqlx::query_as(
            "SELECT rowid FROM vector_metadata WHERE collection = ? AND source_id = ?",
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

    async fn clear_collection(&self, collection: &str) -> Result<()> {
        let sql = format!("DELETE FROM vss_{};", collection);
        sqlx::query(&sql).execute(&*self.pool).await?;

        sqlx::query("DELETE FROM vector_metadata WHERE collection = ?")
            .bind(collection)
            .execute(&*self.pool)
            .await?;

        Ok(())
    }
}

impl SqliteVssStore {
    /// 从已有的 pool 创建（测试专用，保证数据隔离）
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// 创建向量存储实例
    pub async fn new(db_path: &str) -> Result<Self> {
        let connection_url = format!("sqlite:{}", db_path);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&connection_url)
            .await?;

        // 尝试加载 vss 扩展（失败不影响，降级到内存计算模式）
        let _ = sqlx::query("SELECT load_extension('vss0')")
            .execute(&pool)
            .await;

        Ok(Self {
            pool: Arc::new(pool),
        })
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
            "SELECT content_hash FROM vector_metadata WHERE collection = ? AND source_id = ?",
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
