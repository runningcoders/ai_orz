//! HNSW 向量存储实现
//!
//! 基于 instant-distance 库的纯 Rust HNSW 索引实现
//! - 零系统依赖，纯 Rust 全平台支持
//! - 支持余弦距离搜索
//! - 每个 collection 独立 HNSW 索引
//! - 增量写入时标记 dirty，搜索时按需重建索引

use crate::models::vector::{VectorIndexParams, VectorMeta, VectorRow, VectorSearchHit};
use async_trait::async_trait;
use instant_distance::{Builder, HnswMap, Point, Search};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 余弦距离的浮点向量点
#[derive(Clone, Debug)]
struct FloatPoint(Vec<f32>);

impl Point for FloatPoint {
    fn distance(&self, other: &Self) -> f32 {
        let dot: f32 = self.0.iter().zip(other.0.iter()).map(|(a, b)| a * b).sum();
        let norm_a: f32 = self.0.iter().map(|a| a * a).sum::<f32>().sqrt();
        let norm_b: f32 = other.0.iter().map(|b| b * b).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 1.0;
        }
        1.0 - dot / (norm_a * norm_b)
    }
}

/// 单个 collection 的数据
struct CollectionData {
    /// 所有向量（id → (点, 行数据)）
    vectors: HashMap<String, (FloatPoint, VectorRow)>,
    /// 已删除的 id（标记删除）
    deleted: HashSet<String>,
    /// 维度
    dimensions: i32,
    /// 缓存的 HNSW 索引（dirty 时为 None，搜索时按需重建）
    cached_index: Option<HnswMap<FloatPoint, String>>,
    /// 索引是否需要重建
    dirty: bool,
}

impl std::fmt::Debug for CollectionData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectionData")
            .field("vectors_len", &self.vectors.len())
            .field("deleted_len", &self.deleted.len())
            .field("dimensions", &self.dimensions)
            .field("dirty", &self.dirty)
            .finish()
    }
}

impl CollectionData {
    fn new(dimensions: i32) -> Self {
        Self {
            vectors: HashMap::new(),
            deleted: HashSet::new(),
            dimensions,
            cached_index: None,
            dirty: true,
        }
    }

    /// 重建 HNSW 索引
    fn rebuild(&mut self) {
        let active: Vec<(FloatPoint, String)> = self
            .vectors
            .iter()
            .filter(|(id, _)| !self.deleted.contains(*id))
            .map(|(id, (point, _))| (point.clone(), id.clone()))
            .collect();

        if active.is_empty() {
            self.cached_index = None;
        } else {
            let points: Vec<FloatPoint> = active.iter().map(|(p, _)| p.clone()).collect();
            let values: Vec<String> = active.iter().map(|(_, v)| v.clone()).collect();
            self.cached_index = Some(Builder::default().build(points, values));
        }
        self.dirty = false;
    }
}

/// HNSW 向量存储
#[derive(Clone, Debug)]
pub struct HnswStore {
    base_path: PathBuf,
    collections: Arc<RwLock<HashMap<String, CollectionData>>>,
}

impl HnswStore {
    pub fn new() -> common::error::Result<Self> {
        let config = crate::config::get();
        let base_path = config.base_data_path().join("hnsw");
        std::fs::create_dir_all(&base_path)?;

        Ok(Self {
            base_path,
            collections: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn with_path<P: AsRef<std::path::Path>>(base_path: P) -> common::error::Result<Self> {
        let base_path = base_path.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_path)?;

        Ok(Self {
            base_path,
            collections: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    async fn ensure_collection(&self, collection: &str, dimensions: i32) -> common::error::Result<()> {
        let mut collections = self.collections.write().await;
        if !collections.contains_key(collection) {
            collections.insert(collection.to_string(), CollectionData::new(dimensions));
        }
        Ok(())
    }
}

#[async_trait]
impl super::VectorStore for HnswStore {
    async fn init_collection(&self, collection: &str, dimensions: i32) -> common::error::Result<()> {
        self.ensure_collection(collection, dimensions).await
    }

    async fn upsert(
        &self,
        collection: &str,
        id: &str,
        params: &VectorIndexParams,
    ) -> common::error::Result<()> {
        let dimensions = params.vector.len() as i32;
        self.ensure_collection(collection, dimensions).await?;

        let mut collections = self.collections.write().await;
        let coll = collections.get_mut(collection).expect("collection should exist");

        let now = chrono::Utc::now().timestamp();
        let point = FloatPoint(params.vector.clone());
        let row = VectorRow {
            id: id.to_string(),
            vector: params.vector.clone(),
            meta: VectorMeta {
                content_hash: params.content_hash.clone(),
                embedding_model: params.embedding_model.clone(),
                indexed_at: now,
                expire_at: params.expire_at,
            },
        };

        coll.deleted.remove(id);
        coll.vectors.insert(id.to_string(), (point, row));
        coll.dirty = true;

        Ok(())
    }

    async fn search(
        &self,
        collection: &str,
        query_vector: &[f32],
        top_k: i32,
    ) -> common::error::Result<Vec<VectorSearchHit>> {
        let now = chrono::Utc::now().timestamp();
        let mut collections = self.collections.write().await;

        let coll = match collections.get_mut(collection) {
            Some(c) => c,
            None => return Ok(vec![]),
        };

        // 按需重建索引
        if coll.dirty {
            coll.rebuild();
        }

        let hnsw = match &coll.cached_index {
            Some(h) => h,
            None => return Ok(vec![]),
        };

        let query = FloatPoint(query_vector.to_vec());
        let mut search = Search::default();
        let mut hits: Vec<VectorSearchHit> = hnsw
            .search(&query, &mut search)
            .take(top_k as usize)
            .filter_map(|item| {
                let id = item.value;
                let (_, row) = coll.vectors.get(id)?;
                if row.meta.expire_at.map_or(true, |exp| exp > now) {
                    Some(VectorSearchHit {
                        row: row.clone(),
                        distance: item.distance,
                    })
                } else {
                    None
                }
            })
            .collect();

        hits.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(hits)
    }

    async fn get(&self, collection: &str, id: &str) -> common::error::Result<Option<VectorRow>> {
        let collections = self.collections.read().await;
        Ok(collections
            .get(collection)
            .and_then(|c| {
                if c.deleted.contains(id) {
                    None
                } else {
                    c.vectors.get(id).map(|(_, row)| row.clone())
                }
            }))
    }

    async fn delete(&self, collection: &str, id: &str) -> common::error::Result<()> {
        let mut collections = self.collections.write().await;
        if let Some(coll) = collections.get_mut(collection) {
            coll.deleted.insert(id.to_string());
            coll.dirty = true;
        }
        Ok(())
    }

    async fn clear_collection(&self, collection: &str) -> common::error::Result<()> {
        let mut collections = self.collections.write().await;
        if let Some(coll) = collections.get_mut(collection) {
            let dimensions = coll.dimensions;
            *coll = CollectionData::new(dimensions);
        }
        Ok(())
    }
}
