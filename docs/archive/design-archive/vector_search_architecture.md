# 向量搜索与混合搜索架构设计文档

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：vector_search_architecture 设计文档归档冻结，设计决策已沉淀至 wiki 长文。生效方案：见源码和 wiki 长文。

> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目整体分层架构与开发规范 §向量化实体规范
> - [memory_design.md](../memory_design.md) — 记忆系统（短期/长期记忆实体 Vectorizable 实现的使用方）
> - [tool_design.md](./tool_design.md) — 工具/技能实体 Vectorizable 实现的使用方
> - [full_entity_fts5_search_design.md](./full_entity_fts5_search_design.md) — FTS5 全文搜索统一标准（6 实体同构 search 模式）
> - [memory_search_enhancement_design.md](./memory_search_enhancement_design.md) — 记忆搜索扩展（tags 过滤/图谱遍历/种子节点推荐）
> - [entity_list_query_search_design.md](./entity_list_query_search_design.md) — list/query/search 三接口职责边界
> - 【② Plan 落地】（占位：待 ai-orz-doc-maintainer 落地后回填真实 Plan 路径）
> - 【③ Wiki 长文】[存储系统.md](docs/wiki/zh/content/基础设施/存储系统/存储系统.md) — §VectorStore 多后端 + 混合检索合并排序
> - 【③ Wiki 长文】[基础设施.md](docs/wiki/zh/content/基础设施/基础设施.md) — §可插拔向量后端初始化流程
> - 【③ Wiki 长文】[记忆和向量系统.md](docs/wiki/zh/content/数据模型/消息和记忆模型/记忆和向量系统.md) — 7 类 PO Vectorizable 实现列表
> - 【③ Wiki 长文】[记忆系统架构.md](docs/wiki/zh/content/架构设计/记忆系统架构.md) — §记忆搜索扩展层
> - 【③ Wiki 长文】[Agent 搜索与查询.md](docs/wiki/zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 搜索与查询.md) — §AgentSearch 三场景 list/query/search
> - 【④ RAG 卡 3 张】
>   - [向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity](docs/wiki/knowledge/zh/向量存储抽象%20VectorStore%20+%20多后端%20+%20Vectorizable%20trait%20统一索引入口%20+%20embed_entity/向量存储抽象%20VectorStore%20+%20多后端%20+%20Vectorizable%20trait%20统一索引入口%20+%20embed_entity.md) — 向量基础设施 + 统一 embed_entity 工厂
>   - [三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）](docs/wiki/knowledge/zh/三位一体混合搜索：FTS5%20关键词%20+%20向量语义%20+%20合并排序%20（6%20DAO%20统一%20search%20模式%20+%20向量失败降级）/三位一体混合搜索：FTS5%20关键词%20+%20向量语义%20+%20合并排序%20（6%20DAO%20统一%20search%20模式%20+%20向量失败降级）.md) — 6 实体 search 同构模式 + 加权融合排序
>   - [记忆领域搜索增强：FTS5 tags 语义过滤 + knowledge graph traverse 图谱遍历 + recommend_seed_nodes 种子节点推荐](docs/wiki/knowledge/zh/记忆领域搜索增强：FTS5%20tags%20语义过滤%20+%20knowledge%20graph%20traverse%20图谱遍历%20+%20recommend_seed_nodes%20种子节点推荐/记忆领域搜索增强：FTS5%20tags%20语义过滤%20+%20knowledge%20graph%20traverse%20图谱遍历%20+%20recommend_seed_nodes%20种子节点推荐.md) — tags 过滤 + BFS 遍历 + 推荐

## 概述

本设计文档描述 ai_orz 项目中搜索能力的架构实现，包括**向量语义搜索**、**FTS5 关键词搜索**以及二者组合的**混合搜索**，为多 Agent 协作系统提供三位一体的检索能力。

向量搜索作为**语义索引附加层**，FTS5 作为**关键词索引层**，二者均与现有关系型数据完全解耦，仅通过 ID 关联，不破坏现有业务表结构。

---

## 核心设计原则

### 1. 索引与数据完全分离（同目录 DAO 拆分模式）

- ✅ 业务逻辑解耦：SkillDao 拆分为 `sqlite.rs`（基础数据 + FTS5 搜索） + `vector.rs`（向量索引）
- ✅ 同目录共存：两个 DAO 文件位于 `dao/skill/` 目录下，`mod.rs` 提供统一 trait 定义
- ✅ 各自维护单例：`new_skill_dao()` + `new_skill_vector_dao()` 独立构造
- ✅ 通过 `source_id`（业务表 UUID）跨层关联
- ✅ 未来切换向量存储后端（LanceDB / Qdrant 等）业务层零感知

### 2. 触发式非全量索引

- 不对所有数据强制向量化，节省 Embedding Token 成本
- 由各业务 DAL 层自主决定：
  - **什么时候索引**（创建/更新时？还是后台异步？）
  - **索引什么内容**（name + description? 还是全部字段？）
  - **用什么模型**（text-embedding-3-small / large？）
  - **维度是多少**（1536 / 3072？）
  - **过期策略**（永不过期？还是 TTL？）

### 3. 渐进式实现，可平滑升级

- ✅ **第一阶段**：纯 Rust InMemoryVectorStore（余弦相似度 + Bincode 持久化），零系统依赖
- ✅ **第二阶段**：LanceDB 嵌入式向量数据库（生产级高性能选项）
- ✅ **FTS5 全文搜索**：SQLite 内置 FTS5 + trigram 分词器，支持中文关键词搜索
- 未来：可平滑升级到 Qdrant 等专业向量数据库，上层接口不变

### 4. 分层聚合，职责清晰

- **DAO 层**：只做单一职责，基础数据 DAO 不碰向量，向量 DAO 不碰业务数据
- **DAL 层**：组合基础 DAO + 向量 DAO，实现混合搜索逻辑
- **存储层**：纯通用能力，不感知业务逻辑（FTS5 工具、向量存储抽象）

---

## 架构分层

### 职责边界

| 层级 | 职责 | 位置 |
|------|------|------|
| **pkg/storage/** | 通用存储层，纯底层能力，无业务逻辑<br>- 向量存储抽象：VectorStore trait<br>- FTS5 工具：escape_fts5_keyword<br>只懂增删查改，不知道"技能"/"记忆"是什么 | `src/pkg/storage/` |
| **业务 Vector Dao**（SkillVectorDao 等） | 封装向量存储调用，提供业务友好接口 | `src/service/dao/skill/vector.rs` |
| **业务 Base Dao**（SkillDao 等） | 基础数据 CRUD + FTS5 关键词搜索<br>FTS5 关键词搜索使用 storage 层的转义工具，不依赖其他 DAO | `src/service/dao/skill/sqlite.rs` |
| **业务 DAL** | 组合 Base Dao + Vector Dao，实现混合搜索逻辑 + 向量索引生命周期管理 | `src/service/dal/skill.rs` |

### 调用链路示例

```
业务操作（创建技能）
    ↓
SkillDal.create()
    ├─ SkillDao.create()  →  写入关系型数据库（触发器自动同步 FTS 索引）
    ├─ 构建索引文本：name + description
    ├─ 调用 Cortex 生成 Embedding
    ├─ 计算内容 Hash
    └─ SkillVectorDao.upsert_vector()  →  写入向量存储
```

---

## 混合搜索架构

### 三态匹配模型

每个搜索结果都带有 `search_match: Option<SearchMatchInfo>` 字段，标记命中类型：

| 命中类型 | 说明 | 排序优先级 |
|----------|------|-----------|
| **Hybrid** | 同时命中关键词搜索 + 向量搜索 | 最高（1级） |
| **Vector** | 仅向量语义搜索命中 | 中等（2级） |
| **Keyword** | 仅 FTS5 关键词搜索命中 | 最低（3级） |

### 排序策略

**三级排序 + 组内细排：**

```
1. Hybrid 优先（双命中，相关性最强）
   └─ 组内按 vector_distance 升序
2. Vector 次之（语义匹配）
   └─ 组内按 vector_distance 升序
3. Keyword 最后（关键词精确匹配）
   └─ 组内按 fts_rank 升序（BM25 越小越相关）
```

### DAL 层实现模式

每个实体的 DAL 层统一实现 `search()` 方法，内部执行：

> 相关实现细节见：[dal/memory.rs 向量混合搜索](src/service/dal/memory.rs)

### 向量索引生命周期

DAL 层负责向量索引的完整生命周期管理（FTS5 由数据库触发器自动维护）：

| 操作 | FTS5 索引 | 向量索引 | 负责层 |
|------|-----------|----------|--------|
| **创建** | 触发器自动同步 | create 后主动 upsert | DAL 层 |
| **更新** | 触发器自动同步 | content_hash 变化时重新 upsert | DAL 层 |
| **删除/归档** | 触发器自动同步 | 主动 delete 清理 | DAL 层 |

**降级策略：** 向量索引写入失败时仅 warn 降级，不影响主流程（FTS5 仍可用）。

### 已覆盖实体

| 实体 | FTS5 搜索 | 向量搜索 | 混合搜索 | 集成测试 |
|------|-----------|----------|----------|----------|
| Memory（短期记忆/知识节点） | ✅ | ✅ | ✅ | 单元测试 |
| Skill（技能） | ✅ | ✅ | ✅ | ✅ `tool_skill_vector_test` |
| Tool（工具） | ✅ | ✅ | ✅ | ✅ `tool_skill_vector_test` |
| Message（消息） | ✅ | ✅ | ✅ | ✅ `message_vector_test` |
| Task（任务） | ✅ | ✅ | ✅ | ✅ `project_task_vector_test` |
| Project（项目） | ✅ | ✅ | ✅ | ✅ `project_task_vector_test` |
| Agent（智能代理） | ✅ | ✅ | ✅ | ✅ `agent_management_test` |

### 集成测试覆盖

向量搜索集成测试统一采用 **CI 默认 + 真实 API ignore** 双层模式：

| 测试文件 | 默认测试 | ignored 真实向量测试 |
|----------|----------|---------------------|
| `tool_skill_vector_test.rs` | 2（FTS5 + 过滤） | 5（语义搜索 + 索引维护 + 混合排序） |
| `message_vector_test.rs` | 2（FTS5 + 过滤） | 4（语义搜索 + 索引维护 + 混合排序 + match_type） |
| `project_task_vector_test.rs` | 4（Project/Task FTS5 + 过滤） | 4（Project/Task 语义搜索 + 索引维护 + 混合排序） |

**默认测试**：无 embedding provider，走 FTS5 路径，CI 安全。
**ignored 测试**：需 `TEST_EMBEDDING_API_KEY`，验证 LanceDB + Embedding API 端到端。

**已知修复**：Message 搜索曾因 Handler 层未嵌入 `query_vector` 导致向量搜索不工作，已在 DAL 层统一嵌入 keyword → query_vector（与其他实体一致）。

---

## 向量存储后端

### 统一抽象：VectorStore trait

所有后端实现相同的接口，上层调用零感知：

```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn insert_or_update(&self, row: &VectorRow) -> Result<()>;
    async fn get(&self, collection: &str, source_id: &str) -> Result<Option<VectorRow>>;
    async fn delete(&self, collection: &str, source_id: &str) -> Result<()>;
    async fn search(&self, collection: &str, vector: &[f32], top_k: i32) -> Result<Vec<VectorSearchHit>>;
    async fn list_by_source_ids(&self, collection: &str, source_ids: &[String]) -> Result<Vec<VectorRow>>;
}
```
> 当前实现参考：[vector_search 模块 + dao/memory vector](src/service/dao/memory/)

### 后端对比

| 后端 | 实现位置 | 特点 | 适用场景 |
|------|----------|------|----------|
| **InMemoryVectorStore** | `in_memory.rs` | 纯 Rust，零依赖，余弦相似度，Bincode 持久化 | 开发、测试、小数据集 |
| **LanceVectorStore** | `lance.rs` | 生产级，高性能，列式存储 | 生产环境、大数据集 |

### 统一数据结构

```rust
/// 向量行数据（所有后端统一格式）
pub struct VectorRow {
    pub collection: String,
    pub source_id: String,
    pub vector: Vec<f32>,
    pub content_hash: String,
    pub model: String,
    pub indexed_at: i64,
}

/// 向量搜索结果
pub struct VectorSearchHit {
    pub source_id: String,
    pub distance: f32,
}
```
> 当前实现参考：[vector_search 模块 + dao/memory vector](src/service/dao/memory/)

---

## Skill DAO 拆分设计

### mod.rs 统一入口

> 相关实现细节见：[dal/memory.rs 向量混合搜索](src/service/dal/memory.rs)

### sqlite.rs - 基础数据 DAO

> 相关实现细节见：[向量搜索架构](src/pkg/vector_search/)

### vector.rs - 向量索引 DAO

> 相关实现细节见：[dal/memory.rs 向量混合搜索](src/service/dal/memory.rs)

---

## Domain 层组合示例

> 相关实现细节见：[向量索引模块](src/pkg/vector_search/)

---

## 配置项

在 `common/src/config.rs` 中：

```rust
pub struct DatabaseConfig {
    pub db_file_name: String,              // 核心业务数据库
    pub vector_store_type: VectorStoreType, // 向量存储后端类型
}

pub enum VectorStoreType {
    InMemory,  // 纯 Rust 内存实现（默认）
    LanceDB,   // LanceDB 嵌入式向量数据库
}
```
> 当前实现参考：[vector_search 模块 + dao/memory vector](src/service/dao/memory/)

默认值：
- 核心数据库：`ai_orz.db`
- 向量存储后端：`InMemory`

---

## Storage 门面设计

### 统一入口

```rust
// Storage 内部持有向量存储实例
struct StorageInner {
    sqlite: SqlitePool,          // 核心业务数据库
    vector_store: Arc<dyn VectorStore>, // 向量存储（多后端支持）
}

// 零成本克隆（内部 Arc）
#[derive(Clone)]
struct Storage;
```
> 当前实现参考：[vector_search 模块 + dao/memory vector](src/service/dao/memory/)

### 构造方法

| 方法 | 场景 |
|------|------|
| `Storage::new(config: &DatabaseConfig)` | 生产环境，根据配置选择后端 |
| `Storage::with_sqlite_pool(pool)` | 测试专用，默认使用 InMemory 后端 |
| `storage::init_for_test()` | 全局测试初始化 |

---

## 测试隔离保证

1. **DAO 独立单例**：每个 DAO 维护自己的 `OnceLock` 单例，测试间互不影响
2. **Storage::with_sqlite_pool()**：接受外部传入的 pool，保证测试数据隔离
3. **向量存储复用测试上下文**：测试环境下向量和业务数据使用相同的测试隔离
4. **混合搜索可测试**：关键词和向量搜索可独立测试，也可联合测试

---

## 提交记录

| Commit | 说明 |
|--------|------|
| `fb05c7d` | feat: 向量存储基础设施 + Storage 重构 |
| `6bb9a0c` | fix: 修复时间戳测试使用毫秒而非秒的问题 |
| `...` | refactor: 移除 SQLite VSS 依赖，改用纯 Rust InMemory 实现 |
| `...` | feat: 新增 LanceDB 向量存储后端支持 |
| `...` | refactor: SkillDao 拆分基础数据与向量索引（同目录双 DAO 模式） |
| `0dc9f59` | refactor: 重命名 skill vector dao 文件与结构体，移除 sqlite 前缀 |
| `391faec` | feat: Phase 4C 技能系统增强 + 记忆 FTS5 搜索 |
| `30b78d6` | feat: 全实体 FTS5 搜索改造 - 6 实体混合搜索 + 三态匹配 |
| `5f7694b` | feat: 向量搜索增强 - HNSW 索引 + Embedding Provider 唯一性 + Switch 接口 |
| `0102853` | feat: 索引重建全链路 - VectorDao clear_collection + DAL rebuild_vectors + Domain 编排 |
| `7e472d5` | feat: 前端 Switch Embedding Provider 适配 + 错误响应通用化 |
| `a40e622` | feat: 新增 hnsw_index_dir 配置项 |
| `06568fd` | feat: HNSW 索引持久化（bincode 加载/保存/定时落盘/Drop） |
| `858ebbb` | feat: 新增 RebuildProgressResponse DTO 和 RebuildInProgress 错误码 |
| `2de7792` | feat: 索引重建异步化（后台任务 + 进度查询 + 并发控制） |
| `e4d9a6f` | feat: 新增 rebuild progress handler 和路由 |
| `b29a4d8` | fix: Message 向量搜索修复 + 补充向量搜索集成测试 |
| `c131a9a` | test: Agent awaken 集成测试（Consumer 编排 + Mock + 真实 LLM） |
| `39a9cbb` | test: Project/Task 向量搜索集成测试 |

---

## 2026-07-16 增强内容

### HNSW 向量存储后端

新增 `HnswStore` 向量存储后端，基于 `instant-distance` 0.6.1 库：

- 纯 Rust 实现，零系统依赖
- lazy rebuild 策略（写入时标记 dirty，搜索时按需重建）
- 支持余弦距离（`1 - cos(θ)`）
- 内存驻留 + 持久化支持（见下文）

**注意**：`instant-distance` 0.6.1 不支持增量插入，因此采用 lazy rebuild 策略。

### HNSW 索引持久化

新增 HNSW 索引持久化能力，进程重启后无需 lazy rebuild：

- **配置项**：`database.hnsw_index_dir`（默认 `hnsw_index`，相对于 `base_data_path`）
- **存储格式**：bincode 2.0 序列化，每个 collection 一个文件（`<collection>.bincode`）
- **落盘策略**：后台 60s 定时扫描 dirty flag 落盘 + `Drop` 时同步兜底落盘
- **冷启动**：`HnswStore::new()` 扫描目录加载已有索引，避免冷启动 lazy rebuild
- **VectorStore trait**：新增 `flush()` 方法（默认空实现，HnswStore 覆写）

### Embedding Provider 唯一性约束

同一时刻只能有一个 Embedding 类型的 Provider 处于启用状态：

- Domain 层校验：`update_model_provider` 时检测冲突
- 冲突时返回 `409 Conflict` + 当前 Provider 信息
- 前端弹出确认对话框，用户二次确认后调用 switch 接口

### Switch Embedding Provider 接口

`POST /api/v1/finance/model-providers/:id/switch`

- 原子操作：禁用旧 Provider → 启用新 Provider → 启动后台异步重建任务
- 返回 `task_id`：前端通过 task_id 轮询进度
- 索引重建（后台异步）：清空 7 个 collection → 查全量 PO → 逐条 embed + upsert
- 前端适配：API 客户端 + 列表页/详情页确认对话框

### 索引重建异步化

新增向量索引重建异步化能力，避免 switch 接口阻塞：

- **switch 接口**：立即返回 `task_id`，后台 spawn tokio 任务执行重建
- **进度查询**：`GET /api/v1/finance/model-providers/rebuild-progress?task_id=xxx`
- **并发控制**：同一时刻仅允许一个重建任务，已有任务运行时新 switch 返回 `409 RebuildInProgress`
- **进度结构**：当前实体、实体索引、已处理记录数、总记录数、状态、错误信息
- **任务状态**：Pending / Running / Completed / Failed
- **容错**：单个实体重建失败仅记日志，不中断整体流程

### 分层架构

```
Domain: switch_embedding_provider()
    → start_rebuild_task() spawn 后台任务
    → run_rebuild_task() 依次调用各 DAL

DAL: rebuild_vectors(ctx)
    → vector_dao.clear_collection()
    → 主 DAO.query() 查全量 PO
    → cortex_dao.embed_entity() 生成 embedding
    → vector_dao.upsert_vector() 写入向量

VectorDao: clear_collection()
    → VectorStore.clear_collection()

VectorStore: clear_collection() + upsert() + flush()
```

### 配置变更

`VectorStoreType` 枚举扩展：

```rust
pub enum VectorStoreType {
    #[default]
    LanceDb,     // 默认，生产级
    InMemory,    // 零依赖，用于测试
    Hnsw,        // 纯 Rust HNSW 索引
    SqliteVss,   // SQLite VSS 扩展
}
```
> 当前实现参考：[vector_search 模块 + dao/memory vector](src/service/dao/memory/)

---

*最后更新：2026-08-04*

---

## 附：向量搜索设计原则（来自早期设计 V1.3）

> 本节摘录自早期设计文档 V1.3 的设计原则，作为架构决策的历史背景与最佳实践沉淀。具体实现细节以正文为准。

- **Storage 与 Config 解耦**：依赖方向 `main → Config → Storage`，Storage 不反向依赖全局 Config；新增数据库配置只需扩展 `DatabaseConfig` 字段，`Storage::new()` 签名不变（开闭原则）；测试专用构造器保证 Storage 可独立初始化
- **Vectorizable Trait 信息专家原则**：PO 自己决定哪些字段参与向量化，将向量化字段知识封装在 PO 内部，DAL 层无需感知 PO 字段结构；默认实现 SHA256 `content_hash` + 永不过期 `expire_at` + `needs_reindex` 基于哈希比对；修改向量化字段组合只需改 `vectorize_text()` 一处
- **CortexDao.embed() 一站式向量化**：输入 `ModelProviderPo` + 实现 `Vectorizable` 的实体，输出完整 `VectorIndexParams`（向量 + content_hash + provider_id + embedding_model + expire_at），调用方零感知；内部封装调用 LLM Embedding API + 组装参数的逻辑收敛
- **优雅降级模式**：向量化失败不影响核心写入功能，`embed().await.ok()` + 仅 warn 降级日志；向量搜索不可用时 FTS5 关键词搜索仍可用，业务读写主流程不受影响
- **内容哈希校验避免重复向量化**：update 时通过 `needs_reindex(existing_hash)` 判断内容是否变化，内容未变化时跳过 Embedding API 调用，节省 API 成本；哈希算法默认 SHA256，基于 `vectorize_text()` 内容计算
- **路径统一管理**：所有向量数据统一存储在 `{base_data_path}/vectors/` 目录下；集合命名规范 `vss_{collection}` 表 / `<collection>.bincode` 文件；切换后端时路径策略保持一致，便于迁移
- **构造器分离原则**：默认 `new()` 零依赖后端 + 可选 `with_xxx()` 不同后端使用独立构造器不污染主接口 + 测试专用 `with_sqlite_pool()` 接受外部 pool 保证隔离
- **搜索查询结构复用**：`XxxSearch { query_vector, filters: XxxQuery }` 模式直接复用现有 Query 做业务过滤，零代码重复；执行流程：向量检索拿候选 ID → 业务条件过滤 → 组合结果按相似度排序
- **搜索结果元信息嵌入**：`VectorMatchInfo` 不含泛型可嵌入任何业务实体；普通查询返回 `None`，向量搜索返回 `Some`（含 distance / embedding_model / indexed_at / content_hash），调用方无需二次查询
- **未来扩展方向**（参考）：向量量化压缩（f32 → f16 → int8 渐进式压缩）、多向量字段（同实体多维度索引）、向量版本管理（Embedding 模型升级平滑迁移）、向量缓存层（相同内容结果缓存降低 API 调用）、向量质量监控（命中率 / 相似度分布 / 过期清理统计）

---

## 五、扩展模式

### 5.1 新增向量存储后端（如 Qdrant / Milvus / PgLance）
当前 `VectorStore` trait 有 4 个实现：LanceDB 默认 / HNSW / InMemory / SQLite VSS。新增后端：
1. 在 `src/pkg/storage/vector.rs` 实现 `VectorStore` trait，保持构造器分离原则（默认 new() + 专用 with_xxx()），参考现有 trait：[storage/vector.rs](src/pkg/storage/vector.rs)
2. 集合路径与命名规范沿用 `{base_data_path}/vectors/` + `vss_{collection}` 约定，切换后端时上层 DAO/DAL 零改动，参考：[Storage 初始化入口](src/pkg/storage)
3. 写入/删除失败时保持优雅降级（ok() + warn），绝不阻断业务写入主流程；搜索不可用时回退到 FTS5 关键词通道，参考：[dal 层调用点](src/service/dal)

### 5.2 新增 Vectorizable 实体或扩展现有向量化字段组合
当前已有 AgentPo / ToolPo / TaskPo / SkillPo / ShortTermMemoryIndexPo / LongTermKnowledgeNodePo 实现 Vectorizable。新增实体：
1. 在 PO 上直接 `impl Vectorizable for XxxPo`，向量化字段组合由 PO 自身决定（信息专家原则），禁止在 DAL 层手工 format! 拼接文本，参考：[models/vector.rs](src/models/vector.rs)
2. DAL create/update 后统一调用 `embed_entity(ctx, cortex, po)` 完成索引（含 content_hash 去重与 expire_at），禁止绕过通用 embed_entity 单独向量化，参考：[embed_entity 定义](src/pkg/storage/vector.rs)
3. 对应混合搜索 DAO 查询中，过滤条件复用现有 Query 的 push_query_filters，保持「向量候选 → 业务过滤 → 融合排序」三段流程一致，参考：[dao 层 query 实现](src/service/dao)