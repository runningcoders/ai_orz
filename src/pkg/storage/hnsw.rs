//! HNSW 向量存储实现
//! TODO: 找到不依赖 yanked ort crate 的 HNSW 实现
//! 当前暂时使用 InMemoryVectorStore 作为替代

use crate::models::vector::{VectorIndexParams, VectorRow, VectorSearchHit};
use crate::pkg::storage::InMemoryVectorStore;
use async_trait::async_trait;
use common::error::Result;

/// HNSW 向量存储（当前为 InMemory 的别名）
/// TODO: 替换为真正的 HNSW 实现
#[derive(Clone, Debug)]
pub struct HnswStore {
    inner: InMemoryVectorStore,
}

impl HnswStore {
    /// 创建新的 HNSW 向量存储
    pub fn new() -> Result<Self> {
        Ok(Self {
            inner: InMemoryVectorStore::new()?,
        })
    }
}

impl Default for HnswStore {
    fn default() -> Self {
        Self::new().expect("创建 HnswStore 失败")
    }
}

#[async_trait]
impl super::VectorStore for HnswStore {
    async fn init_collection(&self, collection: &str, dimensions: i32) -> Result<()> {
        self.inner.init_collection(collection, dimensions).await
    }

    async fn upsert(&self, collection: &str, id: &str, params: &VectorIndexParams) -> Result<()> {
        self.inner.upsert(collection, id, params).await
    }

    async fn search(
        &self,
        collection: &str,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>> {
        self.inner.search(collection, query_vector, top_k).await
    }

    async fn get(&self, collection: &str, id: &str) -> Result<Option<VectorRow>> {
        self.inner.get(collection, id).await
    }

    async fn delete(&self, collection: &str, id: &str) -> Result<()> {
        self.inner.delete(collection, id).await
    }
}
