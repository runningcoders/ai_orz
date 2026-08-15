# 记忆系统增强（工具拆分 + 定时触发器 + 休息沉淀）设计

> 🎯 **本文档定位**：记忆系统三阶段增强的设计决策大纲（为什么拆工具、为什么引入 Cron 触发器、休息与人类认知机制的对齐思路；实现细节读代码）
>
> 状态：v2 部分完成（阶段一已落地，阶段二/三进行中，2026-08-15）
>
> 查阅场景：理解记忆写入接口拆分设计、定时触发器架构、记忆自动沉淀机制、新增系统能力类 Domain 时打开。
>
> 关联文档：
> - [memory_design.md](../memory_design.md) — 记忆四层认知模型基础
> - [memory_search_enhancement_design.md](./memory_search_enhancement_design.md) — 搜索增强设计（阶段一配套）
> - [consumer_architecture.md](./consumer_architecture.md) — 事件消费架构（阶段二/三复用）
> - [runtime_design.md](./runtime_design.md) — Agent 状态机（Resting 状态 + BusyGuard 防沉淀并发冲突）
> - 【② Plan 落地快照】
>   - [图谱遍历查询优化.md](../plan/图谱遍历查询优化.md) — 阶段二 traverse 性能优化真实定稿
>   - [知识图谱推荐起点与组件复用重构.md](../plan/知识图谱推荐起点与组件复用重构.md) — 阶段二 recommend_seed_nodes + 前端组件复用真实定稿
> - 【② Plan 落地快照（Batch11 精确新增）】
>   - [唤醒上下文与睡眠约束.md](docs/plan/唤醒上下文与睡眠约束.md) — sleep_and_settle 沉淀约束 + ThinkingOptions 统一参数 + 双层工具过滤
> - 【③ Wiki 长文 ≥4 篇（Batch11 精确对齐）】
>   - [记忆沉淀机制.md](docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/记忆沉淀机制.md) — 沉淀调度完整链路 + CronTrigger 生产者/消费者 + Resting 状态机
>   - [四层记忆系统.md](docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/四层记忆系统.md) — 四层认知模型总入口（Core/Working/Short/Long 边界 + agent_rest 定时沉淀章节）
>   - [Agent 记忆系统.md](docs/wiki/zh/content/项目概述/核心功能特性/Agent%20全生命周期管理/Agent%20记忆系统.md) — Agent 视角下的记忆生命周期 + 入职/休息/沉淀环节联动
>   - [工作记忆 (Working Memory).md](docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/工作记忆%20(Working%20Memory).md) — 小憩时清 Working 上下文边界 + 上下文过载触发小憩
> - 【④ RAG 原子知识卡（Batch11 精确对应 1 张 + 横向关联 3 张）】
>   - [Memory 系统增强与休息沉淀：四层记忆（Core／Working／Short／Long）+ agent_rest 每天 4 点 settle + load_and_settle 向量去重合并](docs/wiki/knowledge/zh/Memory%20系统增强与休息沉淀：四层记忆（Core%2FWorking%2FShort%2FLong）+%20agent_rest%20每天%204%20点%20settle%20+%20load_and_settle%20向量去重合并/Memory%20系统增强与休息沉淀：四层记忆（Core%2FWorking%2FShort%2FLong）+%20agent_rest%20每天%204%20点%20settle%20+%20load_and_settle%20向量去重合并.md) — 三合一总卡（工具拆分 + Cron 定时框架 + 沉淀冲突合并 0.78 阈值 + 双层工具过滤）
>   - [记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐](docs/wiki/knowledge/zh/记忆搜索增强三合一：FTS5%20tags%20语义过滤%20+%20图谱%20traverse%20BFS%2FDFS%20遍历%20+%20recommend_seed_nodes%20三因子推荐/记忆搜索增强三合一：FTS5%20tags%20语义过滤%20+%20图谱%20traverse%20BFS%2FDFS%20遍历%20+%20recommend_seed_nodes%20三因子推荐.md) — 阶段二搜索能力（记忆沉淀依赖的相似节点向量搜索底座）
>   - [Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联](docs/wiki/knowledge/zh/Intent%20感知两阶段唤醒：IntentAnalyze%20Phase1%20七字段意图分析%20+%206%20级%20JSON%20降级兜底%20+%20Awaken%20Phase2%20正式执行串联/Intent%20感知两阶段唤醒：IntentAnalyze%20Phase1%20七字段意图分析%20+%206%20级%20JSON%20降级兜底%20+%20Awaken%20Phase2%20正式执行串联.md) — 复用同一 run_think_loop 引擎（Settle 场景共用）

---

## 一、设计目标与关键决策

### 问题背景

记忆模块四层认知模型（Core/Working/Short-term/Long-term）已就绪，但有三个核心缺口：

| 缺口 | 影响 |
|-----|------|
| 写入接口宽泛 | `create_memory` 单接口既写短期又写长期还带关系，Agent 用错参数概率高 |
| 读无图谱遍历 | `search_memory` 仅支持语义搜索，无法沿知识图谱关系「链式联想」 |
| 缺自动沉淀 | 短期记忆不自动消化为长期知识，随时间滚雪球积累无效数据 |

### 关键决策表

| # | 决策问题 | 选择方案 | 选择原因 |
|---|---------|---------|---------|
| 1 | 写入接口拆分策略 | **save_short_term_memory / save_long_term_memory 两个专用神经工具，create_memory 降级为 HTTP 内部用** | 专用接口参数极简（短期只需 summary+tags），Agent 不需要理解 memory_type；原接口保留供复杂 HTTP 场景 |
| 2 | 图谱遍历实现位置 | **DAL 层 `traverse_knowledge_graph()`，DAO 层补 `list_relations_batch`** | 遍历是业务语义（BFS/DFS 策略选择），不是原子 SQL；BFS/DFS 循环逻辑放 DAL，DAO 只暴露批量查关系 |
| 3 | 定时触发器建设方式 | **新建 SystemDomain + CronManager 子能力；CronScheduler 后台扫描器每分钟扫** | 触发器是通用系统能力（不止沉淀用，未来报表/备份都用），独立成 Domain 避免散落到 Runtime 或 HR |
| 4 | 休息触发双轨 | **上下文过载触发（每 N 轮短暂休息）+ 每日定时触发（长时间睡眠沉淀）** | 对齐人类：工作累了小憩（清上下文）+ 每晚睡觉（整理记忆） |
| 5 | 知识沉淀的冲突处理 | **向量搜索相似节点 → 更新已有节点并合并关系，而非新建重复节点** | 避免知识图谱随时间膨胀；语义相同的内容收敛到同一节点 |
| 6 | 沉淀的 LLM 调用位置 | **Runtime Domain `rest_and_digest()` 内（通过 Cortex 封装）** | 沉淀是 Agent「潜意识」行为，属于 Runtime 职责；不放在 consumer 或 cron scheduler |

---

## 二、架构思路

三阶段对齐人类认知工作流：

```
阶段一（已落地）：工具接口拆分 + 搜索增强
┌──────────────────────────────────────────────────────┐
│  Agent 工作时（Thinking/Tool Use 循环）                │
│    ├─ save_short_term_memory  ──► 短期记忆写入（极简）│
│    ├─ save_long_term_memory   ──► 知识节点+关系写入   │
│    └─ search_memory (+遍历)   ──► 语义 + 链式联想    │
└──────────────────────────────────────────────────────┘
                              │
                              ▼
阶段二（进行中）：SystemDomain 定时触发器框架
┌──────────────────────────────────────────────────────┐
│ CronScheduler（后台线程，每分钟扫）                    │
│    │ list_due() 发现到期触发器                         │
│    ▼                                                  │
│ 消息系统投递 TriggerFired 事件                         │
│    │                                                  │
│    ▼                                                  │
│ Consumer 层 scheduler 消费者 → 路由到对应 Domain 方法 │
│    └─ 例：Agent 睡眠触发器 → Runtime.rest_and_digest()│
└──────────────────────────────────────────────────────┘
                              │
                              ▼
阶段三（进行中）：休息沉淀机制
┌──────────────────────────────────────────────────────┐
│ Runtime Domain 状态机：Idle → Thinking → Resting      │
│    │                                                  │
│    ├─ Resting(短暂)：上下文过载触发，清 Working 内存   │
│    └─ Resting(睡眠)：每日定时，执行沉淀流程             │
│           │                                           │
│           ▼                                           │
│        取近期短期记忆 → LLM 总结归纳 → 提取节点+关系   │
│           │                                           │
│           ▼                                           │
│        向量搜索相似节点（冲突检测）                    │
│           │   命中 → 更新已有节点 + 合并关系           │
│           │   未命中 → 新建节点 + 关系                 │
│           ▼                                           │
│        写入长期知识图谱 → 状态恢复 Idle                │
└──────────────────────────────────────────────────────┘
```

---

## 三、涉及文件清单

### 阶段一（已落地）文件

| 文件 | 角色 | 变更摘要 |
|------|------|---------|
| **神经工具 Handler** | | |
| [src/handlers/hr/agent/save_short_term_memory.rs](../../src/handlers/hr/agent/save_short_term_memory.rs) | 短期记忆写入 | 注册 neural 工具；参数极简（summary/tags/task_id）；内部构造 ShortTermMemoryIndexPo |
| [src/handlers/hr/agent/save_long_term_memory.rs](../../src/handlers/hr/agent/save_long_term_memory.rs) | 长期记忆写入 | 支持节点 + relations 一并创建；不存在的 target 跳过 warn |
| [src/handlers/hr/agent/create_memory.rs](../../src/handlers/hr/agent/create_memory.rs) | 降级内部用 | 移除 neural flag；保留 HTTP handler |
| [src/handlers/hr/agent/search_memory.rs](../../src/handlers/hr/agent/search_memory.rs) | 搜索增强 | 新增 traversal 参数解析 + seed_node_ids 分步搜索支持 |
| **DAO/DAL** | | |
| [src/service/dao/memory/mod.rs](../../src/service/dao/memory/mod.rs) | MemoryDao | 新增 `list_relations_batch(node_ids)` |
| [src/service/dal/memory.rs](../../src/service/dal/memory.rs) | MemoryDal | 新增 `traverse_knowledge_graph(seeds, depth, strategy)`，BFS/DFS 两种策略 |
| **DTO** | | |
| [common/src/api/neural_tools.rs](../../common/src/api/neural_tools.rs) | 参数定义 | SaveShortTermMemoryParams / SaveLongTermMemoryParams / KnowledgeRelationParam；SearchMemoryParams 新增 traversal 四字段 |

### 阶段二+三（进行中）文件（供定位）

| 文件 | 角色 | 职责 |
|------|------|-----|
| [src/service/domain/system/](../../src/service/domain/system/) | SystemDomain 新领域 | CronManager trait + 实现（委托 CronTriggerDal） |
| [src/service/dal/cron_trigger.rs](../../src/service/dal/cron_trigger.rs) | CronTriggerDal | next_run_at 计算 / pause/resume / list_due |
| [src/service/dao/cron_trigger/](../../src/service/dao/cron_trigger/) | CronTriggerDao | CRUD + list_due + update_next_run_at |
| [src/scheduler/](../../src/scheduler/) | CronScheduler | 每分钟扫描；事件投递；并发控制 |
| [src/consumer/scheduler/](../../src/consumer/scheduler/) | 触发器消费者 | 接收事件 → 根据 action 路由 domain 方法 |
| [src/service/domain/runtime/digest.rs](../../src/service/domain/runtime/digest.rs) | 沉淀实现 | rest_and_digest：短期记忆 → LLM → 知识图谱 |
| [src/models/](../../src/models/) / [common/src/enums/](../../common/src/enums/) | Cron 模型 | CronTriggerPo / TriggerType(Once/Cron/Interval) |
| **迁移** | | migrations/*_cron_triggers.sql：cron_triggers 表 |
| **零改动面** | | 已有记忆四层存储结构、Domain 层 create_memory 内部逻辑、消息系统 SSE 推送 |

---

## 四、关键边界（行为红线 / 回归必保）

1. **save_short_term_memory / save_long_term_memory 注入，create_memory 不注入**：Agent 唤醒时的工具列表必须只包含两个专用工具；`create_memory` 的 `neural` flag 必须移除（防止误用）
2. **search_memory 向后兼容**：`traversal_depth` 默认 0（不遍历），不传 traversal 参数的旧调用方行为与改造前 100% 一致
3. **CronScheduler 不阻塞主流程**：后台扫描独立线程，触发器执行失败仅告警不影响主服务启动；触发器并发数限制（默认 4）防雪崩
4. **休息状态不丢消息**：Resting 状态下新消息排队（consumer 延迟处理或重试），不拒绝不丢失；休息完成后自动消费
5. **沉淀的幂等性**：同一天重复触发「每日睡眠」沉淀，第二次不产生重复节点（冲突检测 + 合并必须生效）

---

## 五、扩展模式

### 场景 1：新增 Cron 触发器类型（如每周报表导出）

1. **Consumer 层**：[src/consumer/scheduler/](../../src/consumer/scheduler/) 新增 action 分支（`match payload.action { "export_report" => ... }`）
2. **Domain 层**：在对应 Domain（如 FinanceDomain）新增 `export_weekly_report(ctx)` 方法
3. **注册**：创建触发器时 `payload = { action: "export_report", params: {...} }`，CronScheduler + 消费者无需改动（事件驱动扩展）

### 场景 2：新增第三类休息触发条件（如连续失败 > N 次触发复盘）

1. **Runtime 状态机**：[runtime/awakening.rs](../../src/service/domain/runtime/awakening.rs) 或 [context_assembly.rs](../../src/service/domain/runtime/context_assembly.rs) 中新增触发条件检查点
2. **进入 Resting**：设置状态 + 调用 `rest_and_digest(ctx, RestReason::ConsecutiveFailures(n))`，与现有过载/定时两条路径共享沉淀主流程（沉淀逻辑无需改动）
