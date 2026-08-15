# 代码异味修复 + sleep_and_settle 架构对齐 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 解决 3 个问题：(1) MemoryQuery 的 memory_type 滥用为 node_type 过滤（dead code bug）；(2) idx_ltkn_tags 索引对 json_each 无加速（is_published 方案）；(3) settle 作为与 awaken 对应的沉睡方法，与消息层解耦。

**Architecture:**
1. MemoryQuery 增加 `node_type: Option<String>` 字段，query_knowledge_nodes 改用 node_type 过滤，移除对 memory_type 的错误使用
2. long_term_knowledge_node 表增加 `is_published: BOOL` 字段 + 索引；update_memory 加/移除 published 标签时同步更新 is_published；search/query 改用 is_published 过滤
3. RuntimeAwakening trait 新增 `sleep_and_settle` 方法（与 awaken 对称）；移除 RuntimeDomain 的 `rest_and_settle`；settle_memory handler 和 CronTrigger 改为调用 `sleep_and_settle`

**Tech Stack:** Rust + axum + sqlx + SQLite + migration

---

## File Structure

| 文件 | 责任 | 改动类型 |
|------|------|---------|
| `migrations/20260731000001_knowledge_node_is_published.sql` | 新增 is_published 字段 + 索引 | 新建 |
| `src/service/dao/memory/mod.rs` | MemoryQuery 增加 node_type 字段 | 修改 |
| `src/service/dao/memory/sqlite.rs` | query_knowledge_nodes 改用 node_type；search/query 改用 is_published | 修改 |
| `src/models/memory.rs` | LongTermKnowledgeNodePo 增加 is_published 字段 | 修改 |
| `src/handlers/hr/agent/update_memory.rs` | 加/移除 published 标签时同步 is_published | 修改 |
| `src/handlers/hr/agent/save_long_term_memory.rs` | 创建节点时根据 tags 设置 is_published | 修改 |
| `src/service/dal/memory.rs` | fetch_nodes_by_ids 改用 is_published 过滤 | 修改 |
| `src/service/domain/runtime/mod.rs` | 移除 rest_and_settle | 修改 |
| `src/service/domain/runtime/awakening.rs` | 新增 sleep_and_settle 方法 | 修改 |
| `src/handlers/hr/agent/settle_memory.rs` | 改为加载 Agent + 调用 sleep_and_settle | 修改 |
| `src/consumer/scheduler.rs` | CronTrigger agent_rest 改为调用 sleep_and_settle | 修改 |
| `src/service/domain/system/seed/skills/memory_cognition/skill.md` | 更新 settle_memory 说明 | 修改 |
| `docs/todo.md` | 更新已完成事项 | 修改 |

---

## Task 1: MemoryQuery 增加 node_type 字段，修复 dead code bug

**Files:**
- Modify: `src/service/dao/memory/mod.rs`（MemoryQuery 结构体）
- Modify: `src/service/dao/memory/sqlite.rs`（query_knowledge_nodes）

- [ ] **Step 1: MemoryQuery 增加 node_type 字段**

在 `src/service/dao/memory/mod.rs` 的 `MemoryQuery` 结构体中增加 `node_type` 字段：

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
    pub include_shared: bool,
    /// 新增：按知识节点类型过滤（summary/concept/fact/procedure）
    /// 注意：与 memory_type 不同——memory_type 是记忆大类型，node_type 是知识节点的子类型
    pub node_type: Option<String>,
}
```

- [ ] **Step 2: query_knowledge_nodes 改用 node_type 过滤**

在 `src/service/dao/memory/sqlite.rs` 的 `query_knowledge_nodes` 方法中，将 memory_type 过滤替换为 node_type：

原代码（约第 736-739 行）：
```rust
if let Some(memory_type) = &query.memory_type {
    builder.push(" AND node_type = ");
    builder.push_bind(memory_type.to_string());
}
```

改为：
```rust
if let Some(node_type) = &query.node_type {
    builder.push(" AND node_type = ");
    builder.push_bind(node_type);
}
```

- [ ] **Step 3: 检查 query_knowledge_nodes 的调用方**

用 Grep 搜索所有调用 `query_knowledge_nodes` 的地方，确认是否有调用方依赖原来的 memory_type 过滤行为。由于原行为是 dead code（永远不会命中），不会有调用方依赖它。

- [ ] **Step 4: 验证编译通过**

Run: `cargo check -p ai_orz --lib`
Expected: 编译通过无错误

- [ ] **Step 5: Commit**

```bash
git add src/service/dao/memory/mod.rs src/service/dao/memory/sqlite.rs
git commit -m "fix(memory): add node_type field to MemoryQuery, fix dead code memory_type misuse"
```

---

## Task 2: 新增 is_published 字段 + 索引 + 同步逻辑

**Files:**
- Create: `migrations/20260731000001_knowledge_node_is_published.sql`
- Modify: `src/models/memory.rs`（LongTermKnowledgeNodePo）
- Modify: `src/service/dao/memory/sqlite.rs`（CRUD + search/query 改用 is_published）
- Modify: `src/handlers/hr/agent/update_memory.rs`（同步 is_published）
- Modify: `src/handlers/hr/agent/save_long_term_memory.rs`（创建时设置 is_published）

- [ ] **Step 1: 创建 migration 文件**

创建 `migrations/20260731000001_knowledge_node_is_published.sql`：

```sql
-- 新增 is_published 字段，用于加速 published 标签查询
-- 原有的 json_each(tags) 查询走全表扫描，is_published 字段可走 B-tree 索引
ALTER TABLE long_term_knowledge_node ADD COLUMN is_published INTEGER NOT NULL DEFAULT 0;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_ltkn_is_published ON long_term_knowledge_node(is_published) WHERE is_published = 1;

-- 回填已有数据：从 tags JSON 数组中提取 published 标签
UPDATE long_term_knowledge_node
SET is_published = 1
WHERE EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value = 'published');
```

- [ ] **Step 2: LongTermKnowledgeNodePo 增加 is_published 字段**

在 `src/models/memory.rs` 的 `LongTermKnowledgeNodePo` 结构体中增加字段：

```rust
pub struct LongTermKnowledgeNodePo {
    // ... 现有字段 ...
    /// 是否已发布到蜂巢（tags 含 "published" 时为 true）
    /// 冗余字段，与 tags 中的 "published" 标签同步，用于加速查询
    pub is_published: bool,
}
```

注意 sqlx 的 `#[sqlx(flatten)]` 或字段映射需要适配。确认 sqlx 的 from_row 是否能正确映射 INTEGER 到 bool（SQLite 中 0/1 → false/true）。

- [ ] **Step 3: 修改 SQLite DAO 的 CRUD 操作支持 is_published**

在 `src/service/dao/memory/sqlite.rs` 中：

1. **create_knowledge_node / insert**：INSERT 语句增加 is_published 字段，根据 tags 是否包含 "published" 计算
2. **update_knowledge_node**：UPDATE 语句增加 is_published 字段
3. **query_knowledge_nodes 的 ownership_clause**：将 `EXISTS (SELECT 1 FROM json_each(tags) WHERE json_each.value = 'published')` 改为 `is_published = 1`
4. **search_knowledge_nodes 的 ownership_clause**：同上

新增辅助函数：
```rust
fn tags_has_published(tags: &str) -> bool {
    tags.contains("\"published\"")
}
```

- [ ] **Step 4: 修改 update_memory handler 同步 is_published**

在 `src/handlers/hr/agent/update_memory.rs` 的 KnowledgeNode 分支中，当更新 node_tags 时同步更新 is_published：

```rust
MemoryPo::KnowledgeNode(mut po) => {
    if let Some(content) = params.content {
        po.node_description = content;
    }
    if let Some(summary) = params.summary {
        po.summary = summary;
    }
    if let Some(node_tags) = params.node_tags {
        // 同步 is_published 字段
        po.is_published = node_tags.iter().any(|t| t == "published");
        po.tags = serde_json::to_string(&node_tags)?;
    }
    if let Some(status_str) = params.status {
        po.status = parse_memory_status(&status_str);
    }
    // ...
}
```

- [ ] **Step 5: 修改 save_long_term_memory handler 创建时设置 is_published**

在 `src/handlers/hr/agent/save_long_term_memory.rs` 中，创建 KnowledgeNode 时根据 tags 设置 is_published：

```rust
let is_published = params.tags.as_ref()
    .map(|tags| tags.iter().any(|t| t == "published"))
    .unwrap_or(false);

let node = LongTermKnowledgeNodePo {
    // ... 现有字段 ...
    is_published,
};
```

- [ ] **Step 6: 修改 fetch_nodes_by_ids 改用 is_published 过滤**

在 `src/service/dal/memory.rs` 的 `fetch_nodes_by_ids` 中，将 `n.tags.contains("\"published\"")` 改为 `n.is_published`：

```rust
let visible_nodes: Vec<_> = nodes
    .into_iter()
    .filter(|n| n.agent_id == agent_id || n.is_published)
    .collect();
```

- [ ] **Step 7: 验证编译通过**

Run: `cargo check -p ai_orz --lib`
Expected: 编译通过无错误

- [ ] **Step 8: 运行 memory 测试**

Run: `cargo test -p ai_orz --lib memory`
Expected: 所有测试通过（可能需要更新测试中的 Po 构造，增加 is_published 字段）

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat(memory): add is_published field with index for fast published node queries"
```

---

## Task 3: sleep_and_settle 架构对齐

**Files:**
- Modify: `src/service/domain/runtime/mod.rs`（移除 rest_and_settle）
- Modify: `src/service/domain/runtime/awakening.rs`（新增 sleep_and_settle）
- Modify: `src/handlers/hr/agent/settle_memory.rs`（改为加载 Agent + 调用 sleep_and_settle）
- Modify: `src/consumer/scheduler.rs`（CronTrigger 改用 sleep_and_settle）

- [ ] **Step 1: 在 RuntimeAwakening trait 新增 sleep_and_settle 方法**

在 `src/service/domain/runtime/awakening.rs` 的 `RuntimeAwakening` trait 定义中新增方法：

```rust
#[async_trait]
pub trait RuntimeAwakening: Send + Sync {
    /// 装配 Agent 的 Brain
    async fn wake_agent_brain(&self, ctx: RequestContext, agent: &mut Agent) -> Result<RequestContext>;

    /// 唤醒 Agent 并执行一次思考（响应外部消息）
    async fn awaken(&self, ctx: RequestContext, agent: &Agent, message: &Message) -> Result<AwakeningResult>;

    /// 让 Agent 进入沉睡模式，执行记忆沉淀（与 awaken 对称）
    ///
    /// awaken 是醒来响应外部消息，sleep_and_settle 是沉睡整理内部记忆。
    /// 流程：set_resting → 装配 Brain → 拼装沉淀 Prompt → think → 写 Trace → set_idle
    ///
    /// # 参数
    /// - ctx: 请求上下文（需含 agent_id）
    /// - agent: 已加载的 Agent（含 tools + skills）
    /// - settle_prompt: 沉淀场景 prompt（由调用方拼装，含待沉淀记忆摘要 + 任务指引）
    async fn sleep_and_settle(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        settle_prompt: &str,
    ) -> Result<AwakeningResult>;
}
```

- [ ] **Step 2: 实现 sleep_and_settle**

在 `RuntimeAwakeningImpl` 中实现 `sleep_and_settle`，参考 `awaken` 的逻辑但有关键差异：

```rust
async fn sleep_and_settle(
    &self,
    ctx: RequestContext,
    agent: &Agent,
    settle_prompt: &str,
) -> Result<AwakeningResult> {
    let start_time = std::time::SystemTime::now();

    // 使用 Resting 状态（而非 Busy），RAII guard 恢复 Idle
    AgentRuntimeStateManager::global().set_resting(&agent.po.id);
    let _rest_guard = RestGuard::new(agent.po.id.clone());

    // 补充 Agent 上下文到 ctx
    let ctx = enrich_ctx!(&ctx, agent);

    // Step 1: 读取最近短期记忆作为 history
    let recent_memories = self
        .memory()
        .get_recent_context(ctx.clone(), &agent.po.id, 20)
        .await?;

    // Step 2: 构造 MemoryTrace
    use common::enums::MemoryRole;
    let mut trace = MemoryTrace::new(
        agent.po.id.clone(),
        ctx.log_id.clone(),
        ctx.uid(),
        ctx.organization_id.clone().unwrap_or_default(),
        MemoryRole::System,
        String::new(),
        ctx.task_id().cloned(),
    );
    let trace_id = trace.id.clone();

    // Step 3: 加载工具和技能
    let all_tools: Vec<crate::models::tool::ToolPo> =
        agent.tools().iter().map(|t| t.po.clone()).collect();
    let skill_pos: Vec<crate::models::skill::SkillPo> =
        agent.skills().iter().map(|s| s.po.clone()).collect();

    // Step 4: 拼装 Prompt
    // 与 awaken 的区别：current_message 用沉淀场景 prompt 替代
    // 构造一个虚拟的 Message（沉淀场景）
    let settle_message = Message::from_system(&agent.po.id, settle_prompt);

    let mut builder = self.prompt_builder(agent);
    builder.current_trace_id(&trace_id);
    builder.system_prompt(agent);
    builder.tools(&all_tools);
    builder.skills(&skill_pos);
    builder.history(&recent_memories);
    builder.current_message(&settle_message);

    let prompt = builder.build();

    // Step 5: 调用大脑思考
    let brain = agent
        .brain
        .as_ref()
        .ok_or_else(|| err!(Internal, "Agent 大脑未唤醒，请先调用 wake_brain()"))?;

    const THINK_TIMEOUT_SECS: u64 = 300;
    let think_result = match tokio::time::timeout(
        std::time::Duration::from_secs(THINK_TIMEOUT_SECS),
        self.brain_dal().think(ctx.clone(), brain, &prompt),
    )
    .await
    {
        Ok(result) => result,
        Err(_elapsed) => Err(err!(Internal, "brain think timeout after {}s", THINK_TIMEOUT_SECS)),
    };

    let raw_output = match think_result {
        Ok(output) => output,
        Err(e) => {
            let duration_ms = start_time
                .elapsed()
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            if let Err(stats_err) = record_event!(
                ctx,
                AgentAwakeEvent {
                    agent_id: agent.po.id.clone(),
                    project_id: None,
                    task_id: None,
                    organization_id: ctx.organization_id.clone(),
                    user_id: Some(ctx.uid()),
                    message_id: None,
                    call_count: 1,
                    duration_ms,
                    status: format!("settle failed: {}", e),
                }
            ) {
                log_warn!(&ctx, "sleep_and_settle", "record_event failed: {:?}", stats_err);
            }
            return Err(e);
        }
    };

    // Step 6: 写入 Trace
    trace.input = prompt.clone();
    trace.complete(raw_output.clone());
    self.memory().write_thinking_trace(ctx.clone(), trace).await?;

    // Step 7: 记录统计事件
    let duration_ms = start_time
        .elapsed()
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    if let Err(stats_err) = record_event!(
        ctx,
        AgentAwakeEvent {
            agent_id: agent.po.id.clone(),
            project_id: None,
            task_id: None,
            organization_id: ctx.organization_id.clone(),
            user_id: Some(ctx.uid()),
            message_id: None,
            call_count: 1,
            duration_ms,
            status: "settle success".to_string(),
        }
    ) {
        log_warn!(&ctx, "sleep_and_settle", "record_event failed: {:?}", stats_err);
    }

    Ok(AwakeningResult {
        agent_id: agent.po.id.clone(),
        trace_ids: vec![trace_id],
        raw_input: prompt,
        raw_output,
    })
}
```

注意：
- `RestGuard` 需要新建（类似 BusyGuard 但 set_idle 恢复）。检查是否已有 RestGuard，如果没有，在 awakening.rs 或 busy_guard.rs 旁边新建
- `Message::from_system` 需要确认是否存在，如果不存在需要构造一个简单的系统消息。可能需要用 `MessagePo` 直接构造
- `builder.current_message` 接收 `&Message`，需要确认 Message 的构造方式

- [ ] **Step 3: 创建 RestGuard（如果不存在）**

检查 `src/service/domain/runtime/busy_guard.rs` 是否有 RestGuard。如果没有，创建一个类似的：

```rust
/// RAII guard for Resting state, ensures set_idle is called when dropped
pub struct RestGuard {
    agent_id: String,
}

impl RestGuard {
    pub fn new(agent_id: String) -> Self {
        Self { agent_id }
    }
}

impl Drop for RestGuard {
    fn drop(&mut self) {
        crate::pkg::agent_runtime_state::AgentRuntimeStateManager::global()
            .set_idle(&self.agent_id);
    }
}
```

- [ ] **Step 4: 移除 RuntimeDomain 的 rest_and_settle**

在 `src/service/domain/runtime/mod.rs` 中：
1. 从 `RuntimeDomain` trait 中移除 `rest_and_settle` 方法定义
2. 从 `RuntimeDomainImpl` 中移除 `rest_and_settle` 实现
3. 移除 `RuntimeMemory::settle` 方法（如果只有 rest_and_settle 调用它）

注意：检查是否有其他地方调用 `rest_and_settle` 或 `memory().settle()`，如果有需要一并迁移。

- [ ] **Step 5: 改造 settle_memory handler**

将 `src/handlers/hr/agent/settle_memory.rs` 改为：
1. 查询未沉淀短期记忆
2. 拼装沉淀场景 prompt
3. 加载 Agent（通过 hr_domain.get_agent，with_tools=true, with_skills=true）
4. 唤醒 Brain（wake_agent_brain）
5. 调用 `runtime_domain().awakening().sleep_and_settle(ctx, &agent, &prompt)`
6. 返回

```rust
pub async fn settle_memory(
    ctx: RequestContext,
    params: SettleMemoryParams,
) -> Result<SettleMemoryResponse> {
    let agent_id = ctx.agent_id().cloned().unwrap_or_default();
    if agent_id.is_empty() {
        bail_err!(InvalidRequest, "settle_memory 需要 agent 上下文");
    }
    let limit = params.limit.unwrap_or(10);

    // 1. 查询未沉淀的短期记忆
    let short_term_memories = memory_dao()
        .query_short_term(ctx.clone(), MemoryQuery {
            agent_id: Some(agent_id.clone()),
            status: Some(MemoryStatus::Active),
            memory_type: Some(MemoryType::ShortTerm),
            limit: Some(limit),
            ..Default::default()
        })
        .await?;

    let pending_count = short_term_memories.len();
    if pending_count == 0 {
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
... (复用 Task 5 中的沉淀场景 prompt 模板) ...
"#,
        pending_count, memories_summary
    );

    // 3. 加载 Agent（含 tools + skills）
    let hr_domain = crate::service::domain::hr::domain();
    let fetch_options = AgentFetchOptions::new().with_tools(true).with_skills(true);
    let mut agent = hr_domain.agent_manage().get_agent(ctx.clone(), &agent_id, fetch_options).await?;

    // 4. 唤醒 Brain
    let ctx = runtime_domain().awakening().wake_agent_brain(ctx, &mut agent).await?;

    // 5. 沉睡沉淀
    let result = runtime_domain()
        .awakening()
        .sleep_and_settle(ctx.clone(), &agent, &settle_prompt)
        .await?;

    log_info!(ctx, "settle_memory", "agent_id={}, 沉淀完成，处理 {} 条短期记忆", agent_id, pending_count);

    Ok(SettleMemoryResponse { settled_count: pending_count })
}
```

注意：
- 确认 `hr_domain` 的正确调用路径
- 确认 `AgentFetchOptions` 的 import 路径
- 确认 `get_agent` 的签名

- [ ] **Step 6: 修改 CronTrigger agent_rest**

在 `src/consumer/scheduler.rs` 中，将 `agent_rest` action 从调用 `rest_and_settle` 改为加载 Agent + 调用 `sleep_and_settle`：

```rust
// 原代码：self.runtime_domain.rest_and_settle(ctx, &payload.agent_id, settle_limit).await?;
// 改为：
let hr_domain = crate::service::domain::hr::domain();
let fetch_options = AgentFetchOptions::new().with_tools(true).with_skills(true);
let mut agent = hr_domain.agent_manage().get_agent(ctx.clone(), &payload.agent_id, fetch_options).await?;
let ctx = self.runtime_domain.awakening().wake_agent_brain(ctx, &mut agent).await?;

// 查询未沉淀短期记忆 + 拼装 prompt（复用 settle_memory 的逻辑）
// ... 或提取为公共函数 ...

let result = self.runtime_domain.awakening().sleep_and_settle(ctx, &agent, &settle_prompt).await?;
```

注意：为避免重复，可以将查询短期记忆 + 拼装 prompt 的逻辑提取为公共函数（如 `build_settle_prompt`）。

- [ ] **Step 7: 验证编译通过**

Run: `cargo check -p ai_orz --lib`
Expected: 编译通过无错误

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(runtime): add sleep_and_settle as symmetric counterpart to awaken, decouple from message layer"
```

---

## Task 4: 更新技能文档和 todo.md

**Files:**
- Modify: `src/service/domain/system/seed/skills/memory_cognition/skill.md`
- Modify: `docs/todo.md`

- [ ] **Step 1: 更新 settle_memory 工具说明**

在 skill.md 中更新 settle_memory 的说明，反映架构变化：
- 不再是"发消息触发 awaken"，而是"直接调用 sleep_and_settle 沉睡方法"
- 与 awaken 对称的语义说明

- [ ] **Step 2: 更新 todo.md**

将 3 个已完成事项移到"已完成事项"章节：
1. memory_type 滥用为 node_type 过滤 → 已修复，增加独立 node_type 字段
2. idx_ltkn_tags 索引对 json_each 无加速 → 已解决，新增 is_published 字段 + 索引
3. settle 作为与 awaken 对应的沉睡方法 → 已实现 sleep_and_settle

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "docs: update settle_memory skill doc and mark todo items as completed"
```

---

## Task 5: 最终验证

- [ ] **Step 1: cargo fmt --all**
- [ ] **Step 2: cargo clippy -p ai_orz --lib -- -D warnings**
- [ ] **Step 3: cargo test -p ai_orz --lib memory**
- [ ] **Step 4: 如有修复，提交**

---

## Self-Review

### Spec coverage 检查
- ✅ 代码异味 1：MemoryQuery 增加 node_type 字段，移除 memory_type 滥用（Task 1）
- ✅ 代码异味 2：is_published 字段 + 索引 + 同步逻辑（Task 2）
- ✅ sleep_and_settle 架构对齐（Task 3）
- ✅ 文档更新（Task 4）
- ✅ 最终验证（Task 5）

### 关键风险点
1. **is_published 同步一致性**：update_memory 的 node_tags 更新、save_long_term_memory 的创建都必须同步更新 is_published。如果有遗漏的入口，会导致 is_published 与 tags 不一致
2. **sleep_and_settle 的 Brain 装配**：需要确保 Brain 已装配（wake_agent_brain），否则 think 会失败。settle_memory handler 和 CronTrigger 都需要调用 wake_agent_brain
3. **RestGuard 的 RAII**：确保 set_resting 后一定有 set_idle 恢复，避免 Agent 永远 Resting
4. **Message 构造**：sleep_and_settle 需要构造虚拟 Message 传给 builder.current_message，需要确认 Message 的构造方式
5. **CronTrigger 的 prompt 拼装**：为避免与 settle_memory handler 重复，建议提取公共函数
