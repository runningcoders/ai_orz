# AI Orz - Agent 开发规范总览

> 🎯 **本文档供 AI 助手快速理解项目**：5分钟了解项目是什么、代码怎么组织、开发遵循什么规范
>
> 最后更新：2026-07-27（集成测试与 CI 质量体系建设：29 个集成测试覆盖全链路 + clippy 442 warning 清理 + 集成测试从 238s 降到 3.7s + cargo-llvm-cov 覆盖率门槛 35%；修复 3 个潜伏真 bug：From\<i32\> 无限递归、start_all 持锁 await 死锁、统计 future 静默丢弃）

---

## 一、项目概览

### 1.1 项目是什么

**AI Orz** - 全栈 Rust 多 Agent 协作框架，以组织化形式管理和执行 AI 代理任务

- **后端**：Rust + Axum + SQLite + sqlx 0.8 + rig-core 0.34
- **前端**：Dioxus 0.7 (WebAssembly) + Tailwind CSS v4 + DaisyUI v5
- **技术特色**：严格分层架构、类型安全、906 个测试 100% 通过率（后端 810 = 781 单元 + 29 集成 + 前端 46 + common 50）、clippy `-D warnings` 零容忍、cargo-llvm-cov 覆盖率门槛 35%、30+ 主题切换

### 1.2 已实现核心功能

| 模块 | 状态 | 说明 |
|------|------|------|
| 👥 组织用户权限 | ✅ | 多级组织、用户角色、JWT Cookie 认证 |
| 🤖 Agent 全生命周期 | ✅ | 创建、配置、工具绑定、唤醒执行 |
| 🧠 四层记忆系统 | ✅ | Core/Working/Short-term/Long-term |
| 💬 消息对话系统 | ✅ | 用户 ↔ Agent 双向对话，支持项目上下文 |
| 📨 消息渠道系统 | ✅ | 多渠道消息出站推送（飞书/微信/Slack/邮件/Webhook），飞书 P2P 私信 WebSocket 入站长连接已上线，适配层架构；微信/Slack/邮件/Webhook 出站骨架就绪，入站待实现 |
| 🔌 A2A 外部 Agent | ✅ | 完整 A2A 协议支持：Client（注册外部 CLI/Remote Agent 并委派任务）、Server（对外暴露协议端点）、异步结果回传（Push 回调 + 30秒轮询兜底，适配层直接处理，外部协议不污染内部事件中心） |
| 🛠️ 混合模式工具调用 | ✅ | 简单工具走 rig auto，关键工具走自建 manual 可控链路 |
| 📚 技能库系统 | ✅ | 可复用技能和工作流，支持搜索和分类，tag 技能包安装，唤醒时注入 Prompt |
| 📋 任务 + 项目管理 | ✅ | 任务状态机，项目聚合对话上下文，DAL + Domain 层完整实现 |
| 📎 统一附件存储 | ✅ | 消息附件 + 项目产物，FileMeta + 日期分层路径 |
| 🔌 MCP 服务器集成 | ✅ | MCP 服务器管理、工具同步、MCP 工具调用执行 |
| 🚀 异步消费者系统 | ✅ | 通用消费者框架 + Message Topic 三层分发 |
| 📝 结构化日志系统 | ✅ | JSON 格式、自动上下文关联、日志自动清理 |
| 🔍 向量搜索 | ✅ | LanceDB 默认 + HNSW/inMemory/SqliteVss 多后端、Embedding Provider 唯一性、Switch 接口 |
| 🔎 全文搜索 | ✅ | FTS5 + trigram 分词器，支持中文全文搜索、BM25 相关性排序 |
| 📊 Agent 统计系统 | ✅ | DuckDB 多维统计、Agent/Project/Task/ModelProvider/Tool 五维度覆盖、实体详情页按需动态注入 |
| 🔄 多回合循环控制 | ✅ | 轮次限制检查、任务完成检测、Prompt 上下文差异化、工具失败计数注入 |
| 🎒 工具包机制 | ✅ | tag 分组工具、Agent 入职自动安装、免绑定校验三层逻辑 |
| 📨 任务分配消息 | ✅ | TaskAssignment 消息类型、自动通知 Agent、神经工具封装 |
| ⏰ 定时触发器系统 | ✅ | Cron Trigger 管理、后台扫描、事件投递、系统领域基础设施 |
| 🏛️ 记忆沉淀机制 | ✅ | Agent 休息与沉淀、短期记忆→长期知识图谱、定时触发沉淀 |
| 🎒 技能包机制 | ✅ | tag 分组技能、批量安装、安装即复制、卸载保留副本 |
| 🔎 综合搜索 | ✅ | FTS5 关键词 + 向量语义 + 图谱关系 三位一体混合搜索，Hybrid/Vector/Keyword 三态匹配 |
| 📊 任务进度追踪 | ✅ | Task progress 字段（0-100）、Agent 神经工具更新进度、complete 自动设 100、progress_updated 事件 |
| 🤝 Agent 协作工具 | ✅ | search_agents 搜索、send_message_to_agent Agent 间消息、collaboration tag 分组工具 |
| 🎨 前端架构重构 | ✅ | Dioxus Router 15 路由 + Tailwind CSS v4 + DaisyUI v5 组件库 + 30+ 主题切换 + 统一 API 客户端 + 13 CRUD 页面 |
| 💬 对话功能 MVP | ✅ | 左右分栏布局（项目列表 + 对话区）、双向分页、3秒短轮询、消息气泡展示 |
| 📎 对话附件上传 | ✅ | 多文件上传、图片内联展示、文件下载、消息时间分组 |
| 🔍 消息搜索 | ✅ | FTS5 + 向量混合搜索、搜索结果展示匹配类型和向量距离 |
| 🧠 记忆搜索 | ✅ | 关键词 + 类型筛选、短期记忆/知识节点/关系搜索 |
| 🗺️ 知识图谱可视化 | ✅ | SVG 图谱组件、圆形布局、节点连接线、搜索初始节点；新增 Canvas HUD 驾驶舱风格渲染（深色径向渐变背景 + 节点呼吸光晕 + 边流光发光），支持 Canvas/SVG 风格一键切换 |
| 🗺️ 知识图谱交互完善 | ✅ | 关系类型差异化颜色/样式、边标签防重叠、节点拖拽、缩放平移、搜索高亮与历史、详情侧边栏增强；节点 tags 多色边框 + 动态半径 + 简介展示 |
| 📡 SSE 消息推送 | ✅ | Server-Sent Events 长连接、订阅者模式、DAO 层连接管理、broadcast 广播 |
| 📡 AOP 事件中心 | ✅ | 纯框架（零业务依赖）、Event/Producer/Consumer/Registry 抽象、同步/异步消费模式、内置内存队列、producer/consumer 业务层完全解耦、运行时队列状态监控 |
| 🔔 Toast 通知系统 | ✅ | 全局状态管理、4 种类型（success/error/warning/info）、滑入滑出动画、进度条倒计时、22 页面统一替换旧式提示 |
| 🔐 Cookie 认证统一 | ✅ | 前后端统一 HttpOnly Cookie + JWT、中间件顺序优化、localStorage 标志位 |
| 🔑 双模式认证 | ✅ | Cookie（浏览器）+ Bearer token（API 工具/代码调用），非浏览器请求返回 401 JSON |
| 📊 任务进度可视化 | ✅ | 项目概览卡片、动态进度条、任务状态分布统计 |
| 🤖 Agent 详情页对话 | ✅ | Agent 详情页集成对话功能、SSE 实时消息、历史消息加载 |
| 📋 任务管理核心功能 | ✅ | 任务创建/编辑弹窗、任务详情页、项目详情页集成创建入口 |
| 📋 独立任务管理页面 | ✅ | 全局任务列表、看板视图（按状态分列）、多维度筛选、统计概览 |
| 🧠 Agent 记忆面板 | ✅ | Agent 详情页记忆浏览、Tab 切换（短期记忆/知识节点/关系）、搜索、卡片展示 |
| 🛠️ Tool/ModelProvider 详情页 | ✅ | 工具和模型提供商详情页、统计面板、调用测试、连接测试 |
| 💬 对话体验打磨 | ✅ | 消息复制（hover 显示按钮）、快捷指令（/clear、/help）、键盘导航 |
| 💾 数据备份与恢复 | ✅ | _index.json 索引 + tar.gz 压缩 + 恢复脚本 |
| 📜 日志在线查询 | ✅ | 关键词 + log_id 调用链 + 级别 + 时间范围过滤 |
| 🛡️ 角色权限中间件 | ✅ | 基于并查集的权限中间件，Member → Admin → SuperAdmin 继承体系 |
| 📊 AOP 队列监控 | ✅ | 队列运行时监控 + 实时统计图表（HUD 风格折线图时序 + 环形图分布，纯内存收集器 60 分钟滑动窗口，5 秒轮询，埋点 publish/consume/success/failure） |
| 💬 Workspace 对话机制 | ✅ | 底部对话框跟随当前视图（默认/Project/Agent），SSE 实时消息，HUD 流光提示未读消息源（橙色竖条 + 流动光晕动画），点击切换视图清除 |
| 📊 统计图表可视化 | ✅ | HUD 风格 Canvas 图表：折线图（4 个实体详情页展示模型调用趋势，消费 model_call_time_series；AOP 页面展示最近 60 分钟事件时序；logs 页面展示 24h 日志量时序；workspace 底部展示 60 分钟消息流量）+ 环形图（Project 详情页展示任务状态分布，AOP 页面展示状态/消费者分布，logs 页面展示级别分布，triggers 页面展示状态分布，Agent 详情页展示工具调用分布，消费 DonutSlice 通用数据结构）；共享 hud_palette 背景工具，2.4s 呼吸光晕动画 |
| 📊 运行时统计基础设施 | ✅ | pkg/stats/runtime/ 泛型内存收集器（RuntimeStatsCollector\<K\>），与 pkg/stats/ 顶层 DuckDB 持久化互补；AOP 已接入（AopStatsCollector wrap），未来 SSE/WS 连接数、Channel 推送指标等运行时场景可直接复用 |
| 🎨 通用 HUD 仪表盘 | ✅ | 通用 Gauge 组件（从 AopGauge 抽象），AOP/Health 等场景复用；HUD 视觉统一（呼吸光晕 + 选中发光 + 12 等分刻度 + 颜色编码） |
| 📊 系统健康监控 HUD | ✅ | Health 页面重写为仪表盘墙（10s 轮询，6 个维度：后端/AOP队列/活跃Agent/活跃项目/待处理任务/运行时长），复用通用 Gauge 组件 |
| 📋 看板视图 Canvas | ✅ | tasks 看板视图改为 HUD 风格 KanbanCanvas（多列泳道 + 优先级颜色编码 + 进度条 + HUD 深色径向渐变背景） |
| 🧪 集成测试体系 | ✅ | 29 个集成测试覆盖 Auth/SysInit + Core CRUD + Message Delivery + Vector Degradation + A2A Flow 全链路，3.7s 跑完；向量降级契约守护测试确保无 embedding provider 时主流程仍可用 |
| 🛡️ CI 质量门槛 | ✅ | clippy `-D warnings` 零容忍（442 warning 全清理）+ cargo-llvm-cov `--fail-under-lines 35` + 集成测试 3.7s（从 238s 优化） |

### 1.3 整体完成度与测试统计（2026-07-27 更新）

| 指标 | 数值 | 说明 |
|------|------|------|
| **总测试数** | **906** | 后端 810（781 单元 + 29 集成） + 前端 46 + common 50，DAO + DAL + Domain + Handler + Pkg 完整覆盖（含 8 个 runtime_stats + 7 个 AOP 内存统计 + 3 个 log_stats + 6 个 gauge/aop_gauge + 2 个 kanban_canvas + 15 个宏集成测试 + 14 个 HTTP 集成测试） |
| **通过率** | **100%** | ✅ 全部测试通过 |
| **集成测试覆盖** | 29 个 | Auth/SysInit 4 + Core CRUD 3 + Message Delivery 2 + Vector Degradation 3 + A2A Flow 2 + 宏集成 15 |
| **集成测试耗时** | 3.7s | 并行运行（从 238s 优化，63 倍提升） |
| **CI clippy 门槛** | `-D warnings` | 零容忍，442 warning 全清理 |
| **CI 覆盖率门槛** | 35% | cargo-llvm-cov `--fail-under-lines 35` |
| DAO 模块数 | 25 个 | 全部实现并被使用，零闲置（18 核心 DAO + 5 渠道 DAO + a2a 回调 + 1 触发器 + 消息推送） |
| DAL 模块数 | 23 个 | 全部完整业务承载，零闲置（含 lark 飞书、agent_a2a、agent_codex 专属 DAL） |
| Domain 领域数 | 7 个 | 全部完整实现（新增 SystemDomain） |
| Handler API 领域数 | 8 个上线 | organization, hr, finance, project, user, health, system, a2a（公开回调） |
| **整体架构完成度** | **~99%** | 从下往上扎实推进，适配层架构原则已明确 |

---

## 二、文档快速索引

> 📌 **按需要读取详细设计文档**

### 架构总览
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [README.md](./README.md) | 项目概览、快速开始、功能列表、文档索引 | ⭐⭐⭐ |
| [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) | **最新**完整架构说明、核心概念解释、实体关系、完成状态 | ⭐⭐⭐ |
| [docs/architecture_status_20260725.md](./docs/architecture_status_20260725.md) | 分层架构现状快照、金字塔结构、各层状态统计 | ⭐⭐⭐ |

### 分层架构与最佳实践
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md) | **开发必读** 7 个完整架构实践（含适配层架构原则）、反模式坑、最佳实践总结 | ⭐⭐⭐ |
| [docs/NAMING_CONVENTION.md](./docs/NAMING_CONVENTION.md) | 全项目统一命名约定、DAO/DAL/Domain 命名规则 | ⭐⭐ |
| [docs/external_agent_design.md](./docs/external_agent_design.md) | 外部 Agent 接入（CLI/Remote/A2A 异步回调轮询）、适配层处理模式 | ⭐⭐ |
### 各模块详细设计
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [docs/sqlx_guide.md](./docs/sqlx_guide.md) | SQLx 0.8 + SQLite 开发规范、枚举映射、测试隔离 | ⭐⭐⭐ |
| [docs/runtime_design.md](./docs/runtime_design.md) | **Runtime Domain 总纲**：Agent 唤醒、神经 vs 外骨骼工具二分、上下文极薄设计 | ⭐⭐⭐ |
| [docs/memory_design.md](./docs/memory_design.md) | 四层记忆系统设计、检索策略 | ⭐⭐ |
| [docs/tool_design.md](./docs/tool_design.md) | 混合模式工具调用、工具注册表、调用追踪 | ⭐⭐ |
| [docs/message_interaction_design.md](./docs/message_interaction_design.md) | 消息交互架构、用户↔Agent双向对话、工具调用复用消息表 | ⭐⭐ |
| [docs/message_channel_design.md](./docs/message_channel_design.md) | 消息渠道系统设计、多渠道支持、状态管理 | ⭐⭐ |
| [docs/consumer_architecture.md](./docs/consumer_architecture.md) | 异步消费者框架、按 to_role 分层分发 | ⭐⭐ |
| [docs/task_scheduler_design.md](./docs/task_scheduler_design.md) | 任务调度器设计、Cron 表达式、定时任务执行 | ⭐⭐ |
| [docs/event_design.md](./docs/event_design.md) | 泛型 topic 事件队列、类型安全隔离 | ⭐⭐ |
| [docs/skill_design.md](./docs/skill_design.md) | 技能库系统、Agent 自进化沉淀技能 | ⭐⭐ |
| [docs/vector_search_architecture.md](./docs/vector_search_architecture.md) | 混合搜索架构：FTS5 关键词 + 向量语义 + 三态匹配 | ⭐⭐ |

### 基础设施与规范
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [docs/logging_design.md](./docs/logging_design.md) | **日志系统设计**：统一宏使用规范、上下文检测机制、tracing 语法速查 | ⭐⭐⭐ |
| [docs/sqlx_guide.md](./docs/sqlx_guide.md) | SQLx 0.8 + SQLite 开发规范、枚举映射、STRICT 模式、FTS5 全文搜索、测试隔离 | ⭐⭐⭐ |
| [docs/task_design.md](./docs/task_design.md) | 任务系统设计、状态机、分配与进度追踪 | ⭐ |
| [docs/project_design.md](./docs/project_design.md) | 项目系统设计、聚合对话上下文 | ⭐ |
| [docs/organization_design.md](./docs/organization_design.md) | 组织用户权限体系设计 | ⭐ |
| [docs/attachment_storage.md](./docs/attachment_storage.md) | 产物与消息附件统一存储设计 | ⭐ |

### 前端与 UI
| 文档 | 内容 | 优先级 |
|------|------|--------|
| [docs/frontend_architecture.md](./docs/frontend_architecture.md) | **前端架构设计**：Router/CSS 设计系统/API 客户端/状态管理/页面模块 | ⭐⭐⭐ |
| [docs/ui_design_system.md](./docs/ui_design_system.md) | UI 设计系统、配色、排版、组件规范、实现状态 | ⭐⭐ |

---

## 三、核心架构规范（必须遵守）

### 3.1 代码分层架构

**严格单向调用，禁止跨层和同层互调**：

```
Adapter (适配层)
    ├─ HTTP Handler（用户 API + 外部回调）
    └─ AOP Producer（轮询 + 外部 WS 事件接入）
    │
    │ 只调用 Domain；负责协议解析、校验、ID 映射、DTO↔Command 转换
    ▼
Domain (领域层)
    │ 组合多个 DAL，实现核心业务逻辑，产生内部事件
    ▼
DAL (业务数据层)
    │ 组合多个 DAO，提供业务级数据操作，PO↔Entity 转换
    ▼
DAO (数据访问层)
    ├─ 本地 DB DAO：单一数据源 CRUD
    └─ 外部 API DAO：出站外部调用（如 LarkDao.push、A2aRuntimeDao.send_task）
    │
    ▼
Models (PO 持久化实体)
```

**各层职责边界**：

| 层级 | 可以做 | 禁止做 |
|------|--------|--------|
| **DAO** | 单一/多个数据源访问<br>本地 DB：SQL 拼接、PO 读写<br>外部 API：出站调用、出站格式转换（如 Markdown→飞书卡片） | ❌ DAO 调 DAO、❌ 业务逻辑、❌ 实体组装/装饰 |
| **DAL** | 依赖多个 DAO、PO ↔ Entity 双向转换、业务级数据操作 | ❌ DAL 调 DAL |
| **Domain** | 依赖多个 DAL、核心业务逻辑编排、跨领域事务、产生内部事件 | ❌ Domain 调 Domain、❌ 直接调用 DAO（跨层）、❌ 直接调用外部 API |
| **Adapter（适配层）** | HTTP Handler（用户 API + 公开回调）、AOP Producer（WS/轮询）<br>协议解析、参数校验、鉴权、幂等检查<br>外部 ID ↔ 内部 ID 映射<br>DTO/外部结构 ↔ Command 转换<br>按 Action 编排 Domain 调用<br>组装响应（HTTP Handler） | ❌ 直接调用 DAL/DAO（跨层）、❌ 承载核心业务规则<br>❌ 把外部协议包装成内部事件投递<br>❌ Handler/Producer 之间互调<br>❌ 抽象通用 Adapter 框架 |

**适配层核心认知**：HTTP Handler 是面向用户/前端的 Adapter，公开回调 Handler 是面向外部系统 HTTP 回调的 Adapter，AOP Producer 是面向外部 WS 事件/定时轮询的 Adapter——三者**同属适配层，职责完全相同**：把外部输入适配成 Domain 方法调用。Consumer 不在适配层（它处理 Domain 产生的内部事件）。出站外部调用统一封装在外部 DAO 中。详见 [docs/LAYERED_ARCHITECTURE_PRACTICE.md - 实践 7](./docs/LAYERED_ARCHITECTURE_PRACTICE.md)。

**Handler 设计补充**：HTTP Handler 与用户 Action 直接对应，一个接口按需求完成自己的请求级编排即可；复用优先通过组织 Command/Query 参数和调用 Domain 能力完成，不为了复用提前抽象 `BaseHandler` / `GenericActionHandler`。复杂业务规则、状态流转、权限语义必须下沉到 Domain。

### 3.2 目录结构

```
ai_orz/
├── common/                     # 公共共享 crate（前后端共用）
│   ├── src/api/               # API 请求响应 DTO 按功能分组
│   ├── src/constants/         # 公共常量、基础类型
│   ├── src/enums/            # 公共枚举（UserRole、TaskStatus 等）
│   ├── src/error/            # 统一错误类型
│   └── src/models/           # 跨层共享模型（ToolCallTraceRef、StatsInterval 等）
│
├── ai-orz-macros/             # 自定义宏 crate（日志宏、统计事件宏）
│
├── src/                        # 后端服务
│   ├── handlers/              # HTTP 接口层（适配层：用户 API + 外部回调，按业务域分组，每个方法一个文件）
│   │   └── a2a/               #   └─ A2A 公开回调端点（无 JWT 鉴权）
│   ├── producer/              # AOP 事件生产者（适配层：轮询 + 外部渠道 WS 事件接入）
│   ├── consumer/              # AOP 事件消费者（内部事件处理，消费 Domain 产生的内部事件）
│   ├── service/
│   │   ├── dao/               # 数据访问层 DAO（本地 DB CRUD + 外部 API 出站调用，如 lark/a2a/slack）
│   │   ├── dal/               # 业务数据访问层 DAL
│   │   └── domain/            # 领域层 Domain
│   ├── models/                # PO 持久化实体 + 业务实体 + 内部事件定义
│   ├── middleware/            # Axum 中间件
│   └── pkg/                   # 公共工具包
│       ├── aop/               # AOP 事件中心纯框架（Event/Producer/Consumer/Registry/Queue）
│       ├── adapter/           # 通用适配器基础设施（消息入站适配中台）
│       ├── stats/            # DuckDB 统计模块（record_event! 宏、查询 API）
│       └── *test_support.rs  # 测试支持文件（request_context、storage）
│
├── frontend/                   # Dioxus 前端（Tailwind CSS v4 + DaisyUI v5）
│   ├── src/api/               # API 客户端
│   ├── src/components/        # UI 组件（Button/Modal/Toast/State/Stats/Graph/GraphCanvas/Chat）
│   ├── src/hooks/             # 自定义 Hooks（use_resource/use_breakpoint/use_require_auth）
│   ├── src/layouts/           # 布局组件（AppLayout/Navbar）
│   ├── src/pages/             # 页面模块（按业务域分组）
│   ├── src/store/             # 状态管理（auth/toast）
│   ├── src/utils/             # 通用工具函数（按功能分子模块：time/file/message/status）
│   ├── styles/input.css       # Tailwind CSS 入口（主题配置、自定义工具类）
│   ├── public/output.css      # Tailwind 编译产物（构建时自动生成）
│   ├── build.rs               # 构建脚本（自动 npm install + Tailwind CSS 编译）
│   └── package.json           # npm 依赖（tailwindcss/daisyui/@tailwindcss/cli）
│
└── docs/                       # 详细设计文档
```

#### 3.1.1 基础设施公共工具位置约定（2026-07-12 新增，强制执行）

**核心原则：通用工具函数必须放在基础设施层，禁止散落在业务 DAO 中造成跨 DAO 依赖。**

| 工具类型 | 存放位置 | 示例 |
|----------|----------|------|
| **FTS5 全文搜索工具** | `src/pkg/storage/fts5.rs` | `escape_fts5_keyword` |
| **向量存储抽象** | `src/pkg/storage/vector.rs` | `VectorStore` trait |
| **日志宏** | `src/pkg/logging.rs` + `ai-orz-macros` | `log_info!`, `log_error!` |
| **统计事件宏** | `src/pkg/stats/` + `ai-orz-macros` | `record_event!` |
| **运行时统计基础设施** | `src/pkg/stats/runtime/` | `RuntimeStatsCollector<K>`（内存版，与 `pkg/stats/` 顶层 DuckDB 持久化版互补） |
| **JWT 工具** | `src/pkg/jwt.rs` | `encode_token`, `decode_token` |

**反模式（禁止）：**
- ❌ 在某个业务 DAO 中定义通用工具函数，其他 DAO 直接 import（造成 DAO → DAO 依赖）
- ❌ 为了复用在每个 DAO 中复制粘贴相同代码
- ❌ 把业务逻辑相关的工具放到 pkg 层（pkg 层必须无业务感知）

**正确模式：**
- ✅ 跨模块复用的通用工具 → 放到 `src/pkg/` 对应子模块
- ✅ 模块内部辅助函数 → 模块内部私有，不对外导出
- ✅ 单个文件使用的小工具 → 文件内定义，不上升到模块级

### 3.2 命名规范

| 元素 | 规范 | 示例 |
|------|------|------|
| **变量/函数/方法** | snake_case | `user_id`, `create_agent`, `get_user_by_id` |
| **类型/结构体/枚举/Trait** | PascalCase | `AgentPo`, `RequestContext`, `AgentDao` |
| **常量** | SNAKE_CASE | `MAX_SIZE`, `LOG_ID`, `DEFAULT_TIMEOUT` |
| **文件名/目录名** | snake_case | `agent.rs`, `request_context.rs`, `sqlite_test.rs` |

**函数/方法前缀约定：**

| 操作 | 前缀 | 示例 |
|------|------|------|
| 获取数据（有参数） | `get_` | `get_agent_by_id`, `get_user_name` |
| 获取单例/无参数 | 直接命名 | `agent_dao()`, `uid()` |
| 创建/新增 | `new_`, `create_` | `new_agent()`, `create_user()` |
| 修改/更新 | `update_` | `update_agent()` |
| 删除（软删除） | `delete_` | `delete_agent()` |
| 列表/批量 | `find_all`, `find_by_` | `find_all_agents()`, `find_by_org()` |
| 布尔判断 | `is_`, `has_`, `can_` | `is_deleted()`, `has_permission()` |

**集合变量：** 使用复数形式 `agents`, `user_ids`

**Trait 与实现类命名：**
- Trait 不加 `Trait` 后缀：`trait AgentDao { ... }`
- 实现类加 `Impl` 后缀：`struct AgentDaoSqliteImpl`

**Context 传递规范（强制执行）：**
- **所有 service 层（DAO/DAL/Domain）公共方法的第一个参数必须是 `ctx: RequestContext`**
- 用户相关信息从 `ctx.uid()` / `ctx.uname()` 获取，不再单独传参
- 内部私有方法可省略，只读操作也需要传递（便于日志记录）

**常见错误对照：**

| 错误写法 | 正确写法 |
|---------|---------|
| `userId`, `agentName` | `user_id`, `agent_name` |
| `newAgent()`, `createAgent()` | `new_agent()`, `create_agent()` |
| `getUserById()` | `get_user_by_id()` |
| `agentPO`, `OrgDAO` | `AgentPo`, `org_dao` |
| `maxSize`, `logId` | `max_size`, `log_id` |

### 3.4 数据对象四层清晰定义

| 对象类型 | 定义位置 | 用途 |
|----------|----------|------|
| **API DTO** | `common/src/api/**` | HTTP 请求/响应，前后端复用；通用响应包装使用 `common::api::ApiResponse<T>` |
| **跨层共享模型** | `common/src/models/**` | DAO/DAL/Domain/API 共用的结果结构体（StatsInterval、TimeSeriesPoint、TokenSumResult 等） |
| **Command/Query** | `src/service/domain/*/mod.rs` | Domain 层输入，表达业务意图 |
| **业务实体** | `src/models/*.rs` | 核心业务对象，包含行为和状态 |
| **PO (持久化对象)** | `src/models/*.rs` | 数据库映射，1:1 对应表结构 |

### 3.5 PO 与业务实体分层边界规范（2026-05-11 新增，强制执行）

**核心原则：PO 仅在 DAO/DAL 层内部使用，绝对不对外暴露到 Domain 层及以上**

#### 分层边界定义

| 层级 | 可使用对象 | 数据传递方式 | 说明 |
|------|------------|------------|------|
| **DAO 层** | 仅 PO | PO ↔ 数据库 | 单一数据源 CRUD，SQL 拼接，无业务逻辑；含外部 API 出站调用 |
| **DAL 层** | 内部：PO，对外：业务实体 | PO ↔ 业务实体 双向转换 | 组合 DAO，完成业务级数据操作 |
| **Domain 层** | 仅业务实体 | 业务实体 ↔ Command | 核心业务逻辑编排，产生内部事件，无 PO 依赖 |
| **Adapter 层** | 业务实体 + DTO/外部结构 | DTO/外部结构 ↔ Command | HTTP Handler + AOP Producer，外部协议转换与校验 |

#### 业务实体内部设计

**标准模式：业务实体内部持有 PO 字段**
```rust
// ✅ 正确：业务实体内部持有 PO，便于 DAL 层传递
pub struct Project {
    pub po: ProjectPo,
    // 可选：额外业务方法和字段
}

pub struct Task {
    pub po: TaskPo,
    // 业务方法...
}
```

**设计优势：**
1. **避免重复转换代码**：DAL 层直接通过 `&xxx.po` 传递给 DAO，无需字段逐一映射
2. **减少出错概率**：修改 PO 字段时只需修改一处，业务实体自动兼容
3. **100% 向后兼容**：现有测试和业务逻辑无需修改
4. **性能优化**：写操作使用引用传递 `&`，避免不必要的 clone

#### DAL 层接口签名规范

**所有 DAL 接口统一使用业务实体，不使用 PO：**
```rust
// ✅ 正确：写操作接收 &业务实体 引用
async fn create(&self, ctx: RequestContext, project: &Project) -> Result<(), AppError>;
async fn update(&self, ctx: RequestContext, project: &Project) -> Result<(), AppError>;

// ✅ 正确：读操作返回 业务实体
async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Project>, AppError>;
async fn list_by_user(&self, ctx: RequestContext, user_id: &str) -> Result<Vec<Project>, AppError>;

// ❌ 错误：直接使用 PO
// async fn create(&self, ctx: RequestContext, po: &ProjectPo) -> Result<(), AppError>;
```

#### RequestContext 跨层传递规范

**所有跨层 ctx 传递统一使用 `ctx.clone()`：**
```rust
// ✅ 正确：clone 后传递，避免所有权移动问题
self.project_dal.create(ctx.clone(), project).await?;

// ❌ 错误：直接移动，导致后续无法使用
// self.project_dal.create(ctx, project).await?;
```

**理由：**
- RequestContext 内部是 Arc 引用，clone 成本极低（仅指针复制）
- 避免所有权移动导致的编译错误
- 与 message domain 风格保持一致

#### 软删除设计规范

**`status = 0` 视为软删除，常规查询默认过滤：**
```rust
// DAO 层 find_by_id 示例
sqlx::query_as!(
    TaskPo,
    r#"SELECT ... FROM tasks WHERE id = ? AND "status" != 0"#,
    id
)
```

**典型应用：**
- `TaskStatus::Cancelled = 0` - 取消的任务视为已删除
- 需要查询历史/恢复时，使用 `query` 方法绕过过滤
- 测试需适配此行为：cancel 后 get 返回 None 是预期行为

---

## 四、关键约定（强制执行）

### 4.1 Trait 定义位置规范

| 层级 | Trait 定义位置 | 实现位置 | 示例 |
|------|---------------|---------|------|
| **DAO** | 子模块目录 `mod.rs`（如 `dao/agent/mod.rs`） | 各存储实现文件（如 `sqlite.rs`、`stats_duckdb.rs`） | `AgentDao` 定义在 `dao/agent/mod.rs`，实现在 `dao/agent/sqlite.rs` |
| **DAL** | 各自文件中（如 `dal/agent.rs`） | 同文件内 | `AgentDal` trait + impl 都在 `dal/agent.rs` |
| **Domain** | 主模块 `mod.rs`（如 `domain/message/mod.rs`） | 子模块文件中 | `MessageDelivery` trait 在 `domain/message/mod.rs`，`impl MessageDelivery for MessageDomainImpl` 在 `domain/message/delivery.rs` |

**Domain 层具体约定：**
- 主模块 `mod.rs` 中定义总 trait（如 `MessageDomain`）和所有子能力 trait（如 `MessageDelivery`、`MessageManagement`）
- 子模块文件（如 `delivery.rs`、`management.rs`）中写 `impl SubTrait for DomainImpl`，不要在子模块中定义新的 struct 包装器
- DomainImpl 结构体定义在主模块 `mod.rs` 中，子模块通过 `use super::DomainImpl` 引入

### 4.2 RequestContext 参数

**所有 service 层（DAO/DAL/Domain）公共方法的第一个参数必须是 `ctx: RequestContext`**

```rust
// ✅ 正确
fn wake_cortex(&self, ctx: RequestContext, provider: &ModelProvider, prompt: &str) -> Result<String>;

// ❌ 错误 - 缺少 ctx
fn wake_cortex(&self, provider: &ModelProvider, prompt: &str) -> Result<String>;
```

### 4.3 枚举类型安全

所有存储在数据库中的枚举状态/角色字段，**必须使用 Rust 枚举类型**，禁止直接使用 `i32` 存储

- 添加 `#[repr(i32)]` + `#[derive(sqlx::Type)]`
- 实现 `From<i64>` 适配 sqlx 类型推断
- 枚举统一定义在 `common/src/enums/`

### 4.4 SQLite + SQLx 规范

- **所有表必须启用 `STRICT` 模式**
- **SQL 关键字必须转义**：`status` → `"status"`
- **枚举字段显式标注**：`status as "status: TaskStatus"`
- **软删除约定**：已删除 `status = 0`，查询默认过滤
- **`.sqlx` 目录必须纳入版本控制**
- **测试使用 `#[sqlx::test]`**，每个测试独立内存数据库

### 4.5 Handler 拆分规范

- 按业务域分组（hr、finance、organization、user 等）
- **每个业务方法一个独立文件**，单个文件只放一个 handler 函数
- `mod.rs` 只保留模块导出，不存放实现
- 所有 DTO 从 `common/src/api/` 导入；通用响应包装统一使用 `common::api::ApiResponse<T>`，禁止在 `src/handlers` 定义本地 `ApiResponse`

### 4.6 测试隔离原则

- 无状态组件可使用单例（OnceLock）
- 有状态内存组件必须每次新建实例
- 测试使用独立数据库，不依赖全局状态
- 所有测试使用 `#[sqlx::test]` 宏

### 4.7 日志系统规范（强制执行，2026-05-15 新增）

**核心原则：项目内所有代码必须使用统一日志宏，禁止直接调用 tracing::*!**

#### 必须使用的宏

| 级别 | 宏名 |
|------|------|
| INFO | `log_info!` |
| WARN | `log_warn!` |
| ERROR | `log_error!` |
| DEBUG | `log_debug!` |

#### 两种调用模式（自动检测）

```rust
// ✅ 模式 1：无上下文（系统级别，第一个参数是字符串）
log_info!("application started");
log_info!("config loaded from {}", path);

// ✅ 模式 2：带上下文（请求级别，&ctx + operation 字符串）
log_info!(&ctx, "create_memory", "created memory id={}", memory_id);
log_error!(&ctx, "update_project", "db error: {:?}", err);
```

#### 禁止的写法

```rust
// ❌ 禁止：直接调用 tracing
tracing::info!("some message");

// ❌ 禁止：旧函数形式（已删除）
logging::info("some message");

// ❌ 禁止：传 ctx 值而非引用
log_info!(ctx, "operation", "message");  // 必须是 &ctx
```

#### 检测机制

宏通过**语法模式匹配顺序**自动区分两种模式：
1. **优先匹配**：第一个参数是**字符串字面量** → 无上下文模式
2. **兜底匹配**：第一个参数是表达式，第二个是字符串字面量 → 带上下文模式

> 💡 **重要**：Operation 必须是字符串字面量，不能是变量。完整规范请参考 [docs/logging_design.md](./docs/logging_design.md)

### 4.8 向量化实体规范（强制执行，2026-07-24 新增）

**核心原则：所有支持向量索引的 PO 必须实现 `Vectorizable` trait，禁止在 DAL 层手工拼接向量文本**

#### 必须实现的 Trait

```rust
// src/models/vector.rs
pub trait Vectorizable: Send + Sync {
    /// 生成待向量化的文本内容（由 PO 自己决定哪些字段参与向量化）
    fn vectorize_text(&self) -> String;

    /// 向量集合名称（对应 vss_{collection} 表）
    fn vector_collection() -> &'static str where Self: Sized;

    // 以下为默认实现，通常无需覆盖
    fn vector_content_hash(&self) -> String { ... }   // 默认 SHA256
    fn vector_expire_at(&self) -> Option<i64> { ... } // 默认永不过期
    fn needs_reindex(&self, existing_hash: &str) -> bool { ... }
}
```

#### 调用规范

| 场景 | 正确写法 | 错误写法 |
|------|---------|---------|
| 索引场景（create/update） | `cortex_dao.embed_entity(ctx, cortex, po)` | `cortex_dao.embed_text_for_search(ctx, cortex, &po.some_field)` |
| 重建索引（rebuild） | `cortex_dao.embed_entity(ctx, cortex, po)` | `cortex_dao.embed_text_for_search(ctx, cortex, &format!(...))` |
| 获取 collection 名 | `Po::vector_collection()` | 硬编码 `"namespace"` 字符串 |

#### 已实现 Vectorizable 的实体

| 实体 | collection | 向量化字段 |
|------|-----------|-----------|
| `AgentPo` | `agents` | name + role + description + capabilities |
| `ToolPo` / `Tool` | `tools` | name + description + tags |
| `TaskPo` | `tasks` | name + description |
| `SkillPo` / `Skill` | `skills` | name + description + tags |
| `ShortTermMemoryIndexPo` | `memory:short_term` | summary + tags |
| `LongTermKnowledgeNodePo` | `memory:knowledge_node` | node_description + summary + tags |

#### 禁止的写法

```rust
// ❌ 禁止：在 DAL 层手工拼接向量文本
let text = format!("{}\n{}", po.summary, po.tags);
cortex_dao.embed_text_for_search(ctx, cortex, &text).await

// ❌ 禁止：在 PO 上添加独立的 vector_text() 方法（应实现 trait）
impl ShortTermMemoryIndexPo {
    pub fn vector_text(&self) -> String { ... }  // 应改为 impl Vectorizable
}

// ❌ 禁止：硬编码 collection 名
ctx.vector_store().clear_collection("memory:short_term").await
```

#### 正确的写法

```rust
// ✅ PO 实现 Vectorizable trait
impl Vectorizable for ShortTermMemoryIndexPo {
    fn vectorize_text(&self) -> String {
        // PO 自己决定哪些字段参与向量化
        let tags = flatten_tags(&self.tags);
        if tags.is_empty() { self.summary.clone() }
        else { format!("{}\n{}", self.summary, tags) }
    }
    fn vector_collection() -> &'static str { "memory:short_term" }
}

// ✅ DAL 层调用 embed_entity（自动调用 po.vectorize_text()）
match try_build_vector_params_for_entity(ctx, &cortex_dao, &model_provider_dao, &po).await {
    Ok(Some(params)) => { ... }
    Ok(None) => { /* 无 Embedding Provider，跳过 */ }
    Err(e) => { /* 降级 warn */ }
}

// ✅ 通过 trait 获取 collection 名
ctx.vector_store().clear_collection(ShortTermMemoryIndexPo::vector_collection()).await
```

> 💡 **设计动机**：将"哪些字段参与向量化"的知识封装在 PO 内部（信息专家原则），DAL 层无需感知 PO 的字段结构。未来调整向量化字段组合，只需改 PO 的 `vectorize_text()` 一处。

---

### 4.9 查询接口分页规范（强制执行，2026-07-24 新增）

**核心原则：query 是核心查询能力，list 是语法糖；两者统一返回 `PagedResult<T>`**

#### 设计哲学

| 接口类型 | 职责 | HTTP 方法 | 参数位置 | 返回 |
|---------|------|----------|---------|------|
| **query（核心）** | 完整查询条件 + 分页 | POST body | `XxxQueryRequest { ...查询条件..., pagination }` | `PagedResult<T>` |
| **list（语法糖）** | 只接受分页，内部固定默认过滤和排序 | GET query param | `?limit=10&offset=0` | `PagedResult<T>` |

**list 的"语法糖"含义**：
- 只接受分页参数（limit/offset），**不接受任何查询功能**（ids/status/keyword 等）
- 内部固定默认过滤（如排除 Deleted/Expired 状态）和默认排序（如 created_at DESC）
- 面向"给我第一页数据"的简单列表场景
- **任何涉及查询的操作（ids 批量查询、status 过滤、keyword 搜索等）必须走 query 接口**

#### 分页基础设施（common/src/api/mod.rs）

```rust
/// 统一分页参数
pub struct PaginationParams {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// 统一分页结果
pub struct PagedResult<T> {
    pub items: Vec<T>,       // 当前页数据
    pub total: usize,        // 总条数（忽略分页）
}

impl<T> PagedResult<T> {
    /// 转换 items 类型，保留 total（用于 PO → 业务实体 → ListItem 链式转换）
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> PagedResult<U> { ... }
}
```

#### 全链路分页参数传递

```
Handler                   Domain                   DAL                   DAO
   │                        │                       │                     │
   ├─ XxxQueryRequest ──► XxxQuery ──────────────► XxxQuery ──────────► SQL
   │  (含 pagination)       (含 pagination)         (含 pagination)        │
   │                        │                       │                     │
   ◄─ PagedResult<T> ── ◄─ PagedResult.map(from_po) ◄─ PagedResult<Po> ◄─ COUNT + LIMIT/OFFSET
```

**关键约束**：
- pagination 字段随 Query 结构体一起传递，不需要单独的方法参数
- 每层用 `PagedResult::map()` 转换内部类型，保留 total
- DAO 层的 `query` 方法签名统一返回 `Result<PagedResult<Po>>`

#### DAO 层实现模式

每个 DAO 的 sqlite.rs 必须抽取 `push_query_filters` 函数，COUNT 和 LIST 查询复用同一套 WHERE 条件：

```rust
/// 推送查询过滤条件（COUNT 和 LIST 复用）
fn push_query_filters<'args>(
    builder: &mut sqlx::QueryBuilder<'args, sqlx::Sqlite>,
    query: &XxxQuery,
) {
    if let Some(ids) = &query.ids { /* ... */ }
    if let Some(status) = &query.status { /* ... */ }
    // ... 其他过滤条件
}

async fn query(&self, ctx: RequestContext, query: XxxQuery)
    -> Result<common::api::PagedResult<XxxPo>>
{
    // 1. COUNT 查询（复用 filters）
    let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM xxx WHERE 1=1");
    push_query_filters(&mut count_builder, &query);
    let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;

    // 2. LIST 查询（复用 filters + LIMIT/OFFSET）
    let mut list_builder = sqlx::QueryBuilder::new("SELECT ... FROM xxx WHERE 1=1");
    push_query_filters(&mut list_builder, &query);
    list_builder.push(" ORDER BY created_at DESC");

    if let Some(limit) = query.pagination.limit {
        list_builder.push(" LIMIT ").push_bind(limit as i64);
    } else if query.pagination.offset.is_some() {
        list_builder.push(" LIMIT -1");  // SQLite: offset 单独使用需 LIMIT -1
    }
    if let Some(offset) = query.pagination.offset {
        list_builder.push(" OFFSET ").push_bind(offset as i64);
    }

    let items = list_builder.build_query_as().fetch_all(pool).await?;
    Ok(common::api::PagedResult { items, total: total as usize })
}
```

#### Handler 层模式

**query handler**（POST，接受完整查询条件 + pagination）：

```rust
pub async fn query_agents(
    ctx: RequestContext,
    params: AgentQueryRequest,
) -> Result<common::api::PagedResult<AgentListItem>> {
    let page = domain().agent_manage().query(ctx, AgentQuery {
        ids: params.ids,
        status: params.status,
        pagination: params.pagination,  // 透传分页参数
        ..Default::default()
    }).await?;

    Ok(page.map(|agent| AgentListItem { ... }))  // 用 map 转换类型
}
```

**list handler**（GET，只接受分页，内部固定默认过滤）：

```rust
pub async fn list_agents(
    ctx: RequestContext,
    params: ListAgentsRequest,  // 只含 pagination 字段
) -> Result<common::api::PagedResult<AgentListItem>> {
    // list 是语法糖：内部固定排除 Deleted
    let page = domain().agent_manage().query(ctx, AgentQuery {
        exclude_status: Some(AgentStatus::Deleted),  // 固定默认过滤
        pagination: params.pagination,
        ..Default::default()
    }).await?;

    Ok(page.map(|agent| AgentListItem { ... }))
}
```

#### 各实体的 list 默认过滤和排序

| 实体 | list 默认过滤 | list 默认排序 |
|------|-------------|-------------|
| Agent | `exclude_status = Deleted` | `created_at DESC` |
| Project | `status != 0`（软删除） | `priority DESC, created_at DESC` |
| Task | `status != 0`（软删除） | `priority DESC, created_at DESC` |
| Tool | 无 | `created_at DESC` |
| Skill | `exclude_status = Expired` | `updated_at DESC` |

#### 禁止的写法

```rust
// ❌ 禁止：list 接口接受查询字段
pub struct ListAgentsRequest {
    pub status: Option<AgentStatus>,    // 应移除，走 query 接口
    pub ids: Option<Vec<String>>,       // 应移除，走 query 接口
}

// ❌ 禁止：DAO 层 query 方法返回 Vec 而非 PagedResult
async fn query(&self, ctx: RequestContext, query: AgentQuery) -> Result<Vec<AgentPo>>;

// ❌ 禁止：在 DAL/Domain 层手工拼接 limit/offset 而不用 PaginationParams
let limit = query.limit;  // 应为 query.pagination.limit

// ❌ 禁止：Handler 层把 PagedResult 当 Vec 用
let agents: Vec<Agent> = domain().agent_manage().query(ctx, q).await?;  // 应取 .items
```

#### 参考实现

- **基础设施**：`common/src/api/mod.rs` 的 `PaginationParams` 和 `PagedResult<T>`
- **DAO 参考实现**：`src/service/dao/mcp_server/sqlite.rs`（首个完成分页改造的 DAO）
- **已改造的 10 个实体**：Agent / Project / Task / Tool / Skill（DAO + Domain + Handler 全链路）+ McpServer / MessageChannel / Artifact / ModelProvider / User（仅 DAO query 通用化，Handler 按需适配）

> 💡 **设计动机**：统一分页接口避免每个实体自定义分页逻辑，list 作为语法糖降低简单场景的使用成本，query 作为核心能力覆盖所有复杂查询需求。前端只需处理统一的 `PagedResult<T>` 结构。

---

### 4.10 通用 count 方法规范（强制执行，2026-07-25 新增）

**核心原则：count 与 query 复用查询结构体和 SQL 拼接逻辑；特定 count_* 方法退化为语法糖直接调用通用 count**

#### 设计哲学

| 接口类型 | 职责 | SQL 复用 | 返回 |
|---------|------|---------|------|
| **count（核心）** | 统计符合 Query 条件的总数 | 复用 `push_query_filters`，只跑 `SELECT COUNT(*)` 不跑 LIST | `u64` |
| **count_by_xxx（语法糖）** | 针对单字段条件的快捷方法 | 内部构造 Query 后调用通用 count | `u64` |

**与 4.9 分页规范的关系**：4.9 规定 `query` 返回 `PagedResult<T>`（含 items + total），其中 total 来自 COUNT 查询；本规范将 COUNT 抽取为独立的通用方法，避免每次只为拿 total 跑完整 query。

#### 三层透传链路

```
Handler                   Domain                   DAL                   DAO
   │                        │                       │                     │
   ├─ XxxQuery ──────────► count_xxx(ctx, query) ─► count(ctx, query) ─► SELECT COUNT(*)
   │  (复用 Query 结构体)    (透传 DAL)              (透传 DAO)            │
   │                                                │                     │
   ◄─ u64 ────────────── ◄─ u64 ──────────────── ◄─ u64 ───────────── ◄─ COUNT(*) AS total
```

**关键约束**：
- 三层 count 方法的签名统一：`async fn count(&self, ctx: RequestContext, query: XxxQuery) -> Result<u64>`
- Domain 层方法命名可以叫 `count_agents` / `count_projects` 等（更贴近业务语义），但内部只透传 DAL 的 `count`
- 特定的 `count_by_xxx` 方法（如 `count_by_assignee`、`count_by_root_user_and_status`）一律改为构造 Query 后调用通用 count

#### DAO 层实现模式

每个 DAO 的 sqlite.rs 必须复用 `push_query_filters`（与 `query` 方法共享同一套 WHERE 条件）：

```rust
async fn count(&self, ctx: RequestContext, query: XxxQuery) -> Result<u64> {
    let pool = ctx.db_pool();
    let mut count_builder = sqlx::QueryBuilder::new("SELECT COUNT(*) FROM xxx WHERE 1=1");
    push_query_filters(&mut count_builder, &query);  // 复用与 query 相同的过滤逻辑
    let total: i64 = count_builder.build_query_scalar().fetch_one(pool).await?;
    Ok(total as u64)
}

/// 语法糖：按 assignee 统计
async fn count_by_assignee(&self, ctx: RequestContext, assignee_id: &str) -> Result<u64> {
    // 语法糖：调用通用 count
    self.count(ctx, XxxQuery {
        assignee_id: Some(assignee_id.to_string()),
        ..Default::default()
    }).await
}
```

#### 各层实现要点

| 层级 | 实现要求 |
|------|---------|
| **DAO** | `count(ctx, query)` 复用 `push_query_filters`；所有 `count_by_xxx` 改为构造 Query 后调用 `self.count(...)` |
| **DAL** | `count(ctx, query)` 透传 DAO；所有 `count_by_xxx` 改为构造 Query 后调用 `self.count(...)` |
| **Domain** | `count_xxx(ctx, query)` 透传 DAL；特定 `count_by_xxx` 同样构造 Query 后调用通用 `count_xxx` |

#### 已落地的实体

| 实体 | 通用 count | 退化为语法糖的方法 |
|------|-----------|------------------|
| Agent | ✅ `AgentDao::count(ctx, AgentQuery)` | （无特定 count 方法） |
| Project | ✅ `ProjectDao::count(ctx, ProjectQuery)` | `count_by_root_user`、`count_by_root_user_and_status` |
| Task | ✅ `TaskDao::count(ctx, TaskQuery)` | `count_by_assignee`、`count_by_assignee_and_status` |
| Message | ✅ `MessageDao::count(ctx, MessageQuery)` | `count_by_task_id` |
| Artifact | ✅ `ArtifactDao::count(ctx, ArtifactQuery)` | `count_by_project`、`count_by_task` |
| User | ✅ `UserDao::count(ctx, UserQuery)` | `count_by_organization_id` |
| Organization | ✅ `OrganizationDao::count(ctx, OrganizationQuery)` | `count_all` |

#### 禁止的写法

```rust
// ❌ 禁止：在 count 方法中独立拼接 WHERE 条件，不复用 push_query_filters
async fn count(&self, ctx: RequestContext, query: XxxQuery) -> Result<u64> {
    let mut sql = String::from("SELECT COUNT(*) FROM xxx WHERE 1=1");
    if query.assignee_id.is_some() { sql.push_str(" AND assignee_id = ?"); }  // 应复用 push_query_filters
    // ...
}

// ❌ 禁止：count_by_xxx 方法独立实现 SQL，不调用通用 count
async fn count_by_assignee(&self, ctx: RequestContext, assignee_id: &str) -> Result<u64> {
    let count = sqlx::query!("SELECT COUNT(*) FROM xxx WHERE assignee_id = ?", assignee_id)
        .fetch_one(ctx.db_pool()).await?;
    Ok(count.count as u64)  // 应改为 self.count(ctx, XxxQuery { ... }).await
}

// ❌ 禁止：DAL/Domain 层独立实现 count 逻辑，不透传到 DAO
async fn count_by_xxx(&self, ctx: RequestContext, ...) -> Result<u64> {
    let items = self.query(ctx, ...).await?;  // 不能用 query 然后取 len()
    Ok(items.len() as u64)
}
```

#### 参考实现

- **DAO 层**：`src/service/dao/project/sqlite.rs` 的 `count` + `count_by_root_user` + `count_by_root_user_and_status`
- **DAL 层**：`src/service/dal/project.rs` 的 `count` + `count_by_root_user`（语法糖）
- **Domain 层**：`src/service/domain/project/project.rs` 的 `count_projects`（透传 DAL）

> 💡 **设计动机**：将 count 与 query 的 WHERE 条件统一到 `push_query_filters` 一处，避免「count 漏掉某个过滤条件」的常见 bug。特定 count 方法退化为语法糖后，新增查询条件时只需改 `push_query_filters` 一处，所有 count_by_xxx 自动同步。

---

## 五、核心概念与实体关系

### 5.1 实体关系

```
Organization (组织)
├── User (用户)
├── ModelProvider (模型配置)
└── Agent (智能代理)
     └── Brain
          └── Cortex
               └── ModelProvider (LLM 配置)
```

### 5.2 Agent 思考 + 记忆

```
Agent
└── Brain
     ├── Cortex           # 思考执行，绑定 ModelProvider
     └── Memory           # 记忆系统
          ├── Core        # 核心认知：角色设定、能力清单
          ├── Working     # 当前会话工作记忆
          ├── Short-Term  # 最近会话摘要索引
          └── Long-Term   # 长期沉淀知识图谱
```

---

## 六、工作流与开发记录

> 💡 **记录原则**：仅保留最近里程碑的详细信息，早期里程碑按月汇总。所有重构背景、问题、解决方案、避坑指南归档在 [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md)，开发前建议先看该文档避免重蹈覆辙。

### 2026-07-27 里程碑（集成测试与 CI 质量体系建设）
**✅ 集成测试基础设施 + 29 个集成测试覆盖全链路**
- **测试脚手架**：`tests/common/` 公共模块（env/app/factories/assertions），`init_full_test_env` 用 `OnceCell` 串行化初始化避免全局 DB 竞争，`bootstrap_system` 一键创建组织+管理员+chat provider，`bootstrap_and_login` 返回 JWT
- **5 个测试套件**：auth_sysinit（4）+ core_crud（3）+ message_delivery（2）+ vector_degradation（3）+ a2a_flow（2）= 14 个 HTTP 集成测试，覆盖 JWT Cookie 认证、系统初始化、Agent/Project/Task CRUD 闭环、消息投递、SSE 冒烟、向量降级契约、A2A agent card 发现 + tasks/send→get 全链路
- **向量降级契约守护**：`vector_degradation_test` 显式验证"无 embedding provider 时主流程仍可用"，防止后续重构破坏降级机制

**✅ Clippy 442 warning 清理 + 3 个真 bug 修复**
- **442 warning → 0**：collapsible_if（198）+ 手工清理（111）+ enum `#[default]`（63）+ too_many_arguments（52）+ dead_code（27）+ module_inception（4）+ 其他（13）
- **真 bug 1：`unconditional_recursion`** — `common/src/enums/agent.rs` 和 `task.rs` 的 `From<i32>` 实现里 `v.into()` 解析为自身，造成无限递归栈溢出（3 处）
- **真 bug 2：`await_holding_lock`** — `pkg/aop/core/registry.rs` 的 `start_all()` 持有 `started` 写锁跨 `producer.start().await`，死锁隐患
- **真 bug 3：`let_underscore_future`** — `tool_call_logger.rs` 中 `let _ = stats.record(...)` 静默丢弃 future，统计从未真正记录
- **CI 启用** `-D warnings` 强制门槛，未来新代码引入 warning 会被 CI 直接拦截

**✅ 集成测试速度优化（238s → 3.7s，63 倍提升）**
- **根因**：并行测试时多个 bootstrap 同时创建 embedding provider 互相干扰，触发 FastEmbed 模型加载（75s/测试）
- **方案**：`InitializeSystemRequest.embedding_model` 改为 `Option<>` + `#[serde(default)]`，`bootstrap_system` 传 `None` 跳过创建，DB 里永远没有 embedding provider，所有实体创建走 `Ok(None)` 降级路径
- **前端修复**：`reception.rs` 初始化表单添加对话模型和向量模型配置字段（修复 86b3c29 引入的预先存在 bug）

**✅ cargo-llvm-cov 覆盖率门槛**
- **工具**：从 tarpaulin 替换为 cargo-llvm-cov（纯 LLVM source-based coverage，不依赖 ptrace，更稳定）
- **门槛**：`--fail-under-lines 35`
- **过滤**：`--ignore-filename-regex` 过滤依赖库、测试脚手架、build script
- **优化**：`--no-clean` 复用编译产物 + sccache 跨 CI run 共享依赖库编译结果

### 2026-07-26 里程碑（宏 (true, true) 分支修复）
**✅ generate_http_handler 宏 (true, true) 分支用 RawQuery + serde_json::Value 重写**
- **设计动机**：原 `(true, true)` 分支生成 `Path + Query<ParamsTy> + Json<ParamsTy>`，存在两个 bug：1) `Query<ParamsTy>` 尝试从 query string 反序列化所有字段（含必填 path 字段如 `id: String`），缺失字段时返回 400；2) `Json<ParamsTy>` 对无 body 的 GET 请求返回 400。导致 10 个生产 path+query struct 的 GET 请求全部失效
- **修复方案**：用 `axum::extract::RawQuery` 提取原始 query 字符串（不会因缺失字段报错），用 `serde_urlencoded` 解析为 `HashMap`，再构建 `serde_json::Value`（带类型推断 bool/number/null/string），通过 `params.{ident} = parsed` 类型推导反序列化各 query 字段，规避 macro hygiene 问题（handler 文件可能未导入字段类型如 `ToolStatus`）
- **子分支自动判定**：当所有非 path 字段都是 query 字段（无 body 字段）时走 path+query only 子分支（`Path + RawQuery + Default::default()`，无 Json 提取器）；当存在 body 字段时走 path+query+body 混合子分支（`Path + RawQuery + Json`，path > query > body 优先级）
- **flatten query 支持**：`collect_path_and_query_fields_from_type` 重构为返回 4 元组，新增 `flattened_query_fields` 分别存放 `#[serde(flatten)]` 标注的 query 字段（如 `PaginationParams`），用整个 `query_value` 反序列化
- **测试覆盖**：新增 15 个 axum 集成测试覆盖 path+query only GET、path+query+body 混合 PUT、enum 类型（ToolStatus）、flatten pagination、数值类型（u32/f64）、缺失 Option 字段、path > query > body 优先级等边界场景，全部通过
- **影响范围**：10 个生产 path+query struct（GetAgentRequest、ListMcpToolsByServerRequest、ListArtifactsRequest 等）全部修复；18 个 path-only struct 补 Default derive；主项目 Cargo.toml 新增 `serde_urlencoded = "0.7"` 依赖
- **规范沉淀**：[docs/design/unified-idl-http-handler.md](./docs/design/unified-idl-http-handler.md) 更新支持的组合表、新增 Query 字段提取实现细节小节和修复历史

### 2026-07-25 里程碑（通用 count 方法）
**✅ 通用 count 方法三层透传 + 特定 count 退化为语法糖**
- **设计动机**：health_metrics 6 个维度需要跨多个 domain 拿 count，发现各 DAO 的 count_by_xxx 方法各自实现 SQL，与 query 的 WHERE 条件不共享，存在「count 漏掉过滤条件」的隐患
- **DAO 层通用 count**：7 个 DAO（Agent/Project/Task/Message/Artifact/User/Organization）trait 新增 `count(ctx, query) -> Result<u64>` 方法，统一复用 `push_query_filters` 拼接 WHERE 条件，只跑 `SELECT COUNT(*)` 不跑 LIST
- **特定 count 退化为语法糖**：11 个 `count_by_xxx` 方法（如 `count_by_root_user`、`count_by_assignee_and_status`、`count_by_task_id`、`count_by_organization_id` 等）改为构造 Query 后调用通用 count，消除重复 SQL 拼接
- **三层透传**：DAL 的 `count` 直接透传 DAO；Domain 层新增 `count_agents`/`count_projects`/`count_tasks`/`count_organizations`/`count_users` 等业务语义方法，内部透传 DAL
- **测试统计**：14 个 count 相关测试全部通过（project/task/message/artifact/user/organization 各层覆盖）
- **规范沉淀**：[AGENTS.md](./AGENTS.md) 新增 4.10 通用 count 方法规范

### 2026-07-25 里程碑（统计图表 Phase 1）
**✅ 统计图表基础设施 + 实体详情页时序图**
- **HUD 背景工具抽取**：新增 `frontend/src/components/hud_palette.rs`，从 graph_canvas.rs 提取 HUD 背景绘制（径向渐变 + 网格 + 四角装饰 + hex_to_rgba），供知识图谱和统计图表共享，统一驾驶舱视觉语言
- **ChartRenderer trait**：新增 `frontend/src/components/chart_scene.rs`，定义图表渲染器 trait 供未来图表扩展（折线/柱状/环形）
- **HUD 风格折线图**：新增 `frontend/src/components/charts/line_chart.rs`，消费 `Vec<TimeSeriesPoint>` 时序数据，视觉对齐知识图谱 HUD（深色径向渐变背景 + 橙色折线 + shadow_blur 发光 + 数据点呼吸光晕 2.4s 周期 + 折线流光 line_dash_offset 滚动 + 坐标轴刻度 + X 轴日期标签）
- **4 个 StatsPanel 时序图**：AgentStatsPanel / ProjectStatsPanel / TaskStatsPanel / ModelProviderStatsPanel 在数字卡片下方渲染 LineChart，消费后端已就绪的 `model_call_time_series` 字段（此前前端从未读取该字段）
- **测试统计**：前端测试 35 个（+1 新增 line_chart 单元测试），100% 通过

### 2026-07-25 里程碑（统计图表 Phase 2）
**✅ Project 任务状态分布环形图（donut_chart）**
- **DonutChart 组件**：新增 `frontend/src/components/charts/donut_chart.rs`，消费通用 `Vec<DonutSlice>` 数据结构，绘制 HUD 风格环形图（深色径向渐变背景 + 多色扇区 shadow_blur 发光 + 扇区间隙 + 外圈呼吸光晕 2.4s 周期 + 中心总数标签）
- **图例职责分离**：Canvas 只画环形图，图例由 Dioxus + DaisyUI 渲染（彩色圆点 + 标签 + 数值 + 百分比），避免 Canvas 文字模糊
- **task_status_color 辅助函数**：`utils/status.rs` 新增 `task_status_color(status: i32) -> &'static str`，返回 6 种状态对应的 HUD 风格鲜艳颜色（红 #ef4444 / 橙黄 #f59e0b / 蓝 #3b82f6 / HUD 主色橙 #fa520f / 绿 #10b981 / 灰 #6b7280）
- **Project 详情页集成**：概览 Tab 的"项目概览"卡片中，把原"任务统计"文字网格升级为 DonutChart + 图例组合展示；按 6 种状态全量统计（进行中→待处理→待审核→已完成→已归档→已取消），过滤 0 值状态避免图例冗余；无任务时显示"暂无任务"提示
- **测试统计**：前端 38 测试（新增 3 个 donut_chart 测试）+ 后端 746 测试 + common 50 测试 100% 通过，总计 834 测试

### 2026-07-25 里程碑（统计图表 Phase 3）
**✅ AOP 实时内存统计 + 轮询渲染**
- **设计哲学**：AOP 是运行时能力，重启即丢，记录到 DuckDB 无持久化价值。采用纯内存统计收集器，与 AOP 事件本身生命周期一致
- **AopMetricsHook trait**：`pkg/aop/core/metrics_hook.rs` 新增 4 回调 trait（on_publish/on_consume_start/on_consume_success/on_consume_failure），Registry 持有 `Option<Arc<dyn AopMetricsHook>>`，业务层注入实现，保持 AOP 框架零业务依赖原则
- **AopStatsCollector 内存收集器**：`consumer/aop_stats_collector.rs` 纯内存实现（零 DuckDB 依赖），提供总计数器（按 event_kind/consumer_name/status 三维索引）+ 滑动窗口时序数据（最近 60 分钟，按分钟桶，内存占用 < 50KB）
- **AopStatsHook 业务实现**：`consumer/aop_stats_hook.rs` 实现 AopMetricsHook，4 回调用 `tokio::spawn` 调 collector.record（不阻塞 AOP 主流程）
- **3 处埋点**：publish 同步/异步分发 + worker 协程 on_event 调用前后；每个 AOP 事件产生 2-3 条记录（published + consuming + success/failed）
- **SystemDomain AopStats 子能力**：SystemDomain 新增 `aop_stats()` getter + AopStats trait，直接读全局 collector（零 DAO/DAL 中转）
- **3 个 HTTP 端点**：`GET /api/v1/system/aop/stats/{overview|time-series|distribution}`，毫秒级响应（纯内存查询）
- **前端 AOP 页面 Tab 改造**：Tab 1 实时监控（保留现有功能），Tab 2 统计图表（概览卡片 + LineChart 时序 + DonutChart 状态分布 + DonutChart 消费者分布），5 秒轮询自动刷新（基于 js_sys::Promise + set_timeout 的 sleep_ms，避免 tokio 依赖）
- **测试统计**：后端 753 测试（+7 新增 aop_stats 测试：6 collector + 1 hook）+ 前端 38 测试 + common 50 测试 100% 通过，总计 841 测试

### 2026-07-25 里程碑
**✅ 知识图谱 Canvas HUD 驾驶舱风格 + 聊天共享组件抽取 + utils 模块化**
- **知识图谱 Canvas HUD 渲染**：新增 `KnowledgeGraphRenderer` 实现 `CanvasRenderer` trait，HUD 风格渲染（深色径向渐变背景 + 淡橙色网格 + 四角 HUD 装饰；节点选中态扫描环 + 旋转刻度环，未选中态呼吸光晕；边实线流光 + drop-shadow 发光）；知识图谱页右上角 Canvas/SVG 风格切换按钮（join 按钮组），默认 Canvas，SVG 作为兜底；`KnowledgeGraphCanvas` 基于 `CanvasScene` 基础设施，关闭力导向布局和自带粒子避免视觉过载；web-sys features 扩展 `CanvasGradient` 支持渐变效果
- **Workspace HUD 流光提示**：未读消息提示由静态红点升级为 2px 橙色竖条贴在侧边栏项左侧边缘，带 `box-shadow` 形成 glow 光晕，高亮段从上往下流动（1.8s 周期，cubic-bezier(0.4, 0, 0.6, 1) 缓动），像 HUD 扫描线；点击切换视图清除
- **聊天共享组件抽取**：新增 `frontend/src/components/chat/` 模块：`MessageBubble`（单条消息气泡，文本/图片/文件简版渲染）、`TypingIndicator`（Agent 输入指示器，三点动画）。Agent 详情页、Workspace 底部对话框改用共享组件，删除本地重复实现；主对话页 `pages/message/chat.rs` 因含工具调用卡片、任务卡片、视频/音频附件等复杂内容且使用 DaisyUI `chat chat-start/chat-end` 样式，保留独立富渲染实现
- **utils 模块化**：原 `frontend/src/utils.rs` 拆分为 `frontend/src/utils/` 文件夹，按功能分子模块：`time.rs`（时间格式化）、`file.rs`（文件大小格式化）、`message.rs`（消息类型常量、角色映射、乐观消息辅助）、`status.rs`（任务/项目状态映射）。`mod.rs` 通过 `pub use` 重新导出，保持 `use crate::utils::xxx` 向后兼容
- **Workspace 对话机制**：底部对话框跟随当前视图（默认/Project/Agent），SSE 实时消息，自动未读消息源追踪（`project_unread`/`agent_unread` Signal<HashSet<String>>）
- **DTO PartialEq 派生**：`common::api::MessageListItem` 和 `FileMetaInfo` 添加 `PartialEq` 派生（Dioxus 0.7 组件 prop 要求）
- **测试统计**：前端 34 测试 + 后端 746 测试 + common 50 测试 100% 通过

### 2026-07-24 里程碑
**✅ query/list 接口分页改造 + list 接口简化（统一 PagedResult）**
- **设计原则落地**：query 是核心查询能力（POST body，完整查询条件 + pagination），list 是语法糖（GET query param，只接受分页，内部固定默认过滤和排序）；两者统一返回 `PagedResult<T> { items, total }`
- **全链路改造**：5 个实体（Agent/Project/Task/Tool/Skill）的 Query 结构体加 `pagination: PaginationParams`；DAO 层抽取 `push_query_filters` 函数复用 WHERE 条件；Domain 层 `query` 改返回 `PagedResult<业务实体>`；Handler 层 `ListXxxRequest` 简化为只含 pagination，list handler 内部固定默认过滤（如 Agent 排除 Deleted，Skill 排除 Expired）
- **前端适配**：API 层新增 5 个 query_* 函数，list_* 简化为只接受 (limit, offset)；6 个查询场景改用 query_* 接口
- **规范文档**：[AGENTS.md](./AGENTS.md) 新增 4.9 查询接口分页规范；[runtime_design.md](./docs/runtime_design.md) 第十三章从草稿更新为已实现状态
- **参考实现**：`src/service/dao/mcp_server/sqlite.rs` 为首个完成分页改造的 DAO

**✅ 记忆 tags 全链路支持 + 知识图谱节点可视化增强（v3.5）**
- **后端 tags 过滤**：`SearchMemoryParams`/`QueryMemoryParams`/`MemoryResult` 新增 tags 字段；`MemoryQuery.tags` 实现 OR 语义过滤（SQLite `json_each`）；4 个查询/搜索方法增加 tags 过滤分支
- **Vectorizable trait 对齐**：`ShortTermMemoryIndexPo`/`LongTermKnowledgeNodePo` 实现 `Vectorizable` trait，DAL 层统一使用 `embed_entity` 替代手动拼接
- **前端知识图谱节点可视化增强**：GraphNode 新增 tags + summary 字段；多色边框（每个 tag 一段 arc path，hash 稳定取色）；动态半径（信息越多节点越大）；节点下方简介展示
- **前端 tags 展示**：知识图谱搜索区新增 tags 过滤输入框；短期记忆搜索页/Agent 记忆面板结果项展示 tags 徽章

### 2026-07-23 里程碑
**✅ 唤醒流程重构：移除 built-in tools 概念，Auto/Manual 分流**
- **PromptBuilder 顺序调整**：从稳定到易变（人设 → 神经工具/技能 → 常用工具/必加载技能 → 用户画像 → 历史 → 工具失败 → Trace ID + 当前消息）
- **Tag 渐进式加载**：统一查询后 `build()` 时按 tag 分块拼装；`match_keys = agent.roles ∪ installed_tags`
- **AgentFetchOptions 扩展**：新增 `with_tools` + `with_skills` 选项，consumer 加载 Agent 时显式请求
- **工具加载移至 domain 层**：`HrDomainImpl::get_agent(with_tools=true)` 加载绑定工具（enabled_only DB 过滤）+ tag 匹配工具（neural + installed_tags），合并去重写入 `agent.tools`
- **唤醒时 Auto/Manual 分流**：`wake_agent_brain` 用 `std::mem::take` + `partition` 分离所有权，Auto→Rig / Manual→Prompt
- **移除 built-in tools 概念**：`load_builtin_tools` 和 `filter_builtin_tools` 死代码删除；区分 Auto/Manual 由 `control_mode` 决定，与工具定义位置无关
- **ToolPo 替代 Tool**：PromptBuilder 改用可 Clone 的 ToolPo，规避 Tool 含 `dyn Trait` 不可 Clone 的限制
- **技能加载对齐工具模式**：`hr_domain.get_agent(with_skills=true)` 加载 Agent 已安装的技能副本（author_id = agent_id，排除 Expired）写入 `agent.skills`（`Vec<Skill>` 业务实体）；awakening 删除 `load_agent_skills`，直接用 `agent.skills()` 提取 SkillPo
- **技能与工具的策略差异**：技能讲究"安装且自进化"，只在已安装副本范围内查（即便神经技能也需安装到自身目录）；不匹配 match_keys 的技能由 Agent 通过 `search_skill` 神经工具按需渐进式加载
- **DefaultPromptBuilder 对齐**：Local agent 的 builder 移至 `dal/agent.rs`，与 Cli/Remote 通过各自 Dal 的 `prompt_builder()` 获取对齐
- **同步 manual 工具调用**：`request_tool_call` 重新注册为同步神经工具，与异步 `send_tool_call_message` 对齐；参数加 `tool_name`/`project_id`，响应加 `result` 字段；Manual 工具区块提示词更新说明两种调用方式及适用场景
- **测试统计**：745 个测试 100% 通过

**✅ Runtime 执行链路全面修复（v3.4，16 项）**
- **关键正确性**：TOCTOU 竞态修复（`try_set_busy` CAS + `BusyGuard` RAII）；AOP ack/nack 与 consumer 配对；工具调用 trace 完整性（call_id 不再伪造）；`record_event!` 失败记录警告；任务状态检查优先于轮次检查
- **用户体验**：root_id 继承父消息修复消息链断裂；SSE 客户端断开自动注销（`CleanupStream` Drop guard）；所有投递渠道失败时返回错误触发重试；`MessageCreatedEvent` order_key 改为接收者优先策略（Agent→to_id，非 Agent→task_id→project_id）
- **中等问题**：trace_id 加随机后缀避免并发碰撞；stats 查询失败不阻塞 agent 加载；think 添加 5 分钟超时；移除死代码与无效参数
- **优化项**：Builtin/Http 工具错误信息脱敏；`call_manual_tool_for_agent` 校验 agent 存在
- **补充修复**：`wake_agent_brain` 返回的 ctx 补充 model_provider 字段（MEDIUM）；RigCortexDao `_ctx` 扩展点文档化（LOW）；thinking_depth 通知失败告警（LOW）；root_id fallback 改用父消息 ID（LOW）
- **文档更新**：[runtime_design.md](./docs/runtime_design.md) 新增第二十三章：Runtime 执行链路全面修复（v3.4）
- **测试统计**：745 个测试 100% 通过

### 2026-07-21 ~ 22 里程碑（精简）
**✅ Tailwind CSS v4 + DaisyUI v5 集成 + A2A 异步回传 + 适配层架构统一**
- **Tailwind/DaisyUI 集成**：Tailwind v4.1 + DaisyUI v5 + 30+ 主题切换（自定义 `orz-light` 品牌主题）；所有页面迁移到 DaisyUI 类名；`index.html` 内联样式从 1960+ 行精简到 ~380 行
- **A2A 异步回传双通道**：Push 回调（`POST /a2a/callback/{task_id}`，无 JWT）+ Poll 兜底（`A2aPollingProducer` 每 30 秒）；外部 task_id 通过 Task.tags 存储，消息去重通过 `a2a_synced_msgs:N` 计数
- **适配层架构认知统一**：HTTP Handler（用户 API）、公开回调 Handler（外部 HTTP 回调）、AOP Producer（WS/轮询）三者同属适配层；修正分层架构为 Adapter → Domain → DAL → DAO；外部协议不进入事件中心，适配层直接调用 Domain 方法；详见 [LAYERED_ARCHITECTURE_PRACTICE.md - 实践 7](./docs/LAYERED_ARCHITECTURE_PRACTICE.md)
- **前端代码质量优化**：AOP 监控页修复无效 CSS 类；新增 `.card-hover`/`.card-selected`/`.modal-body` 样式；`use_resource` Hook 封装三态资源加载模式

### 2026-07-12 ~ 17 里程碑（精简）
**✅ 向量索引重建 + 飞书 P2P 消息 + SSE 推送 + 任务/记忆/对话前端能力**
- **向量索引重建（07-17）**：HnswStore 集合元数据持久化（`CollectionMeta`：model_provider_id/dimensions/vector_count）；7 个 DAL 的 `rebuild_vectors` 统一为「查元数据 → 一致则跳过 → 重建 → 写回」模式
- **飞书 P2P 消息接入 + AOP 适配中台（v4 架构，07-17）**：LarkDao trait + HTTP + WebSocket 长连接；`pkg/adapter/message` 通用消息入站适配中台，新渠道只需 DAL 注册 producer 自动获得入站消息；Agent 路由策略（渠道绑定 agent_id 优先 → feishu_reception 角色 → 任意 Onboarded Agent）
- **向量搜索增强（07-16）**：HNSW 索引持久化（`hnsw_index` 目录，bincode 2.0 序列化，后台 60s 定时落盘 + Drop 兜底）；索引重建异步化（switch 接口立即返回 task_id，进度查询端点，并发控制 409）
- **定时触发器前端体验优化（07-16）**：7 列展示 + Action 模板化（agent_rest）+ Cron 预设按钮 + 编辑复用创建弹窗
- **SSE 消息推送系统（07-15）**：基于 Server-Sent Events 单向实时推送；`SsePushDao` 用 `tokio::sync::RwLock` + `broadcast` 通道管理 SSE 连接；`GET /api/v1/finance/messages/sse/{user_id}`
- **双模式认证（07-15）**：Cookie（浏览器）+ Bearer token（API 调用）双模提取；浏览器请求 302 重定向登录页，API 调用 401 JSON；JWT 中间件外层先执行注入用户信息，RequestContext 内层后执行
- **任务管理可视化 + 对话附件上传 + Toast 通知（07-15）**：项目概览统计卡片 + 动态进度条；附件上传 API（web_sys FormData）+ 图片内联展示 + 消息时间分组；Toast 4 种类型 + 滑入滑出动画 + 22 页面统一替换旧式提示
- **任务管理核心功能 + 看板视图 + Agent 记忆面板（07-15）**：任务创建/编辑弹窗 + 任务详情页；全局任务列表 API + 看板视图（按状态分列）；Agent 记忆面板 Tab 切换（短期记忆/知识节点/关系）
- **对话体验打磨 + 实体统计数据动态注入（07-15）**：消息复制 + 快捷指令（/clear、/help）+ 键盘导航；FetchOptions 模式 + StatsOptions + 按需注入统计数据
- **前端统计数据集成（07-15）**：`StatsCard` 通用卡片 + 三个实体面板（Agent/Project/Task）；详情页按需展示统计面板
- **管理页面补全 + 对话功能 MVP + 消息/记忆搜索 + 知识图谱（07-13）**：Agent/Project 详情页；左右分栏对话布局 + 双向分页；消息/记忆搜索 API + 知识图谱 SVG 组件

### 2026-07-30 里程碑（预置技能 + 工具同步）
**✅ 开箱即用的预置技能与工具同步**
- **SkillFileDef 三来源（07-30）**：`SkillDef` 新增 `files: Vec<SkillFileDef>` 字段（`#[serde(default)]` 向后兼容）；`SkillFileDef` 支持 content / ref_path / url 三种内容来源，优先级 content > ref_path > url
- **编译期内嵌文件（07-30）**：新增 `seed/embedded.rs`，用 `include_str!` + 静态注册表模式将 `seed/skills/` 下的技能文件嵌入二进制；`read_embedded_file(ref_path)` 按路径读取，`list_embedded_skill_files()` 列出全部（环境无 cc linker，从 include_dir 改为 include_str!）
- **5 个预置技能（07-30）**：`default.json` 的 skills 数组包含 5 个技能：TEMPLATE_TOOL_BASICS（工具基础，neural+tool_management）、TEMPLATE_SKILL_BASICS（技能基础，neural+skill_management）、TEMPLATE_MEMORY_COGNITION（记忆认知，neural+memory）、TEMPLATE_COMMUNICATION（协作沟通，neural+messaging+collaboration）、TEMPLATE_PROJECT_MANAGEMENT（项目管理，project_management 不含 neural），前 4 个为神经技能常驻加载，项目管理按工具包匹配加载；用 ref_path 引用编译期内嵌的 skill.md
- **apply_preset_skills 独立函数（07-30）**：从 `apply_snapshot_to_db` 抽出技能导入逻辑为独立函数，支持 `author_id_override: Option<&str>`（initialize_system 传 Some(owner_id) 对齐组织 owner，apply_snapshot_to_db 传 None 保留模板值）和 `skip_existing: bool`（对应 SkipExisting 策略）；返回 `SkillApplyResult { created, updated, skipped }`；`apply_snapshot_to_db` 的 skill 部分替换为调用此函数
- **resolve_skill_file_content（07-30）**：新增文件内容解析函数，content 直接返回、ref_path 调用 embedded::read_embedded_file、url 用 reqwest 抓取（30s 超时，1MB 限制）
- **assemble_snapshot_from_db 导出文件（07-30）**：新增 `export_skill_files` 辅助函数，调用 `list_skill_files` + `get_skill_file_content` 读取技能文件内容到 `SkillFileDef.content`；assemble 的 skill 部分从同步 map 改为 async 循环
- **ToolProviderManage::sync_builtin_tools（07-30）**：domain trait 新增 `sync_builtin_tools(ctx) -> Result<usize>` 方法，FinanceDomainImpl 委托 `tool_dal.sync_builtin_tools_to_db(ctx)`；分层架构合规（handler → domain → DAL → DAO）
- **initialize_system 集成（07-30）**：handler 新增 Step 4（同步内置工具到 DB）和 Step 5（导入预置技能，author_id 替换为 owner）；首次初始化组织后自动完成工具同步和技能导入
- **集成测试（07-30）**：新增 `tests/integration/preset_skills_test.rs`（4 个测试：预置技能导入验证、工具同步验证、技能文件内容验证、幂等性验证）

### 2026-07-30 里程碑（精简）
**✅ Agent 工具/技能搜索式安装**
- **后端 tags 聚合接口（07-30）**：新增 `GET /finance/tools/tags`（distinct tags from enabled tools）和 `GET /hr/skills/tags`（distinct tags from published skills），DAO 层用 `SELECT DISTINCT json_each.value` 实现；均注册为神经工具（`neural` flag），Agent 可自主查询可用工具/技能分类
- **单技能卸载接口（07-30）**：新增 `DELETE /agents/{id}/skills/{skill_id}`，删除 Agent 私有副本（DB + 文件），仅限 parent_skill_id 不为空的副本；注册为神经工具，Agent 可自主卸载已安装的技能副本
- **技能包卸载扩展（07-30）**：`UninstallSkillPackRequest` 新增 `delete_copies: Option<bool>` 参数，`true` 时同时删除该 tag 下 Agent 的技能副本；SkillQuery 新增 `has_parent: Option<bool>` 字段支持过滤副本（DAO 层转译为 `parent_skill_id IS NOT NULL/IS NULL`）
- **前端 SearchableSelect 组件（07-30）**：新增 `frontend/src/components/searchable_select.rs`，支持静态候选列表（前端 filter）和动态搜索（on_search 回调 + loading 指示器）两种模式；用 `use_memo` + clone 模式解决 `'static` 闭包生命周期问题
- **4 处安装区改造（07-30）**：工具包/技能包安装改为 SearchableSelect（静态 tags 数据源）+ badge 已装列表；单个工具绑定/技能安装改为 SearchableSelect（动态 query 搜索）+ 卡片网格已装列表
- **技能包卸载确认对话框（07-30）**：新增两选项确认对话框（仅移除关联 / 移除关联+删除副本），删除副本选项带风险警告
- **编译修复（07-30）**：`ListAgentSkillsResponse` 加 `Default` derive（支持 `api_get_or_default`）；`RecordingToolDal` 测试 mock 补 `list_tags` 实现；`SkillQuery` 测试初始化补 `has_parent` 字段
- **神经标签补充（07-30）**：本次新增的 3 个 handler 工具（`list_tool_tags`、`list_skill_tags`、`uninstall_skill_from_agent`）均添加 `neural` flag，使 Agent 在运行时可自主查询工具/技能分类并卸载技能副本

### 2026-07-29 里程碑（精简）
**✅ Artifact 编辑能力（统一更新接口）**
- **update_artifact 统一更新接口（07-29）**：`update_artifact_content` 重命名为 `update_artifact`，从「全量替换 content」扩展为「部分更新 content/name/description/tags 单接口」；DTO `UpdateArtifactContentRequest` → `UpdateArtifactRequest`（content 改 `Option<String>`，新增 name/description/tags 字段均为 Option）；Domain trait 方法签名同步扩展；路由 `PUT /artifacts/{id}/content` 迁移到 `PUT /artifacts/{id}`；handler 工具 id 同步重命名，tag 保持 `project_management`；content 更新仅适用于 `GeneratedContent` 类型，metadata 更新适用于所有类型；乐观锁 `expected_updated_at` 保留
- **ArtifactMetaModal 前端元信息编辑（07-29）**：新增 `frontend/src/components/artifact_meta_modal.rs`（Props/Clone/PartialEq），支持 name/description/tags 编辑，调用 `update_artifact(content=None)` 仅更新元信息；artifact 详情页加「编辑信息」入口
- **前端合并返回 + 产物入口（07-29）**：`get_project`/`get_task` 前端 API 加 `with_artifacts` 参数；Project 详情页移除单独 `list_artifacts` 调用改用合并返回；Project/Task 详情页产物行加「查看详情」链接；Task 详情页新增「📦 产物」Tab（第 4 个 Tab）
- **编译修复（07-29）**：`ArtifactDetail` 加 `PartialEq` derive（Dioxus Props 要求）；`ArtifactMetaModalProps` 加 `Clone` derive（匹配代码库约定）；`use_effect` 内 partial move 问题通过提前 clone `props.artifact` 解决

### 2026-07-28 里程碑（精简）
**✅ 通用图形组件 + Project/Task 详情增强**
- **通用图形渲染组件 pkg/utils/graph（07-28）**：`Graph`/`GraphNodeData`/`GraphLine` 数据结构 + `GraphRenderer` trait + `MermaidRenderer` 实现（LR/TD 方向、节点分类着色、外部节点自动补全、标签转义）；零业务依赖，未来可扩展 PlantUml/Dot 等 Renderer
- **Project 详情 Mermaid 任务依赖图（07-28）**：`build_task_graph_mermaid` 基于 Task.dependencies 构建 DAG（箭头表示执行流向：前置→后继）；按任务状态着色（Completed→done、InProgress→doing、Pending→todo 等）；跨项目依赖自动渲染为外部节点；`GET /projects/{id}?with_task_graph=true` 按需返回
- **Project/Task 详情 Artifact 列表暴露（07-28）**：`GET /projects/{id}?with_artifacts=true` 和 `GET /tasks/{id}?with_artifacts=true` 按需返回 `Vec<ArtifactDetail>`（复用现有 DTO，含 id 等关键字段）；Domain 层聚合注入，DAL 层保持单一职责
- **Agent Artifact 创建能力（07-28）**：`fs_write` 路径隔离到 `agents/{agent_id}/`（base_path 从全局改为 `agent_data_dir(agent_id)`）；Domain 层新增 `create_generated_artifact`（文本类，传 content 落盘）+ `create_generated_artifact_from_file`（文件类，从 agent 目录**复制**到 artifact 目录，源文件保留）；两个新工具 `create_text_artifact`（`POST /artifacts/text`）+ `register_artifact_from_path`（`POST /artifacts/register-from-path`，含路径穿越安全校验）；打通 `create_artifact` 的 `GeneratedContent` 分支（原 `bail_err!` stub）；artifact 工具统一归口 `project_management` tag（`query_artifacts` 去 `neural`，`update_artifact_content`/`create_artifact` 加 tag）；新增 `mime_util` 模块（扩展名→MIME→FileType 推断）；IO 失败回滚 DB 记录

### 2026-07-10 ~ 12 里程碑（精简）
**✅ 前端架构重构 + Runtime Phase 4 + 全实体 FTS5**
- **前端架构重构（07-12）**：Dioxus Router 15 条路由 + 统一 API 客户端（OnceLock 单例 + JWT bearer 自动注入）+ 全局认证状态 + 基础 UI 组件库 + 13 个 CRUD 页面；新增 [docs/frontend_architecture.md](./docs/frontend_architecture.md)
- **Task 进度追踪 + FTS5 公共工具重构（07-12）**：Task `progress: i32`（0-100）+ `update_progress` Domain 方法 + `progress_updated` 事件 + `update_task_progress` 神经工具；`escape_fts5_keyword` 提升到 `pkg/storage/fts5.rs` 消除 DAO→DAO 依赖
- **Runtime Phase 4C - 技能系统增强（07-12）**：SkillQuery/ToolQuery 加 tags 字段（`json_each` OR 语义）；技能包完整生命周期（install/uninstall/reinstall/list）；`hr_domain.get_agent(with_skills=true)` 加载已安装技能副本；`search_skill` 神经工具
- **记忆搜索 FTS5 增强 + 综合搜索（07-12）**：`short_term_memory_fts` + `knowledge_node_fts` 虚拟表（trigram 分词器）+ 6 个触发器自动同步；MatchType 三态（Hybrid/Vector/Keyword）+ 三级排序；向量距离阈值可配置
- **全实体 FTS5 全文搜索改造（07-12）**：Skill/Tool/Message/Task/Project/Agent 6 大实体统一混合搜索模式；迁移文件 `20260712000001_entity_fts5.sql`，6 个 FTS5 虚拟表 + 18 个触发器 + 6 条存量回填；向量索引自动维护
- **Runtime Phase 4A - 工具包机制 + 任务执行闭环（07-11）**：tag 分组工具 + Agent 入职自动安装；免绑定校验三层逻辑（绑定 → 神经 → 已安装 tag）；`TaskAssignment` 消息类型 + `send_task_assignment_message` 神经工具；三种角色定位（神经工具 Handler / 普通 HTTP Handler / Consumer）
- **Runtime Phase 4B - 记忆模块增强（07-11）**：定时触发器系统（CronTriggerPo + CronScheduler 后台扫描 + CronTriggerConsumer）；休息与沉淀机制（MemoryStatus::Settled + `settle_short_term_to_long_term`）；`settle_memory` 神经工具；定时触发记忆沉淀
- **Runtime Phase 2 - 神经工具集完整落地（07-10）**：`register_handler_tool` 宏新增 `neural` flag + `tags` 参数；8 个神经工具全部实现（5 记忆 + 1 消息 + 2 工具）；唤醒时自动筛选带 "neural" tag 的工具注入 Prompt；神经工具免绑定
- **Runtime Phase 3 - 多回合循环控制（07-10）**：`ToolStatsDao` 工具调用次数/失败次数查询；`AgentFetchOptions` 按需注入统计数据；轮次限制检查（`max_thinking_depth`）+ 任务完成检测 + Prompt 上下文差异化 + 工具失败计数注入

### 2026-07-01 ~ 06 里程碑（精简）
**✅ 业务事件 + Agent 唤醒统计 + Stats DAO 全实体覆盖 + 附件存储 + MCP 集成**
- **Project/Task 业务事件（07-06）**：`project_events` + `task_events` 表，含 created/started/completed/archived/status_changed 五种事件；`operator_type` + `operator_id` 区分操作者类型；`record_event!` 宏改用 `stats_opt()` 静默跳过未初始化场景
- **Agent 唤醒统计事件 + Stats DAO 数据源切换（07-05）**：`agent_awake_events` 表记录唤醒事件；AgentStatsDao 从 `model_call_events` 切换到 `agent_awake_events`；验证"领域先行，实现后续演进"设计思路
- **Stats DAO 领域拆分重构（07-05 早期）**：按领域而非实体划分职责；通用结构体 `ModelCallStats`（call_summary + token_summary + model_call_time_series）；DAL 层统计接口统一为 `get_stats(id, options)` + `get_model_call_stats(id, options)`
- **全实体 Stats DAO 层建设完成（07-02）**：Agent/Project/Task/ModelProvider Stats DAO 全部 DuckDB 实现；`StatsInterval`/`TimeSeriesPoint`/`TokenSumResult` 迁移到 `common/src/models/stats.rs`；新增 `request_context_test_support.rs`、`storage/test_support.rs`
- **附件存储 + MCP 服务器集成完整落地（07-01）**：通用 Attachment 上传 API（文件上传 + 文本创建）；MCP 服务器 CRUD + 工具同步 + MCP 工具调用执行全链路；Finance Domain 新增 Attachment/McpServer/McpTool/ToolProvider；6 大业务域 API 全部上线

### 2026-06 月度汇总
**✅ MCP 集成 + 产物来源扩展 + 统一附件存储**
- **MCP 服务器与工具集成（06-23）**：MCP 服务器管理 + 工具同步（拉取工具列表并持久化）+ MCP 工具执行（rmcp 客户端）；迁移文件 `20260623000000_mcp_servers.sql`
- **产物来源类型扩展（06-18）**：Artifact 增加 `source_type` 字段，支持引用 Finance 模块 `attachment_id` 创建项目产物；迁移文件 `20260618000000_artifact_add_source_type.sql`
- **统一附件存储系统上线（06-17）**：通用 Attachment 模块（上传/创建文本/查询/删除/内容更新）；FileMeta + 日期分层路径存储；支持 multipart 文件上传和纯文本创建两种模式；迁移文件 `20260617000000_attachments.sql`

### 2026-05 月度汇总
**✅ 日志系统宏化 + PO/业务实体分层架构落地**
- **日志系统完全宏化重构（05-15）**：删除所有旧函数实现，8 个宏合并为 4 个（`log_info!` / `log_warn!` / `log_error!` / `log_debug!`）；语法模式匹配自动检测上下文模式；项目内禁止直接调用 `tracing::*!`；新增 [docs/logging_design.md](./docs/logging_design.md)
- **PO 与业务实体分层架构完整落地（05-11）**：Project/Task/Artifact 三大业务对象完成分层重构；DAO 仅操作 PO，DAL 内部 PO↔业务实体转换对外统一业务实体，Domain 100% 无 PO 依赖；业务实体内部持有 `po: XxxPo` 字段；ctx 跨层传递统一 `ctx.clone()`；`TaskStatus::Cancelled = 0` 软删除约定
- **Project Domain 骨架搭建 + 全项目测试代码重构优化（05-10）**：`ProjectDomain` trait 含 `management` + `execution` 两个子能力；重构 25 个测试文件，抽取 `init_test_env()` 公共初始化函数 + `create_test_agent()`/`create_test_project()` 工厂方法

所有开发过程和经验都归档在 [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md)，包括：每轮重构背景、避坑指南、架构决策权衡、最佳实践沉淀。开发前建议先看该文档避免重蹈覆辙。

---

## 七、Known Issues & 解决方案

### 7.1 Rig 包名问题

**错误现象**：
```
error[E0670]: `async fn` is not permitted in Rust 2015
error[E0432]: unresolved import `rig::completion::ToolDefinition`
```

**解决方案**：
1. 确保 `Cargo.toml` 使用 `edition = "2024"`
2. 从正确路径导入：`use rig::tool::{ToolDyn, ToolError};`
3. 避免从 `rig::completion::*` 导入工具相关类型

---

*本文档是 AI 助手的快速入门手册，详细内容请查阅各专项设计文档*
