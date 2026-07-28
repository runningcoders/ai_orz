# 记忆系统增强 Spec

## Why

当前记忆模块已实现完整的四层认知模型（Core/Working/Short-term/Long-term），但存在三个核心缺口：
1. **写入接口过于宽泛**：`create_memory` 作为神经工具对外暴露时参数复杂，Agent 容易误用
2. **读取接口缺乏图谱遍历能力**：`search_memory` 仅支持语义搜索，无法沿知识图谱关系链式联想
3. **缺少自动沉淀机制**：Agent 不会"休息"和"睡觉"，短期记忆无法自动沉淀为长期知识图谱

本增强旨在对齐人类认知机制：Agent 工作时主动写入短期记忆，休息时由潜意识将短期记忆消化到知识图谱。

## What Changes

### 阶段一：记忆工具接口拆分与搜索增强（4B-1）

- **新增** `save_short_term_memory` 神经工具：简洁的短期记忆写入接口，不涉及图谱关系
- **新增** `save_long_term_memory` 神经工具：长期记忆写入接口，支持节点 + 关系一并创建
- **修改** `create_memory`：保留代码，移除 `neural` tag，不再作为神经工具注入 Agent
- **增强** `search_memory`：新增知识图谱遍历参数（深度、广度、策略），支持语义搜索 + 关联搜索
- **新增** DAL 层知识图谱遍历方法：`traverse_knowledge_graph()`，支持 BFS/DFS 遍历
- **新增** DAO 层关系批量查询方法：`list_relations_batch()`，支持按节点 ID 列表批量获取关系

### 阶段二：系统领域 - 定时触发器（4B-2）

- **新增** SystemDomain：系统基础设施领域，承载 cron 触发器等系统能力
- **新增** `cron_triggers` 数据库表：存储定时触发器（cron/interval/once 三种触发类型）
- **新增** CronTriggerDao：DAO 层 CRUD
- **新增** CronTriggerDal：DAL 层业务逻辑
- **新增** CronManager：SystemDomain 下的子模块，提供触发器管理能力
- **新增** CronScheduler：后台扫描器，每分钟检查到期触发器，投递触发事件
- **新增** 触发器消费者：接收触发事件，通过 consumer 层调用对应 domain 方法
- **新增** API Handler：定时触发器的 CRUD 管理接口

### 阶段三：休息与沉淀机制（4B-3）

- **新增** Runtime Domain `rest_and_digest()` 方法：Agent 休息时的沉淀入口
- **新增** 休息触发策略：
  - 上下文过载触发：连续工作 N 轮后进入短暂休息（清理上下文）
  - 每日定时触发：通过定时任务系统触发长时间睡眠（知识图谱沉淀）
- **新增** 知识图谱沉淀逻辑：
  - 获取近期短期记忆作为上下文
  - 调用 LLM 总结归纳，提取知识节点和关系
  - 写入知识图谱，冲突检测与合并
- **修改** Agent 唤醒流程：根据策略在唤醒后设置 Resting 状态
- **修改** 消费者：处理 Resting 状态的消息（排队等待或拒绝）

## Impact

- Affected specs: 记忆系统设计、Runtime Domain 设计、消费者架构
- Affected code:
  - `src/handlers/hr/agent/` — 新增 2 个 handler，修改 2 个
  - `src/handlers/system/` — 新增触发器管理 API
  - `common/src/api/neural_tools.rs` — 新增 DTO
  - `src/service/dal/memory.rs` — 新增图谱遍历方法
  - `src/service/dao/memory/` — 新增关系批量查询
  - `src/service/domain/runtime/` — 新增 rest_and_digest 方法
  - `src/service/domain/system/` — 新增 SystemDomain + CronManager
  - `src/consumer/` — 新增触发器消费者
  - `src/scheduler/` — CronScheduler 后台扫描器
  - `migrations/` — 新增 cron_triggers 表

## ADDED Requirements

### Requirement: 短期记忆写入工具

系统应提供一个独立的神经工具 `save_short_term_memory`，供 Agent 在思考过程中主动写入短期记忆摘要。

#### Scenario: Agent 思考时写入短期记忆
- **WHEN** Agent 在思考过程中调用 `save_short_term_memory`
- **THEN** 系统创建一条 ShortTermMemoryIndexPo 记录，自动向量化，返回 memory_id

#### Scenario: 参数简洁
- **WHEN** Agent 调用此工具
- **THEN** 仅需提供 summary（摘要内容）、tags（可选标签）、task_id（可选关联任务），无需指定 memory_type

### Requirement: 长期记忆写入工具

系统应提供一个独立的神经工具 `save_long_term_memory`，供 Agent 写入长期知识节点及其关联关系。

#### Scenario: 创建单个知识节点
- **WHEN** Agent 调用 `save_long_term_memory`，仅提供节点信息
- **THEN** 系统创建一个 LongTermKnowledgeNodePo 记录，自动向量化，返回 memory_id

#### Scenario: 创建节点同时创建关系
- **WHEN** Agent 调用 `save_long_term_memory`，提供节点信息和 relations 列表
- **THEN** 系统创建知识节点，并同时创建指定关系（关联到已有节点），全部返回

#### Scenario: 关系目标节点不存在
- **WHEN** relations 中引用的 target_node_id 不存在
- **THEN** 该条关系跳过并 warn，不影响节点创建

### Requirement: 记忆搜索图谱遍历

系统应支持在搜索记忆时沿知识图谱关系进行链式联想，支持广度优先、深度优先和混合策略。

#### Scenario: 纯语义搜索（无遍历）
- **WHEN** Agent 调用 search_memory，未指定 traversal_depth 或 traversal_depth=0
- **THEN** 仅执行语义搜索 + 关键词搜索，不遍历图谱

#### Scenario: 广度优先遍历
- **WHEN** Agent 指定 traversal_strategy=breadth_first, traversal_depth=2, traversal_breadth=5
- **THEN** 先语义搜索获取种子节点，再按 BFS 策略沿关系展开 2 层，每层最多取 5 个节点

#### Scenario: 深度优先遍历
- **WHEN** Agent 指定 traversal_strategy=depth_first, traversal_depth=3
- **THEN** 先语义搜索获取种子节点，再按 DFS 策略沿关系深入 3 层

#### Scenario: 分步搜索
- **WHEN** Agent 第一轮搜索后，根据结果决定第二轮搜索方向
- **THEN** Agent 可使用上一轮返回的节点 ID 作为新一轮搜索的种子，指定不同的遍历策略

### Requirement: 系统领域 - 定时触发器

系统应提供一套通用的定时触发器框架，支持 cron 表达式、固定间隔和一次性触发三种模式。

#### Scenario: 创建 cron 触发器
- **WHEN** 用户通过 API 创建一个 cron 类型的触发器，指定 cron_expression
- **THEN** 系统计算下次执行时间并持久化

#### Scenario: 创建间隔触发器
- **WHEN** 用户创建一个 interval 类型的触发器，指定 interval_seconds
- **THEN** 系统按固定间隔重复执行

#### Scenario: 创建一次性触发器
- **WHEN** 用户创建一个 once 类型的触发器，指定 run_at 时间
- **THEN** 系统在指定时间执行一次后自动禁用

#### Scenario: 后台扫描执行
- **WHEN** CronScheduler 后台扫描发现到期触发器
- **THEN** 投递触发事件到消息队列，由对应消费者处理

#### Scenario: 暂停和恢复
- **WHEN** 用户暂停一个触发器
- **THEN** 该触发器不再被扫描执行，恢复后继续

### Requirement: Agent 休息与知识沉淀

系统应支持 Agent 进入休息状态，在休息时将短期记忆自动沉淀到长期知识图谱。

#### Scenario: 上下文过载触发短暂休息
- **WHEN** Agent 连续工作轮次达到阈值（如 max_thinking_depth）
- **THEN** Agent 进入 Resting 状态，执行短暂休息（清理上下文），完成后恢复 Idle

#### Scenario: 定时触发长时间睡眠
- **WHEN** 定时触发器系统触发 Agent 的睡眠任务（如每日凌晨）
- **THEN** Agent 进入 Resting 状态，执行知识图谱沉淀

#### Scenario: 知识图谱沉淀流程
- **WHEN** Agent 执行 rest_and_digest
- **THEN** 系统获取近期短期记忆，调用 LLM 总结归纳，提取知识节点和关系，写入知识图谱

#### Scenario: 知识冲突检测与合并
- **WHEN** LLM 提取的知识节点与已有节点相似
- **THEN** 系统更新已有节点内容并合并关系，而非创建重复节点

#### Scenario: 休息完成后恢复
- **WHEN** 沉淀流程完成
- **THEN** Agent 状态恢复为 Idle，可接受新消息

## MODIFIED Requirements

### Requirement: create_memory 神经工具标记

`create_memory` 保留代码实现，但移除 `neural` tag，不再作为神经工具自动注入 Agent。

**Reason**: 该接口参数过于宽泛（memory_type 分发），Agent 使用时容易混淆。拆分为专用接口后，此接口仅供 HTTP API 或未来复杂场景使用。

### Requirement: search_memory 搜索参数

`search_memory` 新增知识图谱遍历参数，同时保持向后兼容。

新增参数：
- `traversal_depth: Option<i32>` — 遍历深度，默认 0（不遍历）
- `traversal_breadth: Option<i32>` — 每层广度限制，默认 0（不限制）
- `traversal_strategy: Option<String>` — 遍历策略（breadth_first/depth_first/hybrid），默认 breadth_first
- `seed_node_ids: Option<Vec<String>>` — 指定种子节点 ID（用于分步搜索）

### Requirement: Agent 唤醒后状态转换

Agent 唤醒完成后，根据休息策略决定下一步状态：
- 未达阈值 → Idle（现有行为）
- 达到上下文过载阈值 → Resting，触发短暂休息
