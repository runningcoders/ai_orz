# 记忆系统架构设计文档

## 核心设计思想

### 四层认知记忆模型

ai_orz 的 Agent 记忆系统采用四层认知架构，对齐人类记忆机制：

```
核心记忆 (Core Memory) → 工作记忆 (Working Memory / Trace) → 短期记忆索引 (Short-term Memory) → 长期知识图谱 (Long-term Knowledge Graph)
```

1. **核心记忆**：Agent 的人格、灵魂描述、能力列表，随每个请求携带，持久化保存在 Agent 记录中
2. **工作记忆 / Trace**：当前会话的原始对话/思考记录，客观事实，自动写入，会话结束后归档为 Trace
3. **短期记忆**：Agent 思考过程中主动总结的摘要记忆，由 Agent 主动调用神经工具写入，用于快速检索相关上下文
4. **长期知识图谱**：经过休息沉淀后的结构化知识，在 Agent 休息/睡眠时由潜意识自动消化短期记忆形成

### 核心理念（2026-07-11 更新）

**对齐人类认知机制**：

- **Trace 是客观的**：就像人听到的话、看到的事，客观发生并记录下来，系统自动写入，不需要 Agent 主动操作
- **短期记忆是主动的**：人在交流中会自己总结要点、形成印象，这是主动的认知过程。Agent 在思考时通过 `save_short_term_memory` 神经工具主动写入短期记忆
- **长期记忆是沉淀的**：人在睡眠时会整理当天的经历，形成知识和认知。Agent 在休息/睡眠时，由系统自动将近期短期记忆沉淀为知识图谱
- **读取先短后长**：思考时先检索短期记忆（"你刚刚说过什么"），需要时再通过知识图谱联想到长期记忆（"以前学过的某个知识"）

### 核心设计原则

- **短期记忆由 Agent 主动写入**：不是系统自动聚合，而是 Agent 在思考过程中根据需要主动总结并写入
- **关系独立存储**：知识图谱节点和关系分离存储，关系独立表，符合第三范式，便于查询和维护
- **完整可追溯**：每条原始记忆细节都保留完整的文件位置信息，可从知识引用追溯到原始原文
- **休息时自然沉淀**：Agent 休息/睡眠时自动将短期记忆消化沉淀到长期知识图谱，不需要手动操作
- **搜索支持图谱遍历**：记忆搜索支持语义搜索 + 知识图谱关联搜索，可沿关系链式联想

设计优势：

1. **当前会话上下文简洁**：短期记忆只保留聚合后的关键信息，不会膨胀导致上下文溢出
2. **长期知识结构化**：知识图谱结构方便检索和扩展，持久化保留历史知识
3. **完整可追溯**：任何知识都能追溯到原始对话来源
4. **渐进式演进**：支持增量沉淀，知识不断丰富

---

## 数据库表结构设计

### 1. short_term_memory_index - 短期记忆索引表

存储聚合后的短期记忆摘要，不存储原始细节位置（原始位置信息在 `knowledge_reference`）。

```sql
CREATE TABLE IF NOT EXISTS short_term_memory_index (
    id TEXT PRIMARY KEY,           -- 聚合 ID: 多个原始细节 ID 拼接后二次 hash
    agent_id TEXT NOT NULL,        -- 所属 Agent
    task_id TEXT,                  -- 所属任务 ID (可选，用于追溯到具体任务)
    role TEXT NOT NULL,            -- 记忆角色 (user/assistant/system)
    summary TEXT NOT NULL,         -- 聚合摘要
    tags TEXT NOT NULL,            -- 标签(JSON数组)
    created_at INTEGER NOT NULL,   -- 创建时间戳
    updated_at INTEGER NOT NULL,   -- 更新时间戳
    FOREIGN KEY (agent_id) REFERENCES agents(id)
);
CREATE INDEX IF NOT EXISTS idx_stm_agent ON short_term_memory_index(agent_id);
CREATE INDEX IF NOT EXISTS idx_stm_created ON short_term_memory_index(created_at DESC);
CREATE VIRTUAL TABLE IF NOT EXISTS stm_fts USING FTS5(summary, content=short_term_memory_index);
```

**设计说明**：
- `id` 是多个原始 `trace_id` 拼接后的 SHA256 hash，唯一标识这个聚合短期记忆
- `date_path`/`byte_start`/`byte_length` 已移动到 `knowledge_reference`，本表只保留摘要索引
- `trace_ids` 可通过 `knowledge_reference.short_term_id` 反向查询，无需冗余存储

---

### 2. long_term_knowledge_node - 长期知识节点表

知识图谱中的节点，不存储节点关系（关系在独立表 `knowledge_node_relation`）。

```sql
CREATE TABLE IF NOT EXISTS long_term_knowledge_node (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    node_name TEXT NOT NULL,
    node_description TEXT,
    node_type TEXT NOT NULL,
    summary TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',   -- 标签(JSON 数组字符串)
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (agent_id) REFERENCES agents(id)
);
CREATE INDEX IF NOT EXISTS idx_lkn_agent ON long_term_knowledge_node(agent_id);
CREATE INDEX IF NOT EXISTS idx_lkn_type ON long_term_knowledge_node(node_type);
CREATE INDEX IF NOT EXISTS idx_lkn_tags ON long_term_knowledge_node(tags);
CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_node_fts USING fts5(
    node_name, summary, node_description, tags,
    tokenize = 'trigram'
);
```

**设计说明**：
- 节点关系独立存储，本表只存储节点自身信息
- `tags` 字段为 JSON 数组字符串（默认 `'[]'`），与 `short_term_memory_index.tags` 对齐，用于细粒度关键词检索与过滤
- 支持全文检索节点名称、摘要、描述与标签（trigram 分词支持中英文混合搜索）
- 主表变更通过触发器自动同步到 `knowledge_node_fts`（详见 `20260712000000_memory_fts5.sql` 与 `20260724000000_knowledge_node_tags.sql`）

---

### 3. knowledge_node_relation - 知识节点关系表（新增）

独立存储知识节点之间的关系，支持多种关系类型。

```sql
CREATE TABLE IF NOT EXISTS knowledge_node_relation (
    id TEXT PRIMARY KEY,
    source_node_id TEXT NOT NULL,
    target_node_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (source_node_id) REFERENCES long_term_knowledge_node(id),
    FOREIGN KEY (target_node_id) REFERENCES long_term_knowledge_node(id)
);
CREATE INDEX IF NOT EXISTS idx_knr_source ON knowledge_node_relation(source_node_id);
CREATE INDEX IF NOT EXISTS idx_knr_target ON knowledge_node_relation(target_node_id);
CREATE INDEX IF NOT EXISTS idx_knr_type ON knowledge_node_relation(relation_type);
```

**预定义关系类型** (`KnowledgeRelationType` 枚举):

| 类型 | 说明 | 示例 |
|------|------|------|
| `RelatedTo` | 相关关联 | A 与 B 相关 |
| `Contains` / `BelongsTo` | 包含/属于 | A 包含 B / B 属于 A |
| `ParentOf` / `ChildOf` | 父/子 | A 是 B 的父节点 |
| `DependsOn` | 依赖 | A 依赖 B |
| `Implies` | 蕴含 | A 蕴含 B |
| `SimilarTo` | 相似 | A 与 B 相似 |
| `OppositeOf` | 相反 | A 与 B 相反 |
| `Causes` / `CausedBy` | 导致/由...导致 | A 导致 B |
| `Instanceof` | 实例 | A 是 B 的一个实例 |
| `PropertyOf` | 属性 | A 是 B 的属性 |
| `HasProperty` | 拥有属性 | A 有属性 B |
| `Custom` | 自定义 | 其他关系 |

**设计说明**：
- 节点和关系分离存储，更灵活，便于维护
- 关系类型使用枚举保证类型安全，支持自定义扩展
- 未知类型默认转为 `Custom`，不会 panic

---

### 4. knowledge_reference - 知识引用表（更新）

关联知识节点、短期记忆和原始记忆细节，存储完整的原始文件位置信息。

```sql
CREATE TABLE IF NOT EXISTS knowledge_reference (
    id TEXT PRIMARY KEY,
    knowledge_id TEXT NOT NULL,
    short_term_id TEXT NOT NULL,
    trace_id TEXT NOT NULL,            -- 原始记忆细节 ID
    date_path TEXT NOT NULL,           -- 文件路径 (相对于 agent 目录)
    byte_start INTEGER NOT NULL,       -- 文件起始偏移
    byte_length INTEGER NOT NULL,      -- 内容字节长度
    created_at INTEGER NOT NULL,
    FOREIGN KEY (knowledge_id) REFERENCES long_term_knowledge_node(id),
    FOREIGN KEY (short_term_id) REFERENCES short_term_memory_index(id)
);
CREATE INDEX IF NOT EXISTS idx_kr_knowledge ON knowledge_reference(knowledge_id);
CREATE INDEX IF NOT EXISTS idx_kr_short_term ON knowledge_reference(short_term_id);
CREATE INDEX IF NOT EXISTS idx_kr_trace ON knowledge_reference(trace_id);
```

**设计说明**：
- 新增 `trace_id`/`date_path`/`byte_start`/`byte_length`，每个原始细节都有完整可追溯的位置信息
- 可通过 `short_term_id` 反向查询聚合短期记忆包含哪些原始细节

---

### 5. Agent 表扩展 - 核心记忆存储

Agent 表扩展核心记忆字段：

```sql
-- 在 agents 表中添加
ALTER TABLE agents ADD COLUMN soul TEXT;
ALTER TABLE agents ADD COLUMN capabilities TEXT;
```

核心记忆从 Agent 表读取，运行时组装到 `CoreMemory`。

---

## 文件存储结构

原始记忆细节以 Markdown 格式按日期存储在文件系统中：

**完整路径（相对于 base_data_path）：**
```
agents/{agent_id}/memory/{YYYY-MM-DD}.md
```

**`knowledge_reference` 表中 `date_path` 存储格式：**
```
agents/{agent_id}/memory/{YYYY-MM-DD}.md
```
存储相对路径，读取时与配置的 `base_data_path` 拼接得到完整路径，避免重复拼接。

按日期分层存储，每日一个文件，`knowledge_reference` 中存储的 `byte_start`/`byte_length` 可以快速定位到具体内容片段。

---

## 实体关系图

```
Agent (po)
  └─► CoreMemory (soul + capabilities)
        ↓ (从 AgentPo 读取)

Agent (domain entity)
  └─► Brain 🧠
       ├─► Cortex (model_provider + 推理执行)
       └─► Memory
            ├─► CoreMemory (核心认知)
            └─► working: Vec<MemoryTrace> (当前会话工作记忆)

ShortTermMemoryIndex (聚合摘要)
  └─► KnowledgeReference (多个原始引用)
        ├─► trace_id (原始细节ID)
        └─► date_path + byte_start + byte_length (文件位置)

LongTermKnowledgeNode (知识节点)
  └─► KnowledgeNodeRelation (多个关系)
        ├─► source_node_id
        ├─► target_node_id
        └─► relation_type
```

---

## DAO 接口设计

完整的 `MemoryDaoTrait` 包含四类操作：

```rust
pub trait MemoryDaoTrait: Send + Sync {
    // ========== 短期记忆操作 ==========
    fn append_memory_trace(...) -> Result<ShortTermMemoryIndexPo, AppError>;
    fn batch_append_memory_traces(...) -> Result<Vec<ShortTermMemoryIndexPo>, AppError>;
    fn get_short_term_index(...) -> Result<Option<ShortTermMemoryIndexPo>, AppError>;
    fn list_short_term_by_agent(...) -> Result<Vec<ShortTermMemoryIndexPo>, AppError>;
    fn search_short_term(...) -> Result<Vec<ShortTermMemoryIndexPo>, AppError>;

    // ========== 长期知识节点操作 ==========
    fn save_knowledge_node(...) -> Result<(), AppError>;
    fn batch_save_knowledge_nodes(...) -> Result<(), AppError>;
    fn get_knowledge_node(...) -> Result<Option<LongTermKnowledgeNodePo>, AppError>;
    fn list_knowledge_nodes_by_agent(...) -> Result<Vec<LongTermKnowledgeNodePo>, AppError>;
    fn search_knowledge_nodes(...) -> Result<Vec<LongTermKnowledgeNodePo>, AppError>;
    fn delete_knowledge_node(...) -> Result<(), AppError>;

    // ========== 知识引用操作 ==========
    fn add_knowledge_reference(...) -> Result<(), AppError>;
    fn batch_add_knowledge_references(...) -> Result<(), AppError>;
    fn list_knowledge_references(...) -> Result<Vec<KnowledgeReferencePo>, AppError>;

    // ========== 知识节点关系操作 ==========
    fn add_knowledge_relation(...) -> Result<(), AppError>;
    fn batch_add_knowledge_relations(...) -> Result<(), AppError>;
    fn list_outgoing_relations(...) -> Result<Vec<KnowledgeNodeRelationPo>, AppError>;
    fn list_incoming_relations(...) -> Result<Vec<KnowledgeNodeRelationPo>, AppError>;
    fn list_all_relations_for_node(...) -> Result<Vec<KnowledgeNodeRelationPo>, AppError>;
    fn delete_knowledge_relation(...) -> Result<(), AppError>;
    fn delete_all_relations_for_node(...) -> Result<(), AppError>;
    fn find_relations_by_type(...) -> Result<Vec<KnowledgeNodeRelationPo>, AppError>;
}
```

---

## DAL 业务层设计（2026-05-12）

### 1. 类型枚举

```rust
/// 记忆类型（用于过滤查询）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    ShortTerm,      // 短期记忆索引
    KnowledgeNode,  // 长期知识节点
    Relation,       // 知识节点关系
    All,            // 所有类型
}
```

### 2. PO 统一枚举

包装所有底层 PO，用于 DAO 层返回统一类型：

```rust
/// 记忆底层 PO 统一枚举
#[derive(Debug, Clone)]
pub enum MemoryPo {
    ShortTerm(ShortTermMemoryIndexPo),
    KnowledgeNode(LongTermKnowledgeNodePo),
    Relation(KnowledgeNodeRelationPo),
}
```

### 3. 业务实体

对齐 Skill/Tool 命名模式：

```rust
/// 记忆业务实体（包含 PO + 搜索匹配信息）
#[derive(Debug, Clone)]
pub struct Memory {
    pub po: MemoryPo,
    pub search_match: Option<SearchMatchInfo>,
}
```

命名对齐：
- Skill = SkillPo + search_match
- Tool = ToolPo + search_match
- Memory = MemoryPo + search_match

### 4. 查询参数

直接在 MemoryQuery 中添加 memory_type 字段，MemorySearch 通过包含 MemoryQuery 自动获得：

```rust
/// 记忆通用查询参数
#[derive(Debug, Clone, Default)]
pub struct MemoryQuery {
    pub agent_id: Option<String>,
    pub status: Option<MemoryStatus>,
    pub keyword: Option<String>,
    pub limit: Option<usize>,
    pub memory_type: Option<MemoryType>,  // 按记忆类型过滤
    pub tags: Option<Vec<String>>,        // 按 tags 过滤（OR 语义）
    pub task_id: Option<String>,          // 按 task_id 过滤（注意力机制）
}

/// 记忆搜索统一入参
#[derive(Debug, Clone, Default)]
pub struct MemorySearch {
    pub keyword: Option<String>,
    pub query_vector: Option<Vec<f32>>,
    pub top_k: Option<i32>,
    pub filters: MemoryQuery,  // 包含 memory_type
}
```

### 5. DAL 统一接口

```rust
#[async_trait]
pub trait MemoryDal: Send + Sync {
    /// 🔍 统一混合搜索（关键词 + 向量语义）
    async fn search(&self, ctx: RequestContext, search: MemorySearch) 
        -> Result<Vec<Memory>, AppError>;
    
    /// 📋 通用关系型查询（纯数据库查询，无向量）
    async fn query(&self, ctx: RequestContext, query: MemoryQuery) 
        -> Result<Vec<Memory>, AppError>;
}
```

### 6. 类型关系图

```
MemoryType (enum)
    ↓ 用于过滤
MemoryQuery { memory_type, ... }
    ↓ 被包含
MemorySearch { filters: MemoryQuery, query_vector, ... }
    ↓ 作为入参
MemoryDal::search() / query()
    ↓ 返回
Vec<Memory>
    ↓ 包含
Memory { po: MemoryPo, search_match: Option<SearchMatchInfo> }
    ↓ 包含
MemoryPo (enum)
    ├─ ShortTerm(ShortTermMemoryIndexPo)
    ├─ KnowledgeNode(LongTermKnowledgeNodePo)
    └─ Relation(KnowledgeNodeRelationPo)
```

### 7. 实现要点

**search 方法流程：**
1. 根据 `search.filters.memory_type` 决定搜索哪些类型
2. 对 ShortTerm / KnowledgeNode 执行混合搜索（向量 + 关键词）
3. Relation 不支持向量搜索，只执行关键词查询（如果有关键词）
4. 聚合所有结果，统一排序，应用 limit

**query 方法流程：**
1. 根据 `query.memory_type` 分发到底层 DAO
2. 纯数据库查询，不涉及向量
3. 聚合结果返回（Relation 类型的 search_match 为 None）

---

## DAL 写入逻辑设计（2026-05-12）

### 1. 设计原则

- **两阶段写入**：记忆细节（trace）先入库，归纳总结后再创建短期记忆索引；DAO 层不再耦合两阶段
- **DAO 单一职责**：写 trace 就是写 trace，写 short-term 就是写 short-term，原子操作互不依赖
- **关联显式化**：短期记忆通过新增字段 `trace_ids` 显式记录聚合的 trace id 列表，不再依赖 hash 拼接的隐式约定
- **Trace 不可变**：MemoryTrace 写入后不可修改/删除；update/delete 仅支持 ShortTerm 与 KnowledgeNode
- **向量化降级**：写入时自动生成向量索引（trace 除外），失败仅记录 warn 不影响主流程
- **级联删除**：删除 KnowledgeNode 时同步清理入边/出边关系与引用，避免悬垂指针

### 2. DAO 层接口拆分

**移除**（与现有耦合实现一并删除，不保留 deprecated）：
- `append_memory_trace(ctx, trace, summary, tags) -> ShortTermMemoryIndexPo`
- `batch_append_memory_traces(ctx, traces) -> Vec<ShortTermMemoryIndexPo>`

**新增**：
```rust
// === Trace 阶段：仅写 daily JSONL 文件 ===
async fn append_trace(
    &self,
    ctx: RequestContext,
    trace: &MemoryTrace,
) -> Result<MemoryTrace, AppError>;

async fn batch_append_traces(
    &self,
    ctx: RequestContext,
    traces: &[MemoryTrace],
) -> Result<Vec<MemoryTrace>, AppError>;

// === Short-Term 阶段：仅 INSERT short_term_memory_index ===
async fn create_short_term_index(
    &self,
    ctx: RequestContext,
    index: &ShortTermMemoryIndexPo,
) -> Result<ShortTermMemoryIndexPo, AppError>;
```

### 3. Schema 变更

`short_term_memory_index` 表新增字段：

```sql
ALTER TABLE short_term_memory_index
    ADD COLUMN trace_ids TEXT NOT NULL DEFAULT '[]';  -- JSON 数组
```

`ShortTermMemoryIndexPo` 同步新增 `trace_ids: Vec<String>` 字段（序列化时转 JSON 字符串）。

### 4. MemoryPo / MemoryType 拓展

```rust
pub enum MemoryType {
    Trace,           // 新增
    ShortTerm,
    KnowledgeNode,
    Relation,
    All,
}

pub enum MemoryPo {
    Trace(MemoryTrace),                       // 新增
    ShortTerm(ShortTermMemoryIndexPo),
    KnowledgeNode(LongTermKnowledgeNodePo),
    Relation(KnowledgeNodeRelationPo),
}
```

`Memory` 业务实体新增辅助方法：
```rust
impl Memory {
    pub fn id(&self) -> &str;
    pub fn agent_id(&self) -> &str;
    pub fn memory_type(&self) -> MemoryType;
    pub fn vectorizable_content(&self) -> Option<&str>;  // Trace/Relation 返回 None
    pub fn supports_update(&self) -> bool;               // ShortTerm/KnowledgeNode = true
    pub fn supports_delete(&self) -> bool;               // ShortTerm/KnowledgeNode = true
}
```

### 5. MemoryCreateParams 设计

按写入范式分四个变体，参数全部使用 PO 对象（更优雅，对齐数据层）：

```rust
pub enum MemoryCreateParams {
    /// 阶段 1：仅写 trace 细节（不向量化、不创建索引）
    AppendTraces(Vec<MemoryTrace>),

    /// 阶段 2：基于已存在的 trace 创建短期记忆索引
    /// PO 内的 trace_ids 字段已包含阶段 1 返回的 id 列表
    CreateShortTerm(ShortTermMemoryIndexPo),

    /// 长期知识节点（可选附带引用关系）
    CreateKnowledgeNode {
        node: LongTermKnowledgeNodePo,
        references: Vec<KnowledgeReferencePo>,
    },

    /// 知识关系列表
    CreateRelations(Vec<KnowledgeNodeRelationPo>),
}
```

### 6. DAL 写入接口

```rust
#[async_trait]
pub trait MemoryDal: Send + Sync {
    // ... 已有 search / query

    /// 创建记忆（按变体分发）
    async fn create(
        &self,
        ctx: RequestContext,
        params: MemoryCreateParams,
    ) -> Result<Vec<Memory>, AppError>;

    /// 更新记忆（仅支持 ShortTerm / KnowledgeNode）
    async fn update(
        &self,
        ctx: RequestContext,
        memory: Memory,
    ) -> Result<Memory, AppError>;

    /// 删除记忆（仅支持 ShortTerm / KnowledgeNode；KnowledgeNode 级联删除）
    async fn delete(
        &self,
        ctx: RequestContext,
        memory_type: MemoryType,
        id: &str,
    ) -> Result<(), AppError>;
}
```

### 7. create 方法分发流程

| 变体 | 流程 | 向量化 |
|---|---|---|
| `AppendTraces(traces)` | `dao.batch_append_traces(traces)` → 包装为 `Memory::Trace` 列表 | 否 |
| `CreateShortTerm(po)` | ① `dao.create_short_term_index(po)`<br>② 向量化 `po.summary` → `vector_dao.upsert_short_term_vector`（失败 warn）<br>③ 返回 `Memory::ShortTerm` | 是 |
| `CreateKnowledgeNode { node, references }` | ① `dao.save_knowledge_node(node)`<br>② 遍历 references → `dao.add_knowledge_reference`<br>③ 向量化 `node.content` → `vector_dao.upsert_knowledge_node_vector`（失败 warn）<br>④ 返回 `Memory::KnowledgeNode` | 是 |
| `CreateRelations(rels)` | 批量 `dao.add_knowledge_relation` → 返回 `Memory::Relation` 列表 | 否 |

### 8. update 方法流程

- `Memory::ShortTerm(po)` → `dao.update_short_term_index(po)` + 重新向量化 summary
- `Memory::KnowledgeNode(po)` → `dao.save_knowledge_node(po)`（upsert 语义）+ 重新向量化 content
- `Memory::Trace` / `Memory::Relation` → 返回 `AppError::Unsupported`

### 9. delete 方法流程

- `MemoryType::ShortTerm` → `dao.forget_short_term_index(id)` + 删除向量索引
- `MemoryType::KnowledgeNode` → 级联：
  - 删除该节点的所有入边/出边关系（`dao.delete_relations_by_node`）
  - 删除该节点的所有引用（`dao.delete_references_by_node`）
  - 删除节点本身（`dao.delete_knowledge_node`）
  - 删除向量索引
- `MemoryType::Trace` / `MemoryType::Relation` / `MemoryType::All` → 返回 `AppError::Unsupported`

### 10. 典型业务调用顺序

```rust
// 阶段 1：细节先入库
let traces = dal.create(ctx, AppendTraces(vec![t1, t2, t3])).await?;
let trace_ids: Vec<_> = traces.iter().map(|m| m.id().to_string()).collect();

// 阶段 2：归纳总结后创建短期记忆索引（PO 中含 trace_ids）
let st_po = ShortTermMemoryIndexPo {
    id, agent_id, task_id, role, summary, tags, trace_ids,
    status: Active, created_at, updated_at,
};
let st = dal.create(ctx, CreateShortTerm(st_po)).await?;
```

### 11. 改动范围清单

1. **migration**：新增 `short_term_memory_index.trace_ids TEXT NOT NULL DEFAULT '[]'`
2. **models/memory.rs**：
   - `MemoryType` 加 `Trace`
   - `MemoryPo` 加 `Trace(MemoryTrace)` 变体
   - `ShortTermMemoryIndexPo` 加 `trace_ids: Vec<String>`
   - 新增 `MemoryCreateParams` 枚举
3. **service/dao/memory/mod.rs**：删除 `append_memory_trace` / `batch_append_memory_traces`，新增 `append_trace` / `batch_append_traces` / `create_short_term_index`
4. **service/dao/memory/sqlite.rs**：拆分实现
5. **service/dao/memory 单测**：相应拆分（trace 测试 / short_term 测试独立）
6. **service/dal/memory.rs**：
   - 新增 `Memory` 辅助方法
   - `MemoryDal` 加 `create` / `update` / `delete`
   - `SqliteMemoryDal` 实现写入 + 向量化
7. **调用方**：现有 `append_memory_trace` 调用点全部迁移到两阶段写入

---

## 实现完成总结（2026-05-13）

### ✅ 已实现功能

1. **通用查询方法** `MemoryQuery`：
   - 支持 `ids` / `agent_id` / `status` / `exclude_status` / `memory_type` / `keyword` / `limit`
   - DAO 层：`query_short_term()` / `query_knowledge_nodes()`
   - DAL 层：`query()` 聚合返回

2. **搜索优化**：
   - 统一使用 `cortex.embeddings()` 向量化，不再手动调用 `model_provider_dao`
   - 避免 N+1 查询：先用通用 `query(ids)` 批量获取 PO，再组装 `search_match`

3. **update 方法完整实现**：
   - `ShortTerm`：更新 PO + 重新向量化
   - `KnowledgeNode`：更新 PO + 重新向量化
   - `Trace`/`Relation`：返回 `AppError::Unsupported`

4. **delete 方法完整实现**：
   - `ShortTerm`：删除 PO + 删除向量索引
   - `KnowledgeNode`：级联删除（关系/引用/节点/向量索引）
   - `Trace`/`Relation`/`All`：返回 `AppError::Unsupported`

5. **新增支持**：
   - `AppError::Unsupported` 变体
   - `MemoryVectorDao::delete_short_term_vector()` / `delete_knowledge_node_vector()`
   - `MemoryType` 实现 `Display` trait

### 📋 文件清单

| 文件 | 变更说明 |
|------|----------|
| `src/models/memory.rs` | `MemoryType` 新增 `Display` |
| `src/service/dao/memory/mod.rs` | `MemoryQuery` 新增 `ids`，新增通用 query 方法，新增向量删除方法 |
| `src/service/dao/memory/sqlite.rs` | 实现通用 query 方法 |
| `src/service/dao/memory/vector.rs` | 实现向量删除方法 |
| `src/service/dal/memory.rs` | 完整实现 `query`/`search`/`create`/`update`/`delete` |
| `src/error.rs` | 新增 `Unsupported` 变体 |

---

## 十五、记忆神经工具集（2026-07-11 设计）

### 15.1 设计理念

记忆写入是 Agent 的主动认知行为，不同类型的记忆写入复杂度不同。为了让 Agent 使用更清晰、不容易混淆，我们将写入接口按记忆类型拆分：

| 工具名 | 用途 | 复杂度 | 神经工具 |
|--------|------|--------|---------|
| `save_short_term_memory` | 保存短期记忆摘要 | 低 | ✅ 是 |
| `save_long_term_memory` | 保存长期知识节点（含关系） | 中 | ✅ 是 |
| `search_memory` | 搜索记忆（语义 + 图谱遍历） | 高 | ✅ 是 |
| `query_memory` | 通用查询（纯数据库） | 中 | ✅ 是 |
| `update_memory` | 更新记忆 | 中 | ✅ 是 |
| `delete_memory` | 删除记忆 | 低 | ✅ 是 |
| ~~`create_memory`~~ | 通用创建（历史遗留） | 高 | ❌ 否（仅 HTTP API） |

> **注意**：`create_memory` 保留代码实现，但不再作为神经工具注入 Agent。它作为通用 HTTP API 存在，供复杂场景或管理后台使用。

### 15.2 save_short_term_memory

**用途**：Agent 在思考过程中主动写入短期记忆摘要。

**参数**：
- `summary: String` — 记忆摘要内容
- `tags: Option<Vec<String>>` — 标签列表
- `task_id: Option<String>` — 关联任务 ID
- `content: Option<String>` — 详细内容（可选）
- `trace_ids: Option<Vec<String>>` — 关联的思考 trace ID 列表（v3.7 新增，记录本条记忆从哪些 trace 中提炼）

**特点**：
- 简单直接，只写短期记忆
- 不涉及知识图谱和关系
- Agent 工作时随时可以调用
- **v3.7 起**：`build_sleep_prompt` 和 `build_summary_prompt` 模板会强制要求 Agent 调用此工具写入沉淀/总结摘要，并填入 prompt 提供的 `trace_ids`，保证记忆可追溯

### 15.3 save_long_term_memory

**用途**：Agent 写入长期知识节点，可同时创建与其他节点的关系。

**参数**：
- `node_name: String` — 节点名称
- `node_description: String` — 节点描述
- `node_type: String` — 节点类型
- `summary: String` — 摘要
- `relations: Option<Vec<KnowledgeRelationParam>>` — 关系列表
  - `target_node_id: String` — 目标节点 ID
  - `relation_type: String` — 关系类型

**特点**：
- 支持一次创建节点 + 多个关系
- 关系目标节点不存在时跳过并 warn，不影响节点创建
- 通常在休息沉淀时由系统调用，Agent 工作时也可以主动调用

---

## 十六、记忆搜索增强：知识图谱遍历（2026-07-11 设计）

### 16.1 设计理念

人类思考时，不是只搜索"匹配的知识点"，而是会沿着关联关系链式联想：
- 想到 A → 联想到 B → 联想到 C
- 有时广度优先（先看有哪些相关的）
- 有时深度优先（沿着一条线深入下去）

Agent 的记忆搜索也应该支持这种能力。

### 16.2 搜索模式

**模式一：纯语义搜索（默认）**
- `traversal_depth = 0` 或不设置
- 仅用关键词 + 向量语义搜索
- 返回直接匹配的结果

**模式二：语义 + 图谱遍历**
- `traversal_depth > 0`
- 先用语义搜索获取种子节点
- 从种子节点出发，沿知识图谱关系遍历
- 合并语义结果 + 遍历结果返回

**模式三：纯图谱遍历（指定种子）**
- 设置 `seed_node_ids`
- 跳过语义搜索，直接从指定节点出发遍历
- 用于分步搜索：Agent 第一轮搜索后，选择某个方向深入

### 16.3 遍历策略

| 策略 | 说明 | 适用场景 |
|------|------|---------|
| `breadth_first` | 广度优先，先展开所有直接关联，再深入一层 | 想知道"有哪些相关的" |
| `depth_first` | 深度优先，沿着一条关系深入到底 | 想沿着某个方向深挖 |
| `hybrid` | 混合策略，先广后深 | 先概览再深入（默认） |

### 16.4 搜索参数

```rust
pub struct SearchMemoryParams {
    pub query: String,                    // 搜索关键词
    pub max_results: Option<i32>,         // 最大结果数
    pub memory_type: Option<String>,      // 记忆类型过滤

    // === 新增：图谱遍历参数 ===
    pub traversal_depth: Option<i32>,     // 遍历深度，0=不遍历（默认）
    pub traversal_breadth: Option<i32>,   // 每层广度限制，0=不限制
    pub traversal_strategy: Option<String>, // 遍历策略：breadth_first/depth_first/hybrid
    pub seed_node_ids: Option<Vec<String>>, // 指定种子节点 ID（跳过语义搜索）

    // === 新增：标签过滤参数（2026-07-24） ===
    pub tags: Option<Vec<String>>,         // 标签过滤（OR 语义，命中任一 tag 即可）
}
```

### 16.5 标签过滤（2026-07-24 新增）

**设计理念**：短期记忆和知识节点均带 `tags` 字段（JSON 数组字符串），搜索/查询时支持按 tags 过滤，对齐 Tool/Skill 已有的 `json_each` 过滤范式。

**OR 语义**：传入多个 tag 时，命中任一即返回（非 AND），适合探索性联想场景。

**实现要点**：
- DAO 层 `MemoryQuery.tags: Option<Vec<String>>`，在 `query_short_term` / `query_knowledge_nodes` 中用 `EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value IN (...))` 过滤
- `search_short_term` / `search_knowledge_nodes`（FTS5 + JOIN）使用动态 SQL 拼接 + `json_each(m.tags)`，参数绑定顺序：keyword → agent_id → tags... → limit
- 向量搜索场景下 tags 过滤在 query 层生效（向量命中的节点若不满足 tags 条件会被过滤掉）
- Handler 层透传 `params.tags` 到 `MemorySearch.filters.tags` / `MemoryQuery.tags`，并在 `MemoryResult.tags` 回填（仅 short_term / knowledge_node 有值）
- 前端知识图谱页面新增 tags 过滤输入框（逗号分隔），节点详情面板展示 tags 徽章

### 16.6 task_id 过滤：记忆注意力机制（2026-08-04 新增）

**设计理念**：短期记忆天然关联到 task（`short_term_memory_index.task_id` 字段已存在），补齐查询/搜索链路的 task_id 过滤能力，让 Agent 可以按需聚焦到特定任务的记忆。

**注意力分层**：
| 层级 | 机制 | 默认行为 |
|------|------|----------|
| 时间注意力 | 取最近 N 条 ShortTerm | awaken/sleep 默认取最近 20 条（跨任务全局） |
| 状态注意力 | 排除 Forgotten (status=0) | 默认排除 |
| 场景注意力 | awaken 注入 project/task 实体到 prompt | 有值即拼装 |
| **主题注意力** | **task_id 过滤**（本次新增） | **默认不过滤，由 Agent 按需通过神经工具触发** |

**设计决策：默认不过滤，通过 skill 引导**
- awaken 默认取最近 20 条短期记忆保持不变（跨任务全局），保留跨 task 的"意外关联"能力
- 当 prompt 中有 task_context 时，PromptBuilder 追加【记忆聚焦提示】，告知 Agent 可用 `query_memory` / `search_memory` 的 `task_id` 参数主动聚焦
- project 过滤不新增列，通过 task 关联实现（Agent 先查 project 下的 tasks 再用 task_id 过滤记忆）

**实现要点**：
- `MemoryQuery.task_id: Option<String>`，在 `query_short_term` / `search_short_term` 中用 `AND task_id = ?` 过滤
- `SearchMemoryParams.task_id` / `QueryMemoryParams.task_id` 神经工具参数，handler 透传到 `MemorySearch.filters.task_id` / `MemoryQuery.task_id`
- `None` = 不过滤（默认），`Some(id)` = 精确匹配该 task 的记忆
- `save_short_term_memory` 神经工具已支持 task_id 写入（既有能力）

### 16.7 分步搜索示例

Agent 可以这样使用：

```
第一轮：搜索"Redis 持久化"，traversal_depth=0
  → 返回 5 条直接匹配的记忆
  → Agent 看到有一条关于 "RDB" 的记忆
  
第二轮：搜索，seed_node_ids=["kn_xxx_rdb"], traversal_depth=2, strategy=depth_first
  → 从 RDB 节点出发，深度遍历 2 层
  → 获取 RDB → AOF → 混合持久化 这条链的知识
```

---

## 十七、休息与知识沉淀机制（2026-07-31 重构）

### 17.1 设计理念

对齐人类的睡眠机制：
- **短暂休息**：上下文过载时，清理一下思绪，继续工作
- **长时间睡眠**：每天定时，整理当天的记忆，沉淀为知识

Agent 也一样：
- **短暂休息**：连续工作 N 轮后，进入 Resting 状态，清理上下文
- **睡眠沉淀**：每日定时触发，将近期短期记忆消化为长期知识图谱

### 17.2 沉淀方式的演进

**v1 工程化沉淀（2026-07-11）**：handler 内部构造 prompt，调 LLM 输出结构化 JSON，工程化解析后创建节点。问题：与消息层耦合，沉淀逻辑散落在 handler，难以扩展。

**v2 自主沉淀（2026-07-31）**：沉淀是"信号"，让 Agent 进入特定工作模式自主完成。handler 只负责生成待沉淀记忆摘要 + 调用 `sleep_and_settle`，沉淀约束模板内聚在 `PromptBuilder.build_sleep_prompt`，Agent 用已有记忆类工具自主完成归纳、查询、创建、更新、建关系、加 published 标签等操作。

**v2.1 强制写入沉淀摘要（2026-08-05，v3.7）**：v2 依赖 Agent 自发调用 `save_short_term_memory` 写入短期记忆，实际运行中经常遗忘。v3.7 在 `build_sleep_prompt` 模板中增加"强制写入沉淀摘要"步骤，明确要求 Agent **必须**调用 `save_short_term_memory` 并填入 `trace_ids`（由 prompt 提供，记录本次沉淀依赖的 trace 列表）。同时 `sleep_and_settle` 签名新增 `trace_ids` 参数，awaken 上下文压缩时传入 `pending_trace_ids`，独立沉淀场景传空。

**v2 的核心认知**：
- 沉淀是 Agent 的自主认知行为，不是工程化的 JSON 解析
- 图谱是活的，每次沉淀都是迭代优化，由 Agent 根据语义判断合并/新建/拆分
- 沉淀是内循环：不发送消息、不依赖外部信息，避免触发消息流程导致异步唤醒自己

### 17.3 Agent 运行时状态

```
Idle (空闲)
  │
  ├─ 收到消息 → Busy (忙碌)
  │     │
  │     └─ 处理完成 → Idle
  │
  ├─ 上下文过载 → Resting (休息中) ← 短暂休息
  │     │
  │     └─ 休息完成 → Idle
  │
  └─ 定时睡眠 → Resting (休息中) ← 长时间睡眠（sleep_and_settle）
        │
        └─ 沉淀完成 → Idle
```

**状态说明**：
- `Idle`：空闲，可以接受新消息
- `Busy`：忙碌，正在处理消息，拒绝新消息
- `Resting`：休息中，不接受新消息，正在进行上下文清理或知识沉淀

`sleep_and_settle` 复用 `BusyGuard` 的 RAII 机制保证 `set_idle` 一定被执行（Drop 语义与 Resting 恢复一致）。

### 17.4 触发策略

#### 策略一：上下文过载触发短暂休息

- **触发条件**：Agent 连续工作轮次达到 `max_thinking_depth` 阈值
- **休息内容**：设置为 Resting 状态，简要清理上下文，快速恢复为 Idle
- **类比**：人类工作累了，休息 5 分钟

#### 策略二：定时触发长时间睡眠（sleep_and_settle）

- **触发条件**：通过定时任务系统配置（如每日凌晨 2 点）或 Agent 主动调用 `settle_memory` 神经工具
- **休息内容**：
  - 设置为 Resting 状态
  - 查询未沉淀短期记忆（status=Active），生成编号摘要
  - 唤醒大脑进入 Settle 场景（过滤工具，只保留 neural/memory 标签）
  - 调用 `sleep_and_settle`，Agent 用已有记忆类工具自主完成沉淀
  - 恢复为 Idle
- **类比**：人类晚上睡觉，整理当天的记忆

### 17.5 沉淀流程（v2 自主沉淀）

```
settle_memory handler / CronTrigger / awaken 上下文压缩
    │
    ├── build_pending_memories_summary: 查询未沉淀短期记忆，生成编号摘要
    │
    └── load_and_settle
        │
        ├── 加载 Agent（含 tools + skills）
        ├── wake_agent_brain(scene=Settle): 装配 Brain + 过滤 Auto 工具
        │
        └── sleep_and_settle(options=ThinkingOptions::for_scene(Settle), trace_ids)
            │   └── trace_ids：awaken 压缩传 pending_trace_ids，独立沉淀传空
            │
            ├── set_resting + RAII guard
            ├── 读取最近短期记忆作为 history
            ├── 过滤 skill + Manual 工具（只保留 neural/memory 标签）
            ├── 拼装 Prompt: builder.build_sleep_prompt(summary, trace_ids)
            │     ├── 复用 system_prompt + tools + skills + common_context + history
            │     ├── 保留 user_profile（认知是具身的）
            │     ├── 保留 project/task 上下文（沉淀出的经验自带场景标签）
            │     ├── 附加沉淀约束 + 待沉淀记忆 + 任务步骤
            │     └── 强制写入沉淀摘要指令（trace_ids 渲染为 JSON 数组，v3.7 新增）
            ├── think()（5 分钟超时）
            ├── 写 Trace
            ├── 记录统计事件（status: settle success/failed）
            └── set_idle（RAII guard）
```

**Agent 在沉淀思考中自主完成**：
1. 归纳总结待沉淀的短期记忆，提炼核心概念
2. 用 `search_memory` 查询已有图谱，避免重复节点
3. 用 `save_long_term_memory` 创建新节点 / `update_memory` 更新旧节点
4. 用 `save_long_term_memory` 的 relations 参数建立节点间关系
5. 用 `update_memory` 的 `node_tags` 字段给有共享价值的节点加 `published` 标签
6. 用 `update_memory` 的 `status` 字段把短期记忆标记为 `settled`
7. **强制写入沉淀摘要**（v3.7 新增）：沉淀完成后必须调用 `save_short_term_memory` 将本次沉淀提炼的核心经验摘要写入短期记忆，`trace_ids` 字段填入 prompt 提供的本次沉淀依赖的 trace 列表，保证记忆可追溯

### 17.6 沉淀约束（内循环隔离）

**问题**：沉淀过程中若 Agent 调用消息类工具（send_message 等），会触发消息流程，导致异步唤醒自己，破坏沉淀内循环。

**方案**：双层工具过滤 + prompt 约束

| 层级 | 过滤位置 | 过滤对象 | 触发条件 |
|------|---------|---------|---------|
| 第一层 | `wake_agent_brain` | Auto 工具（Rig function calling） | Settle 场景：只保留 tags 含 `neural` 或 `memory` |
| 第二层 | `sleep_and_settle` | Manual 工具 + skill（Prompt 展示层） | Settle 场景：只保留 tags 含 `neural` 或 `memory` |
| 第三层 | `build_sleep_prompt` | Prompt 约束模板 | 明确告知 Agent 不发消息、只用记忆类工具、必须写入沉淀摘要 |

**沉淀约束模板**（内聚在 `PromptBuilder.build_sleep_prompt`）：
- **不要发送消息**：睡觉是对自身知识的沉淀积累，不应依赖外部信息
- **不要调用消息类工具**（send_message / send_task_assignment_message 等），避免触发消息流程导致异步唤醒自己
- **只使用记忆类工具**：search_memory / save_long_term_memory / update_memory / query_memory / save_short_term_memory
- 这是一个**内循环**：你与自己的记忆对话，不是与外部世界交互
- **强制写入沉淀摘要**：沉淀完成后必须调用 `save_short_term_memory` 写入短期记忆，`trace_ids` 字段必须填入 prompt 提供的列表（v3.7 新增）

### 17.7 沉淀场景保留的上下文

**认知是具身的**：沉淀场景保留 user_profile，不知道自己是谁就不能形成有效认知。

**场景化经验总结**：沉淀场景保留 project/task 上下文（如有），沉淀出的经验自带场景标签，便于场景化复用。这与 awaken 场景的业务上下文注入机制对齐（通过 ThinkingOptions 传递）。

**历史短期记忆**：作为思考素材参与沉淀，让 Agent 能看到自己最近的认知轨迹。

### 17.8 知识冲突检测（Agent 自主判断）

v2 不再使用固定的相似度阈值裁判，而是由 Agent 根据语义判断：

- 向量相似度是参考，告诉 Agent "这条新知识与哪个旧节点相关"
- 合并、更新、新建还是拆分，由 Agent 根据语义判断
- 没有固定阈值，可复用、可抽象、对图谱有贡献的就沉淀

**合并策略**（Agent 自主执行）：
- 节点描述：取更完整的版本
- 关系：合并去重
- 引用：追加新的引用来源
- 过大且可拆分的旧节点：拆分为子节点 + 概述父节点 + `contains` 关系

---

## 更新历史

| 日期 | 变更 | 作者 |
|------|------|------|
| 2026-04-08 | 初始设计，四层记忆模型 |  |
| 2026-04-08 | 重构：短期聚合设计、关系分离存储、引用位置更新 |  |
| 2026-05-12 | 新增 DAL 业务层设计：统一 Memory 实体、混合搜索接口 |  |
| 2026-05-12 | 新增 DAL 写入逻辑：DAO 拆分两阶段、显式 trace_ids、create/update/delete 接口 |  |
| 2026-05-13 | **实现完成**：通用 query、搜索优化、完整 update/delete |  |
| 2026-07-11 | **理念升级**：核心理念对齐人类认知、记忆神经工具拆分、搜索图谱遍历、休息与沉淀机制 |  |
| 2026-07-24 | **tags 全链路支持**：SearchMemoryParams/QueryMemoryParams/MemoryResult 新增 tags 字段；MemoryQuery.tags 实现 OR 语义过滤（json_each）；ShortTermMemoryIndexPo/LongTermKnowledgeNodePo 实现 Vectorizable trait；前端知识图谱节点支持多色边框与动态信息展示 |  |
| 2026-07-31 | **沉淀机制重构（v2 自主沉淀）**：settle_memory 不再工程化创建节点，改为调用 sleep_and_settle 触发 Agent 自主沉淀；沉淀约束模板内聚在 PromptBuilder.build_sleep_prompt；双层工具过滤（Auto 在 wake_agent_brain，Manual+skill 在 sleep_and_settle）确保 Settle 场景只接触记忆类工具；沉淀场景保留 user_profile + project/task 上下文（认知具身 + 场景化经验总结）；详见 runtime_design.md 第二十五章 |  |
| 2026-08-04 | **task_id 记忆注意力机制**：MemoryQuery / SearchMemoryParams / QueryMemoryParams 新增 task_id 字段；query_short_term / search_short_term SQL 支持 task_id WHERE 过滤；PromptBuilder 在 task_context 有值时追加【记忆聚焦提示】引导 Agent 按需聚焦；默认 awaken 行为不变（跨任务全局取最近 20 条），project 过滤通过 task 关联实现不新增列 |  |
| 2026-08-05 | **统一总结流程 + 强制记忆写入（v3.7）**：正常 Final 完成也触发 awaken_for_summary 总结流程；awaken 循环维护 pending_trace_ids 跟踪自上次压缩以来的 trace 列表；build_sleep_prompt / build_summary_prompt 新增 trace_ids 参数，prompt 模板强制要求 Agent 调用 save_short_term_memory 并填入 trace_ids；SaveShortTermMemoryParams 新增 trace_ids 字段；详见 runtime_design.md 25.12 |  |

