# 🏗️ 分层架构现状报告
**快照日期**: 2026-07-13
**优化原则**: 从下往上逐步完善，像盖楼房一样打牢每一层

---

## 📊 整体金字塔结构

```
            ┌─────────────────┐
            │   Handler API   │  前端接入层
            │   (完成 ~95%)   │
            └────────┬────────┘
                     │
            ┌────────▼────────┐
            │   Domain 领域   │  业务逻辑层
            │   (完成 100%)   │
            └────────┬────────┘
                     │
            ┌────────▼────────┐
            │   DAL 数据访问  │  数据编排层
            │   (完成 100%)   │
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
| 2 | artifact | 项目产物 | ✅ dal_artifact |
| 3 | attachment | 通用附件存储 | ✅ dal_attachment |
| 4 | brain | Agent 大脑核心 | ✅ dal_memory, dal_skill, dal_tool, dal_brain |
| 5 | cortex | LLM 调用实现 | ✅ dal_brain |
| 6 | email | 邮件发送 | ✅ dal_message_channel |
| 7 | event_queue | 事件队列 | ✅ dal_message |
| 8 | lark | 飞书渠道 | ✅ dal_message_channel |
| 9 | mcp_server | MCP 服务器管理 | ✅ dal_mcp_server |
| 10 | memory | 记忆存储 + 向量 | ✅ dal_memory |
| 11 | message | 消息存储 | ✅ dal_message |
| 12 | message_channel | 多渠道分发 | ✅ dal_message_channel |
| 13 | model_provider | 模型提供商配置 | ✅ dal_model_provider, dal_memory, dal_skill, dal_tool |
| 14 | organization | 组织管理 | ✅ dal_organization |
| 15 | project | 项目管理 | ✅ dal_project |
| 16 | skill | 技能管理 + 向量 | ✅ dal_skill |
| 17 | slack | Slack 渠道 | ✅ dal_message_channel |
| 18 | task | 任务管理 | ✅ dal_task |
| 19 | tool | 工具管理 + 向量 | ✅ dal_tool |
| 20 | tool_call | 工具调用记录 | ✅ dal_tool, dal_brain |
| 21 | user | 用户管理 | ✅ dal_user |
| 22 | webhook, wechat | Webhook/微信渠道 | ✅ dal_message_channel |

**总计**: 28 个 DAO 模块（21 核心 DAO + 5 渠道 DAO + 1 统计 DAO + 1 触发器 DAO），全部被 DAL 层使用

---

## ✅ 第二层：DAL 层（100% 完成）

**状态**: 🎉 17/17 个模块全部被 Domain 使用，零闲置

| 序号 | DAL 模块 | 状态 | 被 Domain 使用 | 备注 |
|------|----------|------|----------------|------|
| 1 | agent | ✅ | hr/agent, hr/mod | |
| 2 | artifact | ✅ | project/artifact, project/mod | |
| 3 | attachment | ✅ | finance/attachment | 通用附件存储 |
| 4 | brain | ✅ | finance/model_provider | |
| 5 | mcp_server | ✅ | finance/mcp_server | MCP 服务器管理 |
| 6 | mcp_tool | ✅ | finance/mcp_tool | MCP 工具同步 |
| 7 | memory | ✅ | runtime/memory | 运行时记忆读写 |
| 8 | message | ✅ | message/management, message/delivery | |
| 9 | message_channel | ✅ | message/delivery | 多渠道投递 |
| 10 | model_provider | ✅ | finance/model_provider | |
| 11 | organization | ✅ | organization/mod | |
| 12 | project | ✅ | project/project, project/mod | |
| 13 | skill | ✅ | hr/skill, hr/mod | |
| 14 | task | ✅ | project/task, project/mod | |
| 15 | tool | ✅ | hr/mod, finance/tool_provider | |
| 16 | user | ✅ | organization/mod | |

---

## ✅ 第三层：Domain 层（100% 完成）

**状态**: 🎉 7/7 个领域全部完整实现

| 序号 | 业务领域 | 状态 | API 覆盖 | 内部实现程度 | 备注 |
|------|----------|------|----------|--------------|------|
| 1 | finance | ✅ | ✅ 完整 | 完整 | model_provider/attachment/mcp_server/mcp_tool/tool/message_channel 管理 |
| 2 | hr | ✅ | ✅ 完整 | 完整 | agent/skill 管理 |
| 3 | organization | ✅ | ✅ 完整 | 完整 | 组织/用户/认证 |
| 4 | message | ✅ | ✅ 完整 | ✅ 完整 | 消息管理 + 8 个渠道管理 + 多渠道投递 |
| 5 | project | ✅ | ✅ 完整 | ✅ 完整 | 项目/任务/产物管理 |
| 6 | runtime | ✅ | 内部调用 | ✅ 完整 | Agent 唤醒、工具执行、记忆读写 |
| 7 | system | ✅ | ✅ 完整 | ✅ 完整 | Cron Trigger 管理、后台扫描、事件投递 |

---

## ✅ 第四层：Handler 层（~95% 完成）

**定位原则**: Handler 是用户 Action / HTTP API 的入口层，与接口语义直接对应；不做通用 Handler 框架抽象，按单个接口需求完成请求级编排。

**职责边界**:
- ✅ 解析 API DTO、参数校验、从 `RequestContext` 补全组织/用户等请求上下文
- ✅ 将 API DTO 转换为 Domain Command/Query
- ✅ 按用户 Action 编排一个或多个 Domain 调用
- ✅ 将业务实体组装为 Response DTO
- ❌ 直接调用 DAL/DAO
- ❌ 承载复杂业务规则、状态流转、权限语义
- ❌ Handler 间互调或通过 `BaseHandler` / `GenericActionHandler` 复用

**已上线 API 领域**:
- ✅ organization: 组织管理、用户管理、系统初始化、个人资料
- ✅ hr: Agent CRUD、Agent 状态更新、Skill 管理、工具包管理、技能包管理
- ✅ finance: Model Provider、MessageChannel、Tool、Attachment、MCP Server、MCP Tool 管理
- ✅ project: Project CRUD、Task CRUD、Artifact 管理、统一状态更新、任务进度追踪
- ✅ user: 个人资料查看/修改
- ✅ health: 健康检查
- ✅ system: Cron Trigger 管理

---

## ✅ Consumer 层（100% 完成）

**状态**: 🎉 通用消费者框架 + Message Topic 三层分发全部完成

- **GenericConsumer** 泛型框架：适配任意事件类型
- **Message Topic 三层分发**：按 `to_role` 分发到 Agent/User/System Handler
- **崩溃恢复**：服务启动自动从数据库恢复 pending 事件
- **优先级排序**：按 `priority DESC, created_at ASC` 排序

---

## 🏗️ 分层架构执行质量审计

### ✅ 做得好的地方
1. **严格分层执行到位**: 所有 Handler 都通过 Domain 层调用，**没有直接调用 DAL/DAO**
2. **依赖方向正确**: 上层依赖下层，没有循环依赖
3. **DAO 层覆盖率 100%**: 28 个 DAO 全部有对应的 DAL 使用者
4. **DAL 层覆盖率 100%**: 17 个 DAL 全部有对应的 Domain 使用者
5. **依赖注入规范**: Domain 层通过 `Arc<dyn Trait>` 注入 DAL，符合 DIP 原则
6. **PO 边界严格**: PO 仅在 DAO/DAL 层内部使用，Domain 层及以上零 PO 依赖

---

## 📈 完成度里程碑跟踪

| 层级 | 历史完成度 (2026-05-15) | 当前完成度 (2026-07-01) | 更新 (2026-07-12) | 状态 |
|------|------------------------|------------------------|-------------------|------|
| DAO 层 | 100% ✅ | 100% ✅ | 100% ✅ | 已达成（22 个模块） |
| DAL 层 | 85% ⚠️ | 100% ✅ | 100% ✅ | 已达成（16 个模块） |
| Domain 层 | 50% ⚠️ | 100% ✅ | 100% ✅ | 已达成（6 个领域） |
| Handler 层 | ~50% ⚠️ | ~95% ✅ | ~95% ✅ | 基本达成（6 大业务域） |
| Consumer 层 | - | 100% ✅ | 100% ✅ | 已达成 |
| Frontend 层 | - | - | ✅ 已重构 | 新增（Dioxus Router + 13 页面） |
| **整体** | **~71%** | **~92%** | **~98%** | 前端重构完成 |

---

## 🔍 关键架构决策记录

1. ✅ **严格分层原则**: Handler → Domain → DAL → DAO，禁止跨层调用
   - 当前执行情况: 100% 遵守，没有发现绕过 Domain 的调用

2. ✅ **DAO 职责单一**: 每个 DAO 只负责一个实体的持久化
   - 当前执行情况: 22 个 DAO 职责清晰

3. ✅ **Domain 全覆盖**: 每个 DAL 都有对应的 Domain 业务承载
   - 当前执行情况: 16/16 DAL 全部有业务承载

4. ✅ **面向接口编程**: Domain 通过 Trait 依赖 DAL，DAL 通过 Trait 依赖 DAO
   - 当前执行情况: 已实现，使用 `Arc<dyn XxxDal + Send + Sync>` 注入

5. ✅ **状态流转语义归属 Domain**: 管理面统一暴露 `/status` action，但状态合法性、状态流转和副作用由 Domain 统一入口承担
   - 当前执行情况: Agent/Project/Task 已按该模式落地

6. ✅ **Skill 业务实体边界收敛**: Domain / DAL 对上层优先暴露完整 `Skill` 业务实体，`SkillPo` 保持在 DAO/存储映射边界内

7. ✅ **MCP 服务器完整集成**: MCP 服务器 CRUD、工具同步、MCP 工具调用执行全链路打通

8. ✅ **统一附件存储**: 通用 Attachment 上传 API，消息附件和项目产物统一存储，FileMeta + 日期分层路径

---

## 🎨 第五层：Frontend 前端层（2026-07-12 重构完成）

**快照日期**: 2026-07-12
**状态**: 🎉 前端架构重构完成，全栈架构闭环达成

### 重构概览

2026-07-12 完成前端大规模重构，从前端零散状态升级为完整的 Dioxus 0.7 WebAssembly 前端架构，与后端 Handler API 形成全栈闭环。

### 架构组成

| 模块 | 状态 | 说明 |
|------|------|------|
| 🧭 Dioxus Router | ✅ | 15 条路由，替代旧的 signal 状态机，声明式路由导航 |
| 🎨 Mistral CSS 设计系统 | ✅ | CSS 变量 + 组件类，落地于 `frontend/index.html`，对应 `docs/ui_design_system.md` 规范 |
| 🔗 统一 API 客户端 | ✅ | OnceLock 单例 + JWT 自动注入 + 类型化 helper，消除重复样板代码 |
| 🔐 全局认证状态管理 | ✅ | AuthState + token localStorage 持久化，登录态全局共享 |
| 📦 业务域 API 客户端 | ✅ | 7 个业务域：auth/organization/hr/finance/project/message/system |
| 🧱 基础 UI 组件库 | ✅ | Button / Modal / State alerts 等通用组件 |
| 🏛️ 布局组件 | ✅ | Navbar（Router Link 集成）+ AppLayout 统一页面骨架 |
| 📄 CRUD 页面 | ✅ | 13 个页面，覆盖 organization/hr/finance/project/message/system/user 六大域 |

### 业务域页面覆盖

| 业务域 | 页面数 | 覆盖能力 |
|--------|--------|----------|
| organization | ✅ | 组织管理、用户管理 |
| hr | ✅ | Agent CRUD、Skill 管理、工具包/技能包管理 |
| finance | ✅ | Model Provider、Tool、Message Channel 管理 |
| project | ✅ | Project / Task CRUD、项目详情、任务列表 |
| message | ✅ | **对话功能 MVP**（左右分栏、消息气泡、双向分页、3秒轮询） |
| system | ✅ | Cron Trigger 管理 |
| user | ✅ | 个人资料（表单加载 + 保存） |

### 前端页面功能完成度（2026-07-13 更新）

| 页面 | 功能 | 状态 |
|------|------|------|
| **MessageChat** | 左右分栏布局、消息气泡展示、双向分页、3秒短轮询 | ✅ 完整 |
| **HrAgentDetail** | Agent 基本信息、状态管理、工具包管理、技能包管理 | ✅ 完整 |
| **ProjectDetail** | 项目基本信息、状态管理、任务列表 | ✅ 完整 |
| **HrSkills** | 技能列表、标签展示、创建 Modal、删除 | ✅ 完整 |
| **SystemTriggers** | 触发器列表、暂停/恢复、创建 Modal、删除 | ✅ 完整 |
| **FinanceMessageChannels** | 渠道列表、启用/禁用、创建 Modal、删除 | ✅ 完整 |
| **HrAgents** | Agent 列表、创建 Modal、模型提供商下拉选择 | ✅ 完整 |
| **ProjectList** | 项目列表、状态徽章、任务数统计 | ✅ 完整 |
| **UserProfile** | 个人信息表单、保存按钮接通 API | ✅ 完整 |

### 关键架构决策

1. ✅ **Dioxus Router 替代 signal 状态机**: 路由声明式化，URL 与页面状态同步，支持浏览器前进/后退
2. ✅ **Mistral CSS 设计系统落地**: 设计规范从文档（`docs/ui_design_system.md`）转化为可执行的 CSS 变量与组件类（`frontend/index.html`）
3. ✅ **统一 API 客户端**: OnceLock 单例避免重复实例化，JWT 自动注入保证认证一致性
4. ✅ **全局认证状态**: AuthState + localStorage 实现登录态跨页面持久化
5. ✅ **业务域 API 客户端拆分**: 按后端 Handler 域划分 7 个客户端，与后端分层一一对应

### 相关文档

- 前端架构详解: `docs/frontend_architecture.md`
- 设计系统规范: `docs/ui_design_system.md`
- 设计系统实现: `frontend/index.html`

---

*本文档为架构现状快照，每次完成阶段性优化后更新*
