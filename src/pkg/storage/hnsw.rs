//! HNSW 向量存储实现
//! TODO: 找到不依赖 yanked ort crate 的 HNSW 实现
//! 当前暂时使用 InMemoryVectorStore 作为替代

use async_trait::async_trait;
use crate::error::Result;
use crate::pkg::storage::InMemoryVectorStore;
use std::sync::Arc;

/// HNSW 向量存储（当前为 InMemory 的别名）
/// TODO: 替换为真正的 HNSW 实现
#[derive(Clone, Debug)]
pub struct HnswStore {
    inner: InMemoryVectorStore,
}

impl HnswStore {
    /// 创建新的 HNSW 向量存储
    pub fn new() -> Self {
        Self {
            inner: InMemoryVectorStore::new(std::env::temp_dir()).expect("Failed to create InMemoryVectorStore"),
        }
    }
}

impl Default for HnswStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl super::VectorStore for HnswStore {
    async fn init_collection(&self, collection: &str, dimensions: i32) -> Result<()> {
        self.inner.init_collection(collection, dimensions).await
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
        self.inner.upsert(collection, id, vector, content_hash, embedding_model, expire_at).await
    }

    async fn search(
        &self,
        collection: &str,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<(String, f32)>> {
        self.inner.search(collection, query_vector, top_k).await
    }

    async fn get_content_hash(&self, collection: &str, id: &str) -> Result<Option<String>> {
        self.inner.get_content_hash(collection, id).await
    }

    async fn delete(&self, collection: &str, id: &str) -> Result<()> {
        self.inner.delete(collection, id).await
    }
}
