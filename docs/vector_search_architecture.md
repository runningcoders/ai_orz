# 向量搜索与混合搜索架构设计文档

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

```rust
async fn search(&self, ctx, params) -> Result<Vec<Entity>> {
    // 1. FTS5 关键词搜索（Base Dao）
    let keyword_results = self.base_dao.search_tools(ctx.clone(), params).await?;

    // 2. 向量语义搜索（Vector Dao）
    let query_vector = self.embed_query(&keyword).await?;
    let vector_hits = self.vector_dao.search_vector(ctx.clone(), &query_vector, top_k).await?;

    // 3. 结果合并，标记 MatchType
    let mut results = self.merge_and_tag(keyword_results, vector_hits);

    // 4. 三级排序
    results.sort_by(|a, b| /* Hybrid > Vector > Keyword, 组内细排 */);

    Ok(results)
}
```

### 向量索引生命周期

DAL 层负责向量索引的完整生命周期管理（FTS5 由数据库触发器自动维护）：

| 操作 | FTS5 索引 | 向量索引 | 负责层 |
|------|-----------|----------|--------|
| **创建** | 触发器自动同步 | create 后主动 upsert | DAL 层 |
| **更新** | 触发器自动同步 | content_hash 变化时重新 upsert | DAL 层 |
| **删除/归档** | 触发器自动同步 | 主动 delete 清理 | DAL 层 |

**降级策略：** 向量索引写入失败时仅 warn 降级，不影响主流程（FTS5 仍可用）。

### 已覆盖实体

| 实体 | FTS5 搜索 | 向量搜索 | 混合搜索 |
|------|-----------|----------|----------|
| Memory（短期记忆/知识节点） | ✅ | ✅ | ✅ |
| Skill（技能） | ✅ | ✅ | ✅ |
| Tool（工具） | ✅ | ✅ | ✅ |
| Message（消息） | ✅ | ✅ | ✅ |
| Task（任务） | ✅ | ✅ | ✅ |
| Project（项目） | ✅ | ✅ | ✅ |
| Agent（智能代理） | ✅ | ✅ | ✅ |

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

---

## Skill DAO 拆分设计

### mod.rs 统一入口

```rust
// dao/skill/mod.rs

// 1. Trait 定义（接口契约）
#[async_trait]
pub trait SkillDao: Send + Sync {
    // 纯基础数据 CRUD
    async fn insert(&self, ctx: RequestContext, skill: &SkillPo) -> Result<()>;
    async fn update(&self, ctx: RequestContext, skill: &SkillPo) -> Result<()>;
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<SkillPo>>;
    // ... 其他基础方法
}

#[async_trait]
pub trait SkillVectorDao: Send + Sync {
    // 纯向量索引 CRUD
    async fn upsert_vector(&self, ctx: RequestContext, row: VectorRow) -> Result<()>;
    async fn search_vector(&self, ctx: RequestContext, collection: &str, vector: &[f32], top_k: i32) -> Result<Vec<VectorSearchHit>>;
    async fn get_vector_row(&self, ctx: RequestContext, collection: &str, source_id: &str) -> Result<Option<VectorRow>>;
}

// 2. 子模块构造函数别名（用于 DAL 层组合）
pub use sqlite::{dao as base_dao, new as new_skill_dao};
pub use vector::{dao as vector_dao, new as new_skill_vector_dao};

// 3. 统一初始化所有 Skill DAO 单例
pub fn init() {
    sqlite::init();
    vector::init();
}
```

### sqlite.rs - 基础数据 DAO

```rust
// dao/skill/sqlite.rs
// 只负责基础数据 CRUD，完全不感知向量存在

#[derive(Debug, Clone)]
pub struct SkillDaoSqliteImpl;

#[async_trait]
impl SkillDao for SkillDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, skill: &SkillPo) -> Result<()> {
        // 纯 SQLite 插入逻辑
    }
    // ... 其他基础方法
}
```

### vector.rs - 向量索引 DAO

```rust
// dao/skill/vector.rs
// 只负责向量索引，不碰业务数据

#[derive(Debug, Clone)]
pub struct SkillVectorDaoImpl;

#[async_trait]
impl SkillVectorDao for SkillVectorDaoImpl {
    async fn upsert_vector(&self, ctx: RequestContext, row: VectorRow) -> Result<()> {
        ctx.storage().vector_store().insert_or_update(&row).await
    }
    
    async fn search_vector(&self, ctx: RequestContext, collection: &str, vector: &[f32], top_k: i32) -> Result<Vec<VectorSearchHit>> {
        ctx.storage().vector_store().search(collection, vector, top_k).await
    }
    
    async fn get_vector_row(&self, ctx: RequestContext, collection: &str, source_id: &str) -> Result<Option<VectorRow>> {
        ctx.storage().vector_store().get(collection, source_id).await
    }
}
```

---

## Domain 层组合示例

```rust
// domain/skill/mod.rs

pub struct SkillDalImpl {
    base_dao: Arc<dyn SkillDao>,
    vector_dao: Arc<dyn SkillVectorDao>,
}

impl SkillDal for SkillDalImpl {
    async fn create(&self, ctx: RequestContext, skill: SkillPo) -> Result<SkillPo> {
        // 1. 写入基础数据
        self.base_dao.insert(ctx.clone(), &skill).await?;
        
        // 2. 构建索引文本
        let text = format!("{} {}", skill.name, skill.description);
        
        // 3. 生成 Embedding
        let vector = self.generate_embedding(&text).await?;
        let content_hash = sha256(&text);
        
        // 4. 写入向量索引
        let row = VectorRow {
            collection: "skills".to_string(),
            source_id: skill.id.clone(),
            vector,
            content_hash,
            model: "text-embedding-3-small".to_string(),
            indexed_at: now(),
        };
        self.vector_dao.upsert_vector(ctx, row).await?;
        
        Ok(skill)
    }
    
    async fn hybrid_search(&self, ctx: RequestContext, query: &str, top_k: i32) -> Result<Vec<SkillPo>> {
        // 1. 关键词搜索（基础 DAO）
        let keyword_results = self.base_dao.search(ctx.clone(), query).await?;
        
        // 2. 向量搜索（向量 DAO）
        let query_vector = self.generate_embedding(query).await?;
        let vector_hits = self.vector_dao.search_vector(ctx, "skills", &query_vector, top_k).await?;
        let vector_ids: Vec<&str> = vector_hits.iter().map(|h| h.source_id.as_str()).collect();
        let vector_results = self.base_dao.batch_get(ctx, &vector_ids).await?;
        
        // 3. 结果合并去重
        Ok(self.merge_results(keyword_results, vector_results))
    }
}
```

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

---

*最后更新：2026-07-12*
