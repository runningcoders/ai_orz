# AI Orz - Code Wiki

> 🎯 **项目代码全景指南**：完整架构说明、模块职责、关键类与函数、依赖关系、运行方式
>
> 最后更新：2026-07-25

---

## 目录

1. [项目概览](#项目概览)
2. [整体架构](#整体架构)
3. [核心模块职责](#核心模块职责)
4. [关键类与函数说明](#关键类与函数说明)
5. [依赖关系](#依赖关系)
6. [项目运行方式](#项目运行方式)
7. [测试体系](#测试体系)
8. [开发规范](#开发规范)

---

## 项目概览

### 项目定位

**AI Orz** - 全栈 Rust 多 Agent 协作框架，以组织化形式管理和执行 AI 代理任务。

### 技术栈

| 层级 | 技术选型 | 说明 |
|------|----------|------|
| **后端框架** | Axum 0.8 | 高性能异步 Web 框架 |
| **数据库** | SQLite + SQLx 0.8 | 嵌入式数据库，类型安全查询 |
| **LLM 调用** | 原生 CortexDao | OpenAI 兼容 API 直接 HTTP 调用（OpenAiCompatibleCortexDao） |
| **前端框架** | Dioxus 0.7 + Tailwind CSS v4 + DaisyUI v5 | Rust WebAssembly 前端框架 + 组件库 + 30+ 主题切换 |
| **向量搜索** | LanceDB 默认 + HNSW/inMemory/SqliteVss | 多后端向量搜索，Vectorizable trait + embed_entity |
| **异步运行时** | Tokio | 高性能异步运行时 |
| **序列化** | serde + serde_json | 数据序列化/反序列化 |
| **日志系统** | tracing + tracing-subscriber | 结构化日志 |

### 已实现功能

| 功能模块 | 状态 | 说明 |
|---------|------|------|
| 组织用户权限 | ✅ | 多级组织、用户角色、JWT Cookie 认证（双模式：Cookie 浏览器 + Bearer token API） |
| Agent 全生命周期 | ✅ | 创建、配置、工具绑定、唤醒执行 |
| 四层记忆系统 | ✅ | Core/Working/Short-term/Long-term，FTS5 + 向量混合搜索 |
| 消息对话系统 | ✅ | 用户 ↔ Agent 双向对话，支持项目上下文，消息链（root_id + reply_to_id） |
| 消息渠道系统 | ✅ | 多渠道消息接入，支持启用/禁用/测试，飞书 P2P 私信 WebSocket 入站长连接已上线 |
| A2A 外部 Agent | ✅ | A2A 协议支持：Client（注册外部 CLI/Remote Agent）、Server（对外暴露端点）、异步结果回传（Push 回调 + 30 秒轮询兜底） |
| 统一工具调用架构 | ✅ | execute_auto / execute_manual 三层分发，Manual 通过 internal 工具转发 |
| 技能库系统 | ✅ | 可复用技能和工作流，支持搜索和分类，技能包机制 |
| 任务 + 项目管理 | ✅ | 任务状态机，项目聚合对话上下文，任务进度追踪（0-100） |
| 统一附件存储 | ✅ | 消息附件 + 项目产物，FileMeta + 日期分层路径 |
| MCP 服务器集成 | ✅ | MCP 服务器管理、工具同步、MCP 工具调用执行 |
| 异步消费者系统 | ✅ | 通用消费者框架 + Message Topic 三层分发 |
| 结构化日志系统 | ✅ | JSON 格式、自动上下文关联、日志自动清理 |
| 向量搜索 | ✅ | LanceDB 默认 + HNSW/inMemory/SqliteVss 多后端、Vectorizable trait + embed_entity |
| 全文搜索 | ✅ | FTS5 + trigram 分词器，支持中文全文搜索、BM25 相关性排序 |
| Agent 统计系统 | ✅ | DuckDB 多维统计、Agent/Project/Task/ModelProvider/Tool 五维度覆盖 |
| 多回合循环控制 | ✅ | 轮次限制检查、任务完成检测、Prompt 上下文差异化、工具失败计数注入 |
| 工具包机制 | ✅ | tag 分组工具、Agent 入职自动安装、免绑定校验三层逻辑 |
| 任务分配消息 | ✅ | TaskAssignment 消息类型、自动通知 Agent、神经工具封装 |
| 定时触发器系统 | ✅ | Cron Trigger 管理、后台扫描、事件投递、系统领域基础设施 |
| 记忆沉淀机制 | ✅ | Agent 休息与沉淀、短期记忆→长期知识图谱、定时触发沉淀 |
| 技能包机制 | ✅ | tag 分组技能、批量安装、安装即复制、卸载保留副本 |
| 综合搜索 | ✅ | FTS5 关键词 + 向量语义 + 图谱关系 三位一体混合搜索 |
| Agent 协作 | ✅ | search_agents 搜索、send_message_to_agent 消息、collaboration tag 分组工具 |
| AOP 事件中心 | ✅ | 纯框架（零业务依赖）、Event/Producer/Consumer/Registry 抽象、同步/异步消费模式 |
| SSE 消息推送 | ✅ | Server-Sent Events 长连接、订阅者模式、DAO 层连接管理、broadcast 广播 |
| 知识图谱可视化 | ✅ | Canvas HUD 驾驶舱风格（径向渐变 + 节点呼吸光晕 + 边流光）+ SVG 风格一键切换 |
| Toast 通知系统 | ✅ | 全局状态管理、4 种类型、滑入滑出动画、进度条倒计时 |
| Cookie 认证统一 | ✅ | HttpOnly Cookie + JWT；双模式（Cookie 浏览器 + Bearer token API 工具） |
| 前端架构 | ✅ | Dioxus Router 15 路由 + Tailwind CSS v4 + DaisyUI v5 + 30+ 主题切换 + 13 CRUD 页面 |

### 测试统计

| 指标 | 数值 | 说明 |
|------|------|------|
| **总测试数** | **830** | 后端 746 + 前端 34 + common 50，DAO + DAL + Domain + Handler + Pkg 完整覆盖 |
| **通过率** | **100%** | ✅ 全部测试通过 |
| DAO 模块数 | 25 个 | 全部实现并被使用，零闲置（18 核心 DAO + 5 渠道 DAO + a2a 回调 + 1 触发器 + 消息推送） |
| DAL 模块数 | 23 个 | 全部完整业务承载，零闲置（含 lark、agent_a2a、agent_codex、backup、log_query、message_push、mcp_tool、cron_trigger 等专属 DAL） |
| Domain 领域数 | 7 个 | 全部完整实现（含 SystemDomain） |
| Handler API 领域数 | 8 个 | organization, hr, finance, project, user, health, system, a2a（公开回调） |

---

## 整体架构

### 三级 Cargo Workspace 架构

```
ai_orz/
├── common/                     # 公共共享 crate（前后端共用）
│   ├── src/api/               # API 请求响应 DTO 按功能分组
│   ├── src/constants/         # 公共常量、基础类型
│   ├── src/enums/            # 公共枚举（UserRole、TaskStatus 等）
│   ├── src/error/            # 统一错误类型
│   └── config/               # 默认配置模板
│
├── src/                        # 后端服务
│   ├── handlers/              # HTTP 接口层（适配层：用户 API + 外部回调，按业务域分组）
│   │   └── a2a/               #   └─ A2A 公开回调端点（无 JWT 鉴权）
│   ├── producer/              # AOP 事件生产者（适配层：轮询 + 外部渠道 WS 事件接入）
│   ├── service/
│   │   ├── dao/               # 数据访问层 DAO（本地 DB CRUD + 外部 API 出站调用）
│   │   ├── dal/               # 业务数据访问层 DAL
│   │   └── domain/            # 领域层 Domain
│   ├── models/                # PO 持久化实体 + 业务实体 + 内部事件定义
│   ├── middleware/            # Axum 中间件
│   ├── consumer/              # 异步消费者系统（处理 Domain 产生的内部事件）
│   └── pkg/                   # 公共工具包
│
├── frontend/                   # Dioxus 前端（Tailwind CSS v4 + DaisyUI v5）
│   ├── src/api/               # API 客户端
│   ├── src/components/        # UI 组件（Button/Modal/Toast/State/Stats/Graph/GraphCanvas/Chat）
│   ├── src/hooks/             # 自定义 Hooks（use_resource/use_breakpoint/use_require_auth）
│   ├── src/layouts/           # 布局组件（AppLayout/Navbar）
│   ├── src/pages/             # 页面模块（按业务域分组）
│   ├── src/store/             # 状态管理（auth/toast）
│   └── src/utils/             # 通用工具函数（按功能分子模块：time/file/message/status）
│
├── ai-orz-macros/             # 自定义宏 crate
│   └── src/lib.rs             # 统一日志宏定义
│
└── docs/                       # 详细设计文档
```

### 严格分层架构

**核心原则：单向依赖，禁止跨层和同层互调**

```
Adapter (适配层)
    ├─ HTTP Handler（用户 API + 外部回调）
    └─ AOP Producer（轮询 + 外部 WS 事件接入）
    │ 只调用 Domain；负责协议解析、校验、ID 映射、DTO↔Command 转换
    ▼
Domain (领域层)
    │ 组合多个 DAL，实现业务逻辑
    ▼
DAL (业务数据层)
    │ 组合多个 DAO，提供业务级数据操作
    ▼
DAO (数据访问层)
    ├─ 本地 DB DAO：单一数据源 CRUD
    └─ 外部 API DAO：出站外部调用（如 LarkDao.push、A2aRuntimeDao.send_task）
    │
    ▼
Models (PO 持久化实体)
```

### 各层职责边界

| 层级 | 可以做 | 禁止做 |
|------|--------|--------|
| **DAO** | 单一/多个数据源访问；本地 DB：SQL 拼接、PO 读写；外部 API：出站调用、出站格式转换 | ❌ DAO 调 DAO、❌ 业务逻辑、❌ 实体组装/装饰 |
| **DAL** | 依赖多个 DAO、PO ↔ Entity 双向转换、业务级数据操作 | ❌ DAL 调 DAL |
| **Domain** | 依赖多个 DAL、核心业务逻辑编排、跨领域事务、产生内部事件 | ❌ Domain 调 Domain、❌ 直接调用 DAO（跨层）、❌ 直接调用外部 API |
| **Adapter（适配层）** | HTTP Handler（用户 API + 公开回调）、AOP Producer（WS/轮询）；协议解析、参数校验、鉴权、幂等检查；外部 ID ↔ 内部 ID 映射；DTO/外部结构 ↔ Command 转换 | ❌ 直接调用 DAL/DAO（跨层）、❌ 承载核心业务规则、❌ Handler/Producer 之间互调 |

**适配层核心认知**：HTTP Handler 是面向用户/前端的 Adapter，公开回调 Handler 是面向外部系统 HTTP 回调的 Adapter，AOP Producer 是面向外部 WS 事件/定时轮询的 Adapter——三者同属适配层，职责完全相同：把外部输入适配成 Domain 方法调用。Consumer 不在适配层（它处理 Domain 产生的内部事件）。出站外部调用统一封装在外部 DAO 中。

---

## 核心模块职责

### 1. 适配层（Handler + Producer）

**职责：HTTP 路由、参数校验、DTO 转换、响应组装；外部协议适配成 Domain 方法调用**

**模块组织：按业务域分组，每个方法一个文件**

```
src/handlers/
├── organization/           # 组织用户权限管理
│   ├── auth/               # 认证：登录、登出
│   ├── organization/       # 组织 CRUD
│   ├── organization_me/    # 当前组织信息
│   └── user/               # 用户 CRUD
│
├── hr/                     # 人力资源（Agent 管理）
│   ├── agent/              # Agent CRUD、状态更新
│   └── skill/              # 技能库 CRUD、安装
│
├── finance/                # 财务管理（基础设施）
│   ├── attachment/         # 附件上传、管理
│   ├── model_provider/     # LLM 模型配置管理
│   ├── message_channel/    # 消息渠道管理
│   ├── mcp_server/         # MCP 服务器管理
│   ├── mcp_tool/           # MCP 工具同步
│   └── tool/               # 工具 CRUD、绑定
│
├── project/                # 项目管理
│   ├── project/            # 项目 CRUD、状态更新
│   ├── task/               # 任务 CRUD、状态更新
│   └── artifact/           # 产物管理
│
├── user/                   # 用户个人中心
│   └── profile/            # 个人信息查看/修改
│
├── health/                 # 健康检查
├── system/                 # 系统管理（AOP 监控、日志查询、备份、Cron 触发器、Seed 配置迁移、后台任务管理）
└── a2a/                    # A2A 公开回调端点（无 JWT 鉴权，外部 HTTP 回调适配）
```

**AOP Producer 模块（适配层 - 外部 WS 事件/轮询接入）：**

```
src/producer/
└── A2aPollingProducer       # A2A 异步结果 30 秒轮询兜底
└（Lark WebSocket 等外部渠道 WS 事件接入）
```

**设计原则：**
- ✅ Handler 与用户 Action 直接对应，每个接口按需求完成请求级编排
- ✅ 复用优先通过组织 Command/Query 参数和调用 Domain 能力完成
- ❌ 不抽象 `BaseHandler` / `GenericActionHandler`
- ❌ 复杂业务规则、状态流转、权限语义必须下沉到 Domain
- ❌ 外部协议不进入事件中心，适配层直接调用 Domain 方法

### 2. Domain 层（领域逻辑层）

**职责：核心业务逻辑编排、跨领域事务协调**

**已实现领域：**

```
src/service/domain/
├── organization/           # 组织管理领域
│   ├── org.rs              # 组织 CRUD、初始化逻辑
│   └── user.rs             # 用户管理、权限校验
│
├── hr/                     # 人力资源领域
│   ├── agent.rs            # Agent 创建、配置、Brain 装配
│   └── skill.rs            # 技能库搜索、安装、文件管理
│
├── finance/                # 财务管理领域
│   ├── model_provider.rs   # 模型提供商管理、连接测试
│   ├── attachment.rs       # 附件上传、存储管理
│   ├── message_channel.rs  # 消息渠道配置、投递测试
│   ├── mcp_server.rs       # MCP 服务器生命周期
│   ├── mcp_tool.rs         # MCP 工具同步、管理
│   └── tool_provider.rs    # 工具注册、绑定、执行策略
│
├── message/                # 消息领域
│   ├── management.rs       # 消息创建、查询、管理
│   └── delivery.rs         # 多渠道消息投递、状态追踪
│
├── runtime/                # 运行时领域
│   ├── awakening.rs        # Agent 唤醒、按 control_mode 分发（execute_auto / execute_manual）
│   ├── context_assembly.rs # Prompt 上下文组装
│   ├── memory.rs           # 运行时记忆读写
│   ├── tool_execution.rs   # 工具执行、结果追踪
│   └── tool_call_query.rs  # 工具调用查询
│
├── project/                # 项目管理领域
│   ├── project.rs          # 项目生命周期、状态流转
│   ├── task.rs             # 任务分配、进度追踪
│   └── artifact.rs         # 产物创建、内容管理
│
└── system/                 # 系统领域（AOP 监控、日志查询、备份、后台任务注册中心访问）
    └── ...
```

**核心设计思想：**

| 领域 | 组合 DAL | 核心职责 |
|------|---------|---------|
| **Agent Domain** | AgentDal + ModelProviderDal + ToolDal + BrainDal | Agent 生命周期、Brain 装配、工具绑定 |
| **Message Domain** | MessageDal + MessageChannelDal + MessagePushDal | 消息管理 + 多渠道投递 + SSE 推送（事件由 AOP 事件中心处理） |
| **Runtime Domain** | MemoryDal + ToolCallDal + AgentDal | Agent 唤醒、工具执行、记忆读写 |
| **Project Domain** | ProjectDal + TaskDal + ArtifactDal | 项目聚合 Task，Task 聚合 Artifact |
| **System Domain** | LogQueryDal + StatsDao + BackupDal | 系统监控、日志查询、数据备份 |

### 3. DAL 层（业务数据层）

**职责：组合多个 DAO，PO ↔ 业务实体转换，业务级数据操作**

**已实现 DAL：**

```
src/service/dal/
├── agent.rs                # AgentDal：Agent + Brain + Memory 组合
├── brain.rs                # BrainDal：Cortex + Memory 组合
├── memory.rs               # MemoryDal：向量搜索 + SQLite 查询
├── message.rs              # MessageDal：消息 + 工具调用组合
├── message_channel.rs      # MessageChannelDal：多渠道配置管理
├── message_push.rs         # MessagePushDal：SSE 消息推送管理
├── model_provider.rs       # ModelProviderDal：模型配置 + 连接测试
├── mcp_server.rs           # McpServerDal：MCP 服务器管理
├── mcp_tool.rs             # McpToolDal：MCP 工具同步
├── tool.rs                 # ToolDal：工具注册 + Agent 绑定
├── organization.rs         # OrganizationDal：组织 CRUD
├── user.rs                 # UserDal：用户 CRUD + 权限校验
├── project.rs              # ProjectDal：项目 CRUD + 软删除
├── task.rs                 # TaskDal：任务 CRUD + Agent 分配
├── artifact.rs             # ArtifactDal：产物内容管理
├── skill.rs                # SkillDal：技能搜索 + 文件管理
├── attachment.rs           # AttachmentDal：附件上传 + 存储管理
├── lark.rs                 # LarkDal：飞书消息出站 + WebSocket 入站
├── agent_a2a.rs            # AgentA2aDal：A2A 外部 Agent 委派
├── agent_codex.rs          # AgentCodexDal：Codex CLI Agent 接入
├── backup.rs               # BackupDal：数据备份与恢复
├── log_query.rs            # LogQueryDal：日志在线查询
└── cron_trigger.rs         # CronTriggerDal：定时触发器管理
```

**设计范式：**

```rust
// ✅ 正确：写操作接收 &业务实体 引用
async fn create(&self, ctx: RequestContext, project: &Project) -> Result<(), AppError>;

// ✅ 正确：读操作返回 业务实体
async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>, AppError>;

// ✅ 正确：业务实体内部持有 PO
pub struct Project {
    pub po: ProjectPo,
    // 业务方法...
}
```

### 4. DAO 层（数据访问层）

**职责：单一数据源 CRUD，SQL 拼接，PO ↔ 数据库转换**

**已实现 DAO：**

```
src/service/dao/
├── agent/                  # AgentDao：Agent CRUD + 向量索引（Vectorizable）
├── artifact/               # ArtifactDao：Artifact CRUD
├── attachment/             # AttachmentDao：Attachment CRUD
├── brain/                  # BrainDao：Memory CRUD（JSONL + SQLite）
├── cortex/                 # CortexDao：LLM 调用接口 + 向量嵌入
│   ├── native/             # 原生 OpenAI 兼容实现（直接 HTTP 调用）
│   │   ├── openai.rs       # OpenAI 官方
│   │   ├── openai_compatible.rs  # DeepSeek/豆包/通义千问
│   │   └── ollama.rs       # Ollama 本地
│   └── mod.rs              # CortexDao trait 定义（向量存储后端见 pkg/storage）
│
├── mcp_server/             # McpServerDao：MCP 服务器 CRUD
├── memory/                 # MemoryDao：记忆 CRUD + 向量搜索
├── message/                # MessageDao：消息 CRUD
├── message_channel/        # MessageChannelDao：渠道配置 CRUD
├── message_push/           # MessagePushDao：SSE 推送连接管理
├── model_provider/         # ModelProviderDao：模型配置 CRUD
├── organization/           # OrganizationDao：组织 CRUD
├── project/                # ProjectDao：项目 CRUD
├── skill/                  # SkillDao：技能 CRUD + 向量搜索（Vectorizable）
├── task/                   # TaskDao：任务 CRUD + 向量索引（Vectorizable）
├── tool/                   # ToolDao：工具 CRUD + 向量搜索（Vectorizable）
├── tool_call/              # ToolCallDao：工具调用执行
│   ├── impl.rs             # 内置工具执行实现
│   ├── mcp.rs              # MCP 工具执行实现
│   └── mod.rs              # ToolCall trait 定义
│
├── tool_stats/             # ToolStatsDao：工具调用次数/失败次数查询
├── user/                   # UserDao：用户 CRUD
├── stats/                  # StatsDao：DuckDB 多维统计（Agent/Project/Task/ModelProvider/Tool）
├── a2a_callback/           # A2aCallbackDao：A2A 异步回调入站记录
├── agent_runtime/          # AgentRuntimeDao：外部 Agent 运行时
│   ├── codex.rs            # Codex CLI Agent 出站调用
│   └── a2a.rs              # A2A Remote Agent 出站调用（send_task 等）
│
├── cron_trigger/           # CronTriggerDao：定时触发器 CRUD + 后台扫描
│
└── external/               # 外部渠道 DAO（HTTP / WebSocket 实现）
    ├── email/              # EmailDao：邮件发送
    ├── slack/              # SlackDao：Slack 消息
    ├── lark/               # LarkDao：飞书消息（HTTP 出站 + WebSocket 入站长连接）
    ├── wechat/             # WechatDao：微信消息
    └── webhook/            # WebhookDao：HTTP 回调
```

> 💡 **注**：`event_queue/` DAO 已被 AOP 事件中心（`pkg/aop/`）取代，不再作为独立 DAO 存在。

**设计原则：**
- ✅ 单一数据源操作，不组合多个 DAO
- ✅ 仅操作 PO，不包含业务逻辑
- ✅ 外部 API DAO：出站外部调用、出站格式转换（如 Markdown→飞书卡片）
- ❌ DAO 层调用其他 DAO

### 5. Models 层（持久化实体）

**职责：定义所有 PO（持久化对象）和业务实体**

**核心实体：**

```
src/models/
├── agent.rs                # AgentPo + Agent 业务实体
├── brain.rs                # Brain + Cortex + CortexTrait
├── memory.rs               # MemoryTrace + MemoryPo + Memory
├── message.rs              # MessagePo + Message 业务实体
├── message_channel.rs      # MessageChannelPo
├── model_provider.rs       # ModelProviderPo + ModelProvider
├── organization.rs         # OrganizationPo
├── user.rs                 # UserPo + User
├── project.rs              # ProjectPo + Project 业务实体
├── task.rs                 # TaskPo + Task 业务实体
├── artifact.rs             # ArtifactPo + Artifact 业务实体
├── skill.rs                # SkillPo + Skill 业务实体
├── tool.rs                 # ToolPo + Tool 业务实体
├── attachment.rs           # AttachmentPo + Attachment
├── mcp_server.rs           # McpServerPo
├── event.rs                # Event trait 定义
├── file.rs                 # FileMeta 文件元数据
└── vector.rs               # SearchMatchInfo + Vectorizable trait 定义
```

### 6. Consumer 层（异步消费者）

**职责：从 AOP 事件队列消费 Domain 产生的内部事件，调用 Domain 执行业务逻辑**

```
src/consumer/
├── mod.rs                  # GenericConsumer 泛型框架
├── message.rs              # Message Topic 消费者
│   ├── AgentMessageHandler  # Agent 消息处理
│   ├── UserMessageHandler   # User 消息处理
│   └── SystemMessageHandler # System 消息处理
├── cron_trigger.rs         # CronTrigger Topic 消费者
└── tests.rs                # 消费者框架测试
```

**设计机制：**
- ✅ 泛型框架：`GenericConsumer<E, F, H>` 适配任意事件类型
- ✅ 三层分发：按 `to_role` 分发到 Agent/User/System Handler
- ✅ order_key 接收者优先策略：Agent→to_id，非 Agent→task_id→project_id，保证同一接收者事件顺序
- ✅ 崩溃恢复：服务启动自动从数据库恢复 pending 事件
- ✅ 优先级排序：按 `priority DESC, created_at ASC` 排序

### 7. Middleware 层（中间件）

**职责：请求预处理、认证、上下文注入**

```
src/middleware/
├── jwt_auth.rs             # JWT 认证中间件
├── request_context.rs      # RequestContext 注入中间件
└── mod.rs                  # 中间件导出
```

**RequestContext 结构：**

```rust
pub struct RequestContext {
    pub log_id: String,           // 日志追踪 ID
    pub user_id: Option<String>,  // 用户 ID
    pub username: Option<String>, // 用户名
    pub organization_id: Option<String>, // 组织 ID
    pub agent_id: Option<String>, // Agent ID
    pub task_id: Option<String>,  // 任务 ID
    pub project_id: Option<String>, // 项目 ID
}
```

### 8. Pkg 层（公共工具包）

**职责：通用工具、日志系统、向量存储、工具注册、AOP 事件中心、消息入站适配中台、通用后台任务**

```
src/pkg/
├── logging.rs              # 统一日志宏（log_info!/log_warn!/log_error!/log_debug!）
├── jwt.rs                  # JWT 生成/验证
├── request_context.rs      # RequestContext 定义
├── daily_jsonl.rs          # 每日 JSONL 文件写入
│
├── aop/                    # AOP 事件中心（纯框架，零业务依赖）
│   # Event/Producer/Consumer/Registry/Queue 抽象
│   # 同步/异步消费模式、内置内存队列
│   # producer/consumer 业务层完全解耦、运行时队列状态监控
│
├── adapter/                # 通用适配器基础设施（消息入站适配中台）
│   # 新渠道只需 DAL 注册 producer 即可自动获得入站消息
│   # Agent 路由策略：渠道绑定 agent_id 优先 → feishu_reception 角色 → 任意 Onboarded Agent
│
├── tool_registry/          # 工具注册表
│   ├── builtin.rs          # 内置工具定义
│   ├── fs_read.rs          # 文件读取工具
│   ├── fs_write.rs         # 文件写入工具
│   ├── http_fetch.rs       # HTTP 请求工具
│   ├── shell_exec.rs       # Shell 执行工具
│   ├── mcp.rs              # MCP 工具适配
│   └── handler_adapter/    # Handler → Tool 自动转换宏
│
├── tool_tracing/           # 工具调用追踪
│   ├── entry.rs            # ToolCallEntry 记录
│   ├── logger.rs           # 工具调用日志
│   └── tool_call_logger.rs # 工具调用持久化
│
├── storage/                # 存储与向量搜索基础设施
│   ├── fts5.rs             # FTS5 全文搜索工具（escape_fts5_keyword 等）
│   ├── vector.rs           # VectorStore trait 抽象 + Vectorizable trait
│   ├── mem_vector.rs       # 内存向量存储
│   ├── lance.rs            # LanceDB 向量存储（默认后端）
│   ├── hnsw.rs             # HNSW 向量存储（持久化 + 索引重建）
│   └── sqlite_vss.rs       # SQLite VSS 向量存储
│
├── stats/                  # DuckDB 统计模块
│   ├── stats.rs            # 统计数据收集
│   ├── traits.rs           # 统计 trait 定义
│   ├── model_call.rs       # 模型调用统计（agent_awake_events 等）
│   └── tool_call.rs        # 工具调用统计
│
├── background_task/        # 通用后台任务模块（注册中心 + BackgroundTask trait）
│   ├── mod.rs              # BackgroundTask trait（task_id/task_type/progress/run）+ registry() 全局单例
│   └── registry.rs         # BackgroundTaskRegistry（register/get/list_all_progress/cleanup_finished）
│
└── monitoring/             # 监控模块（ChatMessage 多轮对话追踪）
```

---

## 关键类与函数说明

### 1. Agent 核心类

#### Agent 业务实体

```rust
// src/models/agent.rs
pub struct Agent {
    pub po: AgentPo,              // 持久化对象
    pub brain: Option<Brain>,     // 装配好的大脑
    pub tools: Vec<Tool>,         // 绑定的工具列表
}

pub struct AgentPo {
    pub id: String,
    pub name: String,
    pub role: String,             // JSON: 角色标签数组
    pub description: String,
    pub capabilities: String,     // JSON: 能力描述数组
    pub soul: String,             // 长文本：角色/性格/灵魂设定
    pub model_provider_id: String,
    pub runtime_config: String,   // JSON: AgentRuntimeConfig
    pub status: AgentStatus,
    pub created_by: String,
    pub modified_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}
```

#### AgentRuntimeConfig

```rust
// src/models/agent.rs
pub struct AgentRuntimeConfig {
    pub max_thinking_depth: i32,        // 跨消息累计工具调用数安全检查（默认 10）
    pub max_thinking_rounds: usize,     // 单次唤醒 think loop 轮次上限（默认 90，跨压缩累计）
    pub thinking_interval_ms: i32,      // 思考间隔（默认 0）
    pub max_tool_calls_per_step: i32,   // 单步最大工具调用（默认 5）
    pub enable_reflection: bool,        // 是否启用反思模式
    pub require_user_confirm: bool,     // 是否需要用户确认
}
```

**两层轮次限制的区别**：
- `max_thinking_depth`：consumer 层跨唤醒安全检查，统计模块查询当前任务的累计工具调用次数，超限发送提示消息并终止唤醒
- `max_thinking_rounds`：awakening 层单次唤醒 think loop 轮次上限（跨多次上下文压缩累计），超限触发总结退出流程（`awaken_for_summary`），让 Agent 总结进展并通知消息源

### 2. Brain + Cortex 核心类

#### Brain 大脑聚合根

```rust
// src/models/brain.rs
pub struct Brain {
    pub cortex: Cortex,           // 思考执行（绑定 ModelProvider + 推理实例）
    pub memories: Vec<Memory>,    // 记忆集合（运行时按四层体系检索）
}
```

> 💡 **四层记忆体系**：Core（核心认知：角色设定、能力清单）/ Working（当前会话工作记忆）/ Short-Term（最近会话摘要索引）/ Long-Term（长期沉淀知识图谱）。`memories` 字段为运行时按场景检索出的记忆集合，对应存储层由 MemoryDal 统一管理。

#### Cortex 大脑皮层

```rust
// src/models/brain.rs
pub struct Cortex {
    pub model_provider: ModelProvider,  // 模型配置
    pub cortex: Box<dyn CortexTrait + Send + Sync>, // 推理执行实例
}

#[async_trait]
pub trait CortexTrait: Send + Sync + DynClone {
    fn capability(&self) -> ModelCapability;
    fn model_provider_id(&self) -> &str;
    fn model_name(&self) -> &str;
    
    async fn prompt(&self, prompt: &str) -> Result<String>;
    async fn embeddings(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
    fn support_tools(&self) -> bool;
}
```

### 3. Memory 记忆系统核心类

#### MemoryTrace 原始记忆追踪

```rust
// src/models/memory.rs
pub struct MemoryTrace {
    pub id: String,               // trace-{agent_id}-{timestamp}
    pub agent_id: String,
    pub task_id: Option<String>,
    pub log_id: String,
    pub user_id: String,
    pub organization_id: String,
    pub role: MemoryRole,         // System/User/Assistant/Summary
    
    // 思考闭环字段
    pub input: String,            // 思考输入（完整 Prompt）
    pub output: Option<String>,   // 思考输出
    pub created_at: i64,          // 输入时间
    pub completed_at: Option<i64>, // 输出时间
    
    pub metadata: HashMap<String, String>,
    pub position: Option<MemoryTracePosition>, // JSONL 文件位置
}
```

#### Memory 业务实体

```rust
// src/models/memory.rs
pub struct Memory {
    pub po: MemoryPo,             // PO: Trace/ShortTerm/KnowledgeNode
    pub search_match: Option<SearchMatchInfo>, // 向量搜索匹配信息
}

pub enum MemoryPo {
    Trace(MemoryTrace),
    ShortTerm(ShortTermMemoryIndexPo),
    KnowledgeNode(LongTermKnowledgeNodePo),
    Relation(KnowledgeNodeRelationPo),
}
```

### 4. Domain 核心方法

#### Agent Domain

```rust
// src/service/domain/hr/agent.rs
pub trait AgentDomain: Send + Sync {
    // Agent 创建
    async fn create_agent(&self, ctx: RequestContext, cmd: CreateAgentCommand) -> Result<Agent>;
    
    // Agent 配置更新
    async fn update_agent(&self, ctx: RequestContext, cmd: UpdateAgentCommand) -> Result<Agent>;
    
    // Agent 状态更新
    async fn update_status(&self, ctx: RequestContext, cmd: UpdateStatusCommand) -> Result<Agent>;
    
    // Agent 唤醒：装配 Brain + 绑定工具
    async fn wake_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Agent>;
    
    // Agent 查询
    async fn get_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Option<Agent>>;
    async fn list_agents(&self, ctx: RequestContext) -> Result<Vec<Agent>>;
}
```

#### Message Domain

```rust
// src/service/domain/message/management.rs
pub trait MessageManagement: Send + Sync {
    // 消息创建
    async fn create_message(&self, ctx: RequestContext, cmd: CreateMessageCommand) -> Result<Message>;
    
    // 消息查询
    async fn get_message(&self, ctx: RequestContext, message_id: &str) -> Result<Option<Message>>;
    async fn list_messages(&self, ctx: RequestContext, query: MessageQuery) -> Result<Vec<Message>>;
    
    // 消息投递
    async fn send_message(&self, ctx: RequestContext, message_id: &str) -> Result<()>;
}

// src/service/domain/message/delivery.rs
pub trait MessageDelivery: Send + Sync {
    // 多渠道投递
    async fn deliver(&self, ctx: RequestContext, message: &Message) -> Result<()>;
    
    // 渠道连接测试
    async fn test_channel(&self, ctx: RequestContext, channel_id: &str) -> Result<bool>;
}
```

#### Runtime Domain

```rust
// src/service/domain/runtime/awakening.rs
pub trait AgentAwakening: Send + Sync {
    // Agent 唤醒：装配 Brain + 启动思考循环（按 control_mode 分发到 execute_auto / execute_manual）
    async fn wake(&self, ctx: RequestContext, agent_id: &str) -> Result<()>;
}

// src/service/domain/runtime/tool_execution.rs
pub trait ToolExecution: Send + Sync {
    // 工具执行
    async fn execute_tool(&self, ctx: RequestContext, tool_id: &str, args: &str) -> Result<ToolCallResult>;
}

// src/service/domain/runtime/memory.rs
pub trait RuntimeMemory: Send + Sync {
    // 记忆写入
    async fn write(&self, ctx: RequestContext, params: &MemoryCreateParams) -> Result<Memory>;
    
    // 记忆搜索
    async fn search(&self, ctx: RequestContext, search: &MemorySearch) -> Result<Vec<Memory>>;
}
```

### 5. Handler 核心方法

#### Agent Handler

```rust
// src/handlers/hr/agent/create_agent.rs
pub async fn create_agent_handler(
    Extension(ctx): Extension<RequestContext>,
    Json(cmd): Json<CreateAgentCommand>,
) -> Result<Json<ApiResponse<Agent>>> {
    // 1. 从 Domain 获取 Agent
    let agent = agent_domain().create_agent(ctx.clone(), cmd).await?;
    
    // 2. 返回响应
    Ok(Json(ApiResponse::success(agent)))
}
```

#### Message Handler

```rust
// src/handlers/project/project/create_project.rs
pub async fn create_project_handler(
    Extension(ctx): Extension<RequestContext>,
    Json(cmd): Json<CreateProjectCommand>,
) -> Result<Json<ApiResponse<Project>>> {
    // 1. 从 Domain 获取 Project
    let project = project_domain().create_project(ctx.clone(), cmd).await?;
    
    // 2. 返回响应
    Ok(Json(ApiResponse::success(project)))
}
```

### 6. Consumer 核心方法

#### Message Consumer

```rust
// src/consumer/message.rs
pub struct AgentMessageHandler;

#[async_trait]
impl MessageHandler<MessageEvent> for AgentMessageHandler {
    async fn handle(&self, event: &MessageEvent) -> Result<()> {
        // 1. 创建 RequestContext
        let ctx = RequestContext::from_event(event);
        
        // 2. 调用 Runtime Domain 唤醒 Agent
        runtime_domain().wake(ctx.clone(), &event.to_id).await?;
        
        Ok(())
    }
}
```

### 7. 日志系统核心宏

```rust
// src/lib.rs
#[macro_export]
macro_rules! log_info {
    // 无上下文：第一个参数是字符串字面量
    ($msg:literal $(, $($fields:tt)*)?) => {{
        tracing::info!($msg $(, $($fields)*)?);
    }};
    
    // 带上下文：第一个参数非字符串 + 第二个是字符串
    ($ctx:expr, $op:literal, $($fields:tt)*) => {{
        let span = tracing::info_span!(
            "request",
            log_id = %$ctx.log_id,
            user_id = %$ctx.user_id.as_deref().unwrap_or(""),
            username = %$ctx.username.as_deref().unwrap_or(""),
            organization_id = %$ctx.organization_id.as_deref().unwrap_or(""),
            agent_id = %$ctx.agent_id.as_deref().unwrap_or(""),
            task_id = %$ctx.task_id.as_deref().unwrap_or(""),
            project_id = %$ctx.project_id.as_deref().unwrap_or(""),
            operation = %$op
        );
        let _guard = span.enter();
        tracing::info!($($fields)*);
    }};
}
```

### 8. 通用后台任务模块

**设计：** 任务对象自包含进度状态（Mutex + AtomicUsize 实现内部可变性），`run(&self)` 签名保证 dyn compatible，registry 通过 `Arc<dyn BackgroundTask>` 存储分发。system domain 通过 trait 默认实现暴露 `background_task_registry()`（委托 pkg 全局单例）。现有业务进度查询接口（initialize_progress / rebuild_progress）通过装饰模式调用 registry 获取 `TaskProgressSnapshot` 再装饰为业务响应 DTO。

```rust
// src/pkg/background_task/mod.rs
#[async_trait]
pub trait BackgroundTask: Send + Sync + 'static {
    fn task_id(&self) -> &str;
    fn task_type(&self) -> TaskType;
    fn progress(&self) -> TaskProgressSnapshot;
    async fn run(&self) -> Result<serde_json::Value>;
}

// 全局注册中心（任意层可通过 registry() 注册）
pub fn registry() -> &'static BackgroundTaskRegistry { ... }

// src/pkg/background_task/registry.rs
impl BackgroundTaskRegistry {
    pub async fn register(&self, task: Arc<dyn BackgroundTask>) -> String { ... }
    pub async fn get_progress(&self, task_id: &str) -> Option<TaskProgressSnapshot> { ... }
    pub async fn list_all_progress(&self) -> Vec<TaskProgressSnapshot> { ... }
    pub async fn cleanup_finished(&self, max_count: usize) { ... }
}
```

**任务类型：** InitializeSystem / RebuildVectors / SeedSave / SeedLoad / SeedApplyDefault

**统一接口：**
- `GET /api/v1/system/tasks/{task_id}/progress` — 查询单个任务进度
- `GET /api/v1/system/tasks` — 列出所有任务（支持 task_type/status 筛选）
- `POST /api/v1/system/tasks/cleanup` — 清理已完成的旧任务

**装饰模式示例：**
```rust
// src/handlers/organization/initialize_system.rs
pub async fn get_initialize_progress(...) -> Result<InitProgressResponse> {
    // 1. 从 system domain 获取基础任务信息
    let snapshot = system::domain()
        .background_task_registry()
        .get_progress(&params.task_id).await?;
    // 2. 装饰为业务响应 DTO（状态映射 + result 解析）
    Ok(InitProgressResponse {
        status: match snapshot.status {
            TaskStatus::Pending => InitStatus::Pending,
            // ...
        },
        result: snapshot.result.and_then(|v| serde_json::from_value(v).ok()),
        ..
    })
}
```

---

## 依赖关系

### 1. 外部依赖（Cargo.toml）

#### 核心框架依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| `tokio` | 1 | 异步运行时 |
| `axum` | 0.8 | Web 框架 |
| `sqlx` | 0.8.6 | 数据库访问（SQLite） |
| `serde` | 1 | 序列化 |
| `serde_json` | 1 | JSON 处理 |
| `tracing` | 0.1 | 日志系统 |
| `tracing-subscriber` | 0.3 | 日志订阅器 |
| `chrono` | 0.4 | 时间处理 |
| `uuid` | 1.23.0 | UUID 生成 |

#### 向量搜索依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| `fastembed` | 5.13 | 本地向量嵌入（Embedding Provider，纯 Rust） |
| `lancedb` | 0.26 | 嵌入式向量数据库（默认存储后端） |
| `hnsw_rs` | - | HNSW 向量存储（持久化 + 索引重建） |
| `arrow-array` | 57.3.0 | Arrow 数组 |
| `arrow-schema` | 57.3.0 | Arrow Schema |

> 💡 向量存储后端通过 `VectorStore` trait 抽象（`pkg/storage/vector.rs`），支持 LanceDB（默认）/ HNSW / inMemory / SqliteVss 四种实现。所有支持向量索引的 PO 实现 `Vectorizable` trait，DAL 层通过 `embed_entity` 统一调用。

#### 工具调用依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| `rmcp` | 1.7 | MCP 工具调用 |
| `reqwest` | 0.12 | HTTP 客户端 |
| `jsonwebtoken` | 9.3 | JWT 认证 |
| `cookie` | 0.18 | Cookie 管理 |

#### 辅助依赖

| 依赖 | 版本 | 用途 |
|------|------|------|
| `anyhow` | 1.0 | 错误处理 |
| `thiserror` | 1.0 | 自定义错误类型 |
| `async-trait` | 0.1 | 异步 trait |
| `dyn-clone` | 1.0.14 | 动态克隆 |
| `once_cell` | 1.21.4 | 懒初始化 |
| `derive_builder` | 0.20 | Builder 模式 |
| `schemars` | 0.8 | JSON Schema 生成 |

### 2. 内部依赖关系

#### Workspace 结构

```toml
[workspace]
members = [
    ".",          # 主后端 crate
    "frontend",   # Dioxus 前端 crate
    "common",     # 公共共享 crate
    "ai-orz-macros", # 自定义宏 crate
]
```

#### Common crate 依赖

```rust
// common/Cargo.toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.23.0", features = ["v7", "serde"] }
thiserror = "1.0"
toml = "1"  # 配置文件解析
reqwest = { version = "0.12", features = ["json"] }  # HTTP 客户端
bincode = "2.0"  # 二进制序列化
schemars = { version = "0.8", features = ["derive"] }  # JSON Schema
```

#### 后端 crate 依赖 Common

```rust
// Cargo.toml
[dependencies]
common = { path = "./common", features = [
    "sqlx",           # SQLx 类型支持
    "axum-integration", # Axum 集成
    "bincode-integration", # Bincode 支持
    "toml-integration", # TOML 配置支持
    "reqwest-integration" # Reqwest 集成
] }
```

### 3. 分层依赖方向

```
┌─────────────────────────────────────────┐
│ Adapter 层（适配层）                     │
│ - HTTP Handler + AOP Producer           │
│ - 依赖 Domain                            │
│ - 依赖 Common (DTO/Enum)                 │
│ - 依赖 Middleware (RequestContext)       │
└─────────────────────────────────────────┘
            ↓ 只调用 Domain
┌─────────────────────────────────────────┐
│ Domain 层                                │
│ - 组合多个 DAL                           │
│ - 依赖 Common (Command/Query)            │
│ - 依赖 Models (业务实体)                 │
└─────────────────────────────────────────┘
            ↓ 组合 DAL
┌─────────────────────────────────────────┐
│ DAL 层                                   │
│ - 组合多个 DAO                           │
│ - PO ↔ 业务实体转换                      │
│ - 依赖 Models (PO + 业务实体)            │
└─────────────────────────────────────────┘
            ↓ 组合 DAO
┌─────────────────────────────────────────┐
│ DAO 层                                   │
│ - 本地 DB：SQL 拼接 + 数据库访问         │
│ - 外部 API：出站调用（lark/a2a 等）      │
│ - 仅操作 PO                              │
│ - 依赖 Models (PO)                       │
│ - 依赖 sqlx                              │
└─────────────────────────────────────────┘
            ↓ 操作数据库 / 外部 API
┌─────────────────────────────────────────┐
│ Models 层                                │
│ - PO 持久化实体                          │
│ - 业务实体定义                           │
│ - 依赖 Common (Enum)                     │
└─────────────────────────────────────────┘
```

### 4. Consumer 依赖关系

```
Consumer (内部事件消费者，处理 Domain 产生的内部事件)
    ↓ 调用 Domain
Domain (业务逻辑)
    ↓ 调用 DAL
DAL (数据访问)
    ↓ 调用 DAO
DAO (数据库访问 / 外部 API 出站)
```

> 💡 Consumer 不在适配层，它处理的是 Domain 产生的内部事件；外部协议（HTTP 回调、WS 事件、轮询）由适配层直接调用 Domain 方法处理，不进入事件中心。

---

## 项目运行方式

### 1. 配置文件

**默认配置嵌入在二进制中：**

```toml
# common/config/ai_orz.toml
base_data_path = "data"

[server]
listen_addr = "0.0.0.0:3000"

[database]
db_file_name = "ai_orz.db"

[frontend]
dist_dir = "dist"

[logging]
enable_file_log = true
log_subdir = "logs"
format = "json"           # 日志格式: "json" (默认) 或 "text"
retention_days = 30       # 日志保留天数，0 表示不清理

[jwt]
# JWT签名密钥（生产环境务必修改！也可以通过环境变量 JWT_SECRET 设置）
# secret = "your-secret-key-here"
# default_expiry_hours = 168

[consumer]
empty_queue_sleep_ms = 100
error_retry_sleep_ms = 1000

[consumer.topics.message]
concurrency = 3           # Message Topic 消费者并发数
```

**环境变量覆盖：**

| 环境变量 | 对应配置项 | 说明 |
|----------|------------|------|
| `JWT_SECRET` | `jwt.secret` | JWT 签名密钥 |
| `JWT_EXPIRY_HOURS` | `jwt.default_expiry_hours` | JWT 过期时间 |
| `FRONTEND_DIST_DIR` | `frontend.dist_dir` | 前端静态文件目录 |

### 2. 开发模式启动

**一键启动脚本：**

```bash
./start.sh dev
```

**启动内容：**
- 后端服务: http://localhost:3000
- 前端开发服务器 (热重载): http://localhost:8080

**更多模式：**

```bash
./start.sh backend   # 只启动后端（cargo run）
./start.sh frontend  # 只启动前端（dx serve）
./start.sh build     # 仅编译（前端 release + 后端 release）
./start.sh prod      # 生产模式：编译 + 运行 release 二进制
./start.sh help      # 查看帮助
```

**手动启动：**

```bash
# 后端开发模式
cargo run

# 前端开发模式
cd frontend
dx serve
```

### 3. 生产构建

**全量构建脚本：**

```bash
./start.sh build
```

**输出：**
- 后端二进制: `target/release/ai_orz`
- 前端静态文件: `dist/`

**手动构建：**

```bash
# 后端生产构建
cargo build --release

# 前端生产构建
cd frontend
dx build --release
```

### 4. 生产运行

```bash
./target/release/ai_orz
```

**服务启动后：**
- 监听地址：`0.0.0.0:${SERVER_PORT:-3000}`
- 前端静态文件：从 `dist/` 目录提供
- 数据库文件：`data/ai_orz.db`
- 日志文件：`data/logs/`

### 5. 端口说明

| 服务 | 默认端口 | 说明 |
|------|----------|------|
| 后端 API | 3000 | REST API + 静态文件服务 |
| 前端开发服务器 | 8080 | dx serve 热重载开发服务器 |

### 6. 数据目录结构

```
data/
├── ai_orz.db              # 主数据库（SQLite）
├── logs/                  # 日志目录
│   ├── 2026-07-01.json    # 每日日志文件（JSON 格式）
│   ├── 2026-06-30.json
│   └── ...
│
└── long_term_memory/      # 长期记忆原始细节
    └── {agent_id}/        # 按 Agent 分目录
        ├── 2026-07-01.jsonl  # 每日 JSONL 文件
        ├── 2026-06-30.jsonl
        └── ...
```

---

## 测试体系

### 1. 测试统计

| 层级 | 测试数 | 说明 |
|------|--------|------|
| 后端 | 746 | DAO + DAL + Domain + Handler + Pkg 完整覆盖 |
| 前端 | 34 | Dioxus 组件 + 页面测试 |
| common | 50 | 公共 crate 测试 |
| **总计** | **830** | **100% 通过率** |

### 2. 测试运行

```bash
# 运行所有测试
cargo test

# 运行特定模块测试
cargo test --package ai_orz --lib service::dao::agent

# 运行单个测试
cargo test test_create_agent
```

### 3. 测试设计原则

| 原则 | 实现 |
|------|------|
| **独立数据库** | 每个测试使用随机临时 SQLite 文件 |
| **测试隔离** | 使用 `#[sqlx::test]` 宏自动管理数据库生命周期 |
| **干净环境** | 每个测试执行前重新初始化 storage |
| **工厂方法** | `create_test_agent()` / `create_test_project()` 减少重复代码 |
| **公共初始化** | `init_test_env()` 统一初始化模式 |

### 4. 测试文件组织

```
src/
├── service/
│   ├── dao/
│   │   ├── agent/
│   │   │   ├── sqlite_test.rs    # Agent DAO 测试
│   │   │   └── ...
│   │   └── ...
│   │
│   ├── dal/
│   │   ├── agent_test.rs         # Agent DAL 测试
│   │   ├── ...
│   │
│   └── domain/
│       ├── hr/
│       │   ├── agent_test.rs     # Agent Domain 测试
│       │   └── ...
│       └── ...
│
└── consumer/
    ├── tests.rs                  # 消费者框架测试
    └── message_tests.rs          # Message 消费者测试
```

### 5. 测试示例

#### DAO 层测试

```rust
// src/service/dao/agent/sqlite_test.rs
#[sqlx::test]
async fn test_create_agent(pool: SqlitePool) {
    init_test_env(&pool);
    
    let dao = agent_dao();
    let po = create_test_agent_po("test_agent", "model_001");
    
    let result = dao.create(&pool, &po).await;
    assert!(result.is_ok());
    
    let found = dao.find_by_id(&pool, &po.id).await.unwrap();
    assert!(found.is_some());
}
```

#### Domain 层测试

```rust
// src/service/domain/hr/agent_test.rs
#[sqlx::test]
async fn test_create_agent(pool: SqlitePool) {
    init_test_env(&pool);
    
    let ctx = create_test_context();
    let cmd = CreateAgentCommand {
        name: "Test Agent".to_string(),
        model_provider_id: "model_001".to_string(),
        // ...
    };
    
    let agent = agent_domain().create_agent(ctx, cmd).await.unwrap();
    assert_eq!(agent.name(), "Test Agent");
}
```

---

## 开发规范

### 1. 命名规范

| 元素 | 规范 | 示例 |
|------|------|------|
| **变量/函数/方法** | snake_case | `user_id`, `create_agent`, `get_user_by_id` |
| **类型/结构体/枚举/Trait** | PascalCase | `AgentPo`, `RequestContext`, `AgentDao` |
| **常量** | SNAKE_CASE | `MAX_SIZE`, `LOG_ID`, `DEFAULT_TIMEOUT` |
| **文件名/目录名** | snake_case | `agent.rs`, `request_context.rs`, `sqlite_test.rs` |

### 2. 函数前缀约定

| 操作 | 前缀 | 示例 |
|------|------|------|
| 获取数据（有参数） | `get_` | `get_agent_by_id`, `get_user_name` |
| 获取单例/无参数 | 直接命名 | `agent_dao()`, `uid()` |
| 创建/新增 | `new_`, `create_` | `new_agent()`, `create_user()` |
| 修改/更新 | `update_` | `update_agent()` |
| 删除（软删除） | `delete_` | `delete_agent()` |
| 列表/批量 | `find_all`, `find_by_` | `find_all_agents()`, `find_by_org()` |
| 布尔判断 | `is_`, `has_`, `can_` | `is_deleted()`, `has_permission()` |

### 3. RequestContext 传递规范

**强制要求：所有 service 层（DAO/DAL/Domain）公共方法的第一个参数必须是 `ctx: RequestContext`**

```rust
// ✅ 正确
async fn create_agent(&self, ctx: RequestContext, cmd: CreateAgentCommand) -> Result<Agent>;

// ❌ 错误 - 缺少 ctx
async fn create_agent(&self, cmd: CreateAgentCommand) -> Result<Agent>;
```

**跨层传递统一使用 `ctx.clone()`：**

```rust
// ✅ 正确：clone 后传递
self.project_dal.create(ctx.clone(), project).await?;
self.task_dal.create(ctx.clone(), task).await?;

// ❌ 错误：直接移动所有权
self.project_dal.create(ctx, project).await?;  // ctx 已移动，后续无法使用
```

### 4. 枚举类型安全

**所有存储在数据库中的枚举字段必须使用 Rust 枚举类型，禁止直接使用 `i32`**

```rust
// ✅ 正确
#[repr(i32)]
#[derive(sqlx::Type)]
pub enum AgentStatus {
    Interviewing = 1,
    Active = 2,
    Inactive = 3,
}

// ❌ 错误
pub struct AgentPo {
    pub status: i32,  // 禁止直接使用整数
}
```

### 5. SQLite + SQLx 规范

| 规范 | 说明 |
|------|------|
| **STRICT 模式** | 所有表必须启用 `STRICT` 模式 |
| **SQL 关键字转义** | `status` → `"status"` |
| **枚举字段标注** | `status as "status: TaskStatus"` |
| **软删除约定** | 已删除 `status = 0`，查询默认过滤 |
| **`.sqlx` 目录** | 必须纳入版本控制 |
| **测试使用 `#[sqlx::test]`** | 每个测试独立内存数据库 |

### 6. Handler 拆分规范

| 规范 | 说明 |
|------|------|
| **按业务域分组** | hr、finance、organization、project 等 |
| **每个方法一个文件** | 单个文件只放一个 handler 函数 |
| **`mod.rs` 只导出** | 不存放实现代码 |
| **DTO 从 common 导入** | 禁止在 handlers 定义本地 DTO |

### 7. 日志系统规范

**强制使用统一日志宏，禁止直接调用 `tracing::*!`**

```rust
// ✅ 正确：使用统一宏
log_info!("application started");
log_info!(&ctx, "create_agent", "created agent id={}", agent_id);
log_error!(&ctx, "db_error", "database query timeout");

// ❌ 错误：直接调用 tracing
tracing::info!("some message");
```

**两种调用模式：**
1. 无上下文（系统级别）：`log_info!("message")`
2. 带上下文（请求级别）：`log_info!(&ctx, "operation", "message")`

---

## 附录：关键文件索引

### 核心架构文档

| 文档 | 说明 |
|------|------|
| [AGENTS.md](../AGENTS.md) | AI 开发规范总览 + 文档索引 |
| [ARCHITECTURE.md](../docs/ARCHITECTURE.md) | 完整架构说明、核心概念、实体关系 |
| [LAYERED_ARCHITECTURE_PRACTICE.md](../docs/LAYERED_ARCHITECTURE_PRACTICE.md) | 分层架构实践记录、避坑指南 |

### 各模块设计文档

| 文档 | 说明 |
|------|------|
| [sqlx_guide.md](../docs/sqlx_guide.md) | SQLx 0.8 + SQLite 开发规范 |
| [runtime_design.md](../docs/runtime_design.md) | Runtime Domain 总纲：Agent 唤醒、工具执行 |
| [memory_design.md](../docs/memory_design.md) | 四层记忆系统设计 |
| [tool_design.md](../docs/tool_design.md) | 混合模式工具调用、工具注册表 |
| [message_interaction_design.md](../docs/message_interaction_design.md) | 消息交互架构、工具调用复用消息表 |
| [consumer_architecture.md](../docs/consumer_architecture.md) | 异步消费者框架、三层分发 |
| [logging_design.md](../docs/logging_design.md) | 日志系统设计、统一宏使用规范 |

### 配置文件

| 文件 | 说明 |
|------|------|
| [Cargo.toml](../Cargo.toml) | Workspace 配置 + 主 crate 依赖 |
| [common/Cargo.toml](../common/Cargo.toml) | Common crate 配置 |
| [common/config/ai_orz.toml](../common/config/ai_orz.toml) | 默认应用配置模板 |

### 入口文件

| 文件 | 说明 |
|------|------|
| [src/main.rs](../src/main.rs) | 后端服务入口 |
| [src/lib.rs](../src/lib.rs) | 后端库入口 + 日志宏定义 |
| [src/router.rs](../src/router.rs) | HTTP 路由配置 |
| [frontend/src/main.rs](../frontend/src/main.rs) | 前端入口 |

---

**本文档是项目代码全景指南，详细设计请查阅各专项文档**