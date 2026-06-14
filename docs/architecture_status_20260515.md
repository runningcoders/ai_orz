# 🏗️ 分层架构现状报告
**快照日期**: 2026-05-15  
**优化原则**: 从下往上逐步完善，像盖楼房一样打牢每一层

---

## 📊 整体金字塔结构

```
            ┌─────────────────┐
            │   Handler API   │  前端接入层
            │   (完成 50%)    │
            └────────┬────────┘
                     │
            ┌────────▼────────┐
            │   Domain 领域   │  业务逻辑层
            │    (完成 50%)   │
            └────────┬────────┘
                     │
            ┌────────▼────────┐
            │   DAL 数据访问  │  数据编排层
            │    (完成 85%)   │
            └────────┬────────┘
                     │
            ┌────────▼────────┐
            │   DAO 持久化    │  数据读写层
            │   (完成 100%)   │
            └─────────────────┘
```

---

## ✅ 第一层：DAO 层（100% 完成）

**状态**: 🎉 完美！全部被 DAL 层使用，零闲置

| 序号 | DAO 模块 | 承载职责 | 被 DAL 使用情况 |
|------|----------|----------|----------------|
| 1 | agent | Agent 基础信息 | ✅ dal_agent |
| 2 | artifact | 项目附件 | ✅ dal_artifact |
| 3 | brain | Agent 大脑核心 | ✅ dal_memory, dal_skill, dal_tool, dal_brain |
| 4 | cortex | LLM 调用实现 | ✅ dal_brain |
| 5 | email | 邮件发送 | ✅ dal_message_channel |
| 6 | event_queue | 事件队列 | ✅ dal_message |
| 7 | lark | 飞书渠道 | ✅ dal_message_channel |
| 8 | memory | 记忆存储 + 向量 | ✅ dal_memory |
| 9 | message | 消息存储 | ✅ dal_message |
| 10 | message_channel | 多渠道分发 | ✅ dal_message_channel |
| 11 | model_provider | 模型提供商配置 | ✅ dal_model_provider, dal_memory, dal_skill, dal_tool |
| 12 | organization | 组织管理 | ✅ dal_organization |
| 13 | project | 项目管理 | ✅ dal_project |
| 14 | skill | 技能管理 + 向量 | ✅ dal_skill |
| 15 | slack | Slack 渠道 | ✅ dal_message_channel |
| 16 | task | 任务管理 | ✅ dal_task |
| 17 | tool | 工具管理 + 向量 | ✅ dal_tool |
| 18 | tool_call | 工具调用记录 | ✅ dal_tool, dal_brain |
| 19 | user | 用户管理 | ✅ dal_user |
| 20 | webhook, wechat | Webhook/微信渠道 | ✅ dal_message_channel |

**总计**: 20 个 DAO 模块，全部被 DAL 层使用

---

## ⚠️ 第二层：DAL 层（85% 完成）

**状态**: 11/13 个模块被 Domain 使用，2 个闲置（仅测试调用）

| 序号 | DAL 模块 | 状态 | 被 Domain 使用 | 备注 |
|------|----------|------|----------------|------|
| 1 | agent | ✅ | hr/agent, hr/mod | |
| 2 | artifact | ✅ | project/artifact, project/mod | |
| 3 | brain | ✅ | finance/model_provider | ⚠️ 职责边界待理清 |
| 4 | memory | ❌ 闲置 | **无业务调用，仅测试** | 🔧 待接入 Agent/Brain 领域 |
| 5 | message | ✅ | message/mod | |
| 6 | message_channel | ❌ 闲置 | **无业务调用，仅测试** | 🔧 待接入 Message 领域 |
| 7 | model_provider | ✅ | finance/model_provider | |
| 8 | organization | ✅ | organization/mod | |
| 9 | project | ✅ | project/project, project/mod | |
| 10 | skill | ✅ | hr/skill, hr/mod | |
| 11 | task | ✅ | project/task, project/mod | |
| 12 | tool | ✅ | hr/mod, tool/management | |
| 13 | user | ✅ | organization/mod | |

**闲置模块修复优先级**:
1. **P0**: dal_message_channel → 接入 domain_message（改动最小）
2. **P1**: dal_memory → 设计接入路径（扩展 hr/agent 或新增 brain domain）

---

## ⚠️ 第三层：Domain 层（50% 完成）

**状态**: 3/6 个领域暴露了 Handler API，3 个闲置

| 序号 | 业务领域 | 状态 | API 覆盖 | 内部实现程度 | 备注 |
|------|----------|------|----------|--------------|------|
| 1 | finance | ✅ | 7 个 Handler | 完整 | model_provider 管理 |
| 2 | hr | ✅ | 5 个 Handler | 完整 | agent/skill 管理 |
| 3 | organization | ✅ | 15+ 个 Handler | 完整 | 组织/用户/认证 |
| 4 | message | ✅ 激活 | ❌ 无 Handler | ✅ **完整实现** - 消息管理 + 8 个渠道管理 + 多渠道投递 | ✅ 67/67 测试通过 |
| 5 | project | ⚠️ 假闲置 | ❌ 无 Handler | ✅ **完整实现** - 23 个方法带 DAL | 🔧 P0 - 仅需补充 Handler |
| 6 | tool | ⚠️ 假闲置 | ❌ 无 Handler | ✅ **完整实现** - management 27 方法带 DAL | 🔧 P0 - 仅需补充 Handler |

### 🎯 重点发现
- **domain_project** 和 **domain_tool** 内部实现都已经完整（DAL 注入 + 业务方法），**只差 Handler 暴露**
- **domain_message** 已完成 DAL 集成，**消息管理 + 渠道管理 + 多渠道投递** 全部实现，测试 100% 通过

---

## ⚠️ 第四层：Handler 层（~50% 完成）

**定位原则**: Handler 是用户 Action / HTTP API 的入口层，与接口语义直接对应；不做通用 Handler 框架抽象，按单个接口需求完成请求级编排。

**职责边界**:
- ✅ 解析 API DTO、参数校验、从 `RequestContext` 补全组织/用户等请求上下文
- ✅ 将 API DTO 转换为 Domain Command/Query
- ✅ 按用户 Action 编排一个或多个 Domain 调用
- ✅ 将业务实体组装为 Response DTO
- ❌ 直接调用 DAL/DAO
- ❌ 承载复杂业务规则、状态流转、权限语义
- ❌ Handler 间互调或通过 `BaseHandler` / `GenericActionHandler` 复用

**复用方式**: 优先复用 Domain 能力与 Command/Query 参数结构；当多个接口共享流程时，把可复用逻辑沉到 Domain，而不是抽象 Handler。

**已上线 API 领域**:
- ✅ organization: 组织管理、用户管理、系统初始化、个人资料
- ✅ hr: Agent CRUD、Agent 状态更新
- ✅ finance: Model Provider CRUD、模型调用测试、MessageChannel 管理、Tool 管理与 Agent 绑定

**待补充 API 领域**（详见 `docs/handler_management_api_plan.md`）:
- 🔧 Phase 2 / Batch 2.1 project/project: 项目管理面 CRUD 与统一状态更新（P1 - 优先开始）；状态流转语义先补/使用 Domain 统一入口，Handler 不分发到具体状态方法
- 🔧 Phase 2 / Batch 2.2 project/task: 任务管理面 CRUD、按 Project/Agent 列表与统一状态更新（P1）；状态合法性与流转规则下沉 Domain
- 🔧 Phase 2 / Batch 2.3 hr/skill: Skill 元数据、主内容、搜索、安装到 Agent（P1 - 路由统一 `/api/v1/hr/...`，安装能力已具备完整 `Skill` 实体返回；暂不扩展附件级文件副作用）
- 🔧 artifact/message-management: 附件与消息管理查询（P2 - 受文件上传/消息语义影响）
- ⏸️ message delivery / runtime awakening / tool execution: 运行面能力，单独随 Consumer / Runtime 链路推进

---

## 🏗️ 分层架构执行质量审计

### ✅ 做得好的地方
1. **严格分层执行到位**: 所有 Handler 都通过 Domain 层调用，**没有直接调用 DAL/DAO**
2. **依赖方向正确**: 上层依赖下层，没有循环依赖
3. **DAO 层覆盖率 100%**: 每个 DAO 都有对应的 DAL 使用者
4. **依赖注入规范**: Domain 层通过 Arc<dyn Trait> 注入 DAL，符合 DIP 原则

### ⚠️ 待优化的架构细节
1. **dal_brain 职责边界模糊**: 目前被 finance/model_provider 使用，命名和实际职责不匹配
2. **dal_memory 缺少业务承载者**: 没有对应的 memory domain 或 brain domain
3. **domain_message 没有 DAL 依赖**: 12 个方法都是空实现，需要接入真实 DAL

---

## 🗺️ 从下往上优化路线图

按照"盖楼房"原则，从底层开始完善，逐步向上：

### 🎯 第一阶段：打牢 DAL 层基础（P0）
**目标**: 消除 DAL 层闲置，100% DAL 都有业务使用者

| 任务 | 预估工作量 | 说明 |
|------|------------|------|
| domain_message 集成 dal_message_channel | 低 | 注入 DAL 依赖，实现 12 个方法 |
| 理清 dal_brain 职责边界 | 中 | 重命名 or 拆分 or 调整使用者 |

### 🎯 第二阶段：补全 Domain 层（P0）
**目标**: 消除 Domain 层假闲置，内部实现 100% 可用

| 任务 | 预估工作量 | 说明 |
|------|------------|------|
| domain_project 补充 Handler API | 极低 | 内部已完成，仅需暴露 |
| domain_tool 补充 Handler API | 极低 | 内部已完成，仅需暴露 |

> 管理面 API 的第二阶段实施顺序按 `Project → Task → Skill` 推进。Project / Task 的状态更新必须通过 Domain 统一 `update_status` / `transition_status` 入口承载业务语义，Handler 只做 DTO 解析和统一方法调用；Skill 属于 HR Domain，管理面路由统一使用 `/api/v1/hr/skills...` 与 `/api/v1/hr/agents/{agent_id}/skills...`。

### 🎯 第三阶段：扩展核心能力（P1）
**目标**: 为 Memory 设计业务承载者

| 任务 | 预估工作量 | 说明 |
|------|------------|------|
| 设计 memory/brain domain | 中 | 确定记忆业务归属 |
| dal_memory 接入对应 domain | 中 | 实现记忆读写业务逻辑 |

### 🎯 第四阶段：完善全链路（P2）
**目标**: 端到端功能打通

| 任务 | 预估工作量 | 说明 |
|------|------------|------|
| message 消费推送全链路 | 中 | consumer → domain_message → dal_message_channel |
| Agent 思考记忆链路 | 高 | Agent 触发 → memory DAL 读写 |

---

## 📈 完成度里程碑跟踪

| 层级 | 当前完成度 | 目标完成度 | 达成时间 |
|------|------------|------------|----------|
| DAO 层 | 100% ✅ | 100% | 已达成 |
| DAL 层 | 85% ⚠️ | 100% | 第一阶段后 |
| Domain 层 | 50% ⚠️ | 100% | 第二阶段后 |
| Handler 层 | ~50% ⚠️ | 100% | 第三阶段后 |
| **整体** | **~71%** | **100%** | |

---

## 🔍 关键架构决策记录

1. ✅ **严格分层原则**: Handler → Domain → DAL → DAO，禁止跨层调用
   - 当前执行情况: 100% 遵守，没有发现绕过 Domain 的调用

2. ✅ **DAO 职责单一**: 每个 DAO 只负责一个实体的持久化
   - 当前执行情况: 20 个 DAO 职责清晰

3. ⚠️ **Domain 全覆盖**: 每个 DAL 都应该有对应的 Domain 业务承载
   - 当前执行情况: 2/13 DAL 缺失业务承载

4. ✅ **面向接口编程**: Domain 通过 Trait 依赖 DAL，DAL 通过 Trait 依赖 DAO
   - 当前执行情况: 已实现，使用 Arc<dyn XxxDal + Send + Sync> 注入

5. ✅ **状态流转语义归属 Domain**: 管理面统一暴露 `/status` action，但状态合法性、状态流转和副作用由 Domain 统一入口承担
   - 当前执行情况: Agent 已按该模式落地；Project / Task 在 Phase 2 实施时先补/使用统一 `update_status` / `transition_status`，避免 Handler 分发到 `start/complete/archive/cancel`

6. ✅ **Skill 业务实体边界收敛**: Domain / DAL 对上层优先暴露完整 `Skill` 业务实体，`SkillPo` 保持在 DAO/存储映射边界内
   - 当前执行情况: `install_to_agent` 已具备完整 `Skill` 返回能力，可纳入 HR Skill 管理面正式 API；附件级文件读写/删除仍暂缓

---

*本文档为架构现状快照，每次完成阶段性优化后更新*
