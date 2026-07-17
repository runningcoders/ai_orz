# HNSW 持久化与索引重建异步化实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 HNSW 索引持久化（配置路径 + 定时落盘 + 冷启动加载）和向量索引重建异步化（后台任务 + 进度查询 + 并发控制）

**Architecture:** 
- HNSW 持久化：`DatabaseConfig` 新增 `hnsw_index_dir` 配置，`HnswStore` 使用 bincode 序列化每个 collection 到独立文件，后台 60s 定时扫描 dirty flag 落盘 + `Drop` 时兜底，冷启动时扫描目录加载已有索引
- 索引重建异步化：Domain 层持有 `Arc<RwLock<Option<RebuildTask>>>`，switch 时 spawn 后台 task，前端通过 task_id 轮询进度，同一时刻仅允许一个重建任务
- 增量重建：HnswStore 维护每个集合的 `CollectionMeta`（model_provider_id / dimensions / vector_count / updated_at），重建时对比当前启用的 embedding provider，一致则跳过，从源头避免数据不一致

**Tech Stack:** Rust + bincode + tokio::sync + chrono

---

## 文件结构

| 文件 | 职责 | 操作 |
|------|------|------|
| `common/src/config.rs` | 添加 `hnsw_index_dir` 配置项和 `hnsw_index_dir()` 方法 | 修改 |
| `src/pkg/storage/hnsw.rs` | HNSW 持久化核心实现（加载/保存/定时落盘/Drop） | 修改 |
| `src/pkg/storage/vector.rs` | VectorStore trait 新增 `flush()` 方法 | 修改 |
| `src/service/domain/finance/model_provider.rs` | 索引重建异步化核心逻辑（后台任务 + 进度更新） | 修改 |
| `src/service/domain/finance/mod.rs` | ModelProviderManage trait 新增 `get_rebuild_progress` 方法 | 修改 |
| `common/src/api/model_provider.rs` | 新增 `RebuildProgressResponse` DTO | 修改 |
| `src/handlers/finance/model_provider/rebuild_progress.rs` | 新增进度查询 Handler | 新建 |
| `src/handlers/finance/model_provider/mod.rs` | 注册进度查询路由 | 修改 |
| `src/handlers/finance/model_provider/switch_embedding.rs` | 修改返回 task_id | 修改 |
| `common/src/error/code.rs` | 新增 `RebuildInProgress` 错误码 | 修改 |

---

## Task 1: 添加 HNSW 索引目录配置

**Files:**
- Modify: `common/src/config.rs`

- [ ] **Step 1: 修改 DatabaseConfig 结构体，新增 hnsw_index_dir 字段**

```rust
/// 数据库配置
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    /// SQLite 数据库文件名（相对于 base_data_path）
    #[serde(default = "default_db_file_name")]
    pub db_file_name: String,

    /// 向量数据库文件名（相对于 base_data_path）
    #[serde(default = "default_vector_db_file_name")]
    pub vector_db_file_name: String,

    /// 向量存储后端类型
    #[serde(default)]
    pub vector_store_type: VectorStoreType,

    /// HNSW 索引持久化目录（相对于 base_data_path，仅使用 Hnsw 后端时生效）
    #[serde(default = "default_hnsw_index_dir")]
    pub hnsw_index_dir: String,
}
```

- [ ] **Step 2: 添加 default_hnsw_index_dir 函数**

```rust
fn default_hnsw_index_dir() -> String {
    "hnsw_index".to_string()
}
```

- [ ] **Step 3: 更新 DatabaseConfig::default() 方法**

```rust
impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_file_name: default_db_file_name(),
            vector_db_file_name: default_vector_db_file_name(),
            vector_store_type: VectorStoreType::default(),
            hnsw_index_dir: default_hnsw_index_dir(),
        }
    }
}
```

- [ ] **Step 4: 在 AppConfig 中添加 hnsw_index_dir() 方法**

```rust
impl AppConfig {
    // ... 其他方法 ...

    /// 获取 HNSW 索引持久化目录路径
    pub fn hnsw_index_dir(&self) -> PathBuf {
        self.base_data_path().join(&self.database.hnsw_index_dir)
    }
}
```

- [ ] **Step 5: 运行 cargo check 验证编译**

Run: `cargo check --lib`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add common/src/config.rs
git commit -m "feat: add hnsw_index_dir config"
```

---

## Task 2: 实现 HNSW 索引持久化（加载/保存/定时落盘）

**Files:**
- Modify: `src/pkg/storage/hnsw.rs`
- Modify: `src/pkg/storage/vector.rs`

### Step 2.1: VectorStore trait 新增 flush 方法

- [ ] **Step 1: 修改 VectorStore trait，新增 flush 方法**

```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    // ... 其他方法 ...

    /// 刷新所有脏数据到持久化存储（仅 Hnsw 后端有实际操作）
    async fn flush(&self) -> common::error::Result<()> {
        Ok(())
    }
}
```

- [ ] **Step 2: 运行 cargo check 验证编译**

Run: `cargo check --lib`
Expected: PASS

### Step 2.2: HnswStore 添加持久化逻辑

- [ ] **Step 1: 修改 CollectionData 结构体，添加 bincode Serialize/Deserialize**

```rust
use bincode::{deserialize_from, serialize_into};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FloatPoint(Vec<f32>);

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CollectionData {
    vectors: HashMap<String, (FloatPoint, VectorRow)>,
    deleted: HashSet<String>,
    dimensions: i32,
    cached_index: Option<HnswMap<FloatPoint, String>>,
    dirty: bool,
}
```

- [ ] **Step 2: 修改 HnswStore 结构体，添加定时任务句柄**

```rust
use tokio::task::JoinHandle;

#[derive(Debug)]
pub struct HnswStore {
    base_path: PathBuf,
    collections: Arc<RwLock<HashMap<String, CollectionData>>>,
    flush_task: Option<JoinHandle<()>>,
}

impl Clone for HnswStore {
    fn clone(&self) -> Self {
        Self {
            base_path: self.base_path.clone(),
            collections: self.collections.clone(),
            flush_task: None,
        }
    }
}
```

- [ ] **Step 3: 修改 HnswStore::new()，冷启动加载已有索引**

```rust
impl HnswStore {
    pub fn new() -> common::error::Result<Self> {
        let config = crate::config::get();
        let base_path = config.hnsw_index_dir();
        std::fs::create_dir_all(&base_path)?;

        let mut collections = HashMap::new();
        Self::load_all_collections(&base_path, &mut collections)?;

        let collections_rwlock = Arc::new(RwLock::new(collections));
        let store = Self {
            base_path: base_path.clone(),
            collections: collections_rwlock.clone(),
            flush_task: None,
        };

        store.start_flush_task(collections_rwlock, base_path);

        Ok(store)
    }

    fn load_all_collections(
        base_path: &PathBuf,
        collections: &mut HashMap<String, CollectionData>,
    ) -> common::error::Result<()> {
        if let Ok(entries) = std::fs::read_dir(base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "bincode" {
                        if let Some(collection_name) = path.file_stem().and_then(|s| s.to_str()) {
                            if let Ok(data) = Self::load_collection(&path) {
                                collections.insert(collection_name.to_string(), data);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn load_collection(path: &PathBuf) -> common::error::Result<CollectionData> {
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        let data: CollectionData = deserialize_from(&mut reader)?;
        Ok(data)
    }

    fn start_flush_task(
        &self,
        collections: Arc<RwLock<HashMap<String, CollectionData>>>,
        base_path: PathBuf,
    ) {
        let flush_task = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if let Err(e) = Self::flush_all_dirty(&collections, &base_path).await {
                    tracing::warn!("HNSW flush failed: {:?}", e);
                }
            }
        });
        let _ = self.flush_task.insert(flush_task);
    }

    async fn flush_all_dirty(
        collections: &Arc<RwLock<HashMap<String, CollectionData>>>,
        base_path: &PathBuf,
    ) -> common::error::Result<()> {
        let collections = collections.read().await;
        for (name, data) in collections.iter() {
            if data.dirty {
                Self::save_collection(name, data, base_path)?;
            }
        }
        Ok(())
    }

    fn save_collection(
        name: &str,
        data: &CollectionData,
        base_path: &PathBuf,
    ) -> common::error::Result<()> {
        let path = base_path.join(format!("{}.bincode", name));
        let file = std::fs::File::create(&path)?;
        let mut writer = std::io::BufWriter::new(file);
        serialize_into(&mut writer, data)?;
        Ok(())
    }
}
```

- [ ] **Step 4: 添加 Drop 实现，进程退出时落盘**

```rust
impl Drop for HnswStore {
    fn drop(&mut self) {
        if let Some(task) = self.flush_task.take() {
            task.abort();
        }
        if let Ok(collections) = self.collections.try_read() {
            for (name, data) in collections.iter() {
                if data.dirty {
                    let _ = Self::save_collection(name, data, &self.base_path);
                }
            }
        }
    }
}
```

- [ ] **Step 5: 实现 VectorStore::flush 方法**

```rust
#[async_trait]
impl super::VectorStore for HnswStore {
    // ... 其他方法 ...

    async fn flush(&self) -> common::error::Result<()> {
        Self::flush_all_dirty(&self.collections, &self.base_path).await
    }
}
```

- [ ] **Step 6: 添加 Duration import**

```rust
use std::time::Duration;
```

- [ ] **Step 7: 运行 cargo check 验证编译**

Run: `cargo check --lib`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add src/pkg/storage/hnsw.rs src/pkg/storage/vector.rs
git commit -m "feat: HNSW index persistence (load/save/timed flush/Drop)"
```

---

## Task 3: 定义 RebuildProgress 结构体和错误码

**Files:**
- Modify: `common/src/api/model_provider.rs`
- Modify: `common/src/error/code.rs`

### Step 3.1: 新增 RebuildProgressResponse DTO

- [ ] **Step 1: 在 model_provider.rs 末尾添加**

```rust
/// Rebuild status
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RebuildStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

/// Rebuild progress response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RebuildProgressResponse {
    /// Task ID
    pub task_id: String,
    /// Rebuild status
    pub status: RebuildStatus,
    /// Current entity being processed (e.g., "memory", "skill")
    pub current_entity: Option<String>,
    /// Current entity index (0..total_entities)
    pub current_entity_index: usize,
    /// Total entities to rebuild
    pub total_entities: usize,
    /// Number of records processed in current entity
    pub processed_records: usize,
    /// Total records in current entity
    pub total_records: usize,
    /// Start timestamp (ms)
    pub started_at: i64,
    /// Finish timestamp (ms, optional)
    pub finished_at: Option<i64>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Get rebuild progress request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetRebuildProgressRequest {
    /// Task ID to query
    #[param(source = "query")]
    pub task_id: String,
}
```

- [ ] **Step 2: 运行 cargo check 验证编译**

Run: `cargo check --lib`
Expected: PASS

### Step 3.2: 新增 RebuildInProgress 错误码

- [ ] **Step 1: 在 common/src/error/code.rs 中添加**

```rust
EmbeddingProviderSwitchRequired {
    type: Biz,
    http: 409,
    code: "embedding_provider_switch_required",
},

RebuildInProgress {
    type: Biz,
    http: 409,
    code: "rebuild_in_progress",
},
```

- [ ] **Step 2: 运行 cargo check 验证编译**

Run: `cargo check --lib`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add common/src/api/model_provider.rs common/src/error/code.rs
git commit -m "feat: add RebuildProgressResponse DTO and RebuildInProgress error code"
```

---

## Task 4: Domain 层实现索引重建异步化

**Files:**
- Modify: `src/service/domain/finance/model_provider.rs`
- Modify: `src/service/domain/finance/mod.rs`

### Step 4.1: 修改 FinanceDomainImpl 添加重建任务状态

- [ ] **Step 1: 修改 FinanceDomainImpl 结构体，添加重建任务状态**

```rust
use std::sync::{Arc, RwLock};
use tokio::task::JoinHandle;
use uuid::Uuid;

pub struct FinanceDomainImpl {
    pub model_provider_dal: Arc<dyn ModelProviderDal>,
    pub message_channel_dal: Arc<dyn MessageChannelDal>,
    pub mcp_server_dal: Arc<dyn McpServerDal + Send + Sync>,
    pub mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
    pub tool_dal: Arc<dyn ToolDal>,
    pub brain_dal: Arc<dyn BrainDal>,
    pub attachment_dal: Arc<dyn AttachmentDal + Send + Sync>,
    pub rebuild_task: Arc<RwLock<Option<RebuildTask>>>,
}

struct RebuildTask {
    task_id: String,
    status: RebuildStatus,
    current_entity: Option<String>,
    current_entity_index: usize,
    total_entities: usize,
    processed_records: usize,
    total_records: usize,
    started_at: i64,
    finished_at: Option<i64>,
    error: Option<String>,
    task_handle: JoinHandle<()>,
}
```

- [ ] **Step 2: 修改 FinanceDomainImpl::new() 方法**

```rust
impl FinanceDomainImpl {
    pub fn new(
        model_provider_dal: Arc<dyn ModelProviderDal>,
        message_channel_dal: Arc<dyn MessageChannelDal>,
        mcp_server_dal: Arc<dyn McpServerDal + Send + Sync>,
        mcp_tool_dal: Arc<dyn McpToolDal + Send + Sync>,
        tool_dal: Arc<dyn ToolDal>,
        brain_dal: Arc<dyn BrainDal>,
        attachment_dal: Arc<dyn AttachmentDal + Send + Sync>,
    ) -> Self {
        Self {
            model_provider_dal,
            message_channel_dal,
            mcp_server_dal,
            mcp_tool_dal,
            tool_dal,
            brain_dal,
            attachment_dal,
            rebuild_task: Arc::new(RwLock::new(None)),
        }
    }
}
```

### Step 4.2: 修改 switch_embedding_provider 为异步重建

- [ ] **Step 1: 修改 switch_embedding_provider 方法**

```rust
async fn switch_embedding_provider(
    &self,
    ctx: RequestContext,
    new_provider_id: &str,
) -> Result<(Option<ModelProvider>, String)> {
    let new_provider = self.get_model_provider(ctx.clone(), new_provider_id).await?
        .ok_or_else(|| Error::not_found(format!("ModelProvider {} not found", new_provider_id)))?;

    if !new_provider.po.capability.is_embedding() {
        return Err(Error::bad_request("Target provider is not an embedding provider"));
    }

    let current_provider = self.model_provider_dal.find_enabled_embedding_provider(ctx.clone()).await?;

    if let Some(mut current) = current_provider.clone() {
        if current.po.id == new_provider_id {
            return Ok((current_provider, "".to_string()));
        }

        current.po.status = ModelProviderStatus::Deleted;
        self.model_provider_dal.update(ctx.clone(), &current).await?;
    }

    let mut new_provider_to_enable = new_provider.clone();
    new_provider_to_enable.po.status = ModelProviderStatus::Normal;
    self.update_model_provider(ctx.clone(), &new_provider_to_enable).await?;

    let task_id = self.start_rebuild_task(ctx).await?;

    Ok((current_provider, task_id))
}
```

- [ ] **Step 2: 添加 start_rebuild_task 方法**

```rust
async fn start_rebuild_task(&self, ctx: RequestContext) -> Result<String> {
    let mut task_guard = self.rebuild_task.write().await;
    if task_guard.is_some() {
        let existing = task_guard.as_ref().unwrap();
        return Err(Error::new(
            ErrorCode::RebuildInProgress,
            "Another rebuild task is already in progress".to_string()
        ).with_field({
            let mut f = ErrorField::new();
            f.insert("task_id".into(), json!(existing.task_id));
            f
        }));
    }

    let task_id = Uuid::new_v4().to_string();
    let ctx_clone = ctx.clone();
    let rebuild_task_clone = self.rebuild_task.clone();
    let model_provider_dal_clone = self.model_provider_dal.clone();

    let task_handle = tokio::spawn(async move {
        let result = Self::run_rebuild_task(rebuild_task_clone, ctx_clone).await;
        if let Err(e) = result {
            tracing::error!("Rebuild task {} failed: {:?}", task_id, e);
            let mut guard = rebuild_task_clone.write().await;
            if let Some(task) = guard.as_mut() {
                task.status = RebuildStatus::Failed;
                task.error = Some(e.to_string());
                task.finished_at = Some(chrono::Utc::now().timestamp_millis());
            }
        }
    });

    let now = chrono::Utc::now().timestamp_millis();
    *task_guard = Some(RebuildTask {
        task_id: task_id.clone(),
        status: RebuildStatus::Running,
        current_entity: None,
        current_entity_index: 0,
        total_entities: 7,
        processed_records: 0,
        total_records: 0,
        started_at: now,
        finished_at: None,
        error: None,
        task_handle,
    });

    Ok(task_id)
}

async fn run_rebuild_task(
    rebuild_task: Arc<RwLock<Option<RebuildTask>>>,
    ctx: RequestContext,
) -> common::error::Result<()> {
    use crate::service::dal;

    log_info!(&ctx, "rebuild_vectors", "开始重建所有向量索引");

    let entities = vec![
        ("agent", || async { dal::agent::dal().rebuild_vectors(ctx.clone()).await }),
        ("memory", || async { dal::memory::dal().rebuild_vectors(ctx.clone()).await }),
        ("skill", || async { dal::skill::dal().rebuild_vectors(ctx.clone()).await }),
        ("task", || async { dal::task::dal().rebuild_vectors(ctx.clone()).await }),
        ("project", || async { dal::project::dal().rebuild_vectors(ctx.clone()).await }),
        ("message", || async { dal::message::dal().rebuild_vectors(ctx.clone()).await }),
        ("tool", || async { dal::tool::dal().rebuild_vectors(ctx.clone()).await }),
    ];

    for (index, (entity_name, rebuild_fn)) in entities.iter().enumerate() {
        {
            let mut guard = rebuild_task.write().await;
            if let Some(task) = guard.as_mut() {
                task.current_entity = Some(entity_name.to_string());
                task.current_entity_index = index;
                task.processed_records = 0;
                task.total_records = 0;
            }
        }

        if let Err(e) = rebuild_fn().await {
            log_warn!(&ctx, "rebuild_vectors", error = ?e, "{} 向量重建失败", entity_name);
        }
    }

    {
        let mut guard = rebuild_task.write().await;
        if let Some(task) = guard.as_mut() {
            task.status = RebuildStatus::Completed;
            task.finished_at = Some(chrono::Utc::now().timestamp_millis());
        }
    }

    log_info!(&ctx, "rebuild_vectors", "所有向量索引重建完成");
    Ok(())
}
```

### Step 4.3: 添加 get_rebuild_progress 方法

- [ ] **Step 1: 在 FinanceDomainImpl 中添加方法**

```rust
async fn get_rebuild_progress(&self, ctx: RequestContext, task_id: &str) -> Result<Option<RebuildProgressResponse>> {
    let task_guard = self.rebuild_task.read().await;
    if let Some(task) = task_guard.as_ref() {
        if task.task_id == task_id {
            return Ok(Some(RebuildProgressResponse {
                task_id: task.task_id.clone(),
                status: task.status.clone(),
                current_entity: task.current_entity.clone(),
                current_entity_index: task.current_entity_index,
                total_entities: task.total_entities,
                processed_records: task.processed_records,
                total_records: task.total_records,
                started_at: task.started_at,
                finished_at: task.finished_at,
                error: task.error.clone(),
            }));
        }
    }
    Ok(None)
}
```

### Step 4.4: 修改 ModelProviderManage trait

- [ ] **Step 1: 修改 trait 中的 switch_embedding_provider 签名**

```rust
async fn switch_embedding_provider(
    &self,
    ctx: RequestContext,
    new_provider_id: &str,
) -> Result<(Option<ModelProvider>, String)>;
```

- [ ] **Step 2: 新增 get_rebuild_progress 方法**

```rust
async fn get_rebuild_progress(
    &self,
    ctx: RequestContext,
    task_id: &str,
) -> Result<Option<common::api::RebuildProgressResponse>>;
```

- [ ] **Step 3: 添加 necessary imports**

```rust
use common::api::{RebuildProgressResponse, RebuildStatus};
use uuid::Uuid;
```

- [ ] **Step 4: 运行 cargo check 验证编译**

Run: `cargo check --lib`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/service/domain/finance/model_provider.rs src/service/domain/finance/mod.rs
git commit -m "feat: async rebuild with task tracking and progress query"
```

---

## Task 5: 修改 Handler 层支持异步重建

**Files:**
- Modify: `src/handlers/finance/model_provider/switch_embedding.rs`
- Create: `src/handlers/finance/model_provider/rebuild_progress.rs`
- Modify: `src/handlers/finance/model_provider/mod.rs`

### Step 5.1: 修改 switch_embedding Handler

- [ ] **Step 1: 修改 switch_embedding_provider.rs**

```rust
#[generate_http_handler]
pub async fn switch_embedding_provider(
    ctx: RequestContext,
    params: SwitchEmbeddingProviderRequest,
) -> Result<SwitchEmbeddingProviderResponse> {
    if !params.confirm {
        return Err(Error::bad_request("Confirmation required - set confirm: true to proceed"));
    }

    let (previous_provider, task_id) = domain()
        .model_provider_manage()
        .switch_embedding_provider(ctx.clone(), &params.id)
        .await?;

    let new_provider = domain()
        .model_provider_manage()
        .get_model_provider(ctx, &params.id)
        .await?
        .ok_or_else(|| Error::not_found(format!("ModelProvider {} not found", params.id)))?;

    Ok(SwitchEmbeddingProviderResponse {
        id: new_provider.po.id.clone(),
        name: new_provider.po.name.clone(),
        previous_provider_id: previous_provider.as_ref().map(|p| p.po.id.clone()),
        previous_provider_name: previous_provider.as_ref().map(|p| p.po.name.clone()),
        rebuild_status: if task_id.is_empty() {
            "completed".to_string()
        } else {
            "in_progress".to_string()
        },
        task_id,
    })
}
```

- [ ] **Step 2: 更新 SwitchEmbeddingProviderResponse 添加 task_id 字段**

在 `common/src/api/model_provider.rs` 中：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SwitchEmbeddingProviderResponse {
    pub id: String,
    pub name: String,
    pub previous_provider_id: Option<String>,
    pub previous_provider_name: Option<String>,
    pub rebuild_status: String,
    pub task_id: String,
}
```

### Step 5.2: 新建 rebuild_progress Handler

- [ ] **Step 1: 创建 src/handlers/finance/model_provider/rebuild_progress.rs**

```rust
//! Handler: GET /api/v1/finance/model-providers/rebuild-progress - Get rebuild progress

use common::error::{Error, Result};
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{GetRebuildProgressRequest, RebuildProgressResponse};

#[generate_http_handler]
pub async fn get_rebuild_progress(
    ctx: RequestContext,
    params: GetRebuildProgressRequest,
) -> Result<RebuildProgressResponse> {
    let progress = domain()
        .model_provider_manage()
        .get_rebuild_progress(ctx, &params.task_id)
        .await?
        .ok_or_else(|| Error::not_found(format!("Rebuild task {} not found", params.task_id)))?;

    Ok(progress)
}
```

### Step 5.3: 注册路由

- [ ] **Step 1: 修改 src/handlers/finance/model_provider/mod.rs**

```rust
pub mod switch_embedding;
pub mod rebuild_progress;

use axum::Router;

pub fn routes() -> Router {
    Router::new()
        // ... 其他路由 ...
        .route("/:id/switch", axum::routing::post(switch_embedding::switch_embedding_provider))
        .route("/rebuild-progress", axum::routing::get(rebuild_progress::get_rebuild_progress))
}
```

- [ ] **Step 2: 运行 cargo check 验证编译**

Run: `cargo check --lib`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/handlers/finance/model_provider/switch_embedding.rs src/handlers/finance/model_provider/rebuild_progress.rs src/handlers/finance/model_provider/mod.rs common/src/api/model_provider.rs
git commit -m "feat: add rebuild progress handler and update switch response"
```

---

## Task 6: 更新 spec 文档和 AGENTS.md

**Files:**
- Modify: `docs/vector_search_architecture.md`
- Modify: `AGENTS.md`

- [ ] **Step 1: 更新 vector_search_architecture.md，添加 2026-07-16 增强内容（HNSW 持久化 + 异步重建）**

在 "2026-07-16 增强内容" 章节后添加新小节：

```markdown
### HNSW 索引持久化

新增 HNSW 索引持久化能力：

- 配置项：`hnsw_index_dir`（默认 `data/hnsw_index`）
- 存储格式：bincode 序列化，每个 collection 一个文件（`<collection>.bincode`）
- 落盘策略：后台 60s 定时扫描 dirty flag 落盘 + `Drop` 时兜底
- 冷启动：扫描目录加载已有索引，避免 lazy rebuild

### 索引重建异步化

新增向量索引重建异步化能力：

- switch 接口立即返回 `task_id`，后台异步执行重建
- 进度查询：`GET /api/v1/finance/model-providers/rebuild-progress?task_id=xxx`
- 并发控制：同一时刻仅允许一个重建任务，已有任务运行时新 switch 返回 409
- 进度结构：当前实体、实体索引、已处理记录数、总记录数、状态、错误信息
```

- [ ] **Step 2: 更新 AGENTS.md 里程碑**

在 2026-07-16 里程碑中添加：

```markdown
**✅ HNSW 索引持久化**
- 新增 `hnsw_index_dir` 配置项，默认 `data/hnsw_index`
- bincode 序列化每个 collection 到独立文件
- 后台 60s 定时落盘 + `Drop` 时兜底
- 冷启动扫描目录加载已有索引

**✅ 索引重建异步化**
- switch 接口立即返回 `task_id`，后台异步重建
- 新增 `GET /rebuild-progress?task_id=xxx` 进度查询接口
- 并发控制：同一时刻仅允许一个重建任务
- 进度结构：当前实体、索引、已处理/总记录数、状态、错误信息
```

- [ ] **Step 3: Commit**

```bash
git add docs/vector_search_architecture.md AGENTS.md
git commit -m "docs: update HNSW persistence and async rebuild documentation"
```

---

## Task 7: 运行测试验证

- [ ] **Step 1: 运行所有测试**

Run: `cargo test --workspace --no-fail-fast 2>&1 | tail -50`
Expected: 697 个测试全部通过

- [ ] **Step 2: 运行 cargo check 验证前端**

Run: `cd frontend && cargo check`
Expected: PASS

- [ ] **Step 3: 推送代码**

```bash
git push
```

---

## API 接口变更

### 新增接口

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/finance/model-providers/rebuild-progress?task_id=xxx` | 查询索引重建进度 |

### 修改接口

| 方法 | 路径 | 变更 |
|------|------|------|
| POST | `/api/v1/finance/model-providers/:id/switch` | 返回值新增 `task_id` 字段，`rebuild_status` 可能为 `"in_progress"` |

### 错误响应

#### 409 RebuildInProgress

```json
{
  "code": "rebuild_in_progress",
  "message": "Another rebuild task is already in progress",
  "fields": {
    "task_id": "task-uuid-xxx"
  }
}
```

---

## 测试计划

1. **HNSW 持久化测试**：
   - 创建 HnswStore，写入数据，drop，重建实例，验证数据恢复
   - 验证 dirty flag 机制（写入后 dirty=true，flush 后 dirty=false）

2. **异步重建测试**：
   - 调用 switch，验证立即返回 task_id
   - 查询进度，验证状态从 Running 变为 Completed
   - 已有任务运行时调用 switch，验证返回 409

3. **回归测试**：确保现有功能不受影响

---

*最后更新：2026-07-17（新增增量重建：集合级 model_provider_id 元数据标记）*

---

## 增量重建（元数据标记方案）

### 设计动机

之前 switch embedding provider 时，7 个实体的向量集合会被无条件全部清空 + 重建。当数据量大时：

- 重复刷数据：embedding API 调用耗时巨大
- 数据不一致风险：重建过程中如果进程崩溃，部分集合是新 embedding，部分是旧 embedding
- 进度无意义：即便进度条 100%，也无法保证新数据可用

### 解决方案

在 HnswStore 中为每个 collection 维护 `CollectionMeta`：

```rust
struct CollectionMeta {
    pub model_provider_id: String,   // 生成该集合向量的 ModelProvider ID
    pub dimensions: i32,
    pub vector_count: usize,
    pub updated_at: i64,
}
```

集合元数据持久化到 `collections_meta.bincode` 单文件，与 collection 数据文件并列。

**重建流程（7 个 DAL 全部统一）：**

1. 获取当前启用的 Embedding Provider ID
2. 调用 `vector_store.get_collection_model_provider_id(collection)` 读取存储的元数据
3. 一致 → 直接返回，跳过重建（不调用任何 embedding API）
4. 不一致 → 清空 + 重建 + 写回元数据

### 关键不变量

- **元数据是事实上的进度说明**：只要元数据写入，集合内容就保证来自对应 provider
- **从源头避免数据不一致**：未写入元数据的集合一定不会被消费者访问
- **进程崩溃安全**：部分集合已重建 + 部分未重建时，未重建的集合仍保留旧 provider 元数据（下次启动一致则不重建，不一致才重建）

### 实施细节

- `VectorStore` trait 新增默认方法（其他后端无需实现）：
  - `get_collection_model_provider_id(&self, collection: &str) -> Result<Option<String>>`
  - `set_collection_model_provider_id(&self, collection: &str, model_provider_id: &str) -> Result<()>`
- 仅 `HnswStore` 覆写这两个方法
- Memory DAL 由于有两个集合（`memory:short_term` + `memory:knowledge_node`），需要分别检查和重建
- 集合名约定：
  - `agents` / `skills` / `tasks` / `projects` / `messages` / `tools`
  - `memory:short_term` / `memory:knowledge_node`

### 验证

```bash
cargo test --lib  # 708 个测试 100% 通过
```