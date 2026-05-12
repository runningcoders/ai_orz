# 记忆系统架构设计文档

## 核心设计思想

### 四层认知记忆模型

ai_orz 的 Agent 记忆系统采用四层认知架构，对齐人类记忆机制：

```
核心记忆 (Core Memory) → 工作记忆 (Working Memory) → 短期记忆索引 (Short-term Memory Index) → 长期知识图谱 (Long-term Knowledge Graph)
```

1. **核心记忆**：Agent 的人格、灵魂描述、能力列表，随每个请求携带，持久化保存在 Agent 记录中
2. **工作记忆**：当前会话的原始对话记录，随每个请求携带，会话结束后归档
3. **短期记忆索引**：多条相关会话细节聚合压缩后的摘要索引，用于快速检索相关上下文
4. **长期知识图谱**：经过沉淀消化后的结构化知识，形成持久化知识网络

### 核心设计原则

- **短期记忆聚合**：多条逻辑相关的对话细节聚合为一条短期记忆，**不是**每条对话细节单独作为一条记忆
- **关系独立存储**：知识图谱节点和关系分离存储，关系独立表，符合第三范式，便于查询和维护
- **完整可追溯**：每条原始记忆细节都保留完整的文件位置信息，可从知识引用追溯到原始原文
- **自然沉淀**：每日自动（"睡眠阶段"）将短期记忆消化沉淀到长期知识图谱，不需要手动操作

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
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (agent_id) REFERENCES agents(id)
);
CREATE INDEX IF NOT EXISTS idx_lkn_agent ON long_term_knowledge_node(agent_id);
CREATE INDEX IF NOT EXISTS idx_lkn_type ON long_term_knowledge_node(node_type);
CREATE VIRTUAL TABLE IF NOT EXISTS lkn_fts USING FTS5(node_name, summary, content=long_term_knowledge_node);
```

**设计说明**：
- 节点关系独立存储，本表只存储节点自身信息
- 支持全文检索节点名称和摘要

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

## 更新历史

| 日期 | 变更 | 作者 |
|------|------|------|
| 2026-04-08 | 初始设计，四层记忆模型 |  |
| 2026-04-08 | 重构：短期聚合设计、关系分离存储、引用位置更新 |  |
| 2026-05-12 | 新增 DAL 业务层设计：统一 Memory 实体、混合搜索接口 |  |
| 2026-05-12 | 新增 DAL 写入逻辑：DAO 拆分两阶段、显式 trace_ids、create/update/delete 接口 |  |
| 2026-05-13 | **实现完成**：通用 query、搜索优化、完整 update/delete |  |

