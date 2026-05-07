# 向量搜索架构设计文档

## 概述

本设计文档描述 ai_orz 项目中向量搜索能力的架构实现，为多 Agent 协作系统提供语义索引和检索能力。

向量搜索作为**语义索引附加层**，与现有关系型数据完全解耦，仅通过 ID 关联，不破坏现有业务表结构。

---

## 核心设计原则

### 1. 索引与数据完全分离

- ✅ 向量数据物理隔离：存储在独立的 `ai_orz_vector.db` SQLite 数据库文件
- ✅ 核心业务表零侵入：不修改任何现有业务表结构
- ✅ 通过 `source_id`（业务表 UUID）跨库关联
- ✅ 未来切换到专业向量数据库（Qdrant 等）业务层零感知

### 2. 触发式非全量索引

- 不对所有数据强制向量化，节省 Embedding Token 成本
- 由各业务 DAO 自主决定：
  - **什么时候索引**（创建/更新时？还是后台异步？）
  - **索引什么内容**（name + description? 还是全部字段？）
  - **用什么模型**（text-embedding-3-small / large？）
  - **维度是多少**（1536 / 3072？）
  - **过期策略**（永不过期？还是 TTL？）

### 3. 渐进式实现，可平滑升级

- 当前阶段：SQLite + VSS 扩展（零运维成本，开箱即用）
- 未来：可平滑升级到 Qdrant / pgvector 等专业向量数据库，上层接口不变

---

## 架构分层

### 职责边界

| 层级 | 职责 | 位置 |
|------|------|------|
| **pkg/storage/vector.rs** | 通用向量索引层，纯底层能力，无业务逻辑<br>只懂向量增删查改，不知道"技能"/"记忆"是什么 | `src/pkg/storage/vector.rs` |
| **业务 DAO**（SkillDao 等） | 决定业务逻辑：什么时候索引、索引什么内容、用什么模型 | `src/service/dao/skill/mod.rs` |
| **业务 Domain** | 组合业务流程：如"创建技能后，调用 Embedding + 向量索引" | `src/service/domain/skill/mod.rs` |

### 调用链路示例

```
业务操作（创建技能）
    ↓
SkillDomain.create_skill()
    ↓
SkillDao.insert()  →  写入关系型数据库
    ↓
SkillDao.index_skill()
    ├─ 调用 Cortex 生成 Embedding
    ├─ 计算内容 Hash
    └─ 调用 ctx.vector_store().upsert()  →  写入向量数据库
```

---

## 数据结构设计

### 1. 向量元数据表（SQLX 迁移管理）

**位置**：`migrations/20260505000000_vector_metadata.sql`

| 字段 | 类型 | 说明 |
|------|------|------|
| collection | TEXT | 集合名称，如 skills, memories, tasks |
| source_id | TEXT | 业务表 UUID，跨库关联主键 |
| content_hash | TEXT | 内容哈希，用于判断是否需要重索引 |
| model | TEXT | 使用的 Embedding 模型名称 |
| dimensions | INTEGER | 向量维度 |
| indexed_at | INTEGER | 索引时间（unix 秒级时间戳） |
| expire_at | INTEGER | 过期时间（NULL 表示永不过期） |

**主键**：(collection, source_id)

### 2. VSS 虚拟表（动态创建）

每个集合一张虚拟表，命名规则：`vss_{collection}`

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS vss_skills USING vss0(embedding(1536));
```

- 由 `create_collection()` 动态创建
- 数量和维度灵活，新增集合不需要改迁移
- 通过 `rowid` 与元数据表关联

---

## 核心 API

### 获取向量存储实例

```rust
let vector_store = ctx.vector_store();
```

### 1. 创建向量集合

```rust
/// 创建向量集合（按领域分表）
/// 幂等，重复调用安全
async fn create_collection(collection: &str, dimensions: i32) -> Result<()>

// 示例：创建技能向量集合，1536 维度
vector_store.create_collection("skills", 1536).await?;
```

### 2. 插入/更新向量

```rust
/// 插入或更新向量索引
async fn upsert(
    collection: &str,
    source_id: &str,
    vector: &[f32],
    content_hash: &str,
    model: &str,
    expire_at: Option<i64>,
) -> Result<()>

// 示例
vector_store.upsert(
    "skills",
    "skill_001",
    &vector,
    "content_hash_abc",
    "text-embedding-3-small",
    None,
).await?;
```

### 3. 语义搜索

```rust
/// 语义搜索，返回 (source_id, distance) 列表
/// distance 越小越相似
async fn search(
    collection: &str,
    query_vector: &[f32],
    top_k: i32,
) -> Result<Vec<(String, f32)>>

// 示例：搜索最相似的 10 个技能
let results = vector_store.search("skills", &query_vector, 10).await?;
for (source_id, distance) in results {
    println!("技能 {} 相似度 {}", source_id, 1.0 - distance);
}
```

### 4. 检查是否需要重索引

```rust
/// 检查内容是否变化，是否需要重索引
async fn needs_reindex(
    collection: &str,
    source_id: &str,
    current_content_hash: &str,
) -> Result<bool>

// 示例：更新技能前先检查
if vector_store.needs_reindex("skills", "skill_001", &new_hash).await? {
    // 需要重索引
}
```

### 5. 删除向量

```rust
/// 删除向量索引
async fn delete(collection: &str, source_id: &str) -> Result<()>

// 示例
vector_store.delete("skills", "skill_001").await?;
```

---

## 迁移策略

### 混合模式：迁移 + 动态创建，互不冲突

| 表类型 | 管理方式 | 原因 |
|--------|----------|------|
| **vector_metadata** | ✅ SQLX 迁移文件 | 只有 1 张全局表，结构固定 |
| **vss_{collection}** | ✅ `create_collection()` 动态创建 | 数量不确定，维度取决于模型，灵活优先 |

### 幂等保证

- 迁移文件使用 `CREATE TABLE IF NOT EXISTS`
- 动态创建使用 `CREATE VIRTUAL TABLE IF NOT EXISTS`
- 两者完全不冲突，谁先执行都可以

---

## Storage 门面设计

### 统一入口

```rust
// Storage 内部持有两个连接池：
struct StorageInner {
    sqlite: SqlitePool,    // 核心业务数据库
    vector: SqliteVssStore, // 向量数据库
}

// 零成本克隆（内部 Arc）
#[derive(Clone)]
struct Storage;
```

### 构造方法

| 方法 | 场景 |
|------|------|
| `Storage::new(db_path, vector_db_path)` | 生产环境，初始化连接池 |
| `Storage::with_sqlite_pool(pool)` | 测试专用，保证数据隔离 |
| `storage::init_for_test()` | 全局测试初始化 |

### RequestContext 集成

```rust
// RequestContext 只持有 storage，不持有独立的 db_pool
pub struct RequestContext {
    storage: Storage,
    // ... 其他字段
}

// 向后兼容接口
impl RequestContext {
    pub fn db_pool(&self) -> &SqlitePool {
        self.storage.sqlite_pool()
    }
    
    pub fn vector_store(&self) -> SqliteVssStore {
        self.storage.vector()
    }
}
```

---

## 业务接入示例

### 技能语义搜索

在 `SkillDao` 中实现：

```rust
impl SkillDao {
    /// 创建技能并建立向量索引
    pub async fn create_with_index(&self, ctx: &RequestContext, skill: &Skill) -> Result<Skill> {
        // 1. 写入关系型数据库
        let skill = self.insert(ctx, skill).await?;
        
        // 2. 构建索引文本
        let text = format!("{} {}", skill.name, skill.description);
        
        // 3. 生成 Embedding（调用 Cortex）
        let vector = cortex_service::embed(&skill.model_provider_id, &text).await?;
        
        // 4. 计算内容哈希
        let content_hash = sha256(&text);
        
        // 5. 写入向量索引
        ctx.vector_store().upsert(
            "skills",
            &skill.id,
            &vector,
            &content_hash,
            "text-embedding-3-small",
            None,
        ).await?;
        
        Ok(skill)
    }
    
    /// 语义搜索技能
    pub async fn search_semantic(&self, ctx: &RequestContext, query: &str, top_k: i32) -> Result<Vec<Skill>> {
        // 1. 生成查询向量
        let query_vector = cortex_service::embed(&model_provider_id, query).await?;
        
        // 2. 向量搜索
        let results = ctx.vector_store().search("skills", &query_vector, top_k).await?;
        
        // 3. 通过 source_id 从关系型数据库获取完整数据
        let skill_ids: Vec<&str> = results.iter().map(|(id, _)| id.as_str()).collect();
        let skills = self.batch_get(ctx, &skill_ids).await?;
        
        Ok(skills)
    }
}
```

---

## 配置项

在 `common/src/config.rs` 中新增：

```rust
pub struct DatabaseConfig {
    pub db_file_name: String,              // 核心业务数据库
    pub vector_db_file_name: String,       // 向量数据库（物理隔离）
}

impl AppConfig {
    pub fn db_path(&self) -> PathBuf;
    pub fn vector_db_path(&self) -> PathBuf;
}
```

默认值：
- 核心数据库：`ai_orz.db`
- 向量数据库：`ai_orz_vector.db`

---

## 测试隔离保证

1. **Storage::with_sqlite_pool()**：接受外部传入的 pool，保证测试数据隔离
2. **`new_simple()` 接口不变**：所有现有测试零改动
3. **向量存储复用测试 pool**：测试环境下向量和业务数据使用同一个连接池

---

## 优雅降级机制

- SQLite VSS 扩展加载失败时自动降级到**内存计算模式**
- 核心功能（向量存储和检索）不受影响，仅性能差异
- 不影响业务正常运行

---

## 提交记录

| Commit | 说明 |
|--------|------|
| `fb05c7d` | feat: 向量存储基础设施 + Storage 重构 |
| `6bb9a0c` | fix: 修复时间戳测试使用毫秒而非秒的问题 |

---

*最后更新：2026-05-05*
