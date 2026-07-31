# 项目遗留事项 TODO

> 用于记录跨会话的遗留事项，便于统一推进。每项包含：背景、现状、待办、优先级。
> 完成的事项请移到"已完成"章节并标注完成时间。

---

## 待办事项

### 1. 被 references 引用的短期记忆保留机制

**背景**：知识图谱节点通过 references 表关联其来源的短期记忆。references 是 episodic memory（情景记忆）与 semantic memory（语义记忆）之间的双向通道，支持认知回溯——当抽象认知遇到质疑时，可回溯到原始场景重新评估。

**现状**：
- 当前没有自动清理短期记忆的逻辑（只有手动 `delete_memory`）
- 短期记忆被删除后，references 自动解除，图谱节点失去情景痕迹
- 这会导致图谱节点成为"只知道结论不知道怎么来的"孤立认知

**待办**：
- [ ] 增加自动清理机制时（如定期清理 Settled 状态的短期记忆），检查 references 表，被引用的短期记忆跳过清理
- [ ] 或：Settled 时检查是否被 references 引用，被引用的加"重要"标记，不参与清理
- [ ] 在 `delete_memory` handler 中增加警告：删除被 references 引用的短期记忆会丢失情景痕迹

**优先级**：中（当前无自动清理，暂不紧急；增加自动清理时必须同步实现）

**相关文件**：
- `src/models/memory.rs`（KnowledgeReferencePo 定义）
- `src/handlers/hr/agent/delete_memory.rs`
- `src/service/dal/memory.rs`（settle_short_term_to_long_term）

**关联计划**：[2026-07-31-hive-knowledge-sharing.md](superpowers/plans/2026-07-31-hive-knowledge-sharing.md) Self-Review 章节确认暂缓

---

## 已完成事项

### 1. MemoryQuery 的 memory_type 字段滥用为 node_type 过滤

**完成时间**：2026-07-31
**提交**：`0222c0d`

**解决方案**：在 MemoryQuery 中增加独立的 `node_type: Option<String>` 字段，`query_knowledge_nodes` 改用 `node_type` 过滤。原 `memory_type` 字段在 `query_knowledge_nodes` 中的使用是 dead code（`MemoryType::KnowledgeNode.to_string()` = "KnowledgeNode"，永远不会等于 node_type 字段的实际值 "summary"/"concept"/"fact"/"procedure"），移除后不影响任何现有功能。

---

### 2. idx_ltkn_tags 索引对 json_each 过滤无加速作用

**完成时间**：2026-07-31
**提交**：`0488c57`

**解决方案**：采用方案 B（is_published 字段）。新增 migration `20260731000001_knowledge_node_is_published.sql`，在 `long_term_knowledge_node` 表增加 `is_published INTEGER NOT NULL DEFAULT 0` 字段 + 部分索引（`WHERE is_published = 1`）+ 从 tags 回填。search/query 的 ownership_clause 从 `json_each(tags)` 改为 `is_published = 1`，走 B-tree 索引。update_memory 和 save_long_term_memory 在更新/创建节点时同步 is_published 字段。

---

### 3. settle 作为与 awaken 对应的沉睡方法

**完成时间**：2026-07-31
**提交**：`977a04a`

**解决方案**：在 `RuntimeAwakening` trait 新增 `sleep_and_settle` 方法（与 `awaken` 对称），语义为"沉睡整理内部记忆"。移除 `RuntimeDomain::rest_and_settle` 和 `RuntimeMemory::settle`。settle_memory handler 和 CronTrigger 不再通过消息层触发 awaken，而是直接调用 `sleep_and_settle`。提取 `build_settle_prompt` 和 `load_and_settle` 公共函数供 handler 和 scheduler 复用。

---

## 维护说明

- 新增遗留事项时，按"背景 → 现状 → 待办 → 优先级 → 相关文件"格式记录
- 完成的事项移到"已完成事项"章节，标注完成时间和提交 hash
- 定期 review，清理过期或已不相关的事项
- 与 plans/ 目录的区别：plans 是有完整实施计划的事项，todo 是尚未规划具体实施步骤的遗留点
