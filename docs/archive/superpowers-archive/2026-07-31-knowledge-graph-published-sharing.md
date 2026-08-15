# 知识图谱蜂巢共享（published 标签）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现知识图谱的蜂巢共享机制——通过 `published` 标签让 Agent 主动共享知识节点，其他 Agent 可检索到 published 节点并通过它作为桥梁遍历跨 Agent 的知识网络。

**Architecture:** 复用现有 `tags` 字段（JSON 数组）承载 `published` 标签，无需 schema 改动。修改查询逻辑为"自己的私有节点 + 所有 published 节点"（OR 条件）。图谱遍历遇到 published 节点时可继续跨 Agent 扩展。settle_memory 时由 LLM 自动评估节点是否值得共享。

**Tech Stack:** Rust, SQLite (FTS5 + json_each), sqlx, CortexDao (LLM 调用)

---

## 文件结构

| 文件 | 责任 | 改动类型 |
|------|------|---------|
| `src/handlers/hr/agent/search_memory.rs` | 注入 agent_id 修复 + published 查询 | 修改 |
| `src/service/dao/memory/sqlite.rs` | search_knowledge_nodes SQL 改 OR 条件 | 修改 |
| `src/service/dal/memory.rs` | traverse_graph published 桥梁 + settle LLM 评估 | 修改 |
| `src/handlers/hr/agent/save_long_term_memory.rs` | node_id 生成加 agent_id | 修改 |
| `src/service/domain/system/seed/skills/memory_cognition/skill.md` | 蜂巢智慧认知章节 | 修改 |
| `src/service/domain/system/seed/default.json` | 更新描述 | 修改 |

---

## Task 1: 修复 search_memory handler 注入 agent_id

**Files:**
- Modify: `src/handlers/hr/agent/search_memory.rs`

**背景**：当前 search_memory handler 未设置 `filters.agent_id`，DAO 层 `unwrap_or_default()` 变成空串 `""`，SQL 变成 `m.agent_id = ''`，导致 Agent 搜不到自己的节点。这是已存在的 bug。

- [ ] **Step 1: 修改 handler 注入 agent_id**

在 `src/handlers/hr/agent/search_memory.rs` 中，所有构造 `MemorySearch` 的地方，把 `..Default::default()` 改为显式设置 `agent_id`：

```rust
// 分支 2（!has_seeds && do_traversal）的 search 构造：
let search = MemorySearch {
    keyword: Some(params.query.clone()),
    top_k: params.max_results,
    filters: crate::service::dao::memory::MemoryQuery {
        agent_id: ctx.agent_id().cloned(),  // ← 新增
        memory_type: Some(MemoryType::KnowledgeNode),
        limit: params.max_results.map(|l| l as usize),
        tags: params.tags.clone(),
        ..Default::default()
    },
    ..Default::default()
};

// 分支 3（普通搜索）的 search 构造：
let search = MemorySearch {
    keyword: Some(params.query.clone()),
    top_k: params.max_results,
    filters: crate::service::dao::memory::MemoryQuery {
        agent_id: ctx.agent_id().cloned(),  // ← 新增
        memory_type: Some(memory_type),
        limit: params.max_results.map(|l| l as usize),
        tags: params.tags.clone(),
        ..Default::default()
    },
    ..Default::default()
};
```

- [ ] **Step 2: 验证编译**

Run: `cargo check -p ai_orz --lib 2>&1 | head -30`
Expected: 无错误

- [ ] **Step 3: Commit**

```bash
git add src/handlers/hr/agent/search_memory.rs
git commit -m "fix(memory): inject agent_id into search_memory filters to fix search-not-found bug"
```

---

## Task 2: DAO search_knowledge_nodes 支持 published OR 条件

**Files:**
- Modify: `src/service/dao/memory/sqlite.rs`（search_knowledge_nodes 方法，约 763-843 行）

**目标**：搜索时返回"自己的私有节点 + 所有 published 节点"。

- [ ] **Step 1: 修改 SQL WHERE 条件**

在 `search_knowledge_nodes` 方法中，把 `AND m.agent_id = ?` 改为 OR 条件：

```rust
// 原代码（约 808 行）：
// WHERE knowledge_node_fts MATCH ?
//   AND m.agent_id = ?
//   AND m.status != 0
//   {tags_clause}

// 改为：
let sql = format!(
    r#"
SELECT m.id, m.agent_id, m.node_name, m.node_description, m.node_type, m.summary, m.tags,
       m.status, m.created_at, m.updated_at,
       knowledge_node_fts.rank as fts_rank
FROM knowledge_node_fts
JOIN long_term_knowledge_node m ON knowledge_node_fts.rowid = m.rowid
WHERE knowledge_node_fts MATCH ?
  AND (m.agent_id = ? OR EXISTS (SELECT 1 FROM json_each(m.tags) WHERE json_each.value = 'published'))
  AND m.status != 0
  {tags_clause}
ORDER BY knowledge_node_fts.rank
LIMIT ?
"#
);
```

- [ ] **Step 2: 同步修改 query_knowledge_nodes 方法**

在 `query_knowledge_nodes` 方法（约 681-761 行）中，当 `query.agent_id` 为 Some 时，把 `AND agent_id = ?` 改为 OR 条件：

```rust
// 原代码（约 720 行）：
// if let Some(agent_id) = &query.agent_id {
//     builder.push(" AND agent_id = ");
//     builder.push_bind(agent_id.clone());
// }

// 改为：
if let Some(agent_id) = &query.agent_id {
    builder.push(" AND (agent_id = ");
    builder.push_bind(agent_id.clone());
    builder.push(" OR EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value = 'published'))");
}
```

- [ ] **Step 3: 验证编译**

Run: `cargo check -p ai_orz --lib 2>&1 | head -30`
Expected: 无错误

- [ ] **Step 4: Commit**

```bash
git add src/service/dao/memory/sqlite.rs
git commit -m "feat(memory): search returns own nodes + published