# Agent 循环驱动引擎 Implementation Plan

> 🎯 **本文档定位**：规划与落地结果快照（概览级，不含代码细节；具体实现以代码路径为准）
>
> **文档状态**：进行中（9 Task 分阶段实施中）
>
> 查阅场景：
> - 新接手 Agent 调度/跟进能力时，快速理解三层兜底架构与消息意图传递模式
> - 排查项目进度通知未送达时，按链路逐层定位（实体字段→事件→定时补偿）
> - 新增系统级通知类型时，参考 Task 1 消息构建器与 §四 速查表
>
> 关联文档：
> - [ARCHITECTURE.md](../ARCHITECTURE.md) — 唯一权威架构总纲
> - [agent_loop_engine_design.md](../design/agent_loop_engine_design.md) — 底层设计决策细节
> - [task_design.md](../design/task_design.md) — 任务状态机与进度追踪

---

## 一、目标（为什么做）

解决 Agent 接收到任务后无法自主跟进项目进度的问题——当前 Agent 仅在被消息/任务显式唤醒时工作，缺少"任务状态变更自动调度 Owner Agent"和"项目进度定期巡检"两条主动链路。

| 问题维度 | 解决方式 |
|---------|---------|
| 任务完成后 Owner Agent 不知情 | Layer 2：DAL 层发布 TaskStatusChangedEvent → Async Consumer 合并去重后发送调度通知 |
| 长时间无交互时项目停滞 | Layer 3：CronTrigger project_followup 每小时扫描 InProgress 项目，唤醒 Owner Agent 巡检 |
| 意图传递污染 Prompt Builder | 不新增 AwakenIntent 枚举；意图指令直接嵌入消息内容文本，复用 MessageConsumer → awaken 链路自动补 project/task 上下文 |
| 重复通知轰炸 | Consumer 发送前去重：查是否已有同 Agent 的 Pending TaskDispatchNotification / ProjectFollowupNotification |
| 并发状态覆盖 | 入口预检查 Agent RuntimeState：非 Idle 跳过，避免覆盖 Busy / Resting |

**收敛后效果**：形成"字段状态可查 + 事件准实时推送 + 定时兜底巡检"三层闭环，Owner Agent 在无人工触发场景下仍能自主推进项目。

---

## 二、架构思路（怎么做的）

三层递进兜底，上层失败下层补：

```
┌─────────────────────────────────────────────────────────┐
│  Layer 3：定时补偿（CronTrigger）                       │
│  每小时扫描 InProgress 且有 Owner 的项目 → 唤醒巡检       │
│  （兜底，解决 Layer 2 丢事件或长期无状态变更的情况）       │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│  Layer 2：AOP 事件通知（Async Consumer）                │
│  DAL update_status 发布 TaskStatusChangedEvent          │
│  → TaskEventConsumer 异步消费 → 合并去重 → 发调度通知    │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│  Layer 1：实体字段扩展（事实源）                         │
│  ProjectPo/TaskPo：execution_plan / execution_result    │
│  ProjectPo：last_followup_at + ProjectProgressSummary   │
│  所有状态变更写 DB，是 Layer 2/3 的数据基础              │
└─────────────────────────────────────────────────────────┘

消息意图传递（核心不变量）：
  System→Agent 通知消息（TaskDispatchNotification / ProjectFollowupNotification）
  ├─ message_type：仅用于去重和分析，不做 prompt 路由
  ├─ 消息本体：携带结构化行动指令文本（如"更新进度→检查计划→调度下一任务"）
  ├─ project_id / task_id 上下文字段：经 MessageConsumer 自动补全上下文
  └─ Agent 读消息内容即可执行，无需特殊 prompt 注入
```

**关键边界 / 行为红线（回归必保）**：
1. **不新增 AwakenIntent 枚举**：意图永远走消息内容文本，这是本方案核心取舍
2. Consumer 发送前必做 **Pending 去重检查**，避免短时间多任务完成导致通知轰炸
3. 所有唤醒链路入口（scheduler / consumer / handler）必做 **Agent RuntimeState 预检查**，非 Idle 跳过
4. TaskStatusChangedEvent 在 **DAL 层 update_status 之后**发布（确保所有 status 变更都触发，不依赖调用方）
5. Seed 系统级 CronTrigger **按 action 去重**（非按 ID），用户已有同类型触发器时不重复创建
6. Project/Tool CRUD 的对外 API 契约不变（新增字段为 Option，前后端兼容）

---

## 三、涉及文件清单（读代码直接跳）

按分层索引，每行带可点击绝对路径链接：

| 文件 | 角色 | 摘要 |
|------|------|------|
| **common 层** | | |
| [common/src/enums/message.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/enums/message.rs) | 消息枚举 | 新增 MessageType::TaskDispatchNotification(10) / ProjectFollowupNotification(11) |
| [common/src/api/project.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/api/project.rs) | Project DTO | GetProjectRequest 增 with_progress_summary；UpdateProjectRequest 增 execution_plan/result |
| [common/src/api/task.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/api/task.rs) | Task DTO | UpdateTaskRequest 增 execution_plan/result；GetTaskResponse 对应字段 |
| [common/src/api/system.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/api/system.rs) | 健康 DTO | （Task 7 复用）LarkWsMetrics 健康指标结构 |
| **models 层** | | |
| [src/models/project.rs](file:///Users/aman/Technology/rust/ai_orz/src/models/project.rs) | Project 实体 | ProjectPo 增 execution_plan/result/last_followup_at；新增 ProjectProgressSummary 结构体；Project 业务实体增 progress_summary |
| [src/models/task.rs](file:///Users/aman/Technology/rust/ai_orz/src/models/task.rs) | Task 实体 | TaskPo 增 execution_plan/execution_result 字段 |
| [src/models/events/task_status.rs](file:///Users/aman/Technology/rust/ai_orz/src/models/events/task_status.rs) | 事件定义 | 新增 TaskStatusChangedEvent（task_id/assignee/old/new_status/progress） |
| **Domain 层** | | |
| [src/service/domain/message/mod.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/message/mod.rs) | 消息域 | SendToAgentCommand 增 message_type 字段；MessageDomain 增 has_pending_message_for_agent 去重方法 |
| [src/service/domain/message/builder.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/message/builder.rs) | 消息构建器 | build_task_dispatch_content / build_project_followup_content（意图指令嵌入文本） |
| [src/service/domain/message/delivery.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/message/delivery.rs) | 消息投递 | send_to_agent 使用 cmd.message_type 替代硬编码 Text |
| [src/service/domain/project/mod.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/project/mod.rs) | Project trait | ProjectManage 增 list_in_progress_with_owner；update_basic 增 execution_plan/result 参数 |
| [src/service/domain/project/service.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/project/service.rs) | Project 实现 | 实现 list_in_progress_with_owner（无用户过滤）；get_project 按需注入 progress_summary |
| [src/service/domain/project/task.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/project/task.rs) | Task 域 | TaskManage::update_basic 增 execution_plan/result 参数 |
| [src/service/domain/system/mod.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/system/mod.rs) | System 域 | 增 ensure_system_cron_triggers（按 action 去重创建 agent_rest + project_followup） |
| **Consumer 层** | | |
| [src/consumer/scheduler.rs](file:///Users/aman/Technology/rust/ai_orz/src/consumer/scheduler.rs) | 定时调度 | CronTriggerConsumer 增 handle_project_followup（扫描 InProgress 项目 + 预检查状态 + 发通知） |
| [src/consumer/task_event_consumer.rs](file:///Users/aman/Technology/rust/ai_orz/src/consumer/task_event_consumer.rs) | 任务事件消费 | Async Consumer；处理 Completed 状态；去重；填充 project_id+task_id 发调度通知 |
| [src/consumer/message.rs](file:///Users/aman/Technology/rust/ai_orz/src/consumer/message.rs) | 消息消费 | MessageConsumer（现有链路，自动补 project/task 上下文 + 调 awaken） |
| **Handler 层** | | |
| [src/handlers/hr/agent/settle_memory.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/hr/agent/settle_memory.rs) | 睡眠入口 | load_and_settle 入口增 Agent RuntimeState 预检查，避免覆盖 Busy |
| [src/handlers/project/task/update_task.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/project/task/update_task.rs) | Task 更新 | 传递 execution_plan/result 到 domain |
| [src/handlers/project/update_project.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/project/update_project.rs) | Project 更新 | 传递 execution_plan/result 到 domain |
| [src/handlers/project/projects/get_project.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/project/projects/get_project.rs) | Project 获取 | 传递 with_progress_summary 查询选项 |
| **DAL 层** | | |
| [src/service/dal/memory.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/dal/memory.rs) | 记忆 DAL | （Task 2 复用）sleep_and_settle 状态预检查入口 |
| [src/service/dal/task/](file:///Users/aman/Technology/rust/ai_orz/src/service/dal/task/) | Task DAL | update_status 成功后发布 TaskStatusChangedEvent（DAL 层发布确保全覆盖） |
| **零改动面 / 对外契约不变** | | |
| 前端 DTO / 路由 / 对外 API（新增字段为 Option） | 兼容 | 旧客户端不感知新增字段；Skill prompt 为 Task 8 独立文档更新 |
| 数据库 migrations 独立执行 | — | 20260806000001_execution_plan_result.sql 为独立 migration |

---

## 四、分发速查表（新增同类功能第一站）

### 4.1 新增系统通知消息类型（类似 TaskDispatchNotification）

新增系统→Agent 通知类型时，改动点 3 处：

| 改动点 | 位置 | 新增类型时参考 |
|--------|------|--------------|
| MessageType 枚举加变体 | [common/src/enums/message.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/enums/message.rs) | 紧接 ProjectFollowupNotification 之后，数值递增 |
| 构建器新增 build_xxx_content | [src/service/domain/message/builder.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/message/builder.rs) | 复制 build_project_followup_content 模式：指令文本按 4 步结构组织 |
| 去重检查（可选） | [src/consumer/task_event_consumer.rs](file:///Users/aman/Technology/rust/ai_orz/src/consumer/task_event_consumer.rs) | has_pending_message_for_agent 查同 MessageType，避免轰炸 |

> 代码入口：[builder.rs 函数区](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/message/builder.rs)

### 4.2 TaskStatusChangedEvent 新增消费场景（新 Consumer）

需对任务状态变更做额外动作时，新增独立 Consumer，不修改现有：

| 分支 | 处理逻辑 | 新增时参考 |
|------|---------|-----------|
| InProgress→Completed（现有） | 触发 Owner Agent 调度通知 | 参见 TaskEventConsumer::on_event Completed 分支体 |
| 其他状态转换 | 按需新增独立 Consumer，注册 interested_events 相同 kind | AOP 框架多 Consumer 同事件解耦，互不影响 |

> 代码入口：[task_event_consumer.rs Consumer impl](file:///Users/aman/Technology/rust/ai_orz/src/consumer/task_event_consumer.rs)

### 4.3 新增系统级默认定时任务（类似 project_followup）

| 改动点 | 处理逻辑 | 参考 |
|--------|---------|------|
| CronTriggerConsumer 加 action 分支 | scheduler.rs on_event match 新增分支 → 调 handle_xxx | 参见 handle_project_followup 结构：ctx 系统级 → domain 查询 → 循环 → 状态预检查 → 构建消息 → send |
| ensure_system_cron_triggers 去重检查 | iter.any payload.contains("\"action\"") 匹配 | 同文件中 agent_rest / project_followup 两处去重模式 |

> 代码入口：[scheduler.rs CronTriggerConsumer impl](file:///Users/aman/Technology/rust/ai_orz/src/consumer/scheduler.rs)

---

## 五、验收清单（按 Task 达成情况）

- [ ] Task 1：MessageType 两变体 + SendToAgentCommand.message_type + builder 模块 + 所有调用方补 message_type: Text
- [ ] Task 2：load_and_settle Agent RuntimeState 预检查，Busy 时跳过
- [ ] Task 3：ProjectManage::list_in_progress_with_owner（系统级无用户过滤）+ 测试
- [ ] Task 4：CronTriggerConsumer handle_project_followup + 集成测试
- [ ] Task 5：DB migration + ProjectPo/TaskPo 新字段 + ProjectProgressSummary + DAO/DAL 读写
- [ ] Task 6：update_task/project 扩展 + GetProjectRequest.with_progress_summary + get_project 进度注入
- [ ] Task 7：TaskStatusChangedEvent（DAL 层发布）+ TaskEventConsumer Async + 去重
- [ ] Task 8：项目管理技能 prompt 更新（execution_plan/result 书写规范 + 系统通知响应）
- [ ] Task 9：Seed ensure_system_cron_triggers（按 action 去重，启动时创建）
- [ ] 全量门槛：cargo check + clippy -D warnings（双端）+ fmt 检查 + 全量测试通过

---

## 六、执行结果摘要

| 模块 | 验证结果 |
|------|---------|
| common 单元测试 | 待执行（预计新增 builder.rs 2 tests） |
| domain 层测试 | 待执行（新增 ProjectServiceImpl list_in_progress_with_owner 等） |
| 后端 lib 全量 | 待执行 |
| 集成测试 | 待编写（project_followup / task_event_consumer / cron 去重等） |
| Clippy 双端 | 待执行 |
| 前端测试 | 零改动（DTO 层自动获得新字段） |

### 与计划的偏离（如有）
1. 暂无（计划启动中，偏离待执行阶段记录）

---

## 七、后续扩展路径（4 步模板）

> **核心不变量**：意图传递走消息内容；Consumer 永远 Async；Domain/DTO 基础不动。

1. **消息构建器扩展**：[builder.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/message/builder.rs) — 新增通知类型时按 build_xxx_content 模式加构建函数，结构统一为 4 步行动指令
2. **Consumer 注册**：[consumer/mod.rs init()](file:///Users/aman/Technology/rust/ai_orz/src/consumer/mod.rs) — 新增 AOP Consumer 时在此处 registry.register(Arc::new(...))；同事件多 Consumer 解耦
3. **Seed 系统触发器扩展**：[domain/system/mod.rs ensure_system_cron_triggers](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/system/mod.rs) — 新增默认定时任务时按 action 去重模式追加（检查 existing.iter().any 匹配 payload 中的 action 字符串）
4. **前端展示层（后续）**：[frontend/src/pages/message/chat.rs](file:///Users/aman/Technology/rust/ai_orz/frontend/src/pages/message/chat.rs) — 系统通知消息气泡特殊样式徽标、快捷行动指令按钮渲染（Task 9 之后独立跟进）
