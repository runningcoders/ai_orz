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

```
data/agent/{agent_id}/memory/
└── long_term_memory/
    └── {YYYY}/{MM}/{YYYY-MM-DD}.md
```

`knowledge_reference` 中存储的 `byte_start`/`byte_length` 可以快速定位到具体内容。

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

## 更新历史

| 日期 | 变更 | 作者 |
|------|------|------|
| 2026-04-08 | 初始设计，四层记忆模型 |  |
| 2026-04-08 | 重构：短期聚合设计、关系分离存储、引用位置更新 |  |

