# A2A Server 架构设计（agent 与 project 维度分离）

> 🎯 **本文档定位**：ai_orz 作为 A2A Server 的设计决策大纲（核心是「agent 与 project 是两个维度」分离原则、统一路由方法 resolve_agent、异步唤醒链路复用；实现细节读代码和原 archive 文档）
>
> 状态：定稿（2026-07-20 功能落地；与 docs/archive/a2a_server_design.md 早期 REST 方案对比，本设计为最终 A2A Protocol 版）
>
> 查阅场景：理解 A2A 异步 tasks/send→tasks/get 轮询流程、前端 Chat 默认对话框/Project 对话框双通道设计、新增对外协议端点时打开。
>
> 关联文档：
> - [external_agent_design.md](./external_agent_design.md) — 反向：ai_orz 作为客户端调用外部 A2A Server 的设计（双向对称）
> - [a2a_server_design.md](../archive/a2a_server_design.md) — 归档：早期 RESTful Callback 方案（已废弃，保留对比）
> - [consumer_architecture.md](./consumer_architecture.md) — 事件消费架构（A2A 异步唤醒复用）
> - [AGENTS.md](../../AGENTS.md) §3.2：Handler 层 mapper 转换约定
> - 【② Plan 落地】（占位：待 ai-orz-doc-maintainer 落地后回填真实 Plan 路径）
> - 【③ Wiki 长文】[API 参考.md](docs/wiki/zh/content/API 参考/API 参考.md) — §A2A 协议接口完整时序图与路由表
> - 【③ Wiki 长文】[Agent 发现机制.md](docs/wiki/zh/content/API 参考/A2A 协议/Agent 发现机制.md) — /.well-known/agent.json 公开端点说明
> - 【③ Wiki 长文】[Agent卡片发现.md](docs/wiki/zh/content/核心模块/处理器层/A2A协议处理器/Agent卡片发现.md) — Handler 层 agent_card 端点
> - 【③ Wiki 长文】[AI Agent 管理.md](docs/wiki/zh/content/功能模块/AI Agent 管理/AI Agent 管理.md) — 外部 Agent（Remote/Cli）管理与配置
> - 【④ RAG 卡 3 张】
>   - [A2A 协议层：AgentCard 发现 + JSON-RPC 2.0 + A2aTask 任务状态机 + A2aMessage 双向消息](docs/wiki/knowledge/zh/A2A%20协议层：AgentCard%20发现%20+%20JSON-RPC%202.0%20+%20A2aTask%20任务状态机%20+%20A2aMessage%20双向消息/A2A%20协议层：AgentCard%20发现%20+%20JSON-RPC%202.0%20+%20A2aTask%20任务状态机%20+%20A2aMessage%20双向消息.md) — common DTO 单一事实源
>   - [A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建](docs/wiki/knowledge/zh/A2A%20Server%20Handler%20层：JSON-RPC%20方法路由%20+%20公开无鉴权路由%20+%20notification_url%20回调渠道自动创建/A2A%20Server%20Handler%20层：JSON-RPC%20方法路由%20+%20公开无鉴权路由%20+%20notification_url%20回调渠道自动创建.md) — 公开路由 + 7 Handler 分发
>   - [A2A Client + 外部 Agent Runtime：A2aRuntimeDao HTTP 调用 + ExternalCortexDao 桥接 + A2aCallbackDao Push 推送](docs/wiki/knowledge/zh/A2A%20Client%20+%20外部%20Agent%20Runtime：A2aRuntimeDao%20HTTP%20调用%20+%20ExternalCortexDao%20桥接%20+%20A2aCallbackDao%20Push%20推送/A2A%20Client%20+%20外部%20Agent%20Runtime：A2aRuntimeDao%20HTTP%20调用%20+%20ExternalCortexDao%20桥接%20+%20A2aCallbackDao%20Push%20推送.md) — 出站 A2A Client + 外部 Agent 桥接统一 think()

---

## 一、设计目标与关键决策

### 目标

让 ai_orz 作为 A2A Server 对外暴露 JSON-RPC 2.0 端点（`.well-known/agent.json` + `/a2a`），外部注册用户持 JWT 通过 A2A 协议调度前台 Agent 处理任务；同步修复 Chat 页面默认对话框需先建 project 的设计缺陷。

### 关键决策表

| # | 决策问题 | 选择方案 | 选择原因 |
|---|---------|---------|---------|
| 1 | A2A 协议传输 | **JSON-RPC 2.0 over HTTP POST + JWT Cookie 认证** | 标准互操作性；复用现有用户 JWT（无需单独 A2A 认证体系）；与飞书/前端聊天共用唤醒链路 |
| 2 | **核心理念：agent 与 project 的关系** | **两个独立维度，不在 Domain 层融合** | agent=「人」（前台接待/PM/专家）；project=「工作上下文」（一次临时任务、一个长期项目）；同一 agent 可同时处理多个 project；resolve_agent 只接受 ctx 不耦合 project |
| 3 | 统一路由位置 | **HrDomain::resolve_agent(ctx) -> Option<Agent>** | Chat 三场景（飞书 IM / 默认对话框 / Project 对话框）+ A2A tasks/send 共用同一路由方法；handler 层按需自行组合两个维度 |
| 4 | tasks/send 执行模式 | **纯异步：handler 创建 project+message 立即返回 Working，consumer 异步唤醒闭环** | 与飞书/前端聊天唤醒链路 100% 同构；无需为 A2A 新增后台执行机制；客户端轮询 tasks/get 直到完成 |
| 5 | A2A ↔ ai_orz 转换层位置 | **Handler 层 `mapper.rs` 纯函数，Domain 层零感知 A2A** | A2A 协议变更不改业务逻辑；协议实体和业务实体解耦；AGENTS §3.2 规范要求 |
| 6 | Chat 默认对话框语义 | **=与前台 Agent 直接沟通（不创建 project）**；Project 对话框=有上下文的协作（有 project）** | 协作关系类比：前台接待直接答疑 vs PMO 建档跟进；Project 创建由 Agent 内部决策触发（复杂需求），用户不被迫建 project |
| 7 | 新增数据库表 | **无新增表**：A2A Task ↔ Project；A2A Message ↔ MessagePo | 复用现有生命周期管理（InProgress/Completed/Archived 状态机、消息 SSE、文件上传）；迁移风险为 0 |
| 8 | to_agent_id 参数 | **保持 Option<String>**：用户选定显式传入 > project.owner_agent_id > resolve_agent(ctx) 兜底 | 三种场景统一兜底链；任何一层都不强制要求必须指定 agent |

### 协作关系类比（一句话贯穿所有设计）

> **默认对话框 = 去公司前台直接沟通（无档案，简单问题当场答复）；Project 对话框 = 前台识别需求复杂后建「工作档案」，PM 跟进协作。**

---

## 二、架构思路

### 2.1 统一路由兜底链（所有场景共用 resolve_agent）

```
用户调用端                    Handler 层组合两个维度              Domain 层原子能力
───────────                  ──────────────────────            ───────────────────
飞书 IM 新消息            ──► resolve_agent(ctx) 兜底 ────────► HrDomain::resolve_agent
Chat 默认对话框(无project) ──► to_agent_id? > resolve_agent ──► (只接受 ctx，不感知 project)
Chat Project 对话框        ──► project.owner_agent_id? >
                             resolve_agent 兜底
A2A tasks/send            ──► resolve_agent(ctx) 取 agent
                             └─► 创建 project 绑定 agent.id ──► ProjectManage::create(owner_agent_id)
                                                           └─► Message 创建入队
                                                                   │
                                                                   ▼
                                                          Consumer 异步唤醒（复用链路）
                                                          ├─ Project 启动 InProgress
                                                          ├─ Agent awaken + 思考循环
                                                          └─ 消息 SSE / 状态流转
                                                                   │
                                                                   ▼
A2A 客户端轮询 ◄──────── tasks/get 查询 project 当前状态 ◄── 状态映射:
  (tasks/get 轮询)                                           Active/PendingReview → Submitted
                                                            InProgress → Working
                                                            Completed → Completed
                                                            Archived → Canceled
                                                            Deleted → Failed
```

### 2.2 分层与依赖方向（Domain 零侵入红线）

```
common/api/a2a.rs  ── A2A 协议实体（AgentCard/JsonRpc/A2aTask...）
        ▲
        │ 纯函数转换
        ▼
src/handlers/a2a/mapper.rs  ← 转换层纯函数（project_status_to_a2a_state 等）
        ▲
        │
        ├─ agent_card.rs  ── GET /.well-known/agent.json（公开）
        ├─ jsonrpc.rs      ── POST /a2a JSON-RPC 分发（JWT 保护）
        │    ├─ send_task.rs ─ tasks/send 异步提交
        │    ├─ get_task.rs  ─ tasks/get 轮询
        │    └─ cancel_task.rs─ tasks/cancel 取消
        │
        ▼
   HrDomain.resolve_agent(ctx)  ─┬─ ProjectManage.create / start
        ▲                        ├─ MessageManage.send_to_agent
        │ （只接受 ctx，不感知 A2A）
   业务 Domain 层（project/message/runtime）— 零改动、零 A2A 感知
```

---

## 三、涉及文件清单

### 新增文件（12 个）

| 文件 | 角色 |
|------|------|
| [common/src/api/a2a.rs](../../common/src/api/a2a.rs) | A2A 协议实体定义 |
| [src/handlers/a2a/mod.rs](../../src/handlers/a2a/mod.rs) | A2A handler 模块入口 |
| [src/handlers/a2a/mapper.rs](../../src/handlers/a2a/mapper.rs) | 转换层纯函数（4 个：状态/消息/产物/构建 Task） |
| [src/handlers/a2a/agent_card.rs](../../src/handlers/a2a/agent_card.rs) | GET `/.well-known/agent.json` |
| [src/handlers/a2a/jsonrpc.rs](../../src/handlers/a2a/jsonrpc.rs) | POST `/a2a` JSON-RPC 入口 + 方法分发 |
| [src/handlers/a2a/send_task.rs](../../src/handlers/a2a/send_task.rs) | `tasks/send`：resolve_agent → 建 project → 入队消息 → 返回 Working |
| [src/handlers/a2a/get_task.rs](../../src/handlers/a2a/get_task.rs) | `tasks/get`：查 project+messages+artifacts → build_a2a_task |
| [src/handlers/a2a/cancel_task.rs](../../src/handlers/a2a/cancel_task.rs) | `tasks/cancel`：archive project → 重新查构建 |
| [src/handlers/hr/agent/get_reception_agent.rs](../../src/handlers/hr/agent/get_reception_agent.rs) | GET `/api/v1/hr/agents/reception` 前台 Agent 查询 |

### 修改文件（核心 5 个，体现维度分离原则）

| 文件 | 关键改动 |
|------|---------|
| [src/service/domain/hr/mod.rs](../../src/service/domain/hr/mod.rs) | `HrDomain` trait + `#[async_trait]`，**新增 `resolve_agent(ctx)` 方法，签名不接受 project 参数**；优先级 feishu_reception → 任意 Onboarded |
| [src/handlers/project/project/create_project.rs](../../src/handlers/project/project/create_project.rs) | **纯粹透传 `params.owner_agent_id`，不调 resolve_agent，不依赖 hr domain**（纯 HTTP 内部用） |
| [src/handlers/finance/message/send_message_to_agent.rs](../../src/handlers/finance/message/send_message_to_agent.rs) | 支持两种对话上下文：to_agent_id > project.owner_agent_id > resolve_agent 兜底 |
| [common/src/api/project.rs](../../common/src/api/project.rs) | `CreateProjectRequest` 新增 `owner_agent_id: Option<String>` |
| [src/router.rs](../../src/router.rs) | 注册 `.well-known/agent.json`（公开）和 `/a2a`（JWT 保护）两条路由；JWT 中间件必须在 RequestContext 外层 |
| [frontend/src/pages/message/chat.rs](../../frontend/src/pages/message/chat.rs) | **默认对话框不创建 project**；侧边栏置顶「默认对话」条目；顶部显示当前前台 Agent 名 |

### 零改动面（验证架构稳定性）

- `src/service/domain/message/` / `src/service/domain/runtime/`：业务逻辑零改动（Trait 签名修改除外）
- 数据库 schema：零新增表；所有状态/数据复用 Project + Message + Artifact
- 飞书 IM 消费者、前端 Chat SSE 推送链路：行为不变（仅复用）

---

## 四、关键边界（架构红线）

1. **resolve_agent 签名红线**：`fn resolve_agent(ctx) -> Result<Option<Agent>>`，只接受 ctx，**永远不新增 project_id 参数**（违反即破坏维度分离）
2. **create_project handler 红线**：纯粹透传 `owner_agent_id`，**禁止内部调用 resolve_agent**，禁止依赖 HrDomain（创建 project 是档案建档，agent 是另一个维度的概念）
3. **tasks/send 不直接唤醒**：handler 层只创建 project+message 立即返回 Working；**禁止调用 wake_agent_brain / awaken**；唤醒由 consumer 异步闭环
4. **Mapper 纯函数红线**：mapper.rs 不导入 DAO/DAL/Domain；只做字段级转换；所有数据库操作在 handler 外层完成后再调用 mapper 构建响应
5. **默认对话框不强制建 project**：Chat 页面首次加载时，禁止自动创建「默认 project」；project 必须由用户主动新建（前端弹窗）或 Agent 内部决策触发（A2A tasks/send 创建）

---

## 五、扩展模式

### 场景 1：新增 A2A 方法（如 tasks/sendSubscribe SSE 流式、PushNotifications 推送）

1. **协议实体层**：[common/src/api/a2a.rs](../../common/src/api/a2a.rs) 新增对应 Params/Response 结构体
2. **Handler 层**：`src/handlers/a2a/` 新增 `send_subscribe.rs`（或其他），业务逻辑复用现有异步链路
3. **JSON-RPC 分发**：[jsonrpc.rs](../../src/handlers/a2a/jsonrpc.rs) 的 `match method` 追加新分支，错误处理模板与现有三方法一致
4. **Mapper 层**（若需）：[mapper.rs](../../src/handlers/a2a/mapper.rs) 新增纯函数转换，禁止在 handler 内 inline 构造响应

### 场景 2：resolve_agent 新增路由优先级（如「指定组织专属前台」）

1. **Domain 层**：[hr/mod.rs](../../src/service/domain/hr/mod.rs) 的 resolve_agent 内部优先级列表新增一级（如 Priority 0.5：org_reception 角色）
2. **所有调用方自动生效**：飞书 IM / Chat 默认对话框 / A2A / send_message_to_agent 兜底 4 个场景**无需改动任何 handler 代码**（统一路由的收益）

### 场景 3：新增异步对外端点（如「Webhook 订阅」「Zapier 集成」）

复用 tasks/send 相同的 handler 设计模式：**Handler 只入队（创建 Project+Message）立即返回，唤醒异步闭环 → 轮询查询（或 Webhook 回调）取结果**。统一的异步模式保证了对外接口的一致性和稳定性。
