# 向量搜索增强计划

## 目标

1. **实现真实的 HNSW 索引**：使用 `instant-distance` 库替换当前的占位实现
2. **Embedding Provider 唯一性限制**：同一时刻只能有一个 Embedding 类型的 Provider 处于启用状态
3. **切换 Embedding Provider 需重建索引**：用户二次确认后，通过 switch 接口原子完成切换和重建

## 背景

### 当前问题

1. **HNSW 未实现**：`src/pkg/storage/hnsw.rs` 只是 `InMemoryVectorStore` 的包装
2. **Embedding Provider 管理缺失**：创建/启用多个 Embedding Provider 会导致向量维度不一致（不同模型输出维度不同：384/768/1536）
3. **模型切换无感知**：切换 Embedding Provider 后，旧索引仍使用旧模型的向量，搜索结果不准确

### 技术选型

| 维度 | 选型 | 理由 |
|------|------|------|
| **HNSW 库** | `instant-distance` | 纯 Rust、无 ORT 依赖、支持 L2/余弦距离、生产验证 |
| **Embedding 唯一性** | Domain 层校验 | 防止多 Embedding Provider 并发使用导致维度混乱 |
| **模型切换处理** | switch 接口 + 二次确认 | 用户显式确认后原子切换，避免误操作 |

## 任务清单

### Task 1: 实现真实的 HNSW 向量存储 ✅ 已完成

**文件**: `src/pkg/storage/hnsw.rs`

使用 `instant-distance` 0.6.1 库实现真正的 HNSW 索引：

1. 添加 `instant-distance = "0.6.1"` 到 Cargo.toml
2. 实现 `VectorStore` trait：
   - `init_collection(collection, dimensions)`: 初始化集合
   - `upsert(collection, id, params)`: 插入/更新向量，标记索引为 dirty
   - `search(collection, query_vector, top_k)`: 按需重建索引后使用 HNSW 搜索最近邻（余弦距离）
   - `get(collection, id)`: 从 HashMap 获取指定向量
   - `delete(collection, id)`: 标记删除 + 标记 dirty
   - `clear_collection(collection)`: 重置为空集合
3. 使用 `tokio::sync::RwLock` 保证线程安全

**实现细节**（与原方案的差异）：
- `instant-distance` 0.6.1 的 `Hnsw`/`HnswMap` **不支持增量插入**，只能在构造时一次性提供所有点
- 因此采用 **lazy rebuild** 策略：
  - 向量存储在 `HashMap<String, (FloatPoint, VectorRow)>` 中
  - upsert/delete 时标记 `dirty = true`
  - search 时如果 dirty，从 HashMap 重建 `HnswMap` 索引，然后搜索
- 自定义 `FloatPoint` 实现 `Point` trait，使用余弦距离（`1 - cos(θ)`）
- 每个 collection 独立一个 `CollectionData`，包含 vectors、deleted set、cached_index、dirty flag

### Task 2: 添加 Embedding Provider 启用唯一性约束 ✅ 已完成

**文件**: `src/service/dao/model_provider/mod.rs` 和 `sqlite.rs`

在 DAO trait 中新增方法：
```rust
async fn find_enabled_embedding_provider(&self, ctx: RequestContext) -> Result<Option<ModelProviderPo>>;
```

**文件**: `src/service/domain/finance/mod.rs`

在 `ModelProviderManage` trait 中新增错误类型响应支持。

**文件**: `common/src/error/code.rs`

新增错误类型：
```rust
EmbeddingProviderSwitchRequired {
    type: Biz,
    http: 409,
    code: "embedding_provider_switch_required",
}
```

**文件**: `src/service/domain/finance/model_provider.rs`

在 `update_model_provider` 中添加校验逻辑：

1. 如果更新的 provider 是 Embedding 类型（`capability == ModelCapability::Embedding`）
2. 且状态变更为启用（`status == ModelProviderStatus::Enabled`）
3. 查询当前是否已有其他启用的 Embedding Provider
4. 如果有且不是当前 provider 本身：
   - 返回 `Error::new(ErrorCode::EmbeddingProviderSwitchRequired)`
   - 在 `ErrorField` 中附带 `current_provider_name` 和 `current_provider_id`

**文件**: `src/handlers/finance/model_provider/update_model_provider.rs`

无需修改，Domain 层返回的错误会自动透传到前端。

**流程说明**：
- 创建新的 Embedding Provider：默认禁用，无唯一性限制 ✓
- 用户通过 update 接口尝试启用：如果冲突，返回 409 + 当前 provider 信息
- 前端收到 409 后提示用户："已有启用的 Embedding Provider 'xxx'，切换将重建所有向量索引，是否继续？"
- 用户确认后：调用 switch 接口（Task 3）

### Task 3: 添加 Embedding Provider Switch 接口 ✅ 已完成

**文件**: `src/service/domain/finance/mod.rs`

在 `ModelProviderManage` trait 中新增方法：
```rust
/// 切换 Embedding Provider（原子操作：禁用旧 → 启用新 → 重建索引）
async fn switch_embedding_provider(
    &self,
    ctx: RequestContext,
    new_provider_id: &str,
) -> Result<()>;
```

**文件**: `src/service/domain/finance/model_provider.rs`

实现 `switch_embedding_provider`：

1. 获取目标 provider，校验其为 Embedding 类型
2. 获取当前启用的 Embedding Provider（如果有）
3. 事务性操作：
   - 禁用旧的 Embedding Provider（status = Disabled）
   - 启用新的 Embedding Provider（status = Enabled）
4. 触发全量索引重建（见下文）

**文件**: `src/pkg/storage/vector.rs`

在 `VectorStore` trait 中新增方法：
```rust
async fn clear_collection(&self, collection: &str) -> Result<()>;
```

**文件**: `src/service/dao/*/mod.rs`（各业务 DAO）

在各业务 DAO trait 中新增方法（以 MemoryDao 为例）：
```rust
async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()>;
```

**文件**: `src/service/dao/*/sqlite.rs`（各业务 DAO 实现）

实现 `rebuild_vectors`：
1. `vector_store.clear_collection("memories")` → 清空向量集合
2. 查询全量实体 PO（如 `SELECT * FROM short_term_memories WHERE status != 0`）
3. 批量生成 embedding（调用 cortex embedding 服务）
4. `vector_store.upsert(...)` → 插入新向量

**文件**: `src/service/dal/*/mod.rs`（各业务 DAL）

新增 DAL 方法，透传调用 DAO：
```rust
async fn rebuild_vectors(&self, ctx: RequestContext) -> Result<()>;
```

**文件**: `src/service/domain/finance/mod.rs`

新增 `rebuild_all_vector_indexes` 方法：

```rust
async fn rebuild_all_vector_indexes(&self, ctx: RequestContext) -> Result<()> {
    // 依次调用各业务 DAL 的 rebuild_vectors
    self.memory_dal.rebuild_vectors(ctx.clone()).await?;
    self.skill_dal.rebuild_vectors(ctx.clone()).await?;
    self.message_dal.rebuild_vectors(ctx.clone()).await?;
    self.task_dal.rebuild_vectors(ctx.clone()).await?;
    self.project_dal.rebuild_vectors(ctx.clone()).await?;
    self.agent_dal.rebuild_vectors(ctx).await?;
    Ok(())
}
```

**分层说明**（严格遵守单向依赖）：
| 层级 | 职责 |
|------|------|
| **Domain** | 编排：依次调用各 DAL 的 `rebuild_vectors()` |
| **DAL** | 透传：调用对应 DAO 的 `rebuild_vectors()` |
| **DAO** | 实现：清空集合 → 查询全量 PO → 生成 embedding → 批量 upsert |
| **VectorStore** | 底层：`clear_collection()` + `upsert()` |

**文件**: `src/handlers/finance/model_provider/switch_embedding.rs`（新建）

新增 Handler：`POST /api/v1/finance/model-providers/:id/switch`

Request DTO（`common/src/api/model_provider.rs`）：
```rust
pub struct SwitchEmbeddingProviderRequest {
    pub confirm: bool,  // 用户二次确认标志，必须为 true
}
```

流程：
1. 检查 `confirm == true`，否则返回 400
2. 获取目标 provider，校验其为 Embedding 类型
3. 调用 `switch_embedding_provider` 完成原子切换和重建
4. 返回成功响应

Response DTO：
```rust
pub struct SwitchEmbeddingProviderResponse {
    pub id: String,
    pub name: String,
    pub previous_provider_id: Option<String>,
    pub previous_provider_name: Option<String>,
    pub rebuild_status: String,  // "completed"
}
```

**文件**: `src/handlers/finance/model_provider/mod.rs`

注册新路由。

## 影响范围

| 文件 | 操作 | 说明 |
|------|------|------|
| `Cargo.toml` | 编辑 | 添加 `instant-distance` 依赖 |
| `src/pkg/storage/hnsw.rs` | 重写 | 实现真实 HNSW 索引 |
| `src/pkg/storage/vector.rs` | 编辑 | 新增 `clear_collection` trait 方法 |
| `src/service/dao/model_provider/mod.rs` | 编辑 | 新增 `find_enabled_embedding_provider` trait 方法 |
| `src/service/dao/model_provider/sqlite.rs` | 编辑 | 实现 `find_enabled_embedding_provider` |
| `src/service/dao/memory/mod.rs` | 编辑 | 新增 `rebuild_vectors` trait 方法 |
| `src/service/dao/memory/sqlite.rs` | 编辑 | 实现 `rebuild_vectors` |
| `src/service/dao/skill/mod.rs` | 编辑 | 新增 `rebuild_vectors` trait 方法 |
| `src/service/dao/skill/sqlite.rs` | 编辑 | 实现 `rebuild_vectors` |
| `src/service/dao/message/mod.rs` | 编辑 | 新增 `rebuild_vectors` trait 方法 |
| `src/service/dao/message/sqlite.rs` | 编辑 | 实现 `rebuild_vectors` |
| `src/service/dao/task/mod.rs` | 编辑 | 新增 `rebuild_vectors` trait 方法 |
| `src/service/dao/task/sqlite.rs` | 编辑 | 实现 `rebuild_vectors` |
| `src/service/dao/project/mod.rs` | 编辑 | 新增 `rebuild_vectors` trait 方法 |
| `src/service/dao/project/sqlite.rs` | 编辑 | 实现 `rebuild_vectors` |
| `src/service/dao/agent/mod.rs` | 编辑 | 新增 `rebuild_vectors` trait 方法 |
| `src/service/dao/agent/sqlite.rs` | 编辑 | 实现 `rebuild_vectors` |
| `src/service/dal/memory.rs` | 编辑 | 新增 `rebuild_vectors` 方法（透传 DAO） |
| `src/service/dal/skill.rs` | 编辑 | 新增 `rebuild_vectors` 方法（透传 DAO） |
| `src/service/dal/message.rs` | 编辑 | 新增 `rebuild_vectors` 方法（透传 DAO） |
| `src/service/dal/task.rs` | 编辑 | 新增 `rebuild_vectors` 方法（透传 DAO） |
| `src/service/dal/project.rs` | 编辑 | 新增 `rebuild_vectors` 方法（透传 DAO） |
| `src/service/dal/agent.rs` | 编辑 | 新增 `rebuild_vectors` 方法（透传 DAO） |
| `src/service/domain/finance/mod.rs` | 编辑 | 新增 `switch_embedding_provider` + `rebuild_all_vector_indexes` |
| `src/service/domain/finance/model_provider.rs` | 编辑 | 添加唯一性校验 + switch 实现 |
| `common/src/error/code.rs` | 编辑 | 新增 `EmbeddingProviderSwitchRequired` 错误码 |
| `common/src/api/model_provider.rs` | 编辑 | 新增 Switch DTO |
| `src/handlers/finance/model_provider/switch_embedding.rs` | 新建 | Switch Handler |
| `src/handlers/finance/model_provider/mod.rs` | 编辑 | 注册 switch 路由 |

## API 接口变更

### 新增接口

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/finance/model-providers/:id/switch` | 切换 Embedding Provider（需 confirm=true） |

### 修改接口行为

| 方法 | 路径 | 变更 |
|------|------|------|
| PUT | `/api/v1/finance/model-providers/:id` | 启用 Embedding Provider 时，如果已有其他启用的，返回 409 + 当前 provider 信息 |

## 错误响应示例

### 409 EmbeddingProviderSwitchRequired

```json
{
  "code": "embedding_provider_switch_required",
  "message": "Another embedding provider is already enabled",
  "fields": {
    "current_provider_id": "mp_xxx",
    "current_provider_name": "BGE-Small-EN"
  }
}
```

前端收到此错误后，展示确认对话框，用户确认后调用 `POST /:id/switch`。

## 测试计划

1. **HNSW 存储测试**：验证插入、搜索、删除、清空功能
2. **Embedding Provider 唯一性测试**：
   - 创建两个 Embedding Provider（默认禁用）→ 成功
   - 启用第一个 → 成功
   - 尝试启用第二个 → 返回 409
3. **Switch 接口测试**：
   - confirm=false → 400
   - switch 到新的 Provider → 旧禁用、新启用、索引重建
4. **回归测试**：确保现有功能不受影响

## 备注

- `instant-distance` 0.6.1 不支持增量插入，采用 lazy rebuild 策略（写入时标记 dirty，搜索时按需重建）
- HNSW 索引内存驻留，重启后需重建（或后续添加持久化）
- 索引重建是耗时操作，但 MVP 阶段同步执行即可，后续可改为异步任务
- 删除操作通过标记删除实现（instant-distance HNSW 不支持物理删除）

## 实现状态

- ✅ Task 1: HNSW 向量存储（instant-distance 0.6.1，lazy rebuild 策略）
- ✅ Task 2: Embedding Provider 唯一性约束（Domain 层校验 + 409 错误码）
- ✅ Task 3: Switch 接口（POST /api/v1/finance/model-providers/:id/switch）
- ✅ 测试验证：697 个测试全部通过
- ⏳ 索引重建逻辑（rebuild_vectors 各 DAO 实现）待后续补充
