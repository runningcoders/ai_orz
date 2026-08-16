> 📦 归档标记（2026-08-15）：被 [Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并](docs/wiki/knowledge/zh/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并/Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: RAG 原子知识卡
name: 四层记忆沉淀：save_short_term 与 save_long_term 工具拆分 + settle 向量去重合并策略
category: 记忆系统
scope:
  - "src/service/dal/memory.rs"
  - "src/service/domain/memory/**"
  - "src/models/memory*.rs"
  - "src/handlers/hr/agent/settle_memory.rs"
  - "src/service/dao/memory/**"
source_files:
  - src/service/dal/memory.rs#L72-L177 (MemoryDal trait：save_short_term_memory / save_long_term_memory / settle_short_term_to_long_term 签名三方法)
  - src/service/dal/memory.rs#L200-L313 (save_short_term / save_long_term 实现：参数极简降级，短期 summary+tags / 长期 content+tags+relations)
  - src/service/dal/memory.rs#L578-L652 (settle_short_term_to_long_term 核心实现：批量拉短期 → LLM 总结 → 向量相似度去重 → 合并关系 + 更新已有节点)
  - src/service/dao/memory/vector.rs#L81-L124 (向量搜索相似节点去重：memory:knowledge_node collection + 阈值 0.75 判定语义重复)
  - src/handlers/hr/agent/settle_memory.rs#L1-L133 (HTTP 神经工具触发 + load_and_settle 公共函数：加载 Agent + 注入 Runtime 上下文 + 调 sleep_and_settle)
  - src/models/memory/short_term_memory_index_po.rs (ShortTermMemoryIndexPo::vectorize_text() 实现 = Vectorizable trait 短期记忆向量化文本来源)
  - src/models/memory/long_term_knowledge_node_po.rs (LongTermKnowledgeNodePo::vectorize_text() 实现 + vector_collection() 返回 "memory:knowledge_node")
  - docs/archive/design-archive/memory_system_enhancement_design.md（§1 决策 1/5：工具拆分策略 + 沉淀冲突处理 = 合并而非新建重复节点）
  - docs/archive/design-archive/memory_search_enhancement_design.md（§5 扩展模式 + §2 三位一体架构含 Vectorizable trait 入口）
  - （占位：待 ai-orz-doc-maintainer 落地后回填真实 Plan 路径 → 预期 docs/archive/plan-archive/记忆系统增强工具拆分与定时沉淀.md）
  - docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/记忆沉淀机制.md（沉淀架构图 + CronTriggerConsumer→DAL 链路）
  - docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/四层记忆系统.md（Core/Working/Short-term/Long-term 四层定位与数据流向）
  - docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/长期记忆 (Long-term Memory)/长期记忆 (Long-term Memory).md（知识节点 + 知识关系 PO 结构与读写）
  - 【平行卡 1】docs/wiki/knowledge/zh/agent_rest 定时休息沉淀：ensure_system_cron_triggers 每天4点 + CronTriggerConsumer 调度 + load_and_settle 链路/agent_rest 定时休息沉淀：ensure_system_cron_triggers 每天4点 + CronTriggerConsumer 调度 + load_and_settle 链路.md（定时触发沉淀的上游调度链路；本卡是沉淀实现细节）
  - 【平行卡 2】docs/wiki/knowledge/zh/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）.md（沉淀写入的知识节点 + 向量索引 = 后续混合搜索的数据基础）
  - 【平行卡 3】docs/wiki/knowledge/zh/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity/向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity.md（save_short/long_term 内都通过 embed_entity 写入向量；去重也通过 VectorStore.search）
---

## §1 概述

**本卡角色**：记忆写入与沉淀链路的领域知识卡，覆盖「工作记忆 → 短期 → 长期」三层写入 API 的接口拆分设计，以及 `settle_short_term_to_long_term` 定时/神经工具触发的「短期消化为长期知识」合并去重策略的核心实现约束。**定位：Agent 开发写记忆工具代码时读 + 调试沉淀重复节点 bug 时读。**

- **写入拆分**：原大而全的 `create_memory` 降级为 HTTP 内部用；Agent 神经工具只暴露 `save_short_term_memory(summary, tags)` 和 `save_long_term_memory(content, tags, relations)` 两个极简签名，参数混淆概率 → 0。
- **沉淀链路**：短期记忆积累后，通过 `settle_short_term_to_long_term(limit=10)` 一次批量拉取 → LLM 调用做「摘要抽取 + 知识归纳」→ 对 LLM 返回的每个候选知识节点，先对 `memory:knowledge_node` 集合按向量距离阈值 0.75 搜索是否已有语义等价节点 → **命中则 update 已有节点 + 合并出入边；未命中则 create 新节点**。
- **硬约束**：沉淀永不硬删除短期记忆；合并后短期记忆 `status = Settled` 软标记，保留追溯链。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| memory.rs (DAL trait) | 记忆业务数据层 | 三方法签名：save_short_term 只写短期索引 PO + 向量；save_long_term 写节点+关系+向量；settle 短期→长期合并 | `:L72-L177` |
| memory.rs (DAL impl) | 沉淀核心实现 | `settle_short_term_to_long_term` 五步：批量拉取(limit) → LLM → embed候选 → 向量搜索去重 → merge/create + 关系合并 | `:L578-L652` |
| memory.rs (DAL impl) | 工具拆分实现 | `save_short_term_memory`：只允许 summary+tags+trace_id 注入；`save_long_term_memory`：content+tags+relations 全参数 | `:L200-L313` |
| vector.rs (DAO) | 向量去重入口 | `VectorStore.search(collection="memory:knowledge_node", embedding, distance_threshold=0.75)` 命中 = 语义重复判定 | `:L81-L124` |
| settle_memory.rs (Handler) | 神经工具触发沉淀 | `load_and_settle(ctx, agent_id, limit)` 公共函数；agent_rest Cron 和 HTTP 工具共用 | `:L1-L133` |
| short_term_memory_index_po.rs | PO + Vectorizable | `vectorize_text()` = 拼接 summary + tags.join(" ") + content_preview | 见 PO 模型 |
| long_term_knowledge_node_po.rs | PO + Vectorizable | `vectorize_text()` = title + content + tags；`vector_collection() = "memory:knowledge_node"` | 见 PO 模型 |

**章节来源**
- [memory.rs:L72-L177](src/service/dal/memory.rs#L72-L177)
- [memory.rs:L578-L652](src/service/dal/memory.rs#L578-L652)
- [vector.rs:L81-L124](src/service/dao/memory/vector.rs#L81-L124)
- [settle_memory.rs:L1-L133](src/handlers/hr/agent/settle_memory.rs#L1-L133)

---

## §3 架构约定与扩展模式

### 3.1 四层流向与工具边界

```
Working Memory（运行时上下文，只在 Runtime 内存中存活）
      │ 每轮思考后：有价值结论 → save_short_term_memory(summary, tags)
      ▼
Short-term Memory（短期索引 PO + 向量 collection="memory:short_term"）
      │ 触发条件：① 神经工具 sleep_and_settle() ② agent_rest Cron 每天 4 点
      ▼ settle_short_term_to_long_term(limit=N)
Long-term Knowledge（知识节点 PO + 知识关系 PO + 向量 collection="memory:knowledge_node"）
      │ 读时：search_memory + traverse_knowledge_graph + recommend_seed_nodes
      ▼
Core Memory（Agent 角色设定 + 能力清单；只在 onboarding 时写入，永不自动沉淀）
```

### 3.2 扩展模式：加一个新的沉淀触发场景
1. **新增触发入口**：在 consumer 或 handler 中复用 `load_and_settle(ctx, agent_id, settle_limit)` 公共函数（不要重写 LLM + merge 代码）。
2. **新增 Cron 动作**：在 `system::ensure_system_cron_triggers` `payload` 新增 action 字符串，然后在 `scheduler.rs` `handle_event` match 块新增分支调用 load_and_settle。
3. **新增 Vectorizable PO**（如新的长期记忆子类型）：① PO 加 `impl Vectorizable`；② DAL 层 `settle_*` 新增分支；③ 向量集合名 `VectorStore.register_collection` 对齐。

---

## §4 硬约束与故障排查

### 4.1 必守红线（回归失败打回）

1. **红线 1**：Agent 神经工具只允许调用 `save_short_term_memory` / `save_long_term_memory`，永不直调 `create_memory` —— 工具白名单由 Runtime Domain `ThinkingOptions.allowed_tool_names` 控制，见 `thinking_task_policy_engine_design.md` §4。
2. **红线 2**：`settle_short_term_to_long_term` 永不做物理 DELETE，短期记忆改 `status = Settled`（软标记）—— 否则审计链断裂。
3. **红线 3**：合并已有节点时，**必须 MERGE 关系**（入边+出边去重），绝不能把旧节点的 relations 丢了 —— 否则图谱遍历断链。
4. **红线 4**：去重向量阈值**硬编码在 DAL 内为 0.75**，不暴露为参数（暴露参数会导致调参时破坏长期知识图谱的一致性）。如果未来要调，**必须连带所有已有节点的相似度重建计划一起提交**。

### 4.2 故障排查路径

| 症状 | 起点锚点 | 次级排查 |
|------|---------|---------|
| 知识图谱出现大量语义重复节点（同一主题 >3 个节点） | [memory.rs:L578-L652](src/service/dal/memory.rs#L578-L652) 检查 `vector_search` 返回是否被过滤 | 确认向量后端是否降级（向量搜索失败 → 全部走 create 分支 → 重复爆炸）；日志 grep `settle_merge` vs `settle_create` 比例 |
| 短期记忆永远 Settled 不完（agent_rest 日志每次都处理同样的 10 条） | 检查短期记忆 PO `status` 字段是否被正确 UPDATE | [memory.rs:L620-L640](src/service/dal/memory.rs#L620-L640) 找到 `status = Settled` UPDATE 是否遗漏 WHERE agent_id 条件 |
| Agent 工具调用说「没有 save_short_term_memory 工具」 | 检查 Runtime `tool_builder().add_tools_for_awake()` 注册处 | 核对 `tool_packages` 表记忆工具包的 `visibility` 是否对当前 Agent 角色可见 |
| 沉淀向量写入慢（单次 settle > 10s，limit=10） | 检查 embed_entity 是否串行调用 cortex（应并发 but 有限流） | [vector.rs:L81-L124](src/service/dao/memory/vector.rs#L81-L124) 或全局 cortex 并发数配置；必要时把 limit 从 10 → 5（Cron 频率从每天 → 每天两次） |
