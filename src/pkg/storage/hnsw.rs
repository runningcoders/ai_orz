//! HNSW 向量存储实现
//!
//! 基于 instant-distance 库的纯 Rust HNSW 索引实现
//! - 零系统依赖，纯 Rust 全平台支持
//! - 支持余弦距离搜索
//! - 每个 collection 独立 HNSW 索引
//! - 增量写入时标记 dirty，搜索时按需重建索引
//! - 持久化：bincode 序列化每个 collection 到独立文件，后台 60s 定时落盘 + Drop 兜底

use crate::models::vector::{VectorIndexParams, VectorMeta, VectorRow, VectorSearchHit};
use async_trait::async_trait;
use bincode::{Decode, Encode};
use instant_distance::{Builder, HnswMap, Point, Search};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// 余弦距离的浮点向量点
#[derive(Clone, Debug, Encode, Decode)]
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
    /// 不参与序列化，加载后按需重建
    cached_index: Option<HnswMap<FloatPoint, String>>,
    /// 索引是否需要重建
    dirty: bool,
}

/// 可序列化的 collection 数据（不含 HNSW 索引缓存）
#[derive(Encode, Decode)]
struct CollectionDataPersist {
    /// 所有向量（id → (点, 行数据)）
    vectors: HashMap<String, (FloatPoint, VectorRow)>,
    /// 已删除的 id（标记删除）
    deleted: HashSet<String>,
    /// 维度
    dimensions: i32,
    /// 索引是否需要重建
    dirty: bool,
}

impl CollectionDataPersist {
    fn from_collection(data: &CollectionData) -> Self {
        Self {
            vectors: data.vectors.clone(),
            deleted: data.deleted.clone(),
            dimensions: data.dimensions,
            dirty: data.dirty,
        }
    }

    fn into_collection(self) -> CollectionData {
        CollectionData {
            vectors: self.vectors,
            deleted: self.deleted,
            dimensions: self.dimensions,
            cached_index: None,
            dirty: true, // 加载后总是需要重建索引
        }
    }
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

/// 集合级元数据（持久化到独立文件）
#[derive(Debug, Clone, Encode, Decode)]
struct CollectionMeta {
    /// 生成该集合向量的 ModelProvider ID
    pub model_provider_id: String,
    /// 向量维度
    pub dimensions: i32,
    /// 记录数
    pub vector_count: usize,
    /// 最后更新时间
    pub updated_at: i64,
}

/// 所有集合的元数据（持久化到单个文件）
#[derive(Debug, Clone, Encode, Decode)]
struct CollectionsMetaFile {
    /// 版本号（用于未来扩展）
    pub version: u32,
    /// collection name → meta
    pub collections: HashMap<String, CollectionMeta>,
}

/// HNSW 向量存储
///
/// 持久化策略：
/// - 每个 collection 序列化为独立 `.bincode` 文件
/// - 集合元数据持久化到 `collections_meta.bincode`
/// - 后台 60s 定时扫描 dirty flag 落盘
/// - Drop 时同步落盘所有 dirty collection（兜底）
/// - 冷启动时扫描目录加载已有索引
pub struct HnswStore {
    base_path: PathBuf,
    collections: Arc<RwLock<HashMap<String, CollectionData>>>,
    /// 集合级元数据（collection name → meta）
    collections_meta: Arc<RwLock<HashMap<String, CollectionMeta>>>,
    /// 元数据是否需要落盘
    meta_dirty: Arc<RwLock<bool>>,
    /// 后台定时落盘任务句柄（Clone 时设为 None，仅原始实例持有）
    flush_task: Option<tokio::task::JoinHandle<()>>,
}

impl Clone for HnswStore {
    fn clone(&self) -> Self {
        Self {
            base_path: self.base_path.clone(),
            collections: self.collections.clone(),
            collections_meta: self.collections_meta.clone(),
            meta_dirty: self.meta_dirty.clone(),
            flush_task: None,
        }
    }
}

impl std::fmt::Debug for HnswStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HnswStore")
            .field("base_path", &self.base_path)
            .field("has_flush_task", &self.flush_task.is_some())
            .finish_non_exhaustive()
    }
}

impl HnswStore {
    /// 创建新的 HNSW 向量存储（使用配置的 hnsw_index_dir）
    pub fn new() -> common::error::Result<Self> {
        let cfg = crate::config::get();
        let base_path =
            crate::pkg::paths::hnsw_index_dir(&cfg.base_data_path(), &cfg.database.hnsw_index_dir);
        Self::with_path(base_path)
    }

    /// 使用指定路径创建（测试专用，也支持持久化）
    pub fn with_path<P: AsRef<std::path::Path>>(base_path: P) -> common::error::Result<Self> {
        let base_path = base_path.as_ref().to_path_buf();
        std::fs::create_dir_all(&base_path)?;

        let mut store = Self {
            base_path: base_path.clone(),
            collections: Arc::new(RwLock::new(HashMap::new())),
            collections_meta: Arc::new(RwLock::new(HashMap::new())),
            meta_dirty: Arc::new(RwLock::new(false)),
            flush_task: None,
        };

        // 冷启动：加载集合元数据
        store.load_collections_meta()?;

        // 冷启动：加载已有索引
        store.load_all_collections()?;

        // 启动后台定时落盘任务
        store.start_flush_task();

        Ok(store)
    }

    /// 从磁盘加载所有 collection（冷启动）
    fn load_all_collections(&mut self) -> common::error::Result<()> {
        let entries = match std::fs::read_dir(&self.base_path) {
            Ok(e) => e,
            Err(e) => {
                sys_warn!(
                    "Failed to read hnsw index dir {:?}: {:?}",
                    self.base_path,
                    e
                );
                return Ok(());
            }
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    sys_warn!("Failed to read dir entry: {:?}", e);
                    continue;
                }
            };

            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("bincode") {
                continue;
            }

            let name = match path.file_stem().and_then(|s| s.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            // 跳过元数据文件
            if name == "collections_meta" {
                continue;
            }

            match Self::load_collection_from_file(&path) {
                Ok(data) => {
                    // 此时 store 尚未被共享，try_write 必定成功
                    if let Ok(mut collections) = self.collections.try_write() {
                        sys_info!(
                            "Loaded hnsw collection '{}' ({} vectors)",
                            name,
                            data.vectors.len()
                        );
                        collections.insert(name, data);
                    }
                }
                Err(e) => {
                    sys_warn!("Failed to load collection from {:?}: {:?}", path, e);
                }
            }
        }

        Ok(())
    }

    /// 加载集合元数据文件
    fn load_collections_meta(&mut self) -> common::error::Result<()> {
        let path = self.base_path.join("collections_meta.bincode");
        if !path.exists() {
            return Ok(());
        }

        let file = std::fs::File::open(&path)?;
        let mut reader = std::io::BufReader::new(file);
        let meta_file: CollectionsMetaFile =
            bincode::decode_from_std_read(&mut reader, bincode::config::standard())?;

        if let Ok(mut collections_meta) = self.collections_meta.try_write() {
            *collections_meta = meta_file.collections;
            sys_info!(
                "Loaded hnsw collections meta ({} collections)",
                collections_meta.len()
            );
        }

        Ok(())
    }

    /// 保存集合元数据文件
    fn save_collections_meta(&self) -> common::error::Result<()> {
        let path = self.base_path.join("collections_meta.bincode");
        let file = std::fs::File::create(&path)?;
        let mut writer = std::io::BufWriter::new(file);

        let meta_file = {
            let collections_meta = self.collections_meta.blocking_read();
            CollectionsMetaFile {
                version: 1,
                collections: collections_meta.clone(),
            }
        };

        bincode::encode_into_std_write(&meta_file, &mut writer, bincode::config::standard())?;
        Ok(())
    }

    /// 从文件加载单个 collection
    fn load_collection_from_file(path: &std::path::Path) -> common::error::Result<CollectionData> {
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        let persist_data: CollectionDataPersist =
            bincode::decode_from_std_read(&mut reader, bincode::config::standard())?;
        Ok(persist_data.into_collection())
    }

    /// 保存单个 collection 到磁盘（同步）
    fn save_collection_to_file(
        base_path: &std::path::Path,
        collection: &str,
        data: &CollectionData,
    ) -> common::error::Result<()> {
        let path = base_path.join(format!("{}.bincode", collection));
        let file = std::fs::File::create(&path)?;
        let mut writer = std::io::BufWriter::new(file);
        let persist_data = CollectionDataPersist::from_collection(data);
        bincode::encode_into_std_write(&persist_data, &mut writer, bincode::config::standard())?;
        Ok(())
    }

    /// 刷新所有 dirty collection 到磁盘
    async fn flush_all_dirty(&self) -> common::error::Result<()> {
        let mut collections = self.collections.write().await;
        for (name, data) in collections.iter_mut() {
            if data.dirty {
                match Self::save_collection_to_file(&self.base_path, name, data) {
                    Ok(()) => {
                        data.dirty = false;
                    }
                    Err(e) => {
                        sys_warn!("Failed to save collection '{}': {:?}", name, e);
                    }
                }
            }
        }

        // 保存元数据（如果有变更）
        let should_save_meta = {
            let meta_dirty = self.meta_dirty.read().await;
            *meta_dirty
        };
        if should_save_meta {
            if let Err(e) = self.save_collections_meta() {
                sys_warn!("Failed to save collections meta: {:?}", e);
            } else {
                let mut meta_dirty = self.meta_dirty.write().await;
                *meta_dirty = false;
            }
        }

        Ok(())
    }

    /// 启动后台定时落盘任务（60s 扫描 dirty flag）
    fn start_flush_task(&mut self) {
        let store = self.clone();
        self.flush_task = Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if let Err(e) = store.flush_all_dirty().await {
                    sys_warn!("Background flush failed: {:?}", e);
                }
            }
        }));
    }

    async fn ensure_collection(
        &self,
        collection: &str,
        dimensions: i32,
    ) -> common::error::Result<()> {
        let mut collections = self.collections.write().await;
        if !collections.contains_key(collection) {
            collections.insert(collection.to_string(), CollectionData::new(dimensions));
        }
        Ok(())
    }
}

impl Drop for HnswStore {
    fn drop(&mut self) {
        // 终止后台定时任务
        if let Some(task) = self.flush_task.take() {
            task.abort();
        }

        // 同步落盘所有 dirty collection（兜底）
        // 使用 try_read 避免阻塞（如果锁被占用则跳过）
        if let Ok(collections) = self.collections.try_read() {
            for (name, data) in collections.iter() {
                if data.dirty
                    && let Err(e) = Self::save_collection_to_file(&self.base_path, name, data)
                {
                    sys_warn!("Failed to flush collection '{}' on drop: {:?}", name, e);
                }
            }
        }

        // 同步落盘元数据
        if let Ok(meta_dirty) = self.meta_dirty.try_read()
            && *meta_dirty
            && let Err(e) = self.save_collections_meta()
        {
            sys_warn!("Failed to flush collections meta on drop: {:?}", e);
        }
    }
}

#[async_trait]
impl super::VectorStore for HnswStore {
    async fn init_collection(
        &self,
        collection: &str,
        dimensions: i32,
    ) -> common::error::Result<()> {
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
        let coll = collections
            .get_mut(collection)
            .expect("collection should exist");

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

        // 更新集合元数据：记录 model_provider_id
        {
            let mut meta = self.collections_meta.write().await;
            let vector_count = coll.vectors.len();
            meta.insert(
                collection.to_string(),
                CollectionMeta {
                    model_provider_id: params.model_provider_id.clone(),
                    dimensions,
                    vector_count,
                    updated_at: now,
                },
            );
        }
        let mut meta_dirty = self.meta_dirty.write().await;
        *meta_dirty = true;

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
                if row.meta.expire_at.is_none_or(|exp| exp > now) {
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
        Ok(collections.get(collection).and_then(|c| {
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

    async fn flush(&self) -> common::error::Result<()> {
        self.flush_all_dirty().await
    }

    async fn get_collection_model_provider_id(
        &self,
        collection: &str,
    ) -> common::error::Result<Option<String>> {
        let meta = self.collections_meta.read().await;
        Ok(meta.get(collection).map(|m| m.model_provider_id.clone()))
    }

    async fn set_collection_model_provider_id(
        &self,
        collection: &str,
        model_provider_id: &str,
    ) -> common::error::Result<()> {
        let now = chrono::Utc::now().timestamp();
        let vector_count = {
            let collections = self.collections.read().await;
            collections
                .get(collection)
                .map(|c| c.vectors.len())
                .unwrap_or(0)
        };

        let mut meta = self.collections_meta.write().await;
        meta.insert(
            collection.to_string(),
            CollectionMeta {
                model_provider_id: model_provider_id.to_string(),
                dimensions: 0, // 重建后不需要记录维度，下次 upsert 会更新
                vector_count,
                updated_at: now,
            },
        );

        let mut meta_dirty = self.meta_dirty.write().await;
        *meta_dirty = true;

        Ok(())
    }
}
