# 向量搜索系统设计文档

> **最后更新**：2026-05-11
> **状态**：✅ 已实现 V1（纯 Rust 线性搜索），🔄 V2 优化待实施（HNSW + FastEmbed）
> **对应代码**：Skill Dao / Skill Dal / Cortex Dao / pkg/storage

---

## 🎯 整体方案概览

### 两层架构设计

```
┌─────────────────────────────────────────────────────────────┐
│                   业务层（DAO/DAL）                          │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐        │
│  │ Skill   │  │ Memory  │  │ Task    │  │ Agent   │  ...   │
│  │ Vector  │  │ Vector  │  │ Vector  │  │ Vector  │        │
│  │ Dao     │  │ Dao     │  │ Dao     │  │ Dao     │        │
│  └─────────┘  └─────────┘  └─────────┘  └─────────┘        │
└──────────────────────────┬──────────────────────────────────┘
                           │
┌──────────────────────────▼──────────────────────────────────┐
│                  技术层（pkg/storage）                        │
│  ┌──────────────────────────────────────────────────────┐  │
│  │              VectorStore Trait（统一接口）              │  │
│  │  • init_collection()  • upsert()  • search()          │  │
│  │  • get_content_hash()  • delete()                     │  │
│  └──────────────────────────────────────────────────────┘  │
│           │                    │                            │
│  ┌────────▼───────┐   ┌────────▼───────┐                   │
│  │ InMemoryStore  │   │   HnswStore    │  ← 可插拔切换      │
│  │ （已实现 V1）   │   │ （优化 V2）    │                   │
│  │ 线性搜索        │   │ HNSW 近似搜索   │                   │
│  │ 余弦距离        │   │ + FastEmbed    │                   │
│  └─────────────────┘   └─────────────────┘                   │
└─────────────────────────────────────────────────────────────┘
```

### V1 已实现（当前版本）
- **✅ 纯 Rust 线性搜索**：完整的向量存储 + 余弦相似度计算
- **✅ Bincode 持久化**：懒加载模式，自动保存到磁盘
- **✅ 分层解耦**：严格遵循 DAO/DAL 分层，无跨层耦合
- **✅ 向后兼容**：SkillDal 所有调用点保持不变

### V2 优化方案（HNSW + FastEmbed）
- **🔄 HNSW 近似搜索**：高性能向量索引，支持百万级数据
- **🔄 FastEmbed 本地向量化**：零 API 成本，不需要调用远程 LLM
- **🔄 Feature Flag 切换**：V1/V2 可无缝切换，上层零改动

---

## 🎯 核心设计原则

1. **严格分层，边界清晰** — DAO 只处理 PO，DAL 负责协调
2. **Trait 驱动，逻辑收敛** — Vectorizable Trait 统一向量化行为
3. **向后兼容，增量增强** — 原方法只加可选参数，不破坏现有调用
4. **信息完整，使用便利** — 搜索结果实体自带匹配元信息
5. **可插拔实现** — V1 线性搜索 / V2 HNSW 通过 Feature Flag 自由切换

---

## 📦 阶段 1：通用向量数据结构

**文件位置**：`src/models/vector.rs`

### 1.1 VectorIndexParams — 向量索引参数

所有 DAO 复用，用于 create/update 时传入向量索引信息：

```rust
#[derive(Debug, Clone)]
pub struct VectorIndexParams {
    pub vector: Vec<f32>,              // 向量数据
    pub content_hash: String,           // 内容哈希（SHA256）
    pub model_provider_id: String,      // 生成该向量的 ModelProvider ID
    pub embedding_model: String,        // 使用的模型名称
    pub expire_at: Option<i64>,         // 过期时间（None 表示永不过期）
}
```

### 1.2 VectorMatchInfo — 向量匹配元信息

不含泛型，可嵌入任何业务实体，用于搜索结果返回：

```rust
#[derive(Debug, Clone)]
pub struct VectorMatchInfo {
    pub distance: f32,                  // 相似度距离（越小越相似）
    pub embedding_model: String,         // 使用的模型
    pub indexed_at: i64,                // 索引创建时间（Unix 时间戳）
    pub content_hash: String,            // 内容哈希（用于判断是否过时）
}
```

### 1.3 VectorSearchResult<T> — 向量搜索结果包装器

通用泛型结构，所有 DAO 搜索返回时使用：

```rust
#[derive(Debug, Clone)]
pub struct VectorSearchResult<T> {
    pub entity: T,                      // 业务 PO 对象
    pub match_info: VectorMatchInfo,    // 匹配元信息
}
```

### 1.4 Vectorizable Trait — 可向量化实体接口

统一所有可向量化实体的行为，实现该 Trait 即可获得自动向量化能力：

```rust
pub trait Vectorizable {
    // ===== 必须实现 =====
    
    /// 生成待向量化的文本内容
    /// 由实体自己决定：哪些字段需要被向量化
    fn vectorize_text(&self) -> String;
    
    /// 向量集合名称（对应 vss_{collection} 表）
    fn vector_collection() -> &'static str
    where
        Self: Sized;
    
    // ===== 默认实现（不需要重写） =====
    
    /// 计算内容哈希（默认 SHA256）
    fn vector_content_hash(&self) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(self.vectorize_text());
        format!("{:x}", hasher.finalize())
    }
    
    /// 向量过期时间（可选覆盖，默认永不过期）
    fn vector_expire_at(&self) -> Option<i64> {
        None
    }
    
    /// 判断内容是否变化，是否需要重索引
    fn needs_reindex(&self, existing_hash: &str) -> bool {
        self.vector_content_hash() != existing_hash
    }
}
```

---

## 🧠 阶段 2：CortexDao — 向量化核心能力

**文件位置**：`src/service/dao/cortex/mod.rs`

### 核心设计要点

- 只接收 `ModelProviderPo`（强化 DAO 边界）
- 接受实现了 `Vectorizable` 的实体（逻辑收敛）
- 直接返回完整的 `VectorIndexParams`（一站式输出）

### 方法定义

```rust
impl CortexDao {
    /// ✅ 向量化完整流程（逻辑收敛，一站式输出）
    ///
    /// 输入：ModelProviderPo + 实现 Vectorizable 的业务实体
    /// 输出：完整的 VectorIndexParams，可直接传给 DAO 使用
    async fn embed<T: Vectorizable>(
        &self,
        _ctx: RequestContext,
        provider_po: &ModelProviderPo,
        entity: &T,
    ) -> Result<VectorIndexParams> {
        // 1. 实体自己提供向量化文本
        let text = entity.vectorize_text();
        
        // 2. 调用 LLM Embedding API
        let vector = self.call_embedding_api(provider_po, &text).await?;
        
        // 3. 组装完整参数（实体自己计算哈希和过期时间）
        Ok(VectorIndexParams {
            vector,
            content_hash: entity.vector_content_hash(),
            model_provider_id: provider_po.id.clone(),
            embedding_model: provider_po.embedding_model.clone(),
            expire_at: entity.vector_expire_at(),
        })
    }
    
    /// 内部：实际调用 Embedding API
    async fn call_embedding_api(&self, provider: &ModelProviderPo, text: &str) -> Result<Vec<f32>> {
        let client = openai::Client::builder()
            .api_key(&provider.api_key)
            .base_url(provider.base_url.as_deref().unwrap_or_default())
            .build()?;
        
        let result = client.embeddings()
            .model(&provider.embedding_model)
            .create(text)
            .await?;
        
        Ok(result.data[0].embedding.clone())
    }
    
    /// ✅ 纯文本向量化（用于搜索场景）
    async fn embed_text(
        &self,
        provider_po: &ModelProviderPo,
        text: &str,
    ) -> Result<Vec<f32>> {
        self.call_embedding_api(provider_po, text).await
    }
}
```

---

## 🗄️ 阶段 3：SkillDao — 持久化层增强

**文件位置**：`src/service/dao/skill/mod.rs`

### 核心设计要点（2026-05-11 更新）

- ✅ **基础数据与向量操作完全解耦** — DAO 职责单一
- ✅ **Trait 分组清晰** — 基础方法和向量方法分开声明，方便管理阅读
- ✅ **向量存储完全封装在 DAO 内部** — DAL 不直接访问 vector_store
- ✅ **复用现有 SkillQuery 做业务过滤** — 零代码重复
- ✅ **向后兼容** — 基础方法保留，不影响现有非向量场景调用

### 设计原则

1. **基础 CRUD 方法：`create` / `update` — 纯关系型数据操作
2. **向量增强方法**：`create_with_vector` / `update_with_vector` — 新增独立方法，内部调用原基础方法
3. **Trait 分组清晰**：基础方法和向量方法分开声明，方便管理和阅读

### 3.1 SkillSearch — 向量搜索查询结构

```rust
/// 技能向量搜索查询（命名简洁）
#[derive(Debug)]
pub struct SkillSearch {
    pub query_vector: Vec<f32>,    // 查询向量（DAL 层填充）
    pub top_k: i32,                 // 返回 Top K 结果
    pub filters: SkillQuery,        // ✅ 直接复用现有 SkillQuery 做业务过滤
}
```

### 3.2 DAO Trait 声明（分组清晰）

```rust
#[async_trait]
pub trait SkillDao: Send + Sync {
    // ---------- 🔵 基础 CRUD 方法（纯关系型数据） ----------
    async fn create(&self, ctx: RequestContext, skill: &SkillPo) -> Result<SkillPo, AppError>;
    async fn update(&self, ctx: RequestContext, skill: &SkillPo) -> Result<SkillPo, AppError>;
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;
    async fn get(&self, ctx: RequestContext, id: &str) -> Result<Option<SkillPo>, AppError>;
    async fn query(&self, ctx: RequestContext, query: SkillQuery) -> Result<Vec<SkillPo>, AppError>;
    
    // ---------- 🟢 向量增强方法（独立分组） ----------
    async fn create_with_vector(&self, ctx: RequestContext, skill: &SkillPo, vector_index: &VectorIndexParams) -> Result<SkillPo, AppError>;
    async fn update_with_vector(&self, ctx: RequestContext, skill: &SkillPo, vector_index: &VectorIndexParams) -> Result<SkillPo, AppError>;
    async fn search_vector(&self, ctx: RequestContext, query: SkillSearch) -> Result<Vec<VectorSearchResult<SkillPo>>, AppError>;
    async fn get_vector_content_hash(&self, ctx: RequestContext, skill_id: &str) -> Result<Option<String>, AppError>;
}
```

### 3.3 DAO 实现

```rust
impl SkillDao for SkillDaoSqliteImpl {
    // ---------- 🔵 基础 CRUD 方法（不变，保持原有实现） ----------
    async fn create(&self, ctx: RequestContext, skill: &SkillPo) -> Result<SkillPo, AppError> {
        // 原有纯关系型数据插入
    }
    
    async fn update(&self, ctx: RequestContext, skill: &SkillPo) -> Result<SkillPo, AppError> {
        // 原有纯关系型数据更新
    }
    
    // ---------- 🟢 向量增强方法（新增，内部调用基础方法） ----------
    async fn create_with_vector(&self, ctx: RequestContext, skill: &SkillPo, vector_index: &VectorIndexParams) -> Result<SkillPo, AppError> {
        // 1. 先调用基础方法写入关系数据
        let skill = self.create(ctx.clone(), skill).await?;
        
        // 2. 再写入向量索引
        ctx.vector_store().upsert(
            "skills", &skill.id, &vector_index.vector,
            &vector_index.content_hash, &vector_index.embedding_model, vector_index.expire_at,
        ).await?;
        
        Ok(skill)
    }
    
    async fn update_with_vector(&self, ctx: RequestContext, skill: &SkillPo, vector_index: &VectorIndexParams) -> Result<SkillPo, AppError> {
        // 1. 先调用基础方法更新关系数据
        let skill = self.update(ctx.clone(), skill).await?;
        
        // 2. 再 upsert 向量索引
        ctx.vector_store().upsert(
            "skills", &skill.id, &vector_index.vector,
            &vector_index.content_hash, &vector_index.embedding_model, vector_index.expire_at,
        ).await?;
        
        Ok(skill)
    }
    
    /// ✅ 向量增强搜索
    ///
    /// 执行流程：先向量检索拿到 source_id → 再按业务条件过滤 → 最终返回
    async fn search_vector(
        &self,
        ctx: RequestContext,
        query: SkillSearch,
    ) -> Result<Vec<VectorSearchResult<SkillPo>>, AppError> {
        // 1. 先做纯向量检索，拿到候选 ID + 相似度
        let vector_results = ctx.vector_store()
            .search("skills", &query.query_vector, query.top_k)
            .await?;
        
        if vector_results.is_empty() {
            return Ok(vec![]);
        }
        
        let skill_ids: Vec<&str> = vector_results.iter()
            .map(|(id, _)| id.as_str())
            .collect();
        let distances: HashMap<String, f32> = vector_results.into_iter().collect();
        
        // 2. ✅ 直接复用 query 方法的过滤逻辑
        let skills = self.query(ctx, SkillQuery {
            ids: Some(skill_ids),  // 注入向量检索出来的 ID
            ..query.filters        // 其他过滤条件直接透传
        }).await?;
        
        // 3. 组合结果，按相似度排序
        let mut results: Vec<_> = skills.into_iter()
            .map(|skill| VectorSearchResult {
                entity: skill.clone(),
                match_info: VectorMatchInfo {
                    distance: distances.get(&skill.id).copied().unwrap_or(1.0),
                    embedding_model: String::new(), // TODO: 从元数据表填充
                    indexed_at: 0,                  // TODO: 从元数据表填充
                    content_hash: String::new(),    // TODO: 从元数据表填充
                },
            })
            .collect();
        
        results.sort_by(|a, b| a.match_info.distance.partial_cmp(&b.match_info.distance).unwrap());
        Ok(results)
    }
    
    /// ✅ 查询技能的向量索引内容哈希
    ///
    /// 封装到 DAO 内部，DAL 不需要知道 vector_store 的存在
    async fn get_vector_content_hash(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<String>, AppError> {
        ctx.vector_store()
            .get_content_hash("skills", skill_id)
            .await
            .map(Some)
            .or_else(|_| Ok(None))
    }
}
```

---

## 🎭 阶段 4：SkillDal — 业务协调层

**文件位置**：`src/service/dal/skill.rs`

### 核心设计要点（2026-05-11 更新）

- ✅ **直接增强原方法** — 不新增 `with_index` 后缀，业务调用方无感知
- ✅ **内部灵活选择** — DAL 内部判断，根据是否有向量选择调用不同 DAO 方法
- ✅ **通过 Trait 实现驱动逻辑** — 实现了 Vectorizable 就自动索引
- ✅ **DAL 负责协调** — 实体 → 文本 → 向量化 → DAO 调用
- ✅ **搜索结果自动转换** — 转换为带 `vector_match` 的业务实体

### 方法实现

```rust
impl SkillDal {
    /// ✅ 创建技能（直接增强原方法，内部判断）
    ///
    /// 逻辑：Skill 实现了 Vectorizable → 自动索引
    /// 优雅降级：向量化失败不影响核心写入功能
    async fn create(&self, ctx: RequestContext, skill: &Skill) -> Result<Skill, AppError> {
        // ✅ 一行调用 CortexDao 完成完整向量化流程
        let vector_index = self.cortex_dao
            .embed(ctx.clone(), &skill.model_provider.po, skill)
            .await
            .ok();  // 优雅降级：失败不影响核心写入
        
        // 根据是否有向量，选择调用不同的 DAO 方法
        let skill_po = match vector_index {
            Some(idx) => self.skill_dao.create_with_vector(ctx, &skill.po, &idx).await?,
            None => self.skill_dao.create(ctx, &skill.po).await?,
        };
        
        Ok(Skill::from_po(skill_po))
    }
    
    /// ✅ 更新技能（直接增强原方法，内部判断）
    async fn update(&self, ctx: RequestContext, skill: &Skill) -> Result<Skill, AppError> {
        // ✅ 通过 DAO 查询，不直接碰 vector_store
        let existing_hash = self.skill_dao
            .get_vector_content_hash(ctx.clone(), &skill.id)
            .await?;
        
        // 判断是否需要重索引
        let vector_index = match existing_hash {
            Some(hash) if !skill.needs_reindex(&hash) => None,  // 内容没变，跳过
            _ => {
                // 内容变化或无索引，重新生成
                self.cortex_dao
                    .embed(ctx.clone(), &skill.model_provider.po, skill)
                    .await
                    .ok()
            }
        };
        
        // 根据是否有向量，选择调用不同的 DAO 方法
        let skill_po = match vector_index {
            Some(idx) => self.skill_dao.update_with_vector(ctx, &skill.po, &idx).await?,
            None => self.skill_dao.update(ctx, &skill.po).await?,
        };
        
        Ok(Skill::from_po(skill_po))
    }
    
    /// ✅ 向量语义搜索
    ///
    /// - DAL 只负责把文本变成向量
    /// - 上游可以自由组装任意复杂的过滤条件
    /// - 返回的 Skill 实体自带 vector_match 元信息
    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_text: &str,
        model_provider: &ModelProvider,
        mut search_query: SkillSearch,
    ) -> Result<Vec<Skill>, AppError> {
        // DAL 只做一件事：文本 → 向量
        let query_vector = self.cortex_dao
            .embed_text(&model_provider.po, query_text)
            .await?;
        
        search_query.query_vector = query_vector;
        
        // 调用 DAO 执行向量搜索
        let results = self.skill_dao.search_vector(ctx, search_query).await?;
        
        // ✅ 自动生成的 Builder API 构建带元信息的实体
        Ok(results.into_iter().map(|r| {
            Skill::builder()
                .po(r.entity)
                .vector_match(Some(r.match_info))
                .build()
                .expect("Skill builder failed")
        }).collect())
    }
    
    /// ✅ 简化版搜索（提供合理默认值）
    async fn search_vector_simple(
        &self,
        ctx: RequestContext,
        query_text: &str,
        model_provider: &ModelProvider,
        top_k: i32,
    ) -> Result<Vec<Skill>, AppError> {
        self.search_vector(
            ctx,
            query_text,
            model_provider,
            SkillSearch {
                query_vector: vec![],  // 会被覆盖
                top_k,
                filters: SkillQuery {
                    exclude_status: Some(SkillStatus::Deleted),  // 默认排除已删除
                    ..Default::default()
                },
            }
        ).await
    }
}
```

---

## 🧬 阶段 5：Skill 业务实体

**文件位置**：`src/models/skill.rs`

### 核心设计要点

- 实现 `Vectorizable` Trait（获得向量化能力）
- 包含可选的 `vector_match` 字段（搜索结果带元信息）
- 使用 Builder 模式构建，适应未来多维度扩展

### 实现代码

```rust
/// Skill 业务实体
#[derive(Debug, Clone)]
pub struct Skill {
    pub po: SkillPo,
    pub model_provider: ModelProvider,
    
    /// ✅ 向量搜索匹配元信息（可选）
    /// - 普通查询返回：None
    /// - 向量搜索返回：Some(包含距离等元信息)
    pub vector_match: Option<VectorMatchInfo>,
    
    // 未来可扩展更多维度字段：
    // pub usage_stats: Option<UsageStats>,
    // pub access_control: Option<AccessControl>,
}

/// ✅ 实现 Vectorizable Trait（统一向量化行为）
impl Vectorizable for Skill {
    fn vectorize_text(&self) -> String {
        // Skill 向量化：名称 + 描述 + 标签
        format!(
            "{} {} {}",
            self.po.name,
            self.po.description,
            self.po.tags.join(" ")
        )
    }
    
    fn vector_collection() -> &'static str {
        "skills"
    }
    
    // 可选：覆盖默认过期时间（例如 30 天后过期）
    // fn vector_expire_at(&self) -> Option<i64> {
    //     Some(chrono::Utc::now().timestamp() + 30 * 86400)
    // }
}

/// ✅ 使用 derive_builder 宏自动生成 Builder 模式
/// 
/// 项目已引入 derive_builder = "0.20"，无需手动实现
/// 自动生成完整的链式 Builder API + 字段校验
use derive_builder::Builder;

#[derive(Debug, Clone, Builder)]
#[builder(default)]  // 支持字段默认值
pub struct Skill {
    pub po: SkillPo,
    pub model_provider: ModelProvider,
    
    /// ✅ 向量搜索匹配元信息（可选）
    /// - 普通查询返回：None
    /// - 向量搜索返回：Some(包含距离等元信息)
    pub vector_match: Option<VectorMatchInfo>,
    
    // 未来可扩展更多维度字段：
    // pub usage_stats: Option<UsageStats>,
    // pub access_control: Option<AccessControl>,
}

/// 默认值实现
impl Default for Skill {
    fn default() -> Self {
        Self {
            po: SkillPo::default(),
            model_provider: ModelProvider::default(),
            vector_match: None,
        }
    }
}

impl Skill {
    /// 从 PO 快速创建实体
    pub fn from(po: SkillPo) -> Self {
        Self {
            po,
            ..Default::default()
        }
    }
}
```

---

## 🎯 完整调用链路展示

```rust
// Domain/Handler 层使用示例
async fn example(ctx: RequestContext) -> Result<()> {
    // 1. 构建技能实体（自动生成的 Builder API）
    let skill = Skill::builder()
        .po(skill_po)
        .model_provider(model_provider)
        .build()?;
    
    // 2. ✅ 一行创建 + 全自动索引
    // 内部自动：文本向量化 → 计算哈希 → 写入向量索引
    let skill = skill_dal.create(ctx.clone(), &skill).await?;
    
    // 3. ✅ 一行搜索
    let results = skill_dal.search(
        ctx,
        "如何写出高性能 Rust",       // 查询文本
        &model_provider,              // 用哪个 ModelProvider
        SkillSearch {
            query_vector: vec![],     // DAL 自动填充
            top_k: 10,
            filters: SkillQuery {
                is_public: Some(true),  // 只搜公开的
                ..Default::default()
            },
        }
    ).await?;
    
    // 4. ✅ 直接使用搜索结果元信息
    for skill in &results {
        if let Some(m) = &skill.vector_match {
            println!("技能: {}", skill.po.name);
            println!("相似度: {:.0}%", (1.0 - m.distance) * 100);
            println!("索引时间: {}", m.indexed_at);
        }
    }
    
    Ok(())
}
```

---

## 📋 实施清单

| 步骤 | 文件 | 核心内容 |
|------|------|---------|
| 1 | `src/models/vector.rs` | 4 个核心结构体 + Vectorizable Trait |
| 2 | `src/models/skill.rs` | Skill 实体 + Builder + Vectorizable 实现 |
| 3 | `src/service/dao/cortex/mod.rs` | `embed()` 方法（Po + Trait → Params） |
| 4 | `src/service/dao/skill/mod.rs` | create/update 增强 + search + get_vector_content_hash |
| 5 | `src/service/dal/skill.rs` | 全自动向量化 create/update/search |
| 6 | 单元测试 | 完整链路验证 + 向后兼容 + 优雅降级 |

---

## ✨ 设计亮点总结

| 亮点 | 说明 |
|------|------|
| 🎯 **逻辑极致收敛** | CortexDao.embed() 一站式完成所有向量化逻辑 |
| 🧩 **Trait 驱动** | Skill 是否实现 Vectorizable 决定是否索引，零硬编码 |
| 🔒 **边界绝对清晰** | DAO 只处理 Po，向量存储封装在 DAO 内部 |
| ♻️ **零代码重复** | SkillSearch 直接复用 SkillQuery，所有过滤能力自动获得 |
| 🚀 **优雅降级** | 向量化失败不影响核心功能，系统继续运行 |
| 📦 **高度可扩展** | Builder 模式支持未来多维度字段，Vectorizable 支持所有实体 |
| ✅ **向后兼容** | 所有现有调用 100% 兼容，不需要修改 |

---

## 🔮 未来扩展方向

1. **批量索引重建工具** — 利用 Vectorizable Trait 实现通用的批量重建工具
2. **更多实体接入** — Memory、Task、Project 等只需实现 Vectorizable Trait
3. **向量缓存层** — 相同内容的向量化结果缓存，降低 API 调用成本
4. **向量质量监控** — 索引命中率、相似度分布、过期清理统计

---

## ✅ 实施完成总结（2026-05-11）

> **完成状态**：100% 完成，已合并到 main 分支  
> **提交**：`a12d3a4` - Skill DAO 分层重构  
> **编译状态**：全项目编译通过 ✅

### 核心架构落地

| 模块 | 状态 | 说明 |
|------|------|------|
| Skill DAO 分层拆分 | ✅ | SkillDao（基础数据）+ SkillVectorDao（向量索引）完全分离 |
| DAL 层业务聚合 | ✅ | create/update/search 全自动向量化，上层无感知 |
| Vectorizable Trait | ✅ | Skill + SkillPo 均实现，统一驱动向量行为 |
| CortexDao embed | ✅ | `embed(&dyn CortexTrait, &dyn Vectorizable)` 一站式向量化 |
| 混合搜索 | ✅ | search() 自动策略路由：向量优先 + 关键词兜底 |

### 文件变更清单

```
src/models/brain.rs                    # CortexTrait 新增元信息 getter
src/service/dao/cortex/mod.rs          # 新增 embed() 方法定义
src/service/dao/cortex/rig.rs          # 完整 embed 实现
src/service/dao/cortex/rig/openai.rs   # Cortex 构造时初始化所有 Rig 对象
src/service/dao/cortex/rig/ollama.rs   # 同上
src/service/dao/cortex/rig/openai_compatible.rs  # 同上
src/service/dao/skill/mod.rs           # 拆分 SkillDao + SkillVectorDao 两个 Trait
src/service/dao/skill/sqlite.rs        # 仅保留基础数据 CRUD，移除向量逻辑
src/service/dao/skill/sqlite_vector.rs # 🌟 新增 - 向量索引独立 DAO 实现
src/service/dal/skill.rs               # DAL 层组合两个 DAO，实现自动向量处理
src/service/dal/skill_test.rs          # 测试适配新架构
```

### 架构演进说明

#### 拆分前（旧架构）
```
SkillDao (sqlite.rs)
├─ 基础数据 CRUD
└─ 向量索引 CRUD + 业务聚合逻辑 ❌
```
- ❌ DAO 承担了过多业务职责
- ❌ 基础数据与向量逻辑耦合
- ❌ 向上层暴露向量细节

#### 拆分后（新架构）
```
DAO 层（仅持久化）
├─ SkillDao (sqlite.rs)         → 基础数据 CRUD
└─ SkillVectorDao (sqlite_vector.rs)  → 向量索引 CRUD

DAL 层（业务聚合）
└─ SkillDal
   ├─ create()  → 自动向量化 + 双表写入
   ├─ update()  → 内容哈希判断 + 智能更新
   └─ search()  → 混合搜索策略路由

✅ 职责清晰：DAO 只做持久化，DAL 做业务协调
✅ 完全解耦：两个 DAO 独立维护，互不影响
✅ 上层透明：调用方完全不知道向量存在
```

### 关键设计决策落地

1. **DAO 单例模式**：两个子 DAO 各自维护独立单例，mod.rs 统一导出
2. **初始化封装**：SkillDal::new() 接收两个 DAO 参数，外部调用简洁
3. **内容哈希校验**：update() 通过 SHA256 对比避免重复向量化
4. **策略路由搜索**：search() 优先向量搜索，失败自动降级到关键词
5. **向后兼容**：所有原有接口签名保持不变，内部实现增强

### 后续工作

- [ ] 补充 Skill DAL 完整单元测试（向量搜索相关）
- [ ] 为 Agent / Task / Project 模块实现相同的向量能力
- [ ] 向量索引定期清理任务
- [ ] 前端搜索接口对接

---

---

## 🚀 架构演进：纯 Rust HNSW + FastEmbed 向量存储

> **更新时间**：2026-05-11
> **状态**：方案确认，待实施
> **背景**：sqlite-vss 扩展跨平台编译困难，开发环境依赖复杂

---

### 🎯 核心设计原则

| 原则 | 说明 |
|------|------|
| ✅ **保持分层架构不变** | pkg/storage 纯技术层，VectorDao 业务逻辑层 |
| ✅ **Trait 可插拔设计** | SqliteVss ↔ HNSW 切换，上层零改动 |
| ✅ **纯 Rust 零依赖** | 不需要任何系统级扩展，跨平台完美支持 |
| ✅ **向后兼容** | VectorStore Trait 保持不变，所有调用点无需修改 |

---

### 📐 整体分层架构

```
┌─────────────────────────────────────────────────────────────┐
│                    SkillDal (业务聚合层)                    │
│   create() / update() / search() - 全自动向量化           │
└─────────────────────────────────────────────────────────────┘
                           │
            ┌──────────────┴──────────────┐
            ▼                              ▼
┌───────────────────┐          ┌───────────────────┐
│    SkillDao       │          │ SkillVectorDao    │
│    (SQLite 基础)   │          │ (业务向量逻辑)      │
└───────────────────┘          └───────────────────┘
                                        │
                                        ▼
                            ┌───────────────────┐
                            │  pkg/storage/     │
                            │  VectorStore      │
                            │  (纯技术层)       │
                            └───────────────────┘
                                  ▲
                                  │
                    ┌─────────────┴─────────────┐
                    │                           │
            ┌───────────────┐          ┌───────────────┐
            │ SqliteVssStore│          │  HnswStore    │
            │ (sqlite-vss)  │          │ (HNSW+FastEmbed)│
            └───────────────┘          └───────────────┘
```

---

### 📦 阶段 6：VectorStore Trait 统一抽象

**文件位置**：`src/pkg/storage/vector.rs`

```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// 初始化向量集合
    async fn init_collection(&self, collection: &str) -> Result<()>;
    
    /// 插入/更新向量
    async fn upsert(
        &self,
        collection: &str,
        id: &str,
        vector: &[f32],
        content_hash: &str,
        embedding_model: &str,
        expire_at: Option<i64>,
    ) -> Result<()>;
    
    /// 向量搜索
    async fn search(
        &self,
        collection: &str,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<(String, f32)>>;
    
    /// 删除向量
    async fn delete(&self, collection: &str, id: &str) -> Result<()>;
    
    /// 获取向量内容哈希
    async fn get_content_hash(&self, collection: &str, id: &str) -> Result<Option<String>>;
}

// 现有 SqliteVssStore 实现 Trait
pub struct SqliteVssStore { ... }

// 新增 HnswStore 也实现同一个 Trait
pub struct HnswStore { ... }
```

---

### 🧠 阶段 7：FastEmbed - 本地向量化集成

**文件位置**：`src/pkg/embedding/mod.rs`

#### 设计目标
- ✅ **零 API 成本**：本地生成 Embedding，不需要调用 OpenAI/其他 LLM API
- ✅ **跨平台支持**：纯 Rust 实现，支持所有主流平台
- ✅ **向后兼容**：通过 Trait 抽象，支持本地 / API 两种模式切换

#### 核心设计

```rust
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// 生成文本的向量表示
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    
    /// 批量生成向量
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    
    /// 返回向量维度
    fn dimensions(&self) -> i32;
    
    /// 返回模型名称
    fn model_name(&self) -> &str;
}

/// FastEmbed 本地向量化实现
pub struct FastEmbedProvider {
    model: fastembed::TextEmbedding,
    dimensions: i32,
}

impl FastEmbedProvider {
    /// 创建 FastEmbed 实例（自动下载模型）
    pub async fn new() -> Result<Self> {
        let model = fastembed::TextEmbedding::try_new(
            Default::default(),  // 使用默认模型：BAAI/bge-small-en-v1.5
        ).await?;
        
        let dimensions = model.get_dimension() as i32;
        
        Ok(Self { model, dimensions })
    }
}

#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let embeddings = self.model.embed(vec![text], None)?;
        Ok(embeddings.into_iter().next().unwrap_or_default())
    }
    
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(self.model.embed(texts.to_vec(), None)?)
    }
    
    fn dimensions(&self) -> i32 {
        self.dimensions
    }
    
    fn model_name(&self) -> &str {
        "BAAI/bge-small-en-v1.5"
    }
}
```

#### 集成到 Storage 层

```rust
// src/pkg/storage/mod.rs

pub struct Storage {
    pub db: SqlitePool,
    pub vector: Arc<dyn VectorStore>,
    pub embedding: Arc<dyn EmbeddingProvider>,  // 新增：向量化 Provider
}

impl Storage {
    pub async fn init(base_path: &Path) -> Result<Self> {
        let db = init_db_pool(base_path).await?;
        
        // 向量存储：V1 InMemory / V2 HNSW
        let vector: Arc<dyn VectorStore> = if cfg!(feature = "hnsw") {
            Arc::new(HnswStore::new(base_path.join("vector_indexes"))?)
        } else {
            Arc::new(InMemoryVectorStore::new(base_path.join("vector_indexes"))?)
        };
        
        // 向量化 Provider：本地 FastEmbed / 远程 API
        let embedding: Arc<dyn EmbeddingProvider> = if cfg!(feature = "remote-embedding") {
            Arc::new(RemoteEmbeddingProvider::new()?)
        } else {
            Arc::new(FastEmbedProvider::new().await?)
        };
        
        Ok(Self { db, vector, embedding })
    }
    
    pub fn embedding(&self) -> &dyn EmbeddingProvider {
        &*self.embedding
    }
}
```

#### 在 DAL 层的使用

```rust
// 自动向量化，DAL 层内部调用，调用方完全感知不到
async fn create_with_auto_vector(&self, ctx: RequestContext, skill: SkillPo) -> Result<SkillPo, AppError> {
    // 1. 调用本地 FastEmbed 生成向量
    let content = format!("{} {}", skill.name, skill.description);
    let vector = ctx.storage().embedding().embed(&content).await?;
    
    // 2. 计算内容哈希
    let content_hash = sha256(&content);
    
    // 3. 创建 + 索引
    let created = self.skill_dao.create(ctx.clone(), skill).await?;
    
    self.skill_vector_dao.upsert_vector(
        ctx,
        &created.id,
        VectorIndexParams {
            vector,
            content_hash,
            embedding_model: "BAAI/bge-small-en-v1.5".to_string(),
            model_provider_id: "local".to_string(),
            expire_at: None,
        },
    ).await?;
    
    Ok(created)
}
```

---

### 🚀 阶段 8：HnswStore - 高性能向量索引

**文件位置**：`src/pkg/storage/hnsw_store.rs`

#### 核心设计

```rust
pub struct HnswStore {
    base_path: PathBuf,
    collections: RwLock<HashMap<String, HnswCollection>>,
}

struct HnswCollection {
    index: HnswIndex<f32, DistCosine>,  // HNSW 索引
    id_map: HashMap<usize, String>,      // rowid -> source_id
    metadata: HashMap<String, VectorMeta>, // source_id -> 元数据
    next_id: usize,
}

struct VectorMeta {
    content_hash: String,
    embedding_model: String,
    indexed_at: i64,
    expire_at: Option<i64>,
}
```

#### 关键实现要点

1. **懒加载**：首次访问 collection 时从磁盘加载
2. **异步持久化**：upsert 后异步写入磁盘，不阻塞主线程
3. **RwLock 并发**：读多写少场景优化
4. **文件格式**：bincode 序列化，高性能二进制格式

```rust
impl HnswStore {
    pub fn new(base_path: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&base_path)?;
        Ok(Self {
            base_path,
            collections: RwLock::new(HashMap::new()),
        })
    }
    
    async fn get_or_load_collection(&self, collection: &str) -> Result<()> {
        // 懒加载：首次访问从磁盘加载
    }
    
    async fn save_collection_async(&self, collection: &str) {
        // 后台异步持久化到磁盘
    }
}
```

---

### 🔌 阶段 9：Storage 统一初始化

**文件位置**：`src/pkg/storage/mod.rs`

```rust
pub struct Storage {
    pub db: SqlitePool,
    pub vector: Arc<dyn VectorStore>,  // 可插拔切换
}

impl Storage {
    pub async fn init(base_path: &Path) -> Result<Self> {
        let db = init_db_pool(base_path).await?;
        
        // 通过 Feature Flag 切换向量存储实现
        let vector: Arc<dyn VectorStore> = if cfg!(feature = "sqlite-vss") {
            Arc::new(SqliteVssStore::from_pool(db.clone()).await?)
        } else {
            // 默认使用纯 Rust HNSW 实现
            Arc::new(HnswStore::new(base_path.join("vector_indexes"))?)
        };
        
        Ok(Self { db, vector })
    }
    
    pub fn vector_store(&self) -> &dyn VectorStore {
        &*self.vector
    }
}
```

---

### 📂 数据目录结构

```
ai_orz_data/
├── sqlite.db                    ← 主数据库（业务数据）
└── vector_indexes/              ← 独立向量索引目录
    ├── skills.hnsw              ← Skill 集合 HNSW 索引
    ├── memories.hnsw           ← Memory 集合
    ├── tasks.hnsw             ← Task 集合
    └── agents.hnsw            ← Agent 集合
```

---

### 🔬 三种实现对比

| 维度 | SqliteVssStore | InMemoryVectorStore (V1 ✅) | HnswStore (V2 🔄) |
|------|---------------|----------------------------|-------------------|
| **跨平台** | ❌ macOS 编译困难 | ✅ 纯 Rust 全平台 | ✅ 纯 Rust 全平台 |
| **向量化** | ❌ 需要远程 API | ❌ 需要远程 API | ✅ FastEmbed 本地生成 |
| **搜索性能** | ✅ 优化 | ⚠️ O(n) 线性扫描 | ✅ HNSW O(log n) |
| **大数据量** | ✅ 支持百万级 | ⚠️ 千级以内较好 | ✅ 支持百万级 |
| **内存占用** | ✅ SQLite 管理 | ⚠️ 全部加载到内存 | ✅ 懒加载按需加载 |
| **依赖管理** | ❌ 需要系统扩展 | ✅ Cargo 一键安装 | ✅ Cargo 一键安装 |
| **持久化** | ✅ SQLite 事务 | ✅ Bincode 二进制 | ✅ Bincode 二进制 |
| **API 兼容** | ✅ Trait 一致 | ✅ Trait 完全一致 | ✅ Trait 完全一致 |
| **实现状态** | ❌ 废弃 | ✅ 已完成，可用 | 🔄 待实现 |

---

### 📋 实施清单

#### V1 已完成 ✅

| 阶段 | 文件 | 核心内容 | 状态 |
|------|------|---------|------|
| V1-1 | `src/pkg/storage/vector.rs` | VectorStore Trait 统一抽象 | ✅ 已完成 |
| V1-2 | `src/pkg/storage/mem_vector.rs` | InMemoryVectorStore 线性搜索实现 | ✅ 已完成 |
| V1-3 | `src/pkg/storage/mod.rs` | Storage 统一门面 | ✅ 已完成 |
| V1-4 | `src/service/dao/skill/sqlite_vector.rs` | SkillVectorDao 独立文件 | ✅ 已完成 |
| V1-5 | `src/service/dal/skill.rs` | DAL 层组合两个 DAO | ✅ 已完成 |
| V1-6 | `Cargo.toml` | 添加 bincode 依赖 | ✅ 已完成 |
| V1-7 | 单元测试 | InMemoryVectorStore 完整测试 | ✅ 已完成 |

#### V2 优化待实施 🔄

| 阶段 | 文件 | 核心内容 | 优先级 |
|------|------|---------|--------|
| V2-1 | `src/pkg/embedding/mod.rs` | EmbeddingProvider Trait + FastEmbed 实现 | 🔴 高 |
| V2-2 | `src/pkg/storage/hnsw_store.rs` | HNSW 高性能向量索引 | 🔴 高 |
| V2-3 | `src/pkg/storage/mod.rs` | 集成 EmbeddingProvider + Feature Flag 切换 | 🟡 中 |
| V2-4 | `src/service/dal/skill.rs` | 自动向量化支持（DAL 内部调用 FastEmbed） | 🟡 中 |
| V2-5 | `Cargo.toml` | 添加 fastembed + hnsw 依赖 | 🟡 中 |
| V2-6 | 单元测试 | FastEmbed 集成测试 + HNSW 性能基准 | 🟢 低 |

---

### ✨ 架构优势总结

| 优势 | 说明 |
|------|------|
| 🎯 **严格分层** | pkg/storage 纯技术层，VectorDao 业务逻辑层，职责明确 |
| 🔌 **可插拔设计** | InMemory ↔ HNSW / 本地 ↔ 远程 API 自由切换，上层零改动 |
| 🦀 **纯 Rust 实现** | 不需要任何系统级依赖，跨平台完美支持 |
| 🧠 **本地向量化** | FastEmbed 集成，零 API 成本，不依赖外部服务 |
| 🏗️ **架构一致** | V1/V2 完全兼容同一个 VectorStore Trait |
| 📦 **单一职责** | Storage 管索引和向量化，VectorDao 只负责业务逻辑 |
| ✅ **向后兼容** | SkillDal 完全不用改，所有调用点保持不变 |

---

### 🔮 未来扩展方向

1. **向量量化压缩**：f32 → f16 → int8 渐进式向量压缩，进一步减少内存占用
2. **多向量字段**：支持同一实体多个不同维度的向量索引（名称向量 + 描述向量）
3. **向量版本管理**：支持 Embedding 模型升级时的平滑迁移和重建
4. **增量索引优化**：支持流式增量索引，避免全量重建
5. **向量缓存层**：热门向量结果缓存，提升查询性能

---

**文档维护**：本设计文档随代码同步更新，实施过程中如有调整请及时更新此文档。
