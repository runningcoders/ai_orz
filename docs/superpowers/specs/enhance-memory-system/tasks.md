# Tasks

## 阶段一：记忆工具接口拆分与搜索增强（4B-1）

- [x] Task 1: 新增 DTO 定义
  - [x] 在 `common/src/api/neural_tools.rs` 新增 `SaveShortTermMemoryParams/Response`
  - [x] 在 `common/src/api/neural_tools.rs` 新增 `SaveLongTermMemoryParams/Response`（含 relations 子结构）
  - [x] 在 `common/src/api/neural_tools.rs` 新增 `KnowledgeRelationParam` 结构体（source_node_id, target_node_id, relation_type）
  - [x] 修改 `SearchMemoryParams`，新增 traversal_depth/traversal_breadth/traversal_strategy/seed_node_ids 字段

- [x] Task 2: 新增 save_short_term_memory 神经工具
  - [x] 在 `src/handlers/hr/agent/` 新增 `save_short_term_memory.rs`
  - [x] 使用 `#[register_handler_tool(id="save_short_term_memory", neural)]` 注册
  - [x] 内部构造 `ShortTermMemoryIndexPo`，调用 `runtime_domain().memory().create()`
  - [x] 在 `mod.rs` 中注册模块和导出

- [x] Task 3: 新增 save_long_term_memory 神经工具
  - [x] 在 `src/handlers/hr/agent/` 新增 `save_long_term_memory.rs`
  - [x] 使用 `#[register_handler_tool(id="save_long_term_memory", neural)]` 注册
  - [x] 内部构造 `LongTermKnowledgeNodePo` + `Vec<KnowledgeReferencePo>` + `Vec<KnowledgeNodeRelationPo>`
  - [x] 调用 `runtime_domain().memory().create()` 创建节点
  - [x] 若有 relations，调用 `runtime_domain().memory().create()` 创建关系
  - [x] 在 `mod.rs` 中注册模块和导出

- [x] Task 4: create_memory 移除 neural 标记
  - [x] 修改 `src/handlers/hr/agent/create_memory.rs`，移除 `neural` flag
  - [x] 保留 `#[register_handler_tool]` 和 `#[generate_http_handler]`，仅不再作为神经工具注入

- [x] Task 5: DAO 层新增关系批量查询
  - [x] 在 `src/service/dao/memory/mod.rs` 的 `MemoryDao` trait 新增 `list_relations_batch()` 方法
  - [x] 在 `src/service/dao/memory/sqlite.rs` 实现该方法：按节点 ID 列表批量查询出入边关系

- [x] Task 6: DAL 层新增知识图谱遍历方法
  - [x] 在 `src/service/dal/memory.rs` 的 `MemoryDal` trait 新增 `traverse_knowledge_graph()` 方法
  - [x] 实现 BFS 遍历策略：从种子节点出发，按广度逐层展开
  - [x] 实现 DFS 遍历策略：从种子节点出发，按深度优先深入
  - [x] 返回结果包含节点和关系

- [x] Task 7: 增强 search_memory 神经工具
  - [x] 修改 `src/handlers/hr/agent/search_memory.rs`
  - [x] 解析新增的 traversal 参数
  - [x] 先执行语义搜索获取种子节点
  - [x] 若 traversal_depth > 0，调用 DAL 层 `traverse_knowledge_graph()` 遍历
  - [x] 合并搜索结果和遍历结果，统一返回
  - [x] 支持 seed_node_ids 参数（跳过语义搜索，直接遍历）

- [x] Task 8: 4B-1 阶段验证
  - [x] cargo check 编译通过
  - [x] cargo test 全部测试通过（569 个测试，100% 通过）
  - [x] 验证 save_short_term_memory 神经工具被正确注入 Agent
  - [x] 验证 save_long_term_memory 神经工具被正确注入 Agent
  - [x] 验证 create_memory 不再被注入 Agent
  - [x] 验证 search_memory 图谱遍历功能正常

## 阶段二：系统领域 - 定时触发器（4B-2）

- [ ] Task 9: 数据库迁移与模型
  - [ ] 新增 migration `cron_triggers` 表（id, name, trigger_type, cron_expression, interval_seconds, run_at, next_run_at, is_enabled, payload, created_at, updated_at）
  - [ ] 在 `src/models/` 新增 `CronTriggerPo` 持久化对象
  - [ ] 在 `common/src/enums/` 新增 `TriggerType` 枚举（Once/Cron/Interval）

- [ ] Task 10: CronTriggerDao
  - [ ] 在 `src/service/dao/` 新增 `cron_trigger/` 模块
  - [ ] 定义 `CronTriggerDao` trait（create, get, list, update, delete, list_due, update_next_run_at）
  - [ ] 实现 SQLite 版本
  - [ ] 新增 DAO 单元测试

- [ ] Task 11: CronTriggerDal
  - [ ] 在 `src/service/dal/` 新增 `cron_trigger.rs`
  - [ ] 定义 `CronTriggerDal` trait（create, get, list, update, delete, pause, resume, list_due）
  - [ ] 实现业务逻辑：创建时根据类型计算 next_run_at，执行后更新 next_run_at
  - [ ] 新增 DAL 单元测试

- [ ] Task 12: SystemDomain + CronManager
  - [ ] 在 `src/service/domain/` 新增 `system/` 模块
  - [ ] 定义 `SystemDomain` trait，包含 `cron_manager()` 子能力
  - [ ] 定义 `CronManager` trait（create, get, list, update, delete, pause, resume）
  - [ ] 实现 CronManager，委托给 CronTriggerDal
  - [ ] 新增 Domain 单元测试

- [ ] Task 13: CronScheduler 后台扫描器
  - [ ] 在 `src/scheduler/` 新增模块
  - [ ] 实现 `CronScheduler`：每分钟扫描 `list_due()`，投递触发事件
  - [ ] 事件投递通过消息系统（复用现有 event topic 机制）
  - [ ] 支持并发执行控制
  - [ ] 在 main.rs 中初始化启动

- [ ] Task 14: 触发器消费者
  - [ ] 在 `src/consumer/` 新增 `scheduler` 消费者模块
  - [ ] 接收触发事件，解析 payload
  - [ ] 根据 payload 中的 action 路由到对应 domain 方法
  - [ ] 支持 agent_rest（休息沉淀）等触发器类型
  - [ ] 在 consumer init 中注册

- [ ] Task 15: 触发器 API Handler
  - [ ] 在 `src/handlers/system/` 新增触发器管理接口
  - [ ] CRUD：创建、查询、更新、删除、暂停、恢复
  - [ ] 在路由中注册

- [ ] Task 16: 4B-2 阶段验证
  - [ ] cargo check 编译通过
  - [ ] cargo test 全部测试通过
  - [ ] 验证触发器 CRUD 功能
  - [ ] 验证 CronScheduler 后台扫描功能
  - [ ] 验证消费者正确处理触发事件

## 阶段三：休息与沉淀机制（4B-3）

- [ ] Task 17: RuntimeMemory 新增沉淀方法
  - [ ] 在 `RuntimeMemory` trait 新增 `rest_and_digest()` 方法
  - [ ] 在 `src/service/domain/runtime/` 新增 `digest.rs` 实现文件
  - [ ] 实现：获取近期短期记忆 → 构造 LLM Prompt → 调用 Cortex 思考 → 解析输出
  - [ ] 新增 Domain 单元测试

- [ ] Task 18: 知识图谱沉淀逻辑
  - [ ] LLM Prompt 设计：输入近期短期记忆，输出结构化知识节点和关系
  - [ ] 解析 LLM 输出：提取节点（name, description, type, summary）和关系（source, target, type）
  - [ ] 冲突检测：向量搜索已有节点，相似度高于阈值则更新而非创建
  - [ ] 关系合并：新关系与已有关系去重
  - [ ] 写入知识图谱

- [ ] Task 19: 上下文过载触发休息
  - [ ] 修改 `src/consumer/message.rs` 的 `handle_agent_message`
  - [ ] 唤醒后检查轮次是否达到阈值
  - [ ] 达到则设置 Agent 为 Resting 状态
  - [ ] 调用 `rest_and_digest()` 执行短暂休息
  - [ ] 完成后恢复 Idle

- [ ] Task 20: 定时睡眠触发器
  - [ ] 通过定时触发器系统创建 Agent 睡眠触发器
  - [ ] 消费者接收睡眠事件，调用 `rest_and_digest()`
  - [ ] 睡眠触发器执行完整知识图谱沉淀
  - [ ] 完成后恢复 Idle

- [ ] Task 21: 4B-3 阶段验证
  - [ ] cargo check 编译通过
  - [ ] cargo test 全部测试通过
  - [ ] 验证上下文过载触发休息
  - [ ] 验证定时睡眠触发沉淀
  - [ ] 验证知识图谱冲突检测与合并

# Task Dependencies

- Task 2, 3 依赖 Task 1（DTO 定义）
- Task 7 依赖 Task 5, 6（DAO 和 DAL 层图谱遍历方法）
- Task 8 依赖 Task 2, 3, 4, 7
- Task 11 依赖 Task 10
- Task 12 依赖 Task 11
- Task 13 依赖 Task 12
- Task 14 依赖 Task 13
- Task 15 依赖 Task 9-14
- Task 16 依赖 Task 15（SystemDomain 就绪）
- Task 17 依赖 Task 16
- Task 18 依赖 Task 17
- Task 19 依赖 Task 17
- Task 20 依赖 Task 17, 18
- Task 21 依赖 Task 17-20
- 阶段间串行：4B-1 → 4B-2 → 4B-3
