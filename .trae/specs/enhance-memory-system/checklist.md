# Checklist

## 阶段一：记忆工具接口拆分与搜索增强（4B-1）

- [ ] `SaveShortTermMemoryParams` DTO 已定义，字段简洁（summary, tags, task_id）
- [ ] `SaveLongTermMemoryParams` DTO 已定义，支持节点信息 + relations 列表
- [ ] `KnowledgeRelationParam` 结构体已定义（source_node_id, target_node_id, relation_type）
- [ ] `SearchMemoryParams` 新增 traversal_depth/traversal_breadth/traversal_strategy/seed_node_ids 字段
- [ ] `save_short_term_memory` 神经工具已实现，使用 `#[register_handler_tool(neural)]` 注册
- [ ] `save_long_term_memory` 神经工具已实现，支持节点 + 关系一并创建
- [ ] `create_memory` 已移除 `neural` tag，保留 HTTP handler 代码
- [ ] DAO 层 `list_relations_batch()` 方法已实现，支持按节点 ID 列表批量查询出入边
- [ ] DAL 层 `traverse_knowledge_graph()` 方法已实现，支持 BFS/DFS 两种遍历策略
- [ ] `search_memory` 增强后支持图谱遍历参数，向后兼容（默认不遍历）
- [ ] `search_memory` 支持 seed_node_ids 参数，允许分步搜索
- [ ] cargo check 编译通过
- [ ] cargo test 全部测试通过（预期 569+ 增量）
- [ ] `save_short_term_memory` 和 `save_long_term_memory` 在 Agent 唤醒时被正确注入
- [ ] `create_memory` 不再被注入 Agent

## 阶段二：系统领域 - 定时触发器（4B-2）

- [ ] `cron_triggers` 表 migration 已创建，包含 STRICT 模式和必要索引
- [ ] `TriggerType` 枚举已定义（Once/Cron/Interval），支持 sqlx::Type 和 From<i32>
- [ ] `CronTriggerPo` 模型已定义
- [ ] CronTriggerDao trait 已定义并实现 SQLite 版本
- [ ] CronTriggerDal trait 已定义并实现，包含 next_run_at 计算逻辑
- [ ] SystemDomain 已创建，包含 cron_manager() 子能力
- [ ] CronManager trait 已定义并实现，委托给 CronTriggerDal
- [ ] CronScheduler 后台扫描器已实现，每分钟扫描到期触发器
- [ ] CronScheduler 事件投递通过消息系统完成
- [ ] 触发器消费者已实现，能接收触发事件并路由到对应 domain
- [ ] 触发器 API Handler 已实现（CRUD + 暂停/恢复），位于 src/handlers/system/
- [ ] cargo check 编译通过
- [ ] cargo test 全部测试通过
- [ ] 触发器 CRUD 功能正常
- [ ] CronScheduler 能正确扫描和执行到期触发器
- [ ] 消费者能正确处理触发事件

## 阶段三：休息与沉淀机制（4B-3）

- [ ] `RuntimeMemory` trait 新增 `rest_and_digest()` 方法
- [ ] `digest.rs` 实现文件已创建
- [ ] LLM Prompt 设计完成：输入短期记忆，输出结构化知识节点和关系
- [ ] LLM 输出解析正确：提取节点和关系
- [ ] 冲突检测实现：向量搜索已有节点，相似度高于阈值则更新
- [ ] 关系合并实现：新关系与已有关系去重
- [ ] 知识图谱沉淀结果正确写入
- [ ] 上下文过载触发休息逻辑已实现
- [ ] Agent 唤醒后轮次检查正确触发 Resting 状态
- [ ] 定时睡眠触发器通过定时触发器系统触发
- [ ] 睡眠触发器执行完整知识图谱沉淀
- [ ] 休息完成后 Agent 状态恢复为 Idle
- [ ] cargo check 编译通过
- [ ] cargo test 全部测试通过
- [ ] 上下文过载触发休息功能正常
- [ ] 定时睡眠触发沉淀功能正常
- [ ] 知识图谱冲突检测与合并功能正常
