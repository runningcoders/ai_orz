# 🏗️ 分层架构现状报告

> 🎯 **本文档定位**：规划与落地结果快照（阶段性架构审计快照，不追赶现状；当前最新分层以 ARCHITECTURE.md 为准）
>
> **文档状态**：完成（快照日期 2026-07-25）
>
> 查阅场景：
> - 对比 2026-05-15 至 2026-07-25 三层架构从 ~71% → ~99% 的演进里程碑
> - 追溯 DAO/DAL/Domain 四层全覆盖达成的时间节点与决策记录
> - 审计早期分层执行质量（如 Handler 严格走 Domain 是否 100% 遵守）
>
> 关联文档：
> - [ARCHITECTURE.md](../ARCHITECTURE.md) — 唯一权威架构总纲（当前生效版本）
> - [LAYERED_ARCHITECTURE_PRACTICE.md](LAYERED_ARCHITECTURE_PRACTICE.md) — 分层实操手册（AGENTS.md §3.1 配套）
> - [AGENTS.md](../../AGENTS.md) — 强制开发规范 §3.1 代码分层架构

---

## 一、目标（为什么做）

2026-07-25 节点做一次阶段性分层架构全面审计，确认「从下往上打牢每一层」的推进节奏是否达标，识别残留盲区（如 Handler 是否有绕过 Domain 直达 DAL 的情况）。

| 问题维度 | 解决方式 |
|---------|---------|
| DAO 层是否全部被 DAL 使用（无闲置） | 逐一核对 25 个 DAO 模块的 DAL 引用情况，形成覆盖表 |
| DAL 层是否全部被 Domain 使用（无闲置） | 逐一核对 23 个 DAL 模块的 Domain 引用情况 |
| Domain 层是否 7 领域全覆盖 | 核对 finance/hr/organization/message/project/runtime/system 领域 API 完整度 |
| Handler 层是否存在跨层调用（绕过 Domain） | 全局 grep Handler→DAL/DAO 直接引用，确认 100% 走 Domain |
| 前端架构从零散状态到完整全栈闭环的里程碑 | 记录 Dioxus Router + Tailwind v4 + DaisyUI v5 的落地状态 |

**收敛后效果**：DAO/DAL/Domain 三层覆盖率 100%，Handler 跨层调用 0 例，整体架构完成度从 ~71% 推进到 ~99%，形成完整质量审计基线。

---

## 二、架构思路（怎么做的）

整体金字塔从下往上建设，每一层打牢再上一层：

```
            ┌─────────────────┐
            │   Handler API   │  前端接入层（~95%，8 大业务域 API）
            │   （完成 ~95%） │  含 a2a 公开回调端点
            └────────┬────────┘
                     │ 严格单向，只调 Domain
            ┌────────▼────────┐
            │   Domain 领域   │  7 大业务领域（100%）
            │   （完成 100%） │  finance/hr/organization/
            └────────┬────────┘  message/project/runtime/system
                     │ 只调 DAL，产生内部事件
            ┌────────▼────────┐
            │   DAL 数据访问  │  23 个模块（100%）
            │   （完成 100%） │  PO↔Entity 转换 + 组合多个 DAO
            └────────┬────────┘
                     │ 只调 DAO，无同层互调
            ┌────────▼────────┐
            │   DAO 持久化    │  25 个模块（100%）
            │   （完成 100%） │  18 核心 DAO + 5 渠道 DAO
            └─────────────────┘   + a2a 回调 + 触发器 + SSE 推送

Consumer 层（100%，横切）：GenericConsumer 框架 + Message Topic 三层分发
Frontend 层（后续新增）：Dioxus 0.7 + Tailwind CSS v4 + DaisyUI v5，41 路由 30+ 主题
```

**关键边界 / 行为红线（回归必保）**：
1. **严格单向调用**：Handler → Domain → DAL → DAO，跨层调用 0 例（本快照审计结果）
2. **依赖方向正确**：上层依赖下层，无循环依赖；Domain 经 `Arc<dyn Trait>` 注入 DAL（DIP 原则）
3. **PO 边界严格**：PO 仅在 DAO/DAL 层内部使用，Domain 及以上零 PO 依赖
4. **DAO 职责单一**：每个 DAO 只负责一个实体的持久化，无 DAO→DAO 互调
5. **Handler 不承载业务**：状态流转语义统一归属 Domain，Handler 仅做请求级编排

---

## 三、涉及文件清单（读代码直接跳）

本快照为审计结果，文件清单按分层罗列（当时覆盖的全部模块）：

| 文件 | 角色 | 摘要 |
|------|------|------|
| **DAO 层（25 个，全覆盖）** | | |
| [src/service/dao/](src/service/dao/) | DAO 根目录 | agent/artifact/attachment/brain/cron_trigger/lark/mcp_server/memory/message/model_provider/project/skill/task/tool/tool_call/user 等 25 模块 |
| [src/service/dao/a2a_callback/](src/service/dao/a2a_callback/) | A2A 回调 DAO | A2A 异步回调 Push 端点（独立 DAO 模块） |
| [src/service/dao/lark/](src/service/dao/lark/) | 飞书渠道 DAO | HTTP + WebSocket P2P（5 渠道 DAO 之一） |
| **DAL 层（23 个，全覆盖）** | | |
| [src/service/dal/](src/service/dal/) | DAL 根目录 | agent/agent_a2a/artifact/attachment/backup/brain/cron_trigger/lark/log_query/mcp_server/mcp_tool/memory/message/message_channel/message_push/model_provider/organization/project/skill/task/tool/user 共 23 |
| **Domain 层（7 领域，全覆盖）** | | |
| [src/service/domain/finance/](src/service/domain/finance/) | finance 领域 | model_provider/attachment/mcp_server/mcp_tool/tool/message_channel |
| [src/service/domain/hr/](src/service/domain/hr/) | hr 领域 | agent 管理 / skill 管理 |
| [src/service/domain/organization/](src/service/domain/organization/) | organization | 组织/用户/认证 |
| [src/service/domain/message/](src/service/domain/message/) | message 领域 | 消息管理 + 5 出站渠道 + 飞书 P2P 入站 + SSE 推送 + 多渠道投递 |
| [src/service/domain/project/](src/service/domain/project/) | project 领域 | Project/Task/Artifact 管理 |
| [src/service/domain/runtime/](src/service/domain/runtime/) | runtime 领域 | Agent 唤醒、工具执行、记忆读写（内部调用） |
| [src/service/domain/system/](src/service/domain/system/) | system 领域 | Cron Trigger 管理、AOP 监控、备份恢复、日志查询 |
| **Consumer 层（100%）** | | |
| [src/consumer/](src/consumer/) | Consumer 根目录 | GenericConsumer 泛型框架 + Message Topic 三层分发 + 崩溃恢复 + 优先级排序 |
| **Handler 层（~95%，8 大域 API）** | | |
| [src/handlers/](src/handlers/) | Handler 根目录 | organization/hr/finance/project/user/health/system/a2a 8 大业务域 |
| **Frontend 层（快照时已落地）** | | |
| [frontend/src/pages/](frontend/src/pages/) | 页面模块 | organization/hr/finance/project/message/system/user + workspace（30+ 页面） |
| [frontend/styles/input.css](frontend/styles/input.css) | 设计系统 | Tailwind CSS v4 主题配置 + DaisyUI v5 组件定制（orz-light 品牌主题） |

---

## 四、分发速查表（新增同类功能第一站）

### 4.1 新增 DAO 模块（接入新实体/外部渠道）

| 改动点 | 位置 | 新增时参考 |
|--------|------|-----------|
| 新增 DAO 目录 | `src/service/dao/<entity>/` 下新建 mod.rs + sqlite.rs（或外部 API 出站实现） | 参考 `src/service/dao/mcp_server/`（单一实体 CRUD 模式） |
| 注册 DAL 消费方 | `src/service/dal/<domain>/` 中注入新 DAO，`Arc<dyn XxxDao + Send + Sync>` | 参考 DAL 目录下任一现有模块依赖注入模式 |
| 注册 Domain 承载 | `src/service/domain/<领域>/` 中通过 DAL 间接调用，永不直调 DAO | 严格单向分层（参见 §二 架构图） |

> 代码入口：[service/dao/ 根目录](src/service/dao/)

### 4.2 新增 Handler API 接口

| 改动点 | 位置 | 新增时参考 |
|--------|------|-----------|
| Handler 目录 | `src/handlers/<业务域>/` 每接口一文件，命名 `<action>_<entity>.rs` | 参考 `src/handlers/hr/agent/` 目录结构（CRUD 6 文件模板） |
| 路由注册 | `src/router.rs` 对应业务域 nest 下追加 | 按领域分组，router 顺序注意避免路径遮蔽 |
| **禁止操作** | ❌ Handler 中直接 use service::dal / service::dao；永远只调 domain | 违反即破坏分层契约 |

> 代码入口：[handlers/ 根目录](src/handlers/)

---

## 五、验收清单（2026-07-25 全部达成 ✅）

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要（2026-07-25 快照数据）

| 层级 | 历史完成度 (2026-05-15) | 当前完成度 (2026-07-25) | 状态 |
|------|------------------------|------------------------|------|
| DAO 层 | 100% ✅ | 100% ✅ | 已达成（25 个模块） |
| DAL 层 | 85% ⚠️ | 100% ✅ | 已达成（23 个模块） |
| Domain 层 | 50% ⚠️ | 100% ✅ | 已达成（7 个领域含 system） |
| Handler 层 | ~50% ⚠️ | ~95% ✅ | 基本达成（8 大业务域含 a2a 回调） |
| Consumer 层 | — | 100% ✅ | 已达成 |
| Frontend 层 | — | ✅ 已重构 | Tailwind v4 + DaisyUI v5 迁移完成（41 路由） |
| **整体** | **~71%** | **~99%** | 全栈架构闭环达成 |

### 与计划的偏离（如有）
1. Frontend 层为 2026-07-12 后新增（本轮规划外），后续快照已纳入整体完成度统计
2. Handler 层未达 100% 判定为正常（部分扩展接口随功能迭代自然补齐，不阻塞主流程）

---

## 七、后续扩展路径（4 步模板）

> **核心不变量**：严格单向分层架构、DAO/DAL/Domain 三层覆盖率 100% 的底线不突破；Handler 永不直调 DAL/DAO。

1. **新实体接入链路**：[src/service/dao/](src/service/dao/) → [src/service/dal/](src/service/dal/) → [src/service/domain/](src/service/domain/) → [src/handlers/](src/handlers/)，严格按 DAO→DAL→Domain→Handler 顺序逐级接入
2. **新 Handler 接口**：复制 [src/handlers/hr/agent/](src/handlers/hr/agent/) 5 文件 CRUD 模板，永远只调对应 Domain
3. **前端新增页面**：[frontend/src/pages/](frontend/src/pages/) 按业务域目录分组，API 客户端调用 [frontend/src/api/](frontend/src/api/) 对应域
4. **下次架构审计节点**：每完成一轮重大功能迭代（如新增业务域），按本报告同款结构生成新快照，对比完成度演进曲线

