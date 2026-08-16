🎯定位：AgentRuntimeInfo 结构体扩展，补充 task_id/project_id 业务上下文字段，使前端可按任务/项目视角过滤运行中 Agent
状态：v1.0（2026-08-16，落地快照）
触发场景：前端运行时面板需要按任务/项目维度展示运行中 Agent；AOP 统计事件需要业务上下文关联
关联文档段：
- 对应 design 文档：[docs/design/thinking_task_policy_engine_design.md](docs/design/thinking_task_policy_engine_design.md) 第 103 行「业务上下文字段归属」决策
- Wiki 长文真实路径：[docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md](docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md)
- RAG 卡真实路径：[docs/wiki/knowledge/zh/AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视/AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视.md](docs/wiki/knowledge/zh/AgentRuntimeInfo%20三态状态机%20+%20BusyGuard%20RAII：Idle%20Busy%20Resting%20转换%20+%20task_id%20project_id%20业务上下文透视/AgentRuntimeInfo%20三态状态机%20+%20BusyGuard%20RAII：Idle%20Busy%20Resting%20转换%20+%20task_id%20project_id%20业务上下文透视.md)

---

## 一、目标

| 问题 | 方式 |
|------|------|
| AgentRuntimeInfo 缺少业务上下文，前端运行时列表无法按任务/项目维度过滤 | 在 AgentRuntimeInfo 结构体新增 task_id / project_id 字段，set_busy / try_set_busy 时注入，set_idle 清空，set_resting 保留 |
| GetAgentResponse DTO 未暴露业务上下文字段 | 扩展 common DTO，同步更新两个 Handler（get_agent / update_agent_status）构造响应 |
| 生产代码调用方和测试代码签名不匹配 | 统一更新 awaken / consumer / dal 测试 / 集成测试 4 处 set_busy / try_set_busy 调用 |

收敛一句话：扩展 AgentRuntimeInfo 加 task_id/project_id 两字段，贯穿 set_busy → DTO → Handler 全链路，前端运行时面板可按任务/项目过滤。

---

## 二、架构思路

```
set_busy(agent_id, message_id, task_id?, project_id?)
    │
    ├─ AgentRuntimeInfo { state, current_message_id, task_id, project_id, state_started_at }
    │       │
    │       ├─ set_idle()  → 清空 task_id + project_id
    │       ├─ set_resting() → 保留 task_id + project_id（沉淀仍属同一业务上下文）
    │       └─ try_set_busy() → 同 set_busy，原子性 TOCTOU 修复
    │
    ├─ 消息入站两条路径注入上下文：
    │   ├─ awaken()     → message.po.task_id / project_id.as_deref()
    │   └─ consumer.rs  → message.po.task_id / project_id.as_deref()
    │
    └─ API 暴露层：
        ├─ GetAgentResponse { current_task_id, current_project_id }
        ├─ get_agent handler
        └─ update_agent_status handler
```

核心语义规则：Busy 期间 = 同一业务上下文，Resting（沉淀）属于同一上下文延续，Idle = 彻底脱离上下文。

---

## 三、涉及文件清单

| 层次 | 文件路径 | 职责 |
|------|----------|------|
| pkg 层（状态核心） | [src/pkg/agent_runtime_state.rs](src/pkg/agent_runtime_state.rs) | AgentRuntimeInfo 结构体扩展 + set_idle/set_resting/set_busy/try_set_busy 方法签名更新 |
| domain 层（awaken 路径） | [src/service/domain/runtime/awakening.rs](src/service/domain/runtime/awakening.rs#L536-L536) | awaken 中 set_busy 调用补 task_id/project_id 参数 |
| consumer 层（消息入站） | [src/consumer/message.rs](src/consumer/message.rs#L153-L153) | consumer 中 try_set_busy 调用补参数 |
| common DTO | [common/src/api/agent.rs](common/src/api/agent.rs#L160-L201) | GetAgentResponse 新增 current_task_id / current_project_id 字段 |
| handler 层 | [src/handlers/hr/agent/get_agent.rs](src/handlers/hr/agent/get_agent.rs#L106-L145) | runtime_info 解构补字段，响应构造补字段 |
| handler 层 | [src/handlers/hr/agent/update_agent_status.rs](src/handlers/hr/agent/update_agent_status.rs#L86-L135) | 同上，对称修改 |
| common 测试 | [common/src/api/agent_test.rs](common/src/api/agent_test.rs#L38-L38) | DTO 测试构造补新字段 |
| dal 测试 | [src/service/dal/agent_test.rs](src/service/dal/agent_test.rs#L877-L877) | set_busy 调用补 None 参数 |
| 集成测试 | [tests/integration/agent_awaken_test.rs](tests/integration/agent_awaken_test.rs#L158-L728) | 两处 set_busy 调用补 None 参数 |

⭐ **落地索引（四类互引）**
- Wiki 长文：[docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md](docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md)
- RAG 卡：[docs/wiki/knowledge/zh/AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视/AgentRuntimeInfo 三态状态机 + BusyGuard RAII：Idle Busy Resting 转换 + task_id project_id 业务上下文透视.md](docs/wiki/knowledge/zh/AgentRuntimeInfo%20三态状态机%20+%20BusyGuard%20RAII：Idle%20Busy%20Resting%20转换%20+%20task_id%20project_id%20业务上下文透视/AgentRuntimeInfo%20三态状态机%20+%20BusyGuard%20RAII：Idle%20Busy%20Resting%20转换%20+%20task_id%20project_id%20业务上下文透视.md)

---

## 四、分发点速查表

| 分发场景 | 入口函数 | 上下文来源 |
|----------|----------|------------|
| 用户消息唤醒 Agent | `RuntimeAwakeningImpl::awaken()` | message.po.task_id / project_id |
| 消费者批量消息入站 | `message.rs consumer` | message.po.task_id / project_id |
| Agent 完成一轮退出到 Idle | `StateManager::set_idle()` | 清空 task_id / project_id |
| Agent 进入沉淀（Resting） | `StateManager::set_resting()` | 保留 task_id / project_id |
| 前端查询运行时列表 | `GetAgentResponse` / `RuntimeListResponse` | 直接透出结构体字段 |
| TOCTOU 安全原子设置 | `StateManager::try_set_busy()` | 同 set_busy，一次 entry 内设置全部字段 |

---

## 五、验收清单

| 验收项 | 结果 |
|--------|------|
| AgentRuntimeInfo 结构体新增 task_id / project_id 两 Option\<String\> 字段 | ✓ 已落地 |
| set_busy / try_set_busy 签名扩展为 (agent_id, message_id, task_id?, project_id?) | ✓ 已落地 |
| set_idle() 清空新字段，set_resting() 保留不清空 | ✓ 已落地 |
| awaken() 调用 set_busy 从 message 上下文传入 task_id/project_id | ✓ 已落地 |
| consumer.rs 调用 try_set_busy 同步传入 | ✓ 已落地 |
| GetAgentResponse 新增 current_task_id / current_project_id | ✓ 已落地 |
| get_agent / update_agent_status 两个 handler 构造响应包含新字段 | ✓ 已落地 |
| 所有测试代码 set_busy / try_set_busy 调用签名更新并编译通过 | ✓ 已落地 |
| 四类互引占位路径已写入 | ✓ |

---

## 六、执行结果摘要

| 指标 | 值 |
|------|----|
| 修改文件数 | 9 个（pkg 1 / domain 1 / consumer 1 / common DTO 1 + test 1 / handler 2 / dal test 1 / integration 1） |
| 新增代码行（约） | 180 行（结构体 2 字段 + 方法签名 + 测试 + DTO） |
| 单元测试新增 | 5 个（set_busy 上下文记录 / try_set_busy 上下文 / set_idle 清空 / set_resting 保留 / None 上下文） |
| 测试通过情况 | 全部 PASS |
| 四类互引覆盖率 | design 1/1 + wiki 1/1 + RAG 1/1 = 100% |

---

## 七、后续扩展路径

1. **common 层**：RuntimeListResponse（后续运行时 API）直接复用 AgentRuntimeInfo 的 task_id/project_id 字段做过滤参数，无需二次建模。
2. **domain 层**：AgentLoopEvent / AgentAwakeEvent 统计事件补充 task_id/project_id 维度，实现任务/项目级统计分析。
3. **handler 层**：runtime-list 接口支持 task_id/project_id 查询参数，前端工作台按任务过滤运行中 Agent 卡片。
4. **前端**：Workspace 运行中 Agent 卡片按 task_id 聚合分组，任务详情页嵌入相关运行时状态。
