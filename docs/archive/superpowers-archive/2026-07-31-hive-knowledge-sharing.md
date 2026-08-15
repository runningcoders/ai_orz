# 蜂巢知识共享 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现"蜂巢知识共享"机制——知识节点通过 `published` 标签全局共享，Agent 在沉淀工作模式中自主决定是否发布；settle_memory 改为发消息触发 Agent 进入沉淀工作模式，由 Agent 用已有工具自主完成沉淀。

**Architecture:**
1. `settle_memory` handler 不再工程化创建节点，改为拼装沉淀场景 prompt + 查询未沉淀短期记忆摘要，通过 `send_to_agent` 给自己发消息触发 awaken，Agent 在 awaken 中用已有工具完成沉淀
2. 知识节点的 `published` 标签实现全局共享：search_memory 返回"自己的私有节点 + 所有 published 节点"；traverse_graph 通过 published 节点作为桥梁跨 Agent 遍历
3. 扩展 `update_memory` 支持 status（标记 Settled）和 KnowledgeNode tags（加 published）；扩展 `query_memory` 支持 status 过滤

**Tech Stack:** Rust + axum + sqlx + SQLite（FTS5 + json_each）+ AOP 事件中心 + MessageConsumer

---

## File Structure

| 文件 | 责任 | 改动类型 |
|------|------|---------|
| `common/src/api/neural_tools.rs` | 神经工具 DTO 定义 | 修改：扩展 UpdateMemoryParams + QueryMemoryParams |
| `src/handlers/hr/agent/settle_memory.rs` | 沉淀记忆 handler | 修改：改为发消息触发 |
| `src/handlers/hr/agent/update_memory.rs` | 更新记忆 handler | 修改：支持 status + KnowledgeNode tags |
| `src/handlers/hr/agent/query_memory.rs` | 查询记忆 handler | 修改：支持 status 过滤 |
| `src/handlers/hr/agent/search_memory.rs` | 搜索记忆 handler | 修改：注入 agent_id + published 共享逻辑 |
| `src/handlers/hr/agent/save_long_term_memory.rs` | 保存长期记忆 handler | 修改：node_id 改为 UUID v7 |
| `src/service/dao/memory/sqlite.rs` | SQLite DAO 实现 | 修改：search_knowledge_nodes 支持 published 跨 Agent |
| `src/service/dao/memory/mod.rs` | DAO trait + 查询结构体 | 修改：MemoryQuery 增加 include_shared 标志 |
| `src/service/domain/runtime/mod.rs` | RuntimeDomain trait | 修改：rest_and_settle 保留但 settle_memory 不再调用 |
| `src/service/domain/system/seed/skills/memory_cognition/skill.md` | 记忆认知技能文档 | 修改：沉淀工作模式 + 蜂巢共享认知 |
| `src/service/domain/system/seed/default.json` | 预置技能配置 | 修改：更新 description |

---

## Task 1: 扩展 update_memory 支持 status 和 KnowledgeNode tags

**Files:**
- Modify: `common/src/api/neural_tools.rs`（UpdateMemoryParams 结构体）
- Modify: `src/handlers/hr/agent/update_memory.rs`（ShortTerm + KnowledgeNode 分支）
- Test: `src/handlers/hr/agent/update_memory.rs`（新增单元测试）

- [ ] **Step 1: 扩展 UpdateMemoryParams DTO**

在 `common/src/api/neural_tools.rs` 找到 `UpdateMemoryParams` 结构体，增加 `status` 和 `node_tags` 字段：

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct UpdateMemoryParams {
    pub memory_id: String,
    pub content: Option<String>,
    pub summary: Option<String>,
    pub tags: Option<Vec<String>>,
    /// 新增：更新记忆状态（如把短期记忆标记为 Settled）
    pub status: Option<String>,
    /// 新增：更新知识节点的 tags（与 tags 字段区分，tags 用于 ShortTerm，node_tags 用于 KnowledgeNode）
    pub node_tags: Option<Vec<String>>,
}
```

注意：由于 `tags` 字段当前在 handler 中只对 ShortTerm 生效，KnowledgeNode 分支未实现 tags 更新。为避免语义混淆，新增 `node_tags` 字段专门用于 KnowledgeNode 的 tags 更新。

- [ ] **Step 2: 修改 update_memory handler 的 ShortTerm 分支支持 status**

在 `src/handlers/hr/agent/update_memory.rs` 的 ShortTerm 分支中，增加 status 更新逻辑：

```rust
MemoryPo::ShortTerm(mut po) => {
    if let Some(content) = params.content {
        po.summary = content;
    }
    if let Some(summary) = params.summary {
        po.summary = summary;
    }
    if let Some(tags) = params.tags {
        po.tags = serde_json::to_string(&tags).unwrap_or_default();
    }
    // 新增：支持 status 更新（如标记为 Settled）
    if let Some(status_str) = params.status {
        po.status = parse_memory_status(&status_str);
    }
    runtime_domain().memory().update(ctx.clone(), Memory { po, distance: None }).await?;
}
```

新增辅助函数 `parse_memory_status`：

```rust
fn parse_memory_status(s: &str) -> common::enums::MemoryStatus {
    match s.to_lowercase().as_str() {
        "active" | "1" => common::enums::MemoryStatus::Active,
        "forgotten" | "0" => common::enums::MemoryStatus::Forgotten,
        "settled" | "2" => common::enums::MemoryStatus::Settled,
        _ => common::enums::MemoryStatus::Active,
    }
}
```

- [ ] **Step 3: 修改 update_memory handler 的 KnowledgeNode 分支支持 node_tags**

在 KnowledgeNode 分支中增加 node_tags 更新逻辑：

```rust
MemoryPo::KnowledgeNode(mut po) => {
    if let Some(content) = params.content {
        po.node_description = content;
    }
    if let Some(summary) = params.summary {
        po.summary = summary;
    }
    // 新增：支持 KnowledgeNode tags 更新（用于加 published 标签等）
    if let Some(node_tags) = params.node_tags {
        po.tags = serde_json::to_string(&node_tags).unwrap_or_default();
    }
    // 新增：支持 status 更新（如遗忘节点）
    if let Some(status_str) = params.status {
        po.status = parse_memory_status(&status_str);
    }
    runtime_domain().memory().update(ctx.clone(), Memory { po, distance: None }).await?;
}
```

- [ ] **Step 4: 验证编译通过**

Run: `cargo check -p ai_orz --lib`
Expected: 编译通过无错误

- [ ] **Step 5: Commit**

```bash
git add common/src/api/neural_tools.rs src/handlers/hr/agent/update_memory.rs
git commit -m "feat(memory): extend update_memory to support status and KnowledgeNode tags"
```

---

## Task 2: 扩展 query_memory 支持 status 过滤

**Files:**
- Modify: `common/src/api/neural_tools.rs`（QueryMemoryParams 结构体）
- Modify: `src/handlers/hr/agent/query_memory.rs`（构造 MemoryQuery 时传入 status）

- [ ] **Step 1: 扩展 QueryMemoryParams DTO**

在 `common/src/api/neural_tools.rs` 找到 `QueryMemoryParams` 结构体，增加 `status` 字段：

```rust
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct QueryMemoryParams {
    pub agent_id: Option<String>,
    pub memory_type: Option<String>,
    pub limit: Option<i32>,
    pub tags: Option<Vec<String>>,
    /// 新增：按状态过滤（active/settled/forgotten）
    pub status: Option<String>,
}
```

- [ ] **Step 2: 修改 query_memory handler 传入 status**

在 `src/handlers/hr/agent/query_memory.rs` 中，构造 MemoryQuery 时解析并传入 status：

```rust
let status = params.status.as_deref().map(parse_memory_status);

let query = MemoryQuery {
    agent_id: params.agent_id.clone(),
    memory_type: Some(memory_type),
    limit: params.limit.map(|l| l as usize),
    tags: params.tags.clone(),
    status,  // 新增
    ..Default::default()
};
```

注意：`parse_memory_status` 函数应复用 Task 1 中定义的。若该函数定义在 update_memory.rs，需考虑提取到公共位置（如 `src/handlers/hr/agent/mod.rs` 或独立工具模块）。为简化，可在 query_memory.rs 内重复定义一个相同函数。

- [ ] **Step 3: 验证编译通过**

Run: `cargo check -p ai_orz --lib`
Expected: 编译通过无错误

- [ ] **Step 4: Commit**

```bash
git add common/src/api/neural_tools.rs src/handlers/hr/agent/query_memory.rs
git commit -m "feat(memory): extend query_memory to support status filter"
```

---

## Task 3: 修复 save_long_term_memory 的 node_id 生成

**Files:**
- Modify: `src/handlers/hr/agent/save_long_term_memory.rs`（node_id 生成逻辑）

- [ ] **Step 1: 修改 node_id 生成方式**

在 `src/handlers/hr/agent/save_long_term_memory.rs` 中，将 node_id 从 `SHA256(name+ts)` 改为 UUID v7：

原代码（约第 33-34 行）：
```rust
let id_content = format!("{}{}", params.node_name, now);
let node_id = format!("kn_{}", sha256::digest(id_content));
```

改为：
```rust
let node_id = format!("kn_{}", uuid::Uuid::now_v7().simple());
```

同时检查 relation_id 生成（约第 76-80 行），也改为 UUID v7：

原代码：
```rust
let id_content = format!("{}{}{}{}", r.source_node_id, r.target_node_id, r.relation_type, now);
let relation_id = format!("kr_{}", sha256::digest(id_content));
```

改为：
```rust
let relation_id = format!("kr_{}", uuid::Uuid::now_v7().simple());
```

- [ ] **Step 2: 移除未使用的 import**

如果 `sha256` import 不再被使用，移除它：
```rust
// 移除：use sha256::digest; 或类似 import
```

- [ ] **Step 3: 验证编译通过**

Run: `cargo check -p ai_orz --lib`
Expected: 编译通过无错误，无 unused import 警告

- [ ] **Step 4: Commit**

```bash
git add src/handlers/hr/agent/save_long_term_memory.rs
git commit -m "fix(memory): use UUID v7 for node_id to avoid cross-agent collision"
```

---

## Task 4: 修复 search_memory agent_id bug + 支持 published 跨 Agent 查询

**Files:**
- Modify: `src/service/dao/memory/mod.rs`（MemoryQuery 增加 include_shared 标志）
- Modify: `src/service/dao/memory/sqlite.rs`（search_knowledge_nodes SQL 修改）
- Modify: `src/handlers/hr/agent/search_memory.rs`（注入 agent_id + include_shared）

- [ ] **Step 1: MemoryQuery 增加 include_shared 标志**

在 `src/service/dao/memory/mod.rs` 的 `MemoryQuery` 结构体中增加字段：

```rust
#[derive(Debug, Clone, Default)]
pub struct MemoryQuery {
    pub ids: Option<Vec<String>>,
    pub agent_id: Option<String>,
    pub status: Option<MemoryStatus>,
    pub exclude_status: Option<MemoryStatus>,
    pub keyword: Option<String>,
    pub limit: Option<usize>,
    pub memory_type: Option<MemoryType>,
    pub tags: Option<Vec<String>>,
    /// 新增：是否包含其他 Agent 共享的 published 节点（默认 false）
    pub include_shared: bool,
}
```

- [ ] **Step 2: 修改 search_knowledge_nodes SQL 支持 published 共享**

在 `src/service/dao/memory/sqlite.rs` 的 `search_knowledge_nodes` 方法中，修改 agent_id 条件：

原代码（约第 818-825 行）：
```rust
let agent_id = search.filters.agent_id.unwrap_or_default();
// ...
let sql = format!(
    r#"
SELECT ... FROM knowledge_node_fts
JOIN long_term_knowledge_node m ON knowledge_node_fts.rowid = m.rowid
WHERE knowledge_node_fts MATCH ?
  AND m.agent_id = ?
  AND m.status != 0
  {tags_clause}
ORDER BY knowledge_node_fts.rank
LIMIT ?
"#
);
```

改为（根据 include_shared 决定是否包含 published 节点）：
```rust
let agent_id = search.filters.agent_id.unwrap_or_default();
let include_shared = search.filters.include_shared;

// 构造归属过滤条件：自己的节点 OR（include_shared 时）published 节点
let ownership_clause = if include_shared {
    "(m.agent_id = ? OR EXISTS (SELECT 1 FROM json_each(m.tags) WHERE json_each.value = 'published'))".to_string()
} else {
    "m.agent_id = ?".to_string()
};

let sql = format!(
    r#"
SELECT m.id, m.agent_id, m.node_name, m.node_description, m.node_type, m.summary, m.tags,
       m.status, m.created_at, m.updated_at,
       knowledge_node_fts.rank as fts_rank
FROM knowledge_node_fts
JOIN long_term_knowledge_node m ON knowledge_node_fts.rowid = m.rowid
WHERE knowledge_node_fts MATCH ?
  AND {ownership_clause}
  AND m.status != 0
  {tags_clause}
ORDER BY knowledge_node_fts.rank
LIMIT ?
"#
);
```

- [ ] **Step 3: 修改 search_memory handler 注入 agent_id 和 include_shared**

在 `src/handlers/hr/agent/search_memory.rs` 中，所有构造 MemorySearch 的地方，注入 agent_id 和 include_shared：

```rust
let agent_id = ctx.agent_id().cloned().unwrap_or_default();

// 分支 1 和 2 的 search 构造（约第 71-80 行和 111-126 行）：
let search = MemorySearch {
    keyword: Some(params.query.clone()),
    top_k: params.max_results,
    filters: crate::service::dao::memory::MemoryQuery {
        memory_type: Some(MemoryType::KnowledgeNode),
        limit: params.max_results.map(|l| l as usize),
        tags: params.tags.clone(),
        agent_id: Some(agent_id.clone()),        // 新增：注入 agent_id
        include_shared: true,                     // 新增：包含 published 共享节点
        ..Default::default()
    },
    ..Default::default()
};

// 分支 3 的 search 构造（约第 130-145 行）：
let search = MemorySearch {
    keyword: Some(params.query.clone()),
    top_k: params.max_results,
    filters: crate::service::dao::memory::MemoryQuery {
        memory_type: Some(memory_type),
        limit: params.max_results.map(|l| l as usize),
        tags: params.tags.clone(),
        agent_id: Some(agent_id.clone()),        // 新增：注入 agent_id
        include_shared: true,                     // 新增：包含 published 共享节点
        ..Default::default()
    },
    ..Default::default()
};
```

注意：对于 ShortTerm 类型的搜索，include_shared 应为 false（短期记忆是私有的，不共享）。需要根据 memory_type 判断：

```rust
let include_shared = matches!(memory_type, MemoryType::KnowledgeNode | MemoryType::All);
```

- [ ] **Step 4: 验证编译通过**

Run: `cargo check -p ai_orz --lib`
Expected: 编译通过无错误

- [ ] **Step 5: Commit**

```bash
git add src/service/dao/memory/mod.rs src/service/dao/memory/sqlite.rs src/handlers/hr/agent/search_memory.rs
git commit -m "feat(memory): fix search_memory agent_id bug and support published cross-agent sharing"
```

---

## Task 4.5: 修复 query_memory 权限校验 + traverse_graph 共享逻辑

**背景**：这两个点是 Task 4 的延伸——Task 4 修复了 search_memory，但 query_memory 和 traverse_graph 仍然有权限漏洞。在 published 共享机制下，应该统一遵循"自己的私有节点 + 所有 published 节点"的可见性规则。

**Files:**
- Modify: `src/handlers/hr/agent/query_memory.rs`（权限校验 + include_shared）
- Modify: `src/service/dal/memory.rs`（fetch_nodes_by_ids 加入 include_shared 过滤）
- Modify: `src/service/dao/memory/sqlite.rs`（query_knowledge_nodes 支持 include_shared）

- [ ] **Step 1: 修改 query_memory handler 加入权限校验和 include_shared**

在 `src/handlers/hr/agent/query_memory.rs` 中，根据传入的 agent_id 与当前 ctx agent_id 的关系决定查询逻辑：

```rust
let ctx_agent_id = ctx.agent_id().cloned().unwrap_or_default();
let query_agent_id = params.agent_id.clone().unwrap_or_else(|| ctx_agent_id.clone());

// 权限校验：查询其他 Agent 的记忆时，只能看到 published 节点
let is_querying_other = query_agent_id != ctx_agent_id && !ctx_agent_id.is_empty();

let query = MemoryQuery {
    agent_id: Some(query_agent_id),
    memory_type: Some(memory_type),
    limit: params.limit.map(|l| l as usize),
    tags: params.tags.clone(),
    status,
    // 查询自己时包含 published 共享节点；查询他人时只返回 published 节点
    include_shared: !is_querying_other,
    ..Default::default()
};
```

但上面的逻辑有问题：查询他人时 `include_shared=false` 且 `agent_id=other`，会返回 other 的所有节点（包括私有）。需要调整 DAO 层逻辑。

**更清晰的方案**：在 DAO 层 query_knowledge_nodes 中实现三种模式：
- `include_shared=false, agent_id=Some(x)` → 只返回 x 的私有节点（不含 published）
- `include_shared=true, agent_id=Some(x)` → 返回 x 的所有节点 + 所有 published 节点
- 查询他人时：用专门的 `only_shared=true` 模式，只返回 published 节点

为简化，采用另一种方案：query_memory 查询他人时，强制 tags 包含 "published"：

```rust
let ctx_agent_id = ctx.agent_id().cloned().unwrap_or_default();
let query_agent_id = params.agent_id.clone().unwrap_or_else(|| ctx_agent_id.clone());
let is_querying_other = query_agent_id != ctx_agent_id && !ctx_agent_id.is_empty();

// 查询他人时，强制只返回 published 节点（通过 tags 过滤实现）
let mut tags = params.tags.clone().unwrap_or_default();
if is_querying_other {
    if !tags.contains(&"published".to_string()) {
        tags.push("published".to_string());
    }
}

let query = MemoryQuery {
    agent_id: Some(query_agent_id),
    memory_type: Some(memory_type),
    limit: params.limit.map(|l| l as usize),
    tags: if tags.is_empty() { None } else { Some(tags) },
    status,
    include_shared: !is_querying_other,  // 查询自己时包含 published 共享
    ..Default::default()
};
```

注意：这里有个语义问题。查询他人时 `agent_id=other` + `tags=["published"]`，DAO 层的 SQL 是 `AND agent_id = ? AND EXISTS (tags IN published)`，这会返回 other agent 的 published 节点——符合预期。但 `include_shared` 此时为 false，所以不会包含其他 agent 的 published 节点，只返回 other agent 自己的 published 节点——这也符合预期。

- [ ] **Step 2: 修改 query_knowledge_nodes DAO 支持 include_shared**

在 `src/service/dao/memory/sqlite.rs` 的 `query_knowledge_nodes` 方法中，参考 Task 4 的 search_knowledge_nodes，加入 include_shared 的归属过滤逻辑：

找到 query_knowledge_nodes 方法中构造 agent_id 条件的部分（类似 `AND agent_id = ?`），改为：

```rust
let agent_id = query.agent_id.unwrap_or_default();
let include_shared = query.include_shared;

let ownership_clause = if include_shared && !agent_id.is_empty() {
    "(agent_id = ? OR EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value = 'published'))".to_string()
} else if agent_id.is_empty() && include_shared {
    "EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value = 'published')".to_string()
} else if !agent_id.is_empty() {
    "agent_id = ?".to_string()
} else {
    "1=1".to_string()
};
```

注意：query_knowledge_nodes 使用 sqlx query_builder，需要用 `builder.push(&ownership_clause)` 而非 format 字符串。具体实现时根据现有代码结构调整。

- [ ] **Step 3: 修改 fetch_nodes_by_ids 加入 include_shared 过滤**

在 `src/service/dal/memory.rs` 的 `fetch_nodes_by_ids` 方法中，加入 agent_id 和 include_shared 过滤，防止跨 Agent 遍历私有节点：

原代码（约第 879-893 行）：
```rust
async fn fetch_nodes_by_ids(
    &self, ctx: RequestContext, node_ids: &HashSet<String>,
) -> Result<Vec<LongTermKnowledgeNodePo>> {
    if node_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<String> = node_ids.iter().cloned().collect();
    let query = MemoryQuery {
        ids: Some(ids),
        ..Default::default()  // ← 没有 agent_id，也没有 tags 过滤！
    };
    self.memory_dao.query_knowledge_nodes(ctx, query).await
}
```

改为（需要传入 agent_id 和 include_shared）：
```rust
async fn fetch_nodes_by_ids(
    &self, ctx: RequestContext, node_ids: &HashSet<String>,
) -> Result<Vec<LongTermKnowledgeNodePo>> {
    if node_ids.is_empty() {
        return Ok(Vec::new());
    }
    let agent_id = ctx.agent_id().cloned().unwrap_or_default();
    let ids: Vec<String> = node_ids.iter().cloned().collect();
    let query = MemoryQuery {
        ids: Some(ids),
        agent_id: Some(agent_id),
        include_shared: true,  // 包含 published 共享节点，支持跨 Agent 遍历
        ..Default::default()
    };
    self.memory_dao.query_knowledge_nodes(ctx, query).await
}
```

但这里有个问题：`MemoryQuery.ids` 和 `agent_id` 同时存在时，DAO 层的 SQL 逻辑是什么？需要确认 query_knowledge_nodes 是否同时支持 ids + agent_id 过滤。

如果 DAO 层的 ids 过滤是 `AND id IN (...)`，agent_id 过滤是 `AND agent_id = ?`，那么同时存在时会返回"属于该 agent 的指定 ids 节点"。但我们需要的是"指定 ids 中属于该 agent 或 published 的节点"。

**关键**：需要修改 query_knowledge_nodes 的 ids 过滤逻辑，使其与 ownership_clause 是 OR 关系而非 AND 关系。或者更简单的方案：fetch_nodes_by_ids 不按 agent_id 过滤（保持原样），但在返回结果中过滤掉不属于当前 agent 且非 published 的节点。

**采用过滤结果的方案**（更简单，不改 DAO 层 ids 逻辑）：

```rust
async fn fetch_nodes_by_ids(
    &self, ctx: RequestContext, node_ids: &HashSet<String>,
) -> Result<Vec<LongTermKnowledgeNodePo>> {
    if node_ids.is_empty() {
        return Ok(Vec::new());
    }
    let agent_id = ctx.agent_id().cloned().unwrap_or_default();
    let ids: Vec<String> = node_ids.iter().cloned().collect();
    let query = MemoryQuery {
        ids: Some(ids),
        ..Default::default()
    };
    let nodes = self.memory_dao.query_knowledge_nodes(ctx, query).await?;
    // 过滤：只保留自己的节点或 published 节点
    let visible_nodes: Vec<_> = nodes
        .into_iter()
        .filter(|n| {
            n.agent_id == agent_id
                || n.tags.contains("\"published\"")
        })
        .collect();
    Ok(visible_nodes)
}
```

注意：`n.tags.contains("\"published\"")` 是简单的字符串包含检查（tags 是 JSON 数组字符串如 `["tag1","published"]`）。更严谨的方式是解析 JSON，但字符串包含在当前场景下足够。

- [ ] **Step 4: 验证编译通过**

Run: `cargo check -p ai_orz --lib`
Expected: 编译通过无错误

- [ ] **Step 5: Commit**

```bash
git add src/handlers/hr/agent/query_memory.rs src/service/dao/memory/sqlite.rs src/service/dal/memory.rs
git commit -m "feat(memory): fix query_memory permission and traverse_graph shared visibility"
```

---

## Task 5: 改造 settle_memory 为发消息触发沉淀工作模式

**Files:**
- Modify: `src/handlers/hr/agent/settle_memory.rs`（改造为发消息触发）
- Reference: `src/service/domain/message/delivery.rs`（send_to_agent 实现）
- Reference: `src/handlers/finance/message/send_message_to_agent.rs`（send_to_agent 调用范例）

- [ ] **Step 1: 改造 settle_memory handler**

将 `src/handlers/hr/agent/settle_memory.rs` 改为：
1. 查询未沉淀的短期记忆数量和摘要
2. 拼装沉淀场景 prompt
3. 调用 send_to_agent 给自己发消息触发 awaken

```rust
//! Handler: 沉淀记忆 - Neural Tool
//!
//! 触发 Agent 进入沉淀工作模式：拼装场景 prompt，通过消息系统给 Agent 自己发消息，
//! Agent 在 awaken 中用已有工具自主完成沉淀（归纳总结、创建/更新节点、建关系、加 published 标签）。

use crate::pkg::RequestContext;
use crate::service::dao::memory::{MemoryQuery, MemoryType, dao as memory_dao};
use crate::service::domain::message::domain as message_domain;
use crate::service::domain::message::{MessageRole, SendToAgentCommand};
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{SettleMemoryParams, SettleMemoryResponse};
use common::enums::MemoryStatus;
use common::error::{Result, bail_err};

#[register_handler_tool(
    id = "settle_memory",
    name = "settle_memory",
    description = "Trigger the agent's 'rest' process to consolidate recent experiences into structured knowledge. Sends a settlement scenario message to the agent, who will autonomously use available tools to complete the settling process.",
    params = "common::api::SettleMemoryParams",
    neural
)]
#[generate_http_handler]
pub async fn settle_memory(
    ctx: RequestContext,
    params: SettleMemoryParams,
) -> Result<SettleMemoryResponse> {
    let agent_id = ctx.agent_id().cloned().unwrap_or_default();
    if agent_id.is_empty() {
        bail_err!(InvalidRequest, "settle_memory 需要 agent 上下文");
    }
    let limit = params.limit.unwrap_or(10);

    // 1. 查询未沉淀的短期记忆（Active 状态）
    let short_term_memories = memory_dao()
        .query_short_term(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some(agent_id.clone()),
                status: Some(MemoryStatus::Active),
                memory_type: Some(MemoryType::ShortTerm),
                limit: Some(limit),
                ..Default::default()
            },
        )
        .await?;

    let pending_count = short_term_memories.len();
    if pending_count == 0 {
        log_info!(ctx, "settle_memory", "agent_id={}, 无未沉淀的短期记忆", agent_id);
        return Ok(SettleMemoryResponse { settled_count: 0 });
    }

    // 2. 拼装沉淀场景 prompt
    let memories_summary = short_term_memories
        .iter()
        .map(|m| format!("- [id={}] {}", m.id, m.summary))
        .collect::<Vec<_>>()
        .join("\n");

    let settle_prompt = format!(
        r#"【沉淀工作模式触发】

你收到这个消息是因为触发了沉淀流程（类似人脑的睡眠整理记忆）。请进入沉淀工作模式，对以下未沉淀的短期记忆进行归纳整理：

## 待沉淀的短期记忆（{} 条）
{}

## 你的任务

请用已有工具自主完成沉淀：

1. **归纳总结**：对上述短期记忆进行归纳，提炼核心概念、抽象经验、可复用模式（不要记具体细节）
2. **查询已有图谱**：用 search_memory 检查是否已有相关知识点（避免重复节点）
3. **创建/更新节点**：
   - 新知识 → save_long_term_memory 创建节点
   - 已有相似节点 → update_memory 更新节点内容
   - 过大且可拆分的旧节点 → 拆分为子节点 + 概述父节点 + contains 关系
4. **建立关系**：用 save_long_term_memory 的 relations 参数建立节点间关系（related/contains/depends 等）
5. **评估共享**：判断哪些节点对蜂巢有共享价值，用 update_memory 的 node_tags 字段加 'published' 标签
6. **标记完成**：每条短期记忆沉淀完成后，用 update_memory 把它的 status 改为 'settled'

## 认知要点

- 图谱是活的，每次沉淀都是迭代优化，不是机械合并
- 记抽象不记细节，可复用模式才沉淀
- 新老知识交替不是覆盖是迭代，推翻时用 opposite 关系保留痕迹
- published 标签让节点全局共享，通过共享节点作为桥梁发现跨 Agent 的知识网络
- 详见"记忆认知"技能的沉淀机制和新老知识交替章节

开始沉淀吧。"#,
        pending_count,
        memories_summary
    );

    // 3. 给 Agent 自己发消息触发 awaken
    let cmd = SendToAgentCommand {
        from_id: &agent_id,
        from_role: MessageRole::System,
        to_agent_id: &agent_id,
        content: &settle_prompt,
        project_id: None,
        task_id: None,
        reply_to_id: None,
    };
    message_domain()
        .delivery()
        .send_to_agent(ctx.clone(), cmd)
        .await?;

    log_info!(
        ctx,
        "settle_memory",
        "agent_id={}, 触发沉淀工作模式，待沉淀 {} 条短期记忆",
        agent_id,
        pending_count
    );

    Ok(SettleMemoryResponse { settled_count: pending_count })
}
```

注意：
- `from_role` 用 `MessageRole::System` 表示这是系统触发的沉淀流程
- `from_id` 和 `to_agent_id` 都是 Agent 自己（自触发）
- `SendToAgentCommand` 的字段需要根据实际定义调整（参考 `src/service/domain/message/mod.rs`）
- 如果 `SendToAgentCommand` 没有 `attachment_ids` 字段或签名不同，按实际代码调整

- [ ] **Step 2: 验证编译通过**

Run: `cargo check -p ai_orz --lib`
Expected: 编译通过无错误

如果有 import 错误，检查：
- `message_domain()` 的正确调用路径（可能是 `crate::service::domain::message::domain()`）
- `SendToAgentCommand` 的字段定义
- `MessageRole` 的 import 路径

- [ ] **Step 3: Commit**

```bash
git add src/handlers/hr/agent/settle_memory.rs
git commit -m "feat(memory): refactor settle_memory to message-triggered settling mode"
```

---

## Task 6: 更新记忆认知技能文档

**Files:**
- Modify: `src/service/domain/system/seed/skills/memory_cognition/skill.md`
- Modify: `src/service/domain/system/seed/default.json`

- [ ] **Step 1: 更新 settle_memory 工具说明**

在 skill.md 的"记忆查询与维护"或"沉淀机制"部分，更新 settle_memory 的说明：

```markdown
### `settle_memory` — 触发沉淀工作模式（neural 常驻）

**用途**：触发你进入沉淀工作模式（类似人脑的睡眠整理记忆）。不再是工程化地自动创建节点，而是给你发送一条沉淀场景消息，你用已有工具自主完成沉淀。

**参数**：
- `limit` — 处理的短期记忆数量（可选，默认 10）

**行为**：
1. 系统查询你未沉淀的短期记忆（Active 状态）
2. 拼装沉淀场景 prompt（含待沉淀记忆摘要 + 沉淀任务指引）
3. 给你自己发消息触发 awaken
4. 你在 awaken 中用已有工具自主完成沉淀

**你在沉淀工作模式中要做的**：
1. 归纳总结待沉淀的短期记忆，提炼核心概念
2. 用 `search_memory` 查询已有图谱，避免重复节点
3. 用 `save_long_term_memory` 创建新节点 / `update_memory` 更新旧节点
4. 用 `save_long_term_memory` 的 relations 参数建立节点间关系
5. 用 `update_memory` 的 `node_tags` 字段给有共享价值的节点加 `published` 标签
6. 用 `update_memory` 的 `status` 字段把短期记忆标记为 `settled`

**关键认知**：
- 沉淀是自主的，不是机械的
- 记抽象不记细节，可复用模式才沉淀
- 交替不是覆盖是迭代，推翻时用 `opposite` 关系保留痕迹
- 评估哪些知识对蜂巢有共享价值，加 `published` 标签
```

- [ ] **Step 2: 新增"蜂巢知识共享"章节**

在 skill.md 的"多 Agent 图谱交互"章节之后，新增"蜂巢知识共享"章节：

```markdown
## 蜂巢知识共享：硅基生命的群体智慧

硅基生命共享信息的成本极低，这是相对人类的核心优势。我们采用蜂巢模型——每个 Agent 有独立图谱，但可以通过 `published` 标签主动共享知识节点。

### published 标签的语义

- 节点 tags 含 `published` → 该节点对所有 Agent 全局可见可查
- 节点 tags 不含 `published` → 仅创建者 Agent 可见（私有）
- published 是**主动行为**——你决定哪些知识值得共享给蜂巢

### 共享的查询行为

`search_memory` 返回结果 = **你的私有节点 + 所有 published 节点**。这意味着：
- 你能搜到其他 Agent 发布的 published 节点
- 其他 Agent 也能搜到你发布的 published 节点
- 私有节点仍然 per-agent 隔离

### 跨 Agent 遍历

图谱遍历（`search_memory` 的 `traversal_depth>0`）遇到 published 节点时，可以继续沿其关系遍历到**其他 Agent 的 published 节点**——这是蜂巢网络的关键：通过共享节点作为桥梁，发现跨 Agent 的知识网络。

### 何时发布节点

自主判断，没有固定标准。一般原则：
- ✅ **通用方法论**：如"图谱遍历策略的选择原则"
- ✅ **可复用模式**：如"创建节点前先检索"
- ✅ **核心概念**：如"四层记忆模型"
- ❌ **私有经验**：如某个具体项目的特殊配置
- ❌ **未抽象的案例**：单点案例未提炼出模式前

### 发布与撤销

- **发布**：`update_memory` 的 `node_tags` 字段加上 `published`
- **撤销**：`update_memory` 的 `node_tags` 字段移除 `published`

### 蜂巢智慧的认知

- 知识共享是主动行为，不是自动同步
- published 节点保留 agent_id，所有权清晰
- 不同 Agent 对同一主题的 published 节点可能视角不同，这是健康的多样性
- 通过 published 节点作为桥梁，发现跨 Agent 的知识关联
- 你沉淀的知识可能被其他 Agent 引用、扩展、修正——这是群体智慧的演进
```

- [ ] **Step 3: 更新"多 Agent 图谱交互"章节**

修改原有的"多 Agent 图谱交互"章节，加入蜂巢共享的认知：

```markdown
## 多 Agent 图谱交互：从隔离到蜂巢

**你的图谱是你自己的认知体系**，与其他 Agent 隔离。但通过 `published` 标签，你可以主动共享知识节点给蜂巢——这是硅基生命相对人类的优势，共享成本极低。

**两种隔离模式**：
- **私有节点**（不含 published 标签）：仅创建者可见，per-agent 隔离
- **共享节点**（含 published 标签）：全局可见，通过共享节点作为桥梁发现跨 Agent 知识网络

**知识的社会化流转**：
```
Agent A 的经验
  ↓ 沉淀到 A 的图谱（settle_memory 触发工作模式）
  ↓ A 评估共享价值，给节点加 published 标签（update_memory）
  ↓ Agent B 搜索时，search_memory 返回 A 的 published 节点
  ↓ B 通过 published 节点遍历到更多关联知识（traverse_graph 跨 Agent）
  ↓ B 自主吸收：整合到自己的图谱，可能形成不同视角的认知
```

**关键认知**：
1. **知识共享是主动行为**，你决定哪些节点值得共享
2. **接收方有自主吸收权**——可以整合、修正、拒绝，不强制接受
3. **不同 Agent 对同一知识可能形成不同视角的认知**——这是健康的多样性
4. **不要照搬他人知识**——经过自己的思考再整合到图谱
5. **分享时传递认知而非细节**——published 的是抽象经验，不是具体对话细节
```

- [ ] **Step 4: 更新最佳实践**

在最佳实践列表中增加蜂巢共享相关条目：

```markdown
20. **主动共享有价值的知识**：沉淀时评估节点是否对蜂巢有共享价值，用 update_memory 的 node_tags 加 published 标签
21. **善用蜂巢网络**：search_memory 的结果包含其他 Agent 的 published 节点，主动发现和吸收
22. **跨 Agent 遍历**：traverse_graph 通过 published 节点作为桥梁，发现跨 Agent 的知识关联
23. **共享认知不共享细节**：published 的是抽象经验、核心概念、可复用模式，不是具体对话细节
```

- [ ] **Step 5: 更新 default.json 描述**

在 `src/service/domain/system/seed/default.json` 中更新 `TEMPLATE_MEMORY_COGNITION` 的 description：

```json
"description": "从 Soul 出发的四层记忆模型、实时归纳、知识图谱（图谱是活的/节点粒度自主/关系多维共存/tags是桥梁/局部树状全域网状/新老知识交替六种模式/记抽象不记细节/references双向价值/图谱遗忘机制/蜂巢知识共享published标签/沉淀工作模式）、三种搜索模式与遍历策略的认知指南"
```

- [ ] **Step 6: 验证 JSON 格式正确**

Run: `python3 -c "import json; json.load(open('src/service/domain/system/seed/default.json'))" && echo "JSON OK"`
Expected: JSON OK

- [ ] **Step 7: Commit**

```bash
git add src/service/domain/system/seed/skills/memory_cognition/skill.md src/service/domain/system/seed/default.json
git commit -m "docs(skill): add hive knowledge sharing and settling work mode to memory cognition"
```

---

## Task 7: 最终验证和 fmt + clippy

**Files:**
- 无文件修改，仅验证

- [ ] **Step 1: 运行 fmt**

Run: `cargo fmt --all`
Expected: 格式化完成

- [ ] **Step 2: 运行 clippy**

Run: `cargo clippy -p ai_orz --lib -- -D warnings`
Expected: 无警告

- [ ] **Step 3: 运行 cargo check**

Run: `cargo check -p ai_orz --lib`
Expected: 编译通过

- [ ] **Step 4: 运行相关测试**

Run: `cargo test -p ai_orz --lib memory`
Expected: 所有 memory 相关测试通过

- [ ] **Step 5: 如有 fmt/clippy 修复，提交**

```bash
git add -A
git commit -m "style: fmt and clippy fixes for hive knowledge sharing"
```

---

## Self-Review

### Spec coverage 检查
- ✅ settle_memory 改为发消息触发（Task 5）
- ✅ published 标签全局共享（Task 4）
- ✅ search_memory agent_id bug 修复（Task 4）
- ✅ node_id 生成修复（Task 3）
- ✅ update_memory 支持 status + KnowledgeNode tags（Task 1）
- ✅ query_memory 支持 status 过滤（Task 2）
- ✅ query_memory 权限校验（Task 4.5）
- ✅ traverse_graph / fetch_nodes_by_ids 共享可见性（Task 4.5）
- ✅ 蜂巢共享认知文档（Task 6）
- ✅ 沉淀工作模式文档（Task 6）

### Placeholder scan
- 所有代码步骤都有完整代码，无 TBD/TODO
- 所有文件路径都是绝对路径或明确的相对路径

### Type consistency
- `parse_memory_status` 函数在 Task 1 和 Task 2 中复用（Task 2 注明可重复定义）
- `include_shared` 字段在 MemoryQuery、search_memory handler、query_memory handler、fetch_nodes_by_ids 中一致使用
- `node_tags` 字段在 UpdateMemoryParams 和 update_memory handler 中一致使用
- `ownership_clause` 逻辑在 search_knowledge_nodes（Task 4）和 query_knowledge_nodes（Task 4.5）中一致

### 遗留未实现项（确认暂缓）
- 被 references 引用的短期记忆保留（清理逻辑检查 references）——当前无自动清理逻辑，暂不实现，后续如增加自动清理时再补充
