# 向量搜索 Pre-Filter 改造：Payload 落库与谓词下推 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 向量库落库原始过滤信息（payload），搜索时谓词下推 pre-filter，消除"全局 Top-K 被其他 Agent/租户污染"的召回缺陷（memory 域全链路试点）。

**Architecture:** 三层改造：① 模型层新增 `VectorPayload` 宽结构 + `VectorFilter` 谓词表达式 + `ReindexDecision` 三态，绑定在 `Vectorizable` trait 上（与 `vectorize_text()` 同源）；② 存储层 4 个后端（LanceDB 为主）统一落 payload 列、`search` 增加 filter 参数（Lance 用 SQL 谓词下推，InMemory/Hnsw 内存求值）；③ memory 域搜索链路把 `MemoryQuery` 在 vector DAO 内转译为 `VectorFilter`（转译下沉 DAO 层，信息专家原则），命中后仍回业务表取完整 PO（业务表是 SSOT）。

**Tech Stack:** Rust + lancedb 0.26（Arrow RecordBatch）+ bincode（InMemory/Hnsw 持久化）+ sqlx SQLite（SqliteVss 后端）

---

## 背景（为什么做）

- 现状：`VectorStore::search(collection, vector, top_k)` 是无过滤全局 Top-K；LanceDB 表只有 `id/vector/content_hash/embedding_model/indexed_at/expire_at` 六列，无任何业务过滤字段。业务过滤全靠事后回表（post-filter），全局 Top-K 被其他租户/Agent 数据占满后过滤所剩无几，多 Agent 场景语义召回失真（`src/service/dal/message.rs` 注释已标记"多租户隔离缺口"）。
- 记忆域可见性规则（`agent_id = 自己 OR is_published`）在业务层过滤，语义完全失真。

## 已对齐的设计决策（不可推翻）

1. **payload 用统一宽结构**（`Option` 字段集），不用 KV map。定义在 `src/models/vector.rs` 与 `Vectorizable` 同文件。
2. **直接改 `search` 签名**加 filter 参数，不留旧签名（过滤是 search 的标准能力）。
3. **命中后回业务表取完整 PO**（payload 不是全字段，业务表是 SSOT；`VectorFilter` 是召回优化，正确性由回表过滤兜底）。
4. **8 实体统一落 payload**，搜索转译先做 memory 试点，其余 7 域各自后续 plan 接入（本 plan 只做编译对齐 `filter: None`）。
5. **转译下沉 DAO 层**：向量 DAO 接收标准 `MemoryQuery`，内部转译为 `VectorFilter`（哪些字段能下推只有实体 DAO 知道）。
6. **`reindex_decision` 三态**：`Skip`（双 hash 未变）/ `PayloadOnly`（仅 payload 变，刷新 payload 不调 embedding）/ `FullReindex`（文本变，重算向量）。防止"过滤字段变了但 vectorize_text 没变 → payload 落后 → pre-filter 漏召回"。
7. **VectorFilter 不含排序**：向量搜索天然按距离排序；业务排序由回表后的业务层负责。
8. **tags 落 payload 但不下推**（第一版转译白名单不含 tags，回表兜底）。
9. **零兼容包袱**：测试阶段无有价值数据，代码直接写成最佳形态——不含旧 schema 检测、ALTER 补列、bincode 容错等任何迁移逻辑；旧向量数据目录在执行前置步骤整体删除。

## 执行前置（一次性，开工前完成）

删除本地旧向量数据目录（均为测试数据，无保留价值；幂等操作，目录不存在则跳过）：

```bash
rm -rf data/vectors/
```

该目录包含全部 4 个后端的旧数据（LanceDB 的 `lancedb/` 子目录、InMemory 的 `*.bin`、Hnsw 的 `*.bincode`、SqliteVss 的 `vectors.db`）。删除后旧格式文件不复存在，后续所有 Task 均无需任何兼容逻辑。（2026-09-13 已确认本机不存在该目录。）

## 范围

- **本 plan**：底座（模型层 + 4 后端 + embed_entity）+ 10 处 `Vectorizable` 实现 + memory 域全链路试点。
- **后续 plan**：agent / message / task / project / tool / skill 各域的 DAO 转译接入（重复模式）。

## 文件结构（全部改动点）

| 文件 | 改动 |
|------|------|
| `src/models/vector.rs` | 新增 VectorPayload/VectorField/FilterValue/VectorFilter/ReindexDecision；扩展 Vectorizable/VectorIndexParams/VectorMeta/VectorRow |
| `src/pkg/storage/vector.rs` | VectorStore trait 签名升级 + SqliteVssStore 实现跟进 |
| `src/pkg/storage/mem_vector.rs` | InMemoryVectorStore：filter 求值 + update_payload |
| `src/pkg/storage/lance.rs` | LanceVectorStore：payload 平铺列 + 谓词下推 + update_payload |
| `src/pkg/storage/hnsw.rs` | HnswStore：filter 求值 + update_payload |
| `src/service/dao/cortex/native/mod.rs` | embed_entity 构建 payload + payload_hash |
| `src/models/memory.rs` | 两个 PO 实现 vector_payload/vector_id |
| `src/models/agent.rs` / `tool.rs` / `skill.rs` / `task.rs` / `project.rs` / `message.rs` | 各 PO 实现 vector_payload/vector_id |
| `src/service/dao/memory/mod.rs` | MemoryVectorDao trait 签名（search 加 filters） |
| `src/service/dao/memory/vector.rs` | MemoryQuery→VectorFilter 转译 + 实现 |
| `src/service/dal/memory.rs` | 三态索引接入（6 个调用点）+ 搜索传 filters |
| `src/service/dao/{agent,message,task,project,tool,skill}/vector.rs` | search 调用传 None（编译对齐） |
| 各 `*_test.rs` | Mock 签名修复 + 新增测试 |

---

## Task 1: 模型层底座——VectorPayload / VectorFilter / ReindexDecision

**Files:**
- Modify: `src/models/vector.rs`

- [ ] **Step 1.1: 写失败测试（payload hash 稳定性 + filter 求值 + SQL 翻译）**

在 `src/models/vector.rs` 的 `#[cfg(test)] mod tests` 中追加：

```rust
    // ==================== VectorPayload / VectorFilter 测试 ====================

    fn sample_payload() -> VectorPayload {
        VectorPayload {
            agent_id: Some("agent-1".to_string()),
            is_published: Some(false),
            ..Default::default()
        }
    }

    #[test]
    fn test_vector_payload_hash_stable() {
        let a = sample_payload();
        let b = sample_payload();
        assert_eq!(a.hash(), b.hash());
        // 任意字段变化 → hash 变化
        let mut c = sample_payload();
        c.is_published = Some(true);
        assert_ne!(a.hash(), c.hash());
    }

    #[test]
    fn test_vector_filter_matches_eq() {
        let f = VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into()));
        assert!(f.matches(&sample_payload()));
        let f2 = VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-2".into()));
        assert!(!f2.matches(&sample_payload()));
        // payload 缺字段（None）不匹配任何值
        let f3 = VectorFilter::Eq(VectorField::ProjectId, FilterValue::Str("p".into()));
        assert!(!f3.matches(&sample_payload()));
    }

    #[test]
    fn test_vector_filter_matches_any_or() {
        // agent_id = agent-1 OR is_published = true（共享可见性）
        let f = VectorFilter::Any(vec![
            VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into())),
            VectorFilter::Eq(VectorField::IsPublished, FilterValue::Bool(true)),
        ]);
        assert!(f.matches(&sample_payload())); // 命中第一支（自己且未发布）
        let mut other = VectorPayload {
            agent_id: Some("agent-2".into()),
            is_published: Some(true),
            ..Default::default()
        };
        assert!(f.matches(&other)); // 命中第二支（别人的已发布）
        other.is_published = Some(false);
        assert!(!f.matches(&other)); // 别人的未发布 → 不可见
    }

    #[test]
    fn test_vector_filter_to_sql_expr() {
        let f = VectorFilter::All(vec![
            VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("a'1".into())),
            VectorFilter::Any(vec![
                VectorFilter::Eq(VectorField::IsPublished, FilterValue::Bool(true)),
                VectorFilter::In(VectorField::EntityType, vec![FilterValue::Str("concept".into())]),
            ]),
        ]);
        let sql = f.to_sql_expr(|field| field.column_name().to_string());
        // 单引号转义（SQL 注入防护）
        assert!(sql.contains("agent_id = 'a''1'"));
        assert!(sql.contains("is_published = true"));
        assert!(sql.contains("entity_type IN ('concept')"));
        assert!(sql.contains(" AND ") && sql.contains(" OR "));
    }

    #[test]
    fn test_reindex_decision_default() {
        use crate::models::vector::*;
        struct Dummy;
        impl Vectorizable for Dummy {
            fn vectorize_text(&self) -> String { "text".into() }
            fn vector_collection() -> &'static str { "dummy" }
            fn vector_payload(&self) -> VectorPayload { VectorPayload { agent_id: Some("a".into()), ..Default::default() } }
            fn vector_id(&self) -> &str { "id-1" }
        }
        let d = Dummy;
        // 双 hash 均未变 → Skip
        assert_eq!(
            d.reindex_decision(&d.vector_content_hash(), &d.vector_payload().hash()),
            ReindexDecision::Skip
        );
        // 文本变 → FullReindex
        assert_eq!(d.reindex_decision("old-text-hash", &d.vector_payload().hash()), ReindexDecision::FullReindex);
        // 仅 payload 变 → PayloadOnly
        assert_eq!(d.reindex_decision(&d.vector_content_hash(), "old-payload-hash"), ReindexDecision::PayloadOnly);
    }
```

注意：`sample_payload` 里不要写 `node_type_placeholder`，按真实结构字段写（`entity_type: None`）。

- [ ] **Step 1.2: 运行测试确认失败**

```bash
cargo test --lib models::vector
```

期望：编译失败，`VectorPayload` / `VectorFilter` 等类型未定义。

- [ ] **Step 1.3: 实现底座类型**

在 `src/models/vector.rs` 顶部 import 区补充 `serde::{Serialize, Deserialize}`（文件已有 serde 依赖则直接用），并在「向量存储通用数据结构」章节前新增：

```rust
// ==================== 向量 Payload（原始过滤信息）与谓词过滤 ====================

/// 向量过滤字段（与 VectorPayload 的列一一对应）
///
/// 注意：tags 落 payload 但不参与谓词下推（回表兜底），故不在此枚举中
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorField {
    OrgId,
    AgentId,
    ProjectId,
    TaskId,
    FromId,
    ToId,
    EntityType,
    Status,
    IsPublished,
}

impl VectorField {
    /// payload 列名（LanceDB 平铺列名 / SQL 字段名 / JSON key，三处统一）
    pub fn column_name(&self) -> &'static str {
        match self {
            Self::OrgId => "org_id",
            Self::AgentId => "agent_id",
            Self::ProjectId => "project_id",
            Self::TaskId => "task_id",
            Self::FromId => "from_id",
            Self::ToId => "to_id",
            Self::EntityType => "entity_type",
            Self::Status => "status",
            Self::IsPublished => "is_published",
        }
    }
}

/// 谓词过滤值
#[derive(Debug, Clone, PartialEq)]
pub enum FilterValue {
    Str(String),
    Bool(bool),
}

/// 向量谓词过滤表达式（结构化；各后端自行翻译执行）
///
/// 定位：召回优化，不是正确性保证——下推不了的业务条件由回业务表过滤兜底
#[derive(Debug, Clone)]
pub enum VectorFilter {
    /// 字段等值匹配（payload 字段为 None 的行不匹配）
    Eq(VectorField, FilterValue),
    /// 字段 IN 集合匹配
    In(VectorField, Vec<FilterValue>),
    /// 全部满足（AND）
    All(Vec<Self>),
    /// 任一满足（OR；用于共享可见性 agent_id = 自己 OR is_published）
    Any(Vec<Self>),
}

impl VectorFilter {
    /// 内存求值（InMemory / Hnsw 后端复用）
    pub fn matches(&self, payload: &VectorPayload) -> bool {
        match self {
            Self::Eq(f, v) => payload.get(f).as_ref() == Some(v),
            Self::In(f, vs) => payload.get(f).is_some_and(|v| vs.contains(&v)),
            Self::All(fs) => fs.iter().all(|f| f.matches(payload)),
            Self::Any(fs) => fs.iter().any(|f| f.matches(payload)),
        }
    }

    /// 翻译为 SQL 谓词（LanceDB 直接用列名；SqliteVss 用 json_extract 包装字段名）
    ///
    /// `col` 闭包负责字段名渲染：Lance 传 `|f| f.column_name().to_string()`，
    /// SqliteVss 传 `|f| format!("json_extract(payload_json, '$.{}')", f.column_name())`
    pub fn to_sql_expr<F: Fn(&VectorField) -> String>(&self, col: F) -> String {
        fn escape_sql_str(s: &str) -> String {
            s.replace('\'', "''")
        }
        fn render_value(v: &FilterValue) -> String {
            match v {
                FilterValue::Str(s) => format!("'{}'", escape_sql_str(s)),
                FilterValue::Bool(b) => b.to_string(),
            }
        }
        match self {
            Self::Eq(f, v) => format!("{} = {}", col(f), render_value(v)),
            Self::In(f, vs) => {
                let items: Vec<String> = vs.iter().map(render_value).collect();
                format!("{} IN ({})", col(f), items.join(", "))
            }
            Self::All(fs) => {
                if fs.len() == 1 {
                    fs[0].to_sql_expr(&col)
                } else {
                    format!(
                        "({})",
                        fs.iter().map(|f| f.to_sql_expr(&col)).collect::<Vec<_>>().join(" AND ")
                    )
                }
            }
            Self::Any(fs) => {
                if fs.len() == 1 {
                    fs[0].to_sql_expr(&col)
                } else {
                    format!(
                        "({})",
                        fs.iter().map(|f| f.to_sql_expr(&col)).collect::<Vec<_>>().join(" OR ")
                    )
                }
            }
        }
    }
}

/// 向量 Payload：随向量落库的原始过滤信息（统一宽结构）
///
/// 「列」全局统一（所有 collection 同 schema，4 个后端共用）；「行」由各 PO 的
/// `Vectorizable::vector_payload()` 自治填充（信息专家原则，与 vectorize_text 同源）。
/// 新增可过滤字段的路径：本结构加列 → 相关 PO 填充 → 对应 vector DAO 转译白名单加映射。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Encode, Decode)]
pub struct VectorPayload {
    /// 组织 ID（Message 有；Agent/Task/Project/Tool/Skill 的 PO 无此字段暂 None）
    pub org_id: Option<String>,
    /// Agent ID（Memory / Message）
    pub agent_id: Option<String>,
    /// 项目 ID（Task / Message）
    pub project_id: Option<String>,
    /// 任务 ID（Memory / Message）
    pub task_id: Option<String>,
    /// 消息发送方（Message）
    pub from_id: Option<String>,
    /// 消息接收方（Message）
    pub to_id: Option<String>,
    /// 实体子类型（node_type / tool_type / message_type 等 String 类型字段统一映射）
    pub entity_type: Option<String>,
    /// 状态（枚举暂以稳定字符串存储；第一期不参与转译白名单，仅落库）
    pub status: Option<String>,
    /// 是否已发布（KnowledgeNode 共享可见性）
    pub is_published: Option<bool>,
    /// 标签（JSON 数组字符串，与 PO tags 同构；第一版不参与谓词下推）
    pub tags: Option<String>,
}

impl VectorPayload {
    /// 按字段取值（求值用）
    pub fn get(&self, field: &VectorField) -> Option<FilterValue> {
        match field {
            VectorField::OrgId => self.org_id.clone().map(FilterValue::Str),
            VectorField::AgentId => self.agent_id.clone().map(FilterValue::Str),
            VectorField::ProjectId => self.project_id.clone().map(FilterValue::Str),
            VectorField::TaskId => self.task_id.clone().map(FilterValue::Str),
            VectorField::FromId => self.from_id.clone().map(FilterValue::Str),
            VectorField::ToId => self.to_id.clone().map(FilterValue::Str),
            VectorField::EntityType => self.entity_type.clone().map(FilterValue::Str),
            VectorField::Status => self.status.clone().map(FilterValue::Str),
            VectorField::IsPublished => self.is_published.map(FilterValue::Bool),
        }
    }

    /// payload 哈希（sha256(稳定 JSON)；serde_json 按 struct 字段顺序序列化，稳定）
    pub fn hash(&self) -> String {
        sha256::digest(serde_json::to_string(self).unwrap_or_default())
    }
}

/// 重索引决策（三态）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReindexDecision {
    /// 文本与 payload 均未变化：跳过
    Skip,
    /// 仅 payload 变化：只刷新 payload 列，不重新调 embedding
    PayloadOnly,
    /// 向量化文本变化：完整重索引（重新 embed）
    FullReindex,
}
```

- [ ] **Step 1.4: 运行测试确认通过（部分）**

```bash
cargo test --lib models::vector
```

期望：`test_reindex_decision_default` 编译失败（trait 方法还没加），其余 payload/filter 测试通过。这是预期的——Task 2 补 trait 后全部通过。如果希望本 Task 完全绿灯，可将 `test_reindex_decision_default` 挪到 Task 2 完成后运行。

- [ ] **Step 1.5: Commit**

```bash
git add src/models/vector.rs
git commit -m "feat(vector): 新增 VectorPayload/VectorFilter/ReindexDecision 底座数据结构"
```

---

## Task 2: Vectorizable trait 扩展 + 数据对象字段扩展

**Files:**
- Modify: `src/models/vector.rs`

- [ ] **Step 2.1: 扩展 Vectorizable trait（删 needs_reindex，加三个方法）**

替换 `src/models/vector.rs` 中 `Vectorizable` trait 定义（原 `needs_reindex` 默认实现删除，全库无调用方，安全删除）：

```rust
/// ✅ 可向量化实体 Trait
///
/// 实现这个 Trait 的实体，表示它支持被向量索引
/// 所有向量相关的业务逻辑都封装在实体内部
pub trait Vectorizable: Send + Sync {
    // ===== 必须实现 =====

    /// 生成待向量化的文本内容（语义检索用）
    fn vectorize_text(&self) -> String;

    /// 向量集合名称（对应各后端的 collection/table）
    fn vector_collection() -> &'static str
    where
        Self: Sized;

    /// 向量行主键（业务表 ID；三态判断取旧行时使用）
    fn vector_id(&self) -> &str;

    /// 过滤元数据（原始信息随向量落库；「行」由实体自治填充）
    fn vector_payload(&self) -> VectorPayload;

    // ===== 默认实现（不需要重写） =====

    /// 计算内容哈希（默认 SHA256）
    fn vector_content_hash(&self) -> String {
        sha256::digest(self.vectorize_text())
    }

    /// 向量过期时间（可选覆盖，默认永不过期）
    fn vector_expire_at(&self) -> Option<i64> {
        None
    }

    /// 重索引决策（三态：双 hash 比对）
    ///
    /// 防止「过滤字段变化但 vectorize_text 未变 → payload 落后 → pre-filter 漏召回」
    fn reindex_decision(
        &self,
        existing_text_hash: &str,
        existing_payload_hash: &str,
    ) -> ReindexDecision {
        if self.vector_content_hash() != existing_text_hash {
            ReindexDecision::FullReindex
        } else if self.vector_payload().hash() != existing_payload_hash {
            ReindexDecision::PayloadOnly
        } else {
            ReindexDecision::Skip
        }
    }
}
```

- [ ] **Step 2.2: 扩展 VectorIndexParams / VectorMeta / VectorRow**

```rust
/// 向量元数据（持久化）
#[derive(Debug, Clone, Encode, Decode)]
pub struct VectorMeta {
    /// 向量化文本哈希（判断是否需要重索引）
    pub content_hash: String,
    /// payload 哈希（判断是否仅需要刷新 payload）
    pub payload_hash: String,
    pub embedding_model: String,
    pub indexed_at: i64,
    pub expire_at: Option<i64>,
}

/// 向量行结构体（完整的一行数据）
#[derive(Debug, Clone, Encode, Decode)]
pub struct VectorRow {
    /// 业务表 ID（如 skill_id, memory_id）
    pub id: String,
    /// 向量数据
    pub vector: Vec<f32>,
    /// 原始过滤信息（命中后随行返回）
    pub payload: VectorPayload,
    /// 元数据
    pub meta: VectorMeta,
}
```

`VectorIndexParams` 增加 `payload` 与 `payload_hash` 字段（`new` 构造同步加参数）：

```rust
#[derive(Debug, Clone)]
pub struct VectorIndexParams {
    /// 向量数据
    pub vector: Vec<f32>,
    /// 内容哈希（用于判断是否需要重索引）
    pub content_hash: String,
    /// 过滤元数据（随向量落库）
    pub payload: VectorPayload,
    /// payload 哈希（用于判断是否仅刷新 payload）
    pub payload_hash: String,
    /// 生成该向量的 ModelProvider ID
    pub model_provider_id: String,
    /// 使用的模型名称
    pub embedding_model: String,
    /// 过期时间（None 表示永不过期）
    pub expire_at: Option<i64>,
}

impl VectorIndexParams {
    /// 从向量化文本和向量创建索引参数
    pub fn new(
        content: &str,
        vector: Vec<f32>,
        payload: VectorPayload,
        model_provider_id: String,
        embedding_model: String,
    ) -> Self {
        let content_hash = sha256::digest(content);
        let payload_hash = payload.hash();
        Self {
            vector,
            content_hash,
            payload,
            payload_hash,
            model_provider_id,
            embedding_model,
            expire_at: None,
        }
    }
    // with_expire_at 保持不变
}
```

注意：`VectorMeta` 增加 `payload_hash` 后，bincode 持久化的旧格式文件会 decode 失败——按零兼容原则，旧数据目录在执行前置步骤中整体删除，代码不含任何容错/迁移逻辑。

- [ ] **Step 2.3: 运行模型层测试**

```bash
cargo test --lib models::vector
```

期望：全部通过（含 `test_reindex_decision_default`）。

此时全库会有大量编译错误（10 处 `impl Vectorizable`、4 个后端、embed_entity、各测试 mock 构造 `VectorIndexParams`/`VectorRow`）——**这是预期的**，由 Task 3-7 逐个消除。本步只验证 models::vector 单测本身。

- [ ] **Step 2.4: Commit**

```bash
git add src/models/vector.rs
git commit -m "feat(vector): Vectorizable 增加 vector_payload/vector_id，reindex 三态；IndexParams/Row/Meta 扩展 payload"
```

---

## Task 3: VectorStore trait 升级 + InMemory 后端

**Files:**
- Modify: `src/pkg/storage/vector.rs`（trait）
- Modify: `src/pkg/storage/mem_vector.rs`（实现）

- [ ] **Step 3.1: 升级 VectorStore trait**

`src/pkg/storage/vector.rs` 的 trait `search` 改签名 + 新增 `update_payload`：

```rust
    /// 语义搜索（支持谓词下推 pre-filter：Top-K 在满足谓词的候选集内选取）
    ///
    /// # 参数
    /// - filter: 结构化谓词；None 表示不过滤
    async fn search(
        &self,
        collection: &str,
        query_vector: &[f32],
        top_k: i32,
        filter: Option<&crate::models::vector::VectorFilter>,
    ) -> Result<Vec<VectorSearchHit>>;

    /// 仅更新 payload（ReindexDecision::PayloadOnly 场景：不重新调 embedding）
    async fn update_payload(
        &self,
        collection: &str,
        id: &str,
        payload: &crate::models::vector::VectorPayload,
    ) -> Result<()>;
```

import 区补充：`use crate::models::vector::{VectorFilter, VectorPayload, ...}`（按需合并现有 import）。

- [ ] **Step 3.2: 写 InMemory 的 filter 求值测试（失败）**

在 `src/pkg/storage/mem_vector.rs` 末尾追加测试模块（若无）：

```rust
#[cfg(test)]
mod filter_tests {
    use super::*;
    use crate::models::vector::{FilterValue, VectorField, VectorFilter, VectorPayload};

    fn payload(agent: &str, published: bool) -> VectorPayload {
        VectorPayload {
            agent_id: Some(agent.to_string()),
            is_published: Some(published),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_inmemory_search_with_filter() {
        let store = InMemoryVectorStore::with_path(std::env::temp_dir().join(format!(
            "vec_filter_test_{}",
            std::process::id()
        )))
        .unwrap();
        let params = |agent: &str, published: bool| crate::models::vector::VectorIndexParams {
            vector: vec![1.0, 0.0],
            content_hash: format!("hash-{agent}-{published}"),
            payload: payload(agent, published),
            payload_hash: payload(agent, published).hash(),
            model_provider_id: "p1".into(),
            embedding_model: "m1".into(),
            expire_at: None,
        };
        store.upsert("t", "a1", &params("agent-1", false)).await.unwrap();
        store.upsert("t", "a2", &params("agent-1", true)).await.unwrap();
        store.upsert("t", "b1", &params("agent-2", true)).await.unwrap();

        // 不过滤：3 条
        let all = store.search("t", &[1.0, 0.0], 10, None).await.unwrap();
        assert_eq!(all.len(), 3);

        // agent-1 视角 + include_shared（OR is_published）
        let filter = VectorFilter::Any(vec![
            VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into())),
            VectorFilter::Eq(VectorField::IsPublished, FilterValue::Bool(true)),
        ]);
        let hits = store.search("t", &[1.0, 0.0], 10, Some(&filter)).await.unwrap();
        let ids: Vec<&str> = hits.iter().map(|h| h.row.id.as_str()).collect();
        assert_eq!(ids.len(), 3); // a1 + a2 + b1(published)

        // agent-1 私有视角
        let private = VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into()));
        let hits = store.search("t", &[1.0, 0.0], 10, Some(&private)).await.unwrap();
        assert_eq!(hits.len(), 2);

        // 命中行携带 payload
        assert!(hits[0].row.payload.agent_id.is_some());
    }

    #[tokio::test]
    async fn test_inmemory_update_payload() {
        let store = InMemoryVectorStore::with_path(std::env::temp_dir().join(format!(
            "vec_payload_test_{}",
            std::process::id()
        )))
        .unwrap();
        let mut p = params_default();
        p.payload.is_published = Some(false);
        store.upsert("t", "a1", &p).await.unwrap();
        let mut new_payload = crate::models::vector::VectorPayload::default();
        new_payload.is_published = Some(true);
        new_payload.agent_id = Some("agent-1".into());
        store.update_payload("t", "a1", &new_payload).await.unwrap();
        let row = store.get("t", "a1").await.unwrap().unwrap();
        assert_eq!(row.payload.is_published, Some(true));
        assert_eq!(row.meta.payload_hash, new_payload.hash());
        // vector 不被破坏
        assert_eq!(row.vector.len(), 2);
    }

    fn params_default() -> crate::models::vector::VectorIndexParams {
        crate::models::vector::VectorIndexParams {
            vector: vec![1.0, 0.0],
            content_hash: "h".into(),
            payload: crate::models::vector::VectorPayload::default(),
            payload_hash: crate::models::vector::VectorPayload::default().hash(),
            model_provider_id: "p1".into(),
            embedding_model: "m1".into(),
            expire_at: None,
        }
    }
}
```

- [ ] **Step 3.3: 运行确认编译失败**

```bash
cargo test --lib pkg::storage::mem_vector
```

- [ ] **Step 3.4: 实现 InMemoryVectorStore**

`upsert`：构造 `VectorRow` 时带上 `payload` 与 `payload_hash`（替换两处 `VectorMeta { content_hash, ... }` 构造）：

```rust
            // 更新与新增两处分支统一改为：
            VectorRow {
                id: id.to_string(),
                vector: params.vector.clone(),
                payload: params.payload.clone(),
                meta: VectorMeta {
                    content_hash: params.content_hash.clone(),
                    payload_hash: params.payload_hash.clone(),
                    embedding_model: params.embedding_model.clone(),
                    indexed_at: now,
                    expire_at: params.expire_at,
                },
            }
```

`search`（签名加 filter，先谓词过滤再取 top-k）：

```rust
    async fn search(
        &self,
        collection: &str,
        query_vector: &[f32],
        top_k: i32,
        filter: Option<&crate::models::vector::VectorFilter>,
    ) -> Result<Vec<VectorSearchHit>> {
        let now = chrono::Utc::now().timestamp();

        let coll = self
            .get_or_load(collection)
            .await?
            .unwrap_or_else(|| VectorCollection::new(query_vector.len() as i32));

        // 先谓词过滤（pre-filter），再按距离排序取 top-k
        let mut results: Vec<VectorSearchHit> = coll
            .entries
            .iter()
            .filter(|e| e.meta.expire_at.is_none_or(|exp| exp > now))
            .filter(|e| filter.is_none_or(|f| f.matches(&e.payload)))
            .map(|e| {
                let distance = cosine_distance(query_vector, &e.vector);
                VectorSearchHit {
                    row: e.clone(),
                    distance,
                }
            })
            .collect();

        results.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results.into_iter().take(top_k as usize).collect())
    }
```

`update_payload`（trait 新方法的实现）：

```rust
    async fn update_payload(
        &self,
        collection: &str,
        id: &str,
        payload: &crate::models::vector::VectorPayload,
    ) -> Result<()> {
        let mut collections = self.collections.write().await;
        if let Some(coll) = collections.get_mut(collection) {
            if let Some(entry) = coll.entries.iter_mut().find(|e| e.id == id) {
                entry.payload = payload.clone();
                entry.meta.payload_hash = payload.hash();
                let coll_clone = coll.clone();
                let store_clone = self.clone();
                let collection_name = collection.to_string();
                tokio::spawn(async move {
                    let _ = store_clone
                        .save_collection(&collection_name, &coll_clone)
                        .await;
                });
            }
        }
        Ok(())
    }
```

`load_collection` 保持原有实现不变（零兼容逻辑；执行本 plan 前已删除旧数据目录，不存在旧格式文件）。

- [ ] **Step 3.5: 运行测试通过**

```bash
cargo test --lib pkg::storage::mem_vector
```

期望：PASS。

- [ ] **Step 3.6: Commit**

```bash
git add src/pkg/storage/vector.rs src/pkg/storage/mem_vector.rs
git commit -m "feat(vector): VectorStore 增加 filter 谓词下推与 update_payload；InMemory 实现"
```

---

## Task 4: LanceDB 后端（主推后端）

**Files:**
- Modify: `src/pkg/storage/lance.rs`

- [ ] **Step 4.1: 统一 schema 构造（payload 平铺列）**

在 `lance.rs` 中新增（供 `get_or_create_table` 和 `upsert` 共用，替换两处重复的 Schema 构造）：

```rust
/// 统一表 schema：基础元信息列 + payload 平铺列（全部 nullable，稀疏无成本）
fn lance_schema(dimensions: i32) -> StdArc<Schema> {
    let payload_columns = [
        ("org_id", DataType::Utf8),
        ("agent_id", DataType::Utf8),
        ("project_id", DataType::Utf8),
        ("task_id", DataType::Utf8),
        ("from_id", DataType::Utf8),
        ("to_id", DataType::Utf8),
        ("entity_type", DataType::Utf8),
        ("status", DataType::Utf8),
        ("is_published", DataType::Boolean),
        ("tags", DataType::Utf8),
    ];
    let mut fields = vec![
        Field::new("id", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                StdArc::new(Field::new("item", DataType::Float32, true)),
                dimensions,
            ),
            false,
        ),
        Field::new("content_hash", DataType::Utf8, false),
        Field::new("payload_hash", DataType::Utf8, false),
        Field::new("embedding_model", DataType::Utf8, false),
        Field::new("indexed_at", DataType::Int64, false),
        Field::new("expire_at", DataType::Int64, true),
    ];
    for (name, ty) in payload_columns {
        fields.push(Field::new(name, ty, true));
    }
    StdArc::new(Schema::new(fields))
}
```

`get_or_create_table` 仅做两处替换（零兼容逻辑：表存在→直接打开；不存在→建新表。旧数据目录已在前置步骤删除，不存在旧 schema 表）：

```rust
        let table = if table_names.iter().any(|t| t == &table_name) {
            self.db
                .open_table(&table_name)
                .execute()
                .await
                .map_err(|e| {
                    common::error::Error::internal(format!("LanceDB open table error: {}", e))
                })?
        } else {
            self.db
                .create_empty_table(&table_name, lance_schema(dimensions))
                .execute()
                .await
                .map_err(|e| {
                    common::error::Error::internal(format!("LanceDB create table error: {}", e))
                })?
        };
```

注意：`lancedb::arrow::DataType` 需要从 `arrow_schema` 导入（文件已 import）。`Field::new(name, ty, true)` 的第三参是 nullable。

- [ ] **Step 4.2: upsert 写入 payload 列**

`upsert` 中（替换原 RecordBatch 构造段；delete 语句顺手修单引号转义）：

```rust
        // 先删除旧数据（id 单引号转义，防注入）
        table
            .delete(&format!("id = '{}'", escape_sql_str(id)))
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB delete error: {}", e)))?;

        let p = &params.payload;
        let batch = RecordBatch::try_new(
            lance_schema(dimensions),
            vec![
                StdArc::new(StringArray::from(vec![id.to_string()])),
                StdArc::new(FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                    vec![Some(vector_with_options)],
                    dimensions,
                )),
                StdArc::new(StringArray::from(vec![params.content_hash.clone()])),
                StdArc::new(StringArray::from(vec![params.payload_hash.clone()])),
                StdArc::new(StringArray::from(vec![params.embedding_model.clone()])),
                StdArc::new(Int64Array::from(vec![now])),
                StdArc::new(Int64Array::from(vec![params.expire_at])),
                StdArc::new(StringArray::from(vec![p.org_id.clone()])),
                StdArc::new(StringArray::from(vec![p.agent_id.clone()])),
                StdArc::new(StringArray::from(vec![p.project_id.clone()])),
                StdArc::new(StringArray::from(vec![p.task_id.clone()])),
                StdArc::new(StringArray::from(vec![p.from_id.clone()])),
                StdArc::new(StringArray::from(vec![p.to_id.clone()])),
                StdArc::new(StringArray::from(vec![p.entity_type.clone()])),
                StdArc::new(StringArray::from(vec![p.status.clone()])),
                StdArc::new(BooleanArray::from(vec![p.is_published])),
                StdArc::new(StringArray::from(vec![p.tags.clone()])),
            ],
        )
        .map_err(|e| common::error::Error::internal(format!("Arrow record batch error: {}", e)))?;
```

文件顶部 import 增加 `arrow_array::BooleanArray`，并新增工具函数：

```rust
/// SQL 字符串字面量转义（单引号双写）
fn escape_sql_str(s: &str) -> String {
    s.replace('\'', "''")
}
```

- [ ] **Step 4.3: search 谓词下推 + payload 解析**

```rust
    async fn search(
        &self,
        collection: &str,
        query_vector: &[f32],
        top_k: i32,
        filter: Option<&crate::models::vector::VectorFilter>,
    ) -> Result<Vec<VectorSearchHit>> {
        let table = self
            .get_or_create_table(collection, query_vector.len() as i32)
            .await?;

        let mut query = table
            .vector_search(query_vector)
            .map_err(|e| {
                common::error::Error::internal(format!("LanceDB vector_search error: {}", e))
            })?
            .limit(top_k as usize);
        // 谓词下推：Top-K 在满足谓词的候选集内选取（pre-filter）
        if let Some(f) = filter {
            query = query.only_if(f.to_sql_expr(|field| field.column_name().to_string()));
        }
        let stream = query.execute().await.map_err(|e| {
            common::error::Error::internal(format!("LanceDB execute error: {}", e))
        })?;
        // ... 后续 try_collect 与解析保持原结构，解析部分换成：
        for batch in results {
            if let Some(rows) = parse_lance_batch(&batch) {
                output.extend(rows);
            }
        }
```

新增批解析函数（search/get 共用；`_distance` 列仅 vector_search 结果携带，get 路径传 `None`）：

```rust
/// 解析 Lance RecordBatch → (VectorRow, Option<distance>)
fn parse_lance_batch(batch: &RecordBatch) -> Option<Vec<(VectorRow, Option<f32>)>> {
    use arrow_array::BooleanArray;

    let str_col = |name: &str| -> Option<Vec<Option<String>>> {
        batch
            .column_by_name(name)
            .and_then(|c| c.as_any().downcast_ref::<StringArray>().ok())
            .map(|a| (0..a.len()).map(|i| if a.is_null(i) { None } else { Some(a.value(i).to_string()) }).collect())
    };
    let i64_col = |name: &str| -> Option<Vec<Option<i64>>> {
        batch
            .column_by_name(name)
            .and_then(|c| c.as_any().downcast_ref::<Int64Array>().ok())
            .map(|a| (0..a.len()).map(|i| if a.is_null(i) { None } else { Some(a.value(i)) }).collect())
    };

    let ids = str_col("id")?;
    let hashes = str_col("content_hash")?;
    let payload_hashes = str_col("payload_hash")?;
    let models = str_col("embedding_model")?;
    let indexed_ats = i64_col("indexed_at")?;
    let expire_ats = i64_col("expire_at")?;
    let distances: Vec<Option<f32>> = batch
        .column_by_name("_distance")
        .and_then(|c| c.as_any().downcast_ref::<Float32Array>().ok())
        .map(|a| (0..a.len()).map(|i| if a.is_null(i) { None } else { Some(a.value(i)) }).collect())
        .unwrap_or_default();

    let mut rows = Vec::with_capacity(ids.len());
    for i in 0..ids.len() {
        let payload = VectorPayload {
            org_id: str_col("org_id").and_then(|c| c.get(i).cloned()).flatten(),
            agent_id: str_col("agent_id").and_then(|c| c.get(i).cloned()).flatten(),
            project_id: str_col("project_id").and_then(|c| c.get(i).cloned()).flatten(),
            task_id: str_col("task_id").and_then(|c| c.get(i).cloned()).flatten(),
            from_id: str_col("from_id").and_then(|c| c.get(i).cloned()).flatten(),
            to_id: str_col("to_id").and_then(|c| c.get(i).cloned()).flatten(),
            entity_type: str_col("entity_type").and_then(|c| c.get(i).cloned()).flatten(),
            status: str_col("status").and_then(|c| c.get(i).cloned()).flatten(),
            is_published: batch
                .column_by_name("is_published")
                .and_then(|c| c.as_any().downcast_ref::<BooleanArray>().ok())
                .and_then(|a| if a.is_null(i) { None } else { Some(a.value(i)) }),
            tags: str_col("tags").and_then(|c| c.get(i).cloned()).flatten(),
        };
        rows.push((
            VectorRow {
                id: ids[i].clone()?,
                vector: Vec::new(), // LanceDB 搜索结果不返回原始向量（保持现状语义）
                payload,
                meta: VectorMeta {
                    content_hash: hashes[i].clone()?,
                    payload_hash: payload_hashes[i].clone()?,
                    embedding_model: models[i].clone()?,
                    indexed_at: indexed_ats[i]?,
                    expire_at: expire_ats[i],
                },
            },
            distances.get(i).copied().flatten(),
        ));
    }
    Some(rows)
}
```

性能提示：`str_col` 闭包在循环内反复调用会重复扫描列。实现时可先取出一次再索引（写成先 `let org_ids = str_col("org_id");` 等到循环外，循环内 `.as_ref().and_then(|c| c.get(i).cloned()).flatten()`）。以编译通过 + 测试通过为准，二选一实现皆可。

search 中组装 hit：

```rust
        for (row, dist) in parse_lance_batch(&batch).unwrap_or_default() {
            if let Some(distance) = dist {
                output.push(VectorSearchHit { row, distance });
            }
        }
```

`get` 方法：同样改用 `parse_lance_batch`（distance 为 None 时返回 row），并把 `only_if(format!("id = '{}'", id))` 改为 `only_if(format!("id = '{}'", escape_sql_str(id)))`。`get` 现有按 `expire_at_array.iter().next()` 取首行的逻辑可保留（`parse_lance_batch` 返回 Vec，取 `.pop()` 或首元素）。

- [ ] **Step 4.4: update_payload 实现（取回旧行 → delete → 重写）**

```rust
    async fn update_payload(
        &self,
        collection: &str,
        id: &str,
        payload: &crate::models::vector::VectorPayload,
    ) -> Result<()> {
        let table = self.get_or_create_table(collection, 0).await?;
        let filter_sql = format!("id = '{}'", escape_sql_str(id));

        // 1. 取回旧行（含向量 + 元信息）
        let stream = table
            .query()
            .only_if(filter_sql.clone())
            .limit(1)
            .execute()
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB execute error: {}", e)))?;
        let batches: Vec<RecordBatch> = stream.try_collect().await.map_err(|e| {
            common::error::Error::internal(format!("LanceDB collect results error: {}", e))
        })?;

        let mut old: Option<(Vec<f32>, VectorMeta)> = None;
        for batch in batches {
            // vector 列：FixedSizeList<Float32>
            if let Some(vcol) = batch.column_by_name("vector")
                && let Some(va) = vcol.as_any().downcast_ref::<FixedSizeListArray>()
                && va.len() > 0
            {
                let flat = va.value(0);
                if let Some(f) = flat.as_any().downcast_ref::<Float32Array>() {
                    let vec: Vec<f32> = (0..f.len()).map(|i| f.value(i)).collect();
                    if let Some(rows) = parse_lance_batch(&batch)
                        && let Some((row, _)) = rows.into_iter().next()
                    {
                        old = Some((vec, row.meta));
                    }
                }
            }
        }
        let Some((vector, meta)) = old else {
            return Ok(()); // 行不存在：no-op
        };

        // 2. delete + 重写（仅 payload 与 payload_hash 变化）
        table
            .delete(&filter_sql)
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB delete error: {}", e)))?;

        let dimensions = vector.len() as i32;
        let vector_with_options: Vec<Option<f32>> = vector.iter().map(|&v| Some(v)).collect();
        let now = chrono::Utc::now().timestamp();
        let p = payload;
        let batch = RecordBatch::try_new(
            lance_schema(dimensions),
            vec![
                StdArc::new(StringArray::from(vec![id.to_string()])),
                StdArc::new(FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                    vec![Some(vector_with_options)],
                    dimensions,
                )),
                StdArc::new(StringArray::from(vec![meta.content_hash])),
                StdArc::new(StringArray::from(vec![p.hash()])),
                StdArc::new(StringArray::from(vec![meta.embedding_model])),
                StdArc::new(Int64Array::from(vec![now])),
                StdArc::new(Int64Array::from(vec![meta.expire_at])),
                StdArc::new(StringArray::from(vec![p.org_id.clone()])),
                StdArc::new(StringArray::from(vec![p.agent_id.clone()])),
                StdArc::new(StringArray::from(vec![p.project_id.clone()])),
                StdArc::new(StringArray::from(vec![p.task_id.clone()])),
                StdArc::new(StringArray::from(vec![p.from_id.clone()])),
                StdArc::new(StringArray::from(vec![p.to_id.clone()])),
                StdArc::new(StringArray::from(vec![p.entity_type.clone()])),
                StdArc::new(StringArray::from(vec![p.status.clone()])),
                StdArc::new(BooleanArray::from(vec![p.is_published])),
                StdArc::new(StringArray::from(vec![p.tags.clone()])),
            ],
        )
        .map_err(|e| common::error::Error::internal(format!("Arrow record batch error: {}", e)))?;
        let batches = RecordBatchIterator::new(vec![Ok(batch)], lance_schema(dimensions));
        table
            .add(batches)
            .execute()
            .await
            .map_err(|e| common::error::Error::internal(format!("LanceDB add error: {}", e)))?;
        Ok(())
    }
```

注意：upsert 与 update_payload 的「行 → RecordBatch」构造重复度高，实现时可提取私有函数 `fn row_to_batch(id, vector, content_hash, payload_hash, model, expire_at, payload) -> Result<RecordBatch>` 供两处共用（DRY；如不提取，两处重复也可接受，以后续 plan 补）。

- [ ] **Step 4.5: 写 LanceDB 后端测试（谓词下推 + update_payload）**

在 `lance.rs` 末尾追加（LanceDB 是纯 Rust 嵌入式库，测试直接跑）：

```rust
#[cfg(test)]
mod prefilter_tests {
    use super::*;
    use crate::models::vector::{FilterValue, VectorField, VectorFilter, VectorPayload, VectorIndexParams};

    fn params(agent: &str, published: bool, dim: usize) -> VectorIndexParams {
        let payload = VectorPayload {
            agent_id: Some(agent.to_string()),
            is_published: Some(published),
            ..Default::default()
        };
        VectorIndexParams {
            vector: vec![1.0; dim],
            content_hash: format!("h-{agent}-{published}"),
            payload_hash: payload.hash(),
            payload,
            model_provider_id: "p".into(),
            embedding_model: "m".into(),
            expire_at: None,
        }
    }

    #[tokio::test]
    async fn test_lance_search_prefilter() {
        let dir = std::env::temp_dir().join(format!("lance_pf_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = LanceVectorStore::new(&dir).unwrap();
        store.init_collection("pf", 4).await.unwrap();
        store.upsert("pf", "a1", &params("agent-1", false, 4)).await.unwrap();
        store.upsert("pf", "b1", &params("agent-2", true, 4)).await.unwrap();
        store.upsert("pf", "b2", &params("agent-2", false, 4)).await.unwrap();

        // 无过滤：3 条
        let all = store.search("pf", &[1.0, 1.0, 1.0, 1.0], 10, None).await.unwrap();
        assert_eq!(all.len(), 3);

        // agent-1 视角：仅 1 条
        let f = VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into()));
        let hits = store.search("pf", &[1.0, 1.0, 1.0, 1.0], 10, Some(&f)).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].row.id, "a1");
        assert_eq!(hits[0].row.payload.agent_id.as_deref(), Some("agent-1"));

        // OR 可见性：agent-1 自己的 + 已发布的
        let vis = VectorFilter::Any(vec![
            VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into())),
            VectorFilter::Eq(VectorField::IsPublished, FilterValue::Bool(true)),
        ]);
        let hits = store.search("pf", &[1.0, 1.0, 1.0, 1.0], 10, Some(&vis)).await.unwrap();
        assert_eq!(hits.len(), 2); // a1 + b1
    }

    #[tokio::test]
    async fn test_lance_update_payload() {
        let dir = std::env::temp_dir().join(format!("lance_up_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = LanceVectorStore::new(&dir).unwrap();
        store.init_collection("up", 4).await.unwrap();
        store.upsert("up", "a1", &params("agent-1", false, 4)).await.unwrap();

        let mut new_p = VectorPayload::default();
        new_p.agent_id = Some("agent-1".into());
        new_p.is_published = Some(true); // 发布翻转场景
        store.update_payload("up", "a1", &new_p).await.unwrap();

        let row = store.get("up", "a1").await.unwrap().unwrap();
        assert_eq!(row.payload.is_published, Some(true));
        assert_eq!(row.meta.payload_hash, new_p.hash());
    }
}
```

- [ ] **Step 4.6: 运行测试**

```bash
cargo test --lib pkg::storage::lance
```

期望：PASS。

- [ ] **Step 4.6: Commit**

```bash
git add src/pkg/storage/lance.rs
git commit -m "feat(lance): payload 平铺列落库 + 谓词下推搜索 + update_payload"
```

---

## Task 5: Hnsw + SqliteVss 后端对齐

**Files:**
- Modify: `src/pkg/storage/hnsw.rs`
- Modify: `src/pkg/storage/vector.rs`（SqliteVssStore 部分）

- [ ] **Step 5.1: HnswStore 实现**

`search` 签名 + filter 求值（HnswMap 迭代先放大 take 再谓词过滤，近似补偿；回表兜底保证正确性）：

```rust
    async fn search(
        &self,
        collection: &str,
        query_vector: &[f32],
        top_k: i32,
        filter: Option<&crate::models::vector::VectorFilter>,
    ) -> common::error::Result<Vec<VectorSearchHit>> {
        let now = chrono::Utc::now().timestamp();
        let mut collections = self.collections.write().await;

        let coll = match collections.get_mut(collection) {
            Some(c) => c,
            None => return Ok(vec![]),
        };

        if coll.dirty {
            coll.rebuild();
        }

        let hnsw = match &coll.cached_index {
            Some(h) => h,
            None => return Ok(vec![]),
        };

        let query = FloatPoint(query_vector.to_vec());
        let mut search = Search::default();
        // 放大候选量再谓词过滤（HNSW 无原生谓词支持，回表兜底保证正确性）
        let fetch = (top_k as usize).saturating_mul(4).max(top_k as usize);
        let mut hits: Vec<VectorSearchHit> = hnsw
            .search(&query, &mut search)
            .take(fetch)
            .filter_map(|item| {
                let id = item.value;
                let (_, row) = coll.vectors.get(id)?;
                if row.meta.expire_at.is_none_or(|exp| exp > now)
                    && filter.is_none_or(|f| f.matches(&row.payload))
                {
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
        hits.truncate(top_k as usize);

        Ok(hits)
    }
```

`upsert` 构造 `VectorRow` 处补 `payload: params.payload.clone()` 与 `meta.payload_hash: params.payload_hash.clone()`。

`update_payload`：

```rust
    async fn update_payload(
        &self,
        collection: &str,
        id: &str,
        payload: &crate::models::vector::VectorPayload,
    ) -> common::error::Result<()> {
        let mut collections = self.collections.write().await;
        if let Some(coll) = collections.get_mut(collection)
            && let Some((_, row)) = coll.vectors.get_mut(id)
        {
            row.payload = payload.clone();
            row.meta.payload_hash = payload.hash();
            coll.dirty = true;
        }
        Ok(())
    }
```

旧 bincode 加载逻辑保持不变（零兼容逻辑；旧数据目录已在前置步骤删除，不存在旧格式文件）。

- [ ] **Step 5.2: SqliteVssStore 实现（json_extract 谓词）**

`SqliteVssStore::new` 中建 metadata 表（独立 vectors.db 无 migration 机制，运行时建。测试阶段无数据兼容负担：**不做 ALTER 补列**，旧 `vectors.db` 文件直接删除即可）：

```rust
        // 建 vector_metadata 表（含 payload 列；不做 ALTER 补列，旧 vectors.db 直接删）
        let _ = sqlx::query(
            "CREATE TABLE IF NOT EXISTS vector_metadata (
             rowid INTEGER PRIMARY KEY AUTOINCREMENT,
             collection TEXT NOT NULL,
             source_id TEXT NOT NULL,
             content_hash TEXT NOT NULL,
             payload_json TEXT,
             payload_hash TEXT NOT NULL DEFAULT '',
             model TEXT NOT NULL,
             dimensions INTEGER NOT NULL,
             expire_at INTEGER)",
        )
        .execute(&pool)
        .await;
```

`upsert`：INSERT 语句列清单加 `payload_json` / `payload_hash`，bind `serde_json::to_string(&params.payload)?` 与 `&params.payload_hash`。

`search`：签名加 `filter: Option<&VectorFilter>`；SQL 的 WHERE 拼接谓词（放在 `ORDER BY` 前）：

```rust
        let mut sql = format!(
            "SELECT m.source_id, m.content_hash, m.payload_json, m.payload_hash, m.model, m.dimensions, m.expire_at, v.distance
             FROM vss_{} v
             JOIN vector_metadata m ON v.rowid = m.rowid
             WHERE v.embedding MATCH json(?)
               AND (m.expire_at IS NULL OR m.expire_at > unixepoch())",
            collection
        );
        let mut binds: Vec<String> = vec![vector_json];
        if let Some(f) = filter {
            sql.push_str(&format!(" AND ({})", f.to_sql_expr(|field| {
                format!("json_extract(m.payload_json, '$.{}')", field.column_name())
            })));
        }
        sql.push_str(" ORDER BY v.distance LIMIT ?;");
        // 绑定顺序：vector_json → filter 无绑定值（字面量渲染）→ top_k
```

结果行解析出 `payload_json` 反序列化为 `VectorPayload` 填入 `VectorRow.payload`；`payload_hash` 填入 meta。

`update_payload`：

```rust
    async fn update_payload(
        &self,
        collection: &str,
        id: &str,
        payload: &crate::models::vector::VectorPayload,
    ) -> Result<()> {
        let json = serde_json::to_string(payload)?;
        sqlx::query(
            "UPDATE vector_metadata SET payload_json = ?, payload_hash = ? WHERE collection = ? AND source_id = ?",
        )
        .bind(&json)
        .bind(payload.hash())
        .bind(collection)
        .bind(id)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }
```

`get`：SELECT 列加 `payload_json, payload_hash` 并解析。

- [ ] **Step 5.3: 编译验证**

```bash
cargo check --workspace --exclude frontend
```

期望：仅剩 models/cortex/DAO 层的 trait impl 缺方法错误（Task 6-9 消除），storage 层无错误。

- [ ] **Step 5.4: Commit**

```bash
git add src/pkg/storage/hnsw.rs src/pkg/storage/vector.rs
git commit -m "feat(vector): Hnsw/SqliteVss 后端对齐 filter 谓词与 update_payload"
```

---

## Task 6: embed_entity 集成 payload + 10 处 Vectorizable 实现补齐

**Files:**
- Modify: `src/service/dao/cortex/native/mod.rs`（embed_entity 默认实现）
- Modify: `src/models/memory.rs`、`agent.rs`、`tool.rs`、`skill.rs`、`task.rs`、`project.rs`、`message.rs`

- [ ] **Step 6.1: embed_entity 提取 payload**

`src/service/dao/cortex/native/mod.rs` 中 `CortexDao` trait 的 `embed_entity` 默认实现改为：

```rust
    async fn embed_entity(
        &self,
        ctx: RequestContext,
        provider: &ModelProviderPo,
        entity: &dyn Vectorizable,
    ) -> Result<VectorIndexParams> {
        let text = entity.vectorize_text();
        let vectors = self
            .embed(ctx, provider, std::slice::from_ref(&text))
            .await?;
        let vector = vectors.into_iter().next().unwrap_or_default();
        let payload = entity.vector_payload();
        let payload_hash = payload.hash();
        Ok(VectorIndexParams {
            vector,
            content_hash: entity.vector_content_hash(),
            payload,
            payload_hash,
            model_provider_id: provider.id.clone(),
            embedding_model: provider.model_name.clone(),
            expire_at: entity.vector_expire_at(),
        })
    }
```

- [ ] **Step 6.2: memory 两个 PO 的实现（试点核心）**

`src/models/memory.rs`：

```rust
impl Vectorizable for ShortTermMemoryIndexPo {
    fn vectorize_text(&self) -> String {
        self.vector_text()
    }

    fn vector_collection() -> &'static str {
        "memory:short_term"
    }

    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            agent_id: Some(self.agent_id.clone()),
            task_id: self.task_id.clone(),
            status: Some(self.status.to_i32().to_string()),
            tags: Some(self.tags.clone()),
            ..Default::default()
        }
    }
}

impl Vectorizable for LongTermKnowledgeNodePo {
    fn vectorize_text(&self) -> String {
        self.vector_text()
    }

    fn vector_collection() -> &'static str {
        "memory:knowledge_node"
    }

    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            agent_id: Some(self.agent_id.clone()),
            entity_type: Some(self.node_type.clone()),
            status: Some(self.status.to_i32().to_string()),
            is_published: Some(self.is_published),
            tags: Some(self.tags.clone()),
            ..Default::default()
        }
    }
}
```

说明：`status` 用 `to_i32().to_string()`（`MemoryStatus` 只有 `to_i32()`，见 `common/src/enums/memory.rs`）；与 SQLite 存储值同构。status 第一期不参与转译白名单，仅落库。

- [ ] **Step 6.3: 其余 PO 的实现（按字段实况填充）**

各文件在现有 `impl Vectorizable for XxxPo` 中补 `vector_id` / `vector_payload` 两个方法。字段以真实 PO 为准（PO 无 org_id 的填 None）：

`src/models/agent.rs`（AgentPo 字段：id/name/role/description/capabilities/soul/model_provider_id/runtime_config/status/kind/...）：

```rust
    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            status: Some(self.status.to_i32().to_string()),
            ..Default::default()
        }
    }
```

`src/models/tool.rs`（ToolPo：id/name/description/protocol/control_mode/config/parameters_schema/tags/status/...）：

```rust
    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            tags: Some(self.tags.clone()),
            status: Some(self.status.to_i32().to_string()),
            ..Default::default()
        }
    }
```

注意：先查 `ToolPo.status` 与 `AgentPo.status` 的类型是否有 `to_i32()`（AgentStatus/ToolStatus 枚举在 `common/src/enums/`），有则用 `to_i32().to_string()`，无则用 `format!("{:?}", self.status)`——以能编译 + 稳定为准。`Tool`（Entity 版本，`src/models/tool.rs:392`）与 `Skill`（Entity 版本，`src/models/skill.rs:127`）的实现委托内部 PO：`self.po.vector_id()` / `self.po.vector_payload()`（Entity 含 `po` 字段，确认字段名后照此写）。

`src/models/skill.rs`（SkillPo：id/name/description/tags/category/parent_skill_id/author_id/author_type/modifier_id/status/...）：

```rust
    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            tags: Some(self.tags.clone()),
            entity_type: Some(self.category.clone()),
            ..Default::default()
        }
    }
```

（category 是 Option<String> 时用 `self.category.clone()` 直接填；具体以编译为准。）

`src/models/task.rs`（TaskPo：id/title/description/status/priority/tags/.../project_id/assignee_type/assignee_id/...）：

```rust
    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            project_id: self.project_id.clone(),
            agent_id: Some(self.assignee_id.clone()),
            tags: Some(self.tags.clone()),
            ..Default::default()
        }
    }
```

（`assignee_id` 若非 String 或 assignee_type 非 Agent 时语义不符，可保守填 None；以编译 + 语义合理为准。）

`src/models/project.rs`（ProjectPo：id/name/description/workflow/guidance/status/priority/tags/root_user_id/owner_agent_id/...）：

```rust
    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            agent_id: self.owner_agent_id.clone(),
            tags: Some(self.tags.clone()),
            ..Default::default()
        }
    }
```

`src/models/message.rs`（MessagePo：id/project_id/task_id/from_id/to_id/from_role/to_role/message_type/file_type/status/.../organization_id/...）：

```rust
    fn vector_id(&self) -> &str {
        &self.id
    }

    fn vector_payload(&self) -> VectorPayload {
        VectorPayload {
            org_id: Some(self.organization_id.clone()),
            project_id: self.project_id.clone(),
            task_id: self.task_id.clone(),
            from_id: Some(self.from_id.clone()),
            to_id: Some(self.to_id.clone()),
            status: Some(self.status.to_i32().to_string()),
            ..Default::default()
        }
    }
```

（`organization_id` 若为 Option<String> 则 `self.organization_id.clone()`；枚举的字符串化以能编译为准，第一期为「原始信息落库」，不参与转译白名单。）

- [ ] **Step 6.4: 编译修复面（测试 mock 与直接构造点）**

`cargo check --workspace --exclude frontend --all-targets` 列出的编译错误逐一修复，已知位置：

- `src/service/domain/runtime/awakening.rs:1425`、`awakening.rs:1481`：mock `embed_entity` 手工构造 `VectorIndexParams` → 补 `payload: VectorPayload::default(), payload_hash: VectorPayload::default().hash()`。
- `src/service/dal/memory_test.rs:1354`（MockCortexVectorDao）：同上。
- 各 `*_test.rs` 中手工构造 `VectorRow`/`VectorMeta`/`VectorIndexParams` 的地方：补 `payload`/`payload_hash` 字段（`VectorPayload::default()` + `.hash()`）。
- `VectorIndexParams::new(...)` 的调用方：补 payload 参数。

原则：mock/测试构造一律 `VectorPayload::default()`，让编译错误引导到每一处。

- [ ] **Step 6.5: 运行已有模型层单测**

```bash
cargo test --lib models::
```

期望：PASS。

- [ ] **Step 6.6: Commit**

```bash
git add -A
git commit -m "feat(vector): embed_entity 携带 payload；10 处 Vectorizable 实现补齐 vector_payload/vector_id"
```

---

## Task 7: MemoryVectorDao 签名 + MemoryQuery→VectorFilter 转译

**Files:**
- Modify: `src/service/dao/memory/mod.rs`（trait）
- Modify: `src/service/dao/memory/vector.rs`（实现 + 转译）

- [ ] **Step 7.1: 写转译测试（失败）**

`src/service/dao/memory/vector.rs` 末尾追加：

```rust
#[cfg(test)]
mod translate_tests {
    use super::*;
    use crate::models::vector::{FilterValue, VectorField, VectorFilter};
    use crate::service::dao::memory::MemoryQuery;

    #[test]
    fn test_translate_agent_only() {
        let mut q = MemoryQuery::default();
        q.agent_id = Some("agent-1".into());
        let f = translate_filters(&q).unwrap();
        assert_eq!(
            f,
            VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into()))
        );
    }

    #[test]
    fn test_translate_include_shared_or() {
        let mut q = MemoryQuery::default();
        q.agent_id = Some("agent-1".into());
        q.include_shared = true;
        let f = translate_filters(&q).unwrap();
        assert_eq!(
            f,
            VectorFilter::Any(vec![
                VectorFilter::Eq(VectorField::AgentId, FilterValue::Str("agent-1".into())),
                VectorFilter::Eq(VectorField::IsPublished, FilterValue::Bool(true)),
            ])
        );
    }

    #[test]
    fn test_translate_combined_all() {
        let mut q = MemoryQuery::default();
        q.agent_id = Some("agent-1".into());
        q.task_id = Some("task-9".into());
        q.node_type = Some("concept".into());
        let f = translate_filters(&q).unwrap();
        // All[Eq(agent), Eq(task), Eq(entity_type)]
        match f {
            VectorFilter::All(fs) => assert_eq!(fs.len(), 3),
            other => panic!("expect All, got {other:?}"),
        }
    }

    #[test]
    fn test_translate_none() {
        assert!(translate_filters(&MemoryQuery::default()).is_none());
    }
}
```

- [ ] **Step 7.2: 运行确认失败**

```bash
cargo test --lib service::dao::memory::vector
```

- [ ] **Step 7.3: trait 签名升级**

`src/service/dao/memory/mod.rs` 的 `MemoryVectorDao` trait 中两个 search 方法：

```rust
    /// 语义搜索短期记忆（业务过滤在 DAO 内转译为向量谓词下推）
    async fn search_short_term_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
        filters: &MemoryQuery,
    ) -> Result<Vec<VectorSearchHit>>;

    /// 语义搜索长期知识节点（业务过滤在 DAO 内转译为向量谓词下推）
    async fn search_knowledge_node_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
        filters: &MemoryQuery,
    ) -> Result<Vec<VectorSearchHit>>;
```

- [ ] **Step 7.4: 实现转译 + 两个 search**

`src/service/dao/memory/vector.rs`：

```rust
/// MemoryQuery → VectorFilter 白名单转译（转译下沉 DAO：哪些字段能下推只有本 DAO 知道）
///
/// 白名单：agent_id / include_shared（OR is_published 可见性）/ task_id / node_type。
/// 未覆盖字段（status / exclude_status / tags / keyword / ids / limit / order）由回业务表过滤兜底。
pub(crate) fn translate_filters(query: &MemoryQuery) -> Option<VectorFilter> {
    let mut conditions: Vec<VectorFilter> = Vec::new();

    if let Some(agent_id) = &query.agent_id {
        if query.include_shared {
            // 共享可见性：自己的节点 OR 已发布节点
            conditions.push(VectorFilter::Any(vec![
                VectorFilter::Eq(VectorField::AgentId, FilterValue::Str(agent_id.clone())),
                VectorFilter::Eq(VectorField::IsPublished, FilterValue::Bool(true)),
            ]));
        } else {
            conditions.push(VectorFilter::Eq(
                VectorField::AgentId,
                FilterValue::Str(agent_id.clone()),
            ));
        }
    }
    if let Some(task_id) = &query.task_id {
        conditions.push(VectorFilter::Eq(
            VectorField::TaskId,
            FilterValue::Str(task_id.clone()),
        ));
    }
    if let Some(node_type) = &query.node_type {
        conditions.push(VectorFilter::Eq(
            VectorField::EntityType,
            FilterValue::Str(node_type.clone()),
        ));
    }

    match conditions.len() {
        0 => None,
        1 => conditions.pop(),
        _ => Some(VectorFilter::All(conditions)),
    }
}
```

`MemoryVectorDaoImpl` 的两个 search：

```rust
    async fn search_short_term_vector(
        &self,
        _ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
        filters: &crate::service::dao::memory::MemoryQuery,
    ) -> Result<Vec<VectorSearchHit>> {
        let vector_store = _ctx.vector_store();
        let filter = translate_filters(filters);
        let results = vector_store
            .search("memory:short_term", query_vector, top_k, filter.as_ref())
            .await?;
        Ok(results)
    }
```

`search_knowledge_node_vector` 同理（collection 换 `"memory:knowledge_node"`）。upsert 两处实现补 `payload` 由 params 携带（`VectorStore::upsert` 签名未变，无需改）。

- [ ] **Step 7.5: 运行转译测试通过**

```bash
cargo test --lib service::dao::memory::vector
```

期望：PASS（此时 `dal/memory_test.rs` 的 `MockMemoryVectorDao` 编译错误留给 Task 8 一起修）。

- [ ] **Step 7.6: Commit**

```bash
git add src/service/dao/memory/
git commit -m "feat(memory): MemoryVectorDao 接收 MemoryQuery，DAO 内转译 VectorFilter 谓词下推"
```

---

## Task 8: memory DAL 三态索引 + pre-filter 搜索接入

**Files:**
- Modify: `src/service/dal/memory.rs`
- Modify: `src/service/dal/memory_test.rs`（Mock 修复）

- [ ] **Step 8.1: 三态统一索引入口**

`src/service/dal/memory.rs` 中替换 `try_build_vector_params_for_entity`（原签名返回 `Option<VectorIndexParams>`）：

```rust
/// 统一向量索引三态决策结果
enum VectorIndexAction {
    /// 无可用 provider 或内容未变化：无需任何操作
    Skipped,
    /// 仅 payload 变化：已在内部完成刷新（未调 embedding）
    PayloadRefreshed,
    /// 需要完整重索引：调用方执行 upsert
    Reindex(VectorIndexParams),
}

/// 尝试为可向量化实体构建向量索引参数（三态：Skip / PayloadOnly / FullReindex）
///
/// 三态判断取旧行比对双 hash（文本 hash + payload hash）；
/// PayloadOnly 直接刷新 payload 列，不重新调 embedding。
async fn try_build_vector_params_for_entity(
    ctx: RequestContext,
    cortex_dao: &Arc<dyn CortexDao>,
    model_provider_dao: &Arc<dyn ModelProviderDao>,
    entity: &dyn Vectorizable,
    collection: &str,
    id: &str,
) -> Result<VectorIndexAction> {
    let Some(provider) = model_provider_dao
        .get_default_embedding_provider(ctx.clone())
        .await?
    else {
        return Ok(VectorIndexAction::Skipped);
    };

    // 三态：取旧行比对双 hash
    if let Some(row) = ctx.vector_store().get(collection, id).await? {
        match entity.reindex_decision(&row.meta.content_hash, &row.meta.payload_hash) {
            ReindexDecision::Skip => return Ok(VectorIndexAction::Skipped),
            ReindexDecision::PayloadOnly => {
                ctx.vector_store()
                    .update_payload(collection, id, &entity.vector_payload())
                    .await?;
                return Ok(VectorIndexAction::PayloadRefreshed);
            }
            ReindexDecision::FullReindex => {}
        }
    }

    let params = cortex_dao
        .embed_entity(ctx.clone(), &provider, entity)
        .await?;
    Ok(VectorIndexAction::Reindex(params))
}
```

文件 import 补 `use crate::models::vector::ReindexDecision;`（按现有 import 结构合并）。

- [ ] **Step 8.2: 更新 6 个调用点（模板）**

调用点位置：`update`（约 L419 / L456）、`rebuild_vectors_if_needed`（约 L709 / L754）、`create_short_term`（约 L1439）、`create_knowledge_node`（约 L1490）。全部按同一模板改：

```rust
        match try_build_vector_params_for_entity(
            ctx.clone(),
            &self.cortex_dao,
            &self.model_provider_dao,
            &index,                                  // entity（node 调用处换成 node）
            ShortTermMemoryIndexPo::vector_collection(), // node 处换 LongTermKnowledgeNodePo
            &index.id,                               // node 处换 &node.id
        )
        .await
        {
            Ok(VectorIndexAction::Reindex(vec_params)) => {
                if let Err(e) = self
                    .memory_vector_dao
                    .upsert_short_term_vector(ctx.clone(), &index.id, &vec_params)
                    .await
                {
                    log_warn!(ctx, "vector_index", memory_id = %index.id, error = ?e, "短期记忆向量索引写入失败，已降级");
                }
            }
            Ok(VectorIndexAction::Skipped) => {
                log_debug!(ctx, "vector_index", memory_id = %index.id, "向量索引未变化，跳过");
            }
            Ok(VectorIndexAction::PayloadRefreshed) => {}
            Err(e) => {
                log_warn!(ctx, "vector_index", memory_id = %index.id, error = ?e, "短期记忆向量化失败，已降级");
            }
        }
```

knowledge_node 处把 `upsert_short_term_vector` → `upsert_knowledge_node_vector`、`memory_id` → `knowledge_id`/`node_id`。rebuild 调用点注意：rebuild 前已 `clear_collection`，旧行不存在必然 `FullReindex`，模板同样适用。

- [ ] **Step 8.3: 搜索调用点传 filters**

`search_short_term_internal`（约 L1075）与 `search_knowledge_nodes_internal`（约 L1225）的向量搜索调用加 filters：

```rust
                    match self
                        .memory_vector_dao
                        .search_short_term_vector(
                            ctx.clone(),
                            &vec_params.vector,
                            50,
                            &search.filters,
                        )
                        .await
```

（knowledge_node 处同理。）后续回表过滤逻辑（`query_for_ids` 带 `search.filters`）保持不变——回表是 SSOT 兜底。

- [ ] **Step 8.4: Mock 修复**

`src/service/dal/memory_test.rs:1403` 的 `MockMemoryVectorDao` 两个 search 方法签名补 `filters: &MemoryQuery` 参数（mock 逻辑不变，`_filters` 或用于返回预设结果均可）；`MockCortexVectorDao::embed_entity`（L1354 附近）构造 `VectorIndexParams` 处补 payload 字段。

- [ ] **Step 8.5: 运行 DAL 测试**

```bash
cargo test --lib service::dal::memory
```

期望：PASS。若既有用例因「向量索引未变化跳过」行为改变而失败（例如同一 PO 重复 create/update 断言 embed 次数），按新语义修正断言。

- [ ] **Step 8.6: Commit**

```bash
git add src/service/dal/memory.rs src/service/dal/memory_test.rs
git commit -m "feat(memory): 三态索引接入（Skip/PayloadOnly/FullReindex）+ 向量搜索谓词下推"
```

---

## Task 9: 其余 6 域 vector DAO 编译对齐

**Files:**
- Modify: `src/service/dao/agent/vector.rs`、`src/service/dao/message/vector.rs`、`src/service/dao/task/vector.rs`、`src/service/dao/project/vector.rs`、`src/service/dao/tool/vector.rs`、`src/service/dao/skill/vector.rs`

- [ ] **Step 9.1: search 调用传 None（后续各域 plan 接真过滤器）**

各文件中 `vector_store.search(collection, query_vector, top_k)` 调用统一改为：

```rust
        let results = vector_store
            .search("agents", query_vector, top_k, None)
            .await?;
```

（每处 collection 名保持原样。）这 6 域的 payload 落库已由 Task 6 完成，仅搜索转译未接。

- [ ] **Step 9.2: 编译 + 单测验证**

```bash
cargo test --lib service::dao
```

期望：PASS。

- [ ] **Step 9.3: Commit**

```bash
git add src/service/dao/
git commit -m "chore(vector): 其余 6 域 vector DAO 搜索调用对齐新签名（filter: None）"
```

---

## Task 10: 集成测试 + 全量验证

**Files:**
- Modify: `tests/integration/memory_test.rs`（新增 pre-filter 行为用例）

- [ ] **Step 10.1: 新增集成测试（agent 隔离 + 共享可见性）**

在 `tests/integration/memory_test.rs` 中参照既有用例的初始化模式（复用已存在组织/测试夹具，最小初始化：InMemory vector store + mock embedding 或跳过真实 embed——查看该文件现有用例如何构造向量索引，照抄其模式）追加：

```rust
/// pre-filter 谓词下推：agent-1 的语义搜索不被 agent-2 的数据稀释 Top-K
#[tokio::test]
async fn test_memory_vector_search_prefilter_agent_isolation() {
    // 1. 为 agent-1 / agent-2 各建若干 ShortTermMemoryIndexPo（不同 agent_id）
    // 2. 直接用向量索引写入（参照本文件既有 "vector" 用例的写入方式）
    // 3. search(agent_id = agent-1)：断言命中的每条 payload.agent_id == agent-1
    // 4. search(agent_id = agent-1, include_shared)：断言命中包含 is_published 的他人节点
}
```

断言核心：

```rust
    // agent 隔离：全部命中均属于 agent-1
    for hit in &hits {
        assert_eq!(hit.row.payload.agent_id.as_deref(), Some("agent-1"));
    }
    // 共享可见性：b-agent 的 published 节点可见
    assert!(hits.iter().any(|h| h.row.id == published_other_node_id));
```

若该文件现有向量用例依赖真实 Embedding Provider（无 provider 时降级），则改用底层 `vector_store` 直接写入向量（`VectorIndexParams` 手工构造 + `upsert`），再走 DAL search 路径断言（搜索侧 query_vector 由测试直接给出，绕开 embed）。以实际文件模式为准，目标不变：**断言 pre-filter 语义**。

- [ ] **Step 10.2: 运行集成测试**

```bash
cargo test --test memory_test
cargo test --test vector_degradation_test
cargo test --test message_vector_test
```

期望：PASS。

- [ ] **Step 10.3: 全量门禁**

```bash
make lint
make test-be
```

期望：fmt/clippy 双端零 warning、全部测试通过。

- [ ] **Step 10.4: Commit**

```bash
git add tests/integration/memory_test.rs
git commit -m "test(memory): pre-filter 谓词下推集成测试（agent 隔离 + 共享可见性）"
```

---

## 验收标准

1. `VectorStore::search` 带 filter 时，Top-K 在满足谓词的候选集内选取（Lance/InMemory/Hnsw 三后端行为一致，SqliteVss json_extract 谓词）。
2. 向量行落库携带 payload 原始信息；`VectorSearchHit.row.payload` 可用。
3. memory 域：`MemoryQuery` 经 DAO 转译下推（agent_id / task_id / node_type / include_shared OR 可见性）；`is_published` 翻转走 `PayloadOnly` 刷新不重 embed；文本未变时 Skip 短路。
4. 零兼容包袱：代码为最佳形态，不含旧 schema 检测/ALTER 补列/bincode 容错等任何迁移逻辑；旧向量数据目录已在执行前置步骤整体删除。
5. `make lint` + `make test-be` 全绿。

## 后续 plan（不在本文件范围）

- agent / message / task / project / tool / skill 各域的 DAO 转译白名单接入（模式照抄 Task 7：Query → VectorFilter 白名单 + search 传 filters）。
- wiki 知识卡同步（`ai-orz-wiki-maintainer`）：向量存储抽象卡 + 三位一体混合搜索卡需补 payload/谓词下推章节。
