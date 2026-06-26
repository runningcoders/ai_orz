//! 纯 Rust 内存向量存储
//!
//! 基于简单余弦相似度的线性搜索实现
//! - 零系统依赖，纯 Rust 全平台支持
//! - 懒加载持久化到文件系统
//! - 支持热插拔替换 SqliteVssStore

use crate::models::vector::{
    VectorCollection, VectorIndexParams, VectorMeta, VectorRow, VectorSearchHit,
};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use common::error::Result;

/// 内存向量存储
///
/// 纯 Rust 实现，零系统依赖，跨平台完美支持
#[derive(Clone, Debug)]
pub struct InMemoryVectorStore {
    base_path: PathBuf,
    collections: Arc<RwLock<HashMap<String, VectorCollection>>>,
}

impl InMemoryVectorStore {
    /// 创建新的内存向量存储（使用配置的 base_path）
    pub fn new() -> Result<Self> {
        let config = crate::config::get();
        let base_path = config.base_data_path().join("vectors");
        std::fs::create_dir_all(&base_path)?;

        Ok(Self {
            base_path,
            collections: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 使用指定路径创建（测试专用）
    pub fn with_path<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let base_path = base_path.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_path)?;

        Ok(Self {
            base_path,
            collections: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// 获取集合文件路径
    fn collection_path(&self, collection: &str) -> PathBuf {
        self.base_path.join(format!("{}.bin", collection))
    }

    /// 从磁盘加载集合
    async fn load_collection(&self, collection: &str) -> Result<Option<VectorCollection>> {
        let path = self.collection_path(collection);

        if !path.exists() {
            return Ok(None);
        }

        let bytes = tokio::fs::read(path).await?;
        let collection_data: VectorCollection =
            bincode::decode_from_slice(&bytes, bincode::config::standard())
                .map_err(Into::<common::error::Error>::into)?.0;

        Ok(Some(collection_data))
    }

    /// 保存集合到磁盘
    async fn save_collection(&self, collection: &str, data: &VectorCollection) -> Result<()> {
        let path = self.collection_path(collection);
        let bytes = bincode::encode_to_vec(data, bincode::config::standard())
            .map_err(Into::<common::error::Error>::into)?;
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    /// 获取或加载集合（懒加载模式）
    async fn get_or_load(&self, collection: &str) -> Result<Option<VectorCollection>> {
        let collections = self.collections.read().await;

        if let Some(data) = collections.get(collection) {
            return Ok(Some(data.clone()));
        }

        drop(collections);

        // 从磁盘加载
        if let Some(data) = self.load_collection(collection).await? {
            let mut collections = self.collections.write().await;
            collections.insert(collection.to_string(), data.clone());
            return Ok(Some(data));
        }

        Ok(None)
    }
}

/// 余弦相似度计算（越小越相似）
fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "向量维度不匹配");

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 1.0; // 最大距离
    }

    let cosine_similarity = dot_product / (norm_a * norm_b);
    1.0 - cosine_similarity // 转换成距离（0=完全相同，1=完全不同）
}

#[async_trait]
impl super::VectorStore for InMemoryVectorStore {
    async fn init_collection(&self, collection: &str, dimensions: i32) -> Result<()> {
        let mut collections = self.collections.write().await;

        if !collections.contains_key(collection) {
            // 先尝试从磁盘加载
            if let Ok(Some(existing)) = self.load_collection(collection).await {
                collections.insert(collection.to_string(), existing);
                return Ok(());
            }

            // 创建新集合
            collections.insert(collection.to_string(), VectorCollection::new(dimensions));
        }

        Ok(())
    }

    async fn upsert(&self, collection: &str, id: &str, params: &VectorIndexParams) -> Result<()> {
        let now = chrono::Utc::now().timestamp();

        // 确保集合存在
        self.init_collection(collection, params.vector.len() as i32)
            .await?;

        let mut collections = self.collections.write().await;
        let coll = collections.get_mut(collection).expect("集合应该已经初始化");

        // 查找已存在的条目
        if let Some(pos) = coll.entries.iter().position(|e| e.id == id) {
            // 更新
            coll.entries[pos] = VectorRow {
                id: id.to_string(),
                vector: params.vector.clone(),
                meta: VectorMeta {
                    content_hash: params.content_hash.clone(),
                    embedding_model: params.embedding_model.clone(),
                    indexed_at: now,
                    expire_at: params.expire_at,
                },
            };
        } else {
            // 新增
            coll.entries.push(VectorRow {
                id: id.to_string(),
                vector: params.vector.clone(),
                meta: VectorMeta {
                    content_hash: params.content_hash.clone(),
                    embedding_model: params.embedding_model.clone(),
                    indexed_at: now,
                    expire_at: params.expire_at,
                },
            });
        }

        // 异步持久化（不阻塞调用）
        let coll_clone = coll.clone();
        let store_clone = self.clone();
        let collection_name = collection.to_string();
        tokio::spawn(async move {
            let _ = store_clone
                .save_collection(&collection_name, &coll_clone)
                .await;
        });

        Ok(())
    }

    async fn search(
        &self,
        collection: &str,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>> {
        let now = chrono::Utc::now().timestamp();

        // 获取或加载集合
        let coll = self
            .get_or_load(collection)
            .await?
            .unwrap_or_else(|| VectorCollection::new(query_vector.len() as i32));

        // 计算所有向量的相似度
        let mut results: Vec<VectorSearchHit> = coll
            .entries
            .iter()
            .filter(|e| {
                // 过滤过期的向量
                e.meta.expire_at.map_or(true, |exp| exp > now)
            })
            .map(|e| {
                let distance = cosine_distance(query_vector, &e.vector);
                VectorSearchHit {
                    row: e.clone(),
                    distance,
                }
            })
            .collect();

        // 按距离排序（越小越相似）
        results.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 返回前 top_k 个结果
        Ok(results.into_iter().take(top_k as usize).collect())
    }

    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRow>> {
        let coll = self.get_or_load(collection).await?;

        Ok(coll.and_then(|c| c.entries.iter().find(|e| e.id == id).cloned()))
    }

    async fn delete(&self, collection: &str, id: &str) -> Result<()> {
        let mut collections = self.collections.write().await;

        if let Some(coll) = collections.get_mut(collection) {
            coll.entries.retain(|e| e.id != id);

            // 异步持久化
            let coll_clone = coll.clone();
            let store_clone = self.clone();
            let collection_name = collection.to_string();
            tokio::spawn(async move {
                let _ = store_clone
                    .save_collection(&collection_name, &coll_clone)
                    .await;
            });
        }

        Ok(())
    }
}
