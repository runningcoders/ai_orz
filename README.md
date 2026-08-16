# ai_orz

> 🎯 **本文档定位**：项目对外门面，1 分钟速览（定位是什么、技术栈、快速启动、测试命令、社区入口）
>
> 状态：持续同步，随版本升级
>
> 查阅场景：
> - 第一次打开本仓库 → 快速了解项目是什么、能做什么、技术选型
> - 要跑测试、启动本地开发、docker 启动 → 查询命令
> - 要找社区/文档入口、License、徽章信息 → 顶部快速获取
>
> 关联文档：
> - [AGENTS.md](./AGENTS.md) — AI 助手快速入门手册（架构规范 + 开发约定强制执行）
> - [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — 唯一权威架构总纲
> - [docs/wiki/](./docs/wiki/) — 代码现状百科（IDE 自动生成，阅读第一站）

**AI 代理协作框架** — 让多个 AI 代理像团队一样协作完成任务

![Tests](https://img.shields.io/badge/tests-1101%20%E2%9C%94-brightgreen)
![Clippy](https://img.shields.io/badge/clippy--D%20warnings-zero-success)
![Coverage](https://img.shields.io/badge/coverage-PR%2038%25%2C%20main%2045%25%20threshold%20(llvm--cov)-yellow)
![Rust](https://img.shields.io/badge/Rust-1.85+-000000?logo=rust)
![License](https://img.shields.io/github/license/runningcoders/ai_orz)

---

## 我们想做的事

传统的 AI 应用把 LLM 当成函数调用——发个 prompt，收个回复，单次交互。

**ai_orz 把 AI 代理当作长期角色来管理**：

- 代理有名字、身份、记忆、技能、工具，就像团队成员
- 代理之间可以相互发消息、协作、委派任务
- 代理可以主动休息、沉淀经验、调用工具、操作外部系统
- 用户和代理在一个统一的消息流里沟通
- 多渠道接入：网页、飞书、未来的微信/Slack/邮件

这不是"又一个 chatbot 框架"，而是 **AI 时代的工作流平台底层**。

---

## 当前能力

| 维度 | 现状 |
|------|------|
| **后端** | Rust + Axum + SQLite + 原生 CortexDao（OpenAI 兼容），单二进制可部署 |
| **前端** | Dioxus 0.7 (WASM) + Tailwind CSS v4 + DaisyUI v5，41 条路由，30+ 主题切换 |
| **架构** | Adapter（Handler/Producer）→ Domain → DAL → DAO 四层严格单向依赖；启动分两阶段（单例注册 + 基础数据注入） |
| **测试** | 1101 个测试，100% 通过（后端 961 = 875 单元 + 86 集成 + 前端 82 + common 58） |
| **CI 质量** | clippy `-D warnings` 零容忍（后端 + 前端 wasm32） + cargo-llvm-cov 覆盖率门槛（PR 38% / main 45%） + E2E Playwright 仅本地 |
| **实体覆盖** | Agent / Project / Task / Message / Memory / Skill / Tool / ModelProvider 全栈 |

核心能力已落地：

- 🤖 **Agent 全生命周期**：创建、入职（自动安装工具包）、绑定工具、唤醒执行；多回合循环控制与任务完成检测
- 🔌 **A2A 协议完整支持**：作为 Client 注册外部 Agent（CLI/Remote）委派任务；作为 Server 对外暴露协议端点；异步结果回传（Push 回调 + 30 秒轮询兜底）
- 🧠 **四层记忆**：Core / Working / Short-term / Long-term（含知识图谱），FTS5 + 向量混合搜索；定时沉淀（agent_rest）自动将短期记忆整理入长期图谱
- 🛠️ **统一工具调用架构**：Auto（LLM 原生）/ Manual（提示词转发）双模式，同步/异步调用 + 调用追踪
- 📨 **消息渠道系统**：飞书 P2P 私信已上线（WebSocket 长连接 + 出站推送 + 退避重连），多渠道适配器架构就绪（微信/Slack/Webhook/邮件待实现）
- 🔑 **用户身份凭证**：用户级凭证中枢（加密存储），渠道仅存凭证引用；前端身份凭证管理页（飞书为首个凭证类型：应用绑定/OAuth 设备流/自动绑定）
- 📋 **任务协作 + 进度追踪**：项目 + 任务 + Agent 间任务分配，状态机、执行计划/执行结果显式记录、DAG 依赖、实时进度汇总，支持委派给外部 A2A Agent
- 💬 **对话体验**：用户 ↔ Agent 双向对话、SSE 实时推送、附件上传、聊天信息侧栏（总览/任务/产物/工具）、Markdown + Mermaid 全链路渲染
- ⏰ **定时触发器**：cron trigger 完整 CRUD/Pause/Resume；启动自动注入 2 条系统默认任务（Agent 记忆沉淀 + 项目进度巡检）
- 🔍 **综合搜索**：FTS5 关键词 + 向量语义 + 知识图谱三合一
- 📊 **多维统计与监控**：Agent / Project / Task / Tool / ModelProvider 五维度统计、AOP 队列监控、系统健康仪表盘
- 🛠️ **系统管理**：数据备份与恢复、日志在线查询、基于角色的权限控制、后台进程管理
- 📡 **AOP 事件中心**：统一生产-消费事件框架，同步/异步消费模式，Producer/Consumer 完全解耦
- 🧪 **质量工程**：86 个集成测试覆盖 Auth/SysInit + CRUD + 消息投递 + 向量降级 + A2A + 飞书集成全链路；E2E Playwright 仅本地

> 功能现状以 [docs/wiki/](./docs/wiki/) 为准，开发规范见 [AGENTS.md](./AGENTS.md)

---

## 快速开始

```bash
# 克隆并启动
git clone https://github.com/runningcoders/ai_orz
cd ai_orz
make prod           # 生产模式：自动编译 + 启动
```

服务监听 `0.0.0.0:3000`，浏览器打开 `http://localhost:3000` 即可使用。

**开发模式**（前端热重载）：

```bash
make dev            # 同时启动后端 cargo run + 前端 dx serve
# 后端: http://localhost:3000
# 前端: http://localhost:8080
```

**更多用法**：

```bash
make help           # 查看所有命令（格式化/测试/门禁/构建一应俱全）
make build          # 仅编译（前端 release + 后端 release）
make run            # 只启动后端
make serve          # 只启动前端
# 等价底层入口：./scripts/start.sh <dev|prod|build|backend|frontend|help>
```

首次启动会自动生成默认配置文件 `ai_orz.toml`，可按需修改。

---

## 项目结构

```
ai_orz/
├── common/              # 前后端共享 DTO/枚举/配置
├── src/                 # 后端
│   ├── handlers/        # HTTP 接口层（适配层：面向用户 API + 外部回调）
│   ├── producer/        # 事件生产者（适配层：轮询 + 外部 WS 事件接入）
│   ├── consumer/        # 事件消费者（内部事件处理，消费 Domain 产生的内部事件）
│   ├── service/
│   │   ├── dao/         # 数据访问（本地 DB CRUD + 外部 API 出站调用）
│   │   ├── dal/         # 业务数据访问（组合 DAO，PO↔Entity 转换）
│   │   └── domain/      # 业务编排（核心业务逻辑，产生内部事件）
│   └── pkg/
│       ├── aop/         # 事件中心纯框架（Event/Producer/Consumer/Registry）
│       ├── adapter/     # 通用适配器基础设施（消息渠道适配中台）
│       └── ...          # 其他基础设施（向量存储、日志）
├── frontend/            # Dioxus 前端（Tailwind CSS v4 + DaisyUI v5）
│   ├── src/             # 前端源码
│   ├── styles/          # Tailwind CSS 入口
│   └── public/          # 构建产物（output.css）
└── docs/                # 详细设计文档
```

详细分层规范、命名约定、PO/Entity 边界、Context 传递、适配层原则等强制规范见 [AGENTS.md](./AGENTS.md)。

---

## 文档

> 📌 **内容脉络（四象限）**：wiki/ 回答「是什么」（现状百科，阅读第一站）；design/ 回答「为什么」（历史决策快照）；plan/ 回答「要去哪」；archive/ 存放已归档方案。

- [AGENTS.md](./AGENTS.md) — AI 开发规范总览 + 完整文档索引
- [docs/wiki/](./docs/wiki/) — **知识库入口**（代码现状百科，IDE 生成随代码演进）
- [docs/CODE_WIKI.md](./docs/CODE_WIKI.md) — 代码认知入口页（模块速查 + 文档导航）
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — 权威架构总纲
- [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md) — **开发必读**分层架构 7 个实践（含适配层架构原则）、反模式避坑

设计决策精选（完整索引见 [AGENTS.md](./AGENTS.md) 文档索引）：

- [docs/design/external_agent_design.md](./docs/design/external_agent_design.md) — 外部 Agent 接入（CLI/Remote/A2A 异步回调轮询）
- [docs/design/runtime_design.md](./docs/design/runtime_design.md) — Agent 唤醒与神经工具
- [docs/design/message_channel_design.md](./docs/design/message_channel_design.md) — 多渠道消息接入（出站推送）
- [docs/design/lark_cli_integration.md](./docs/design/lark_cli_integration.md) — 飞书集成：用户级凭证中枢 + lark-cli 工具 + WS 重连
- [docs/design/vector_search_architecture.md](./docs/design/vector_search_architecture.md) — 混合搜索架构

---

## License

[Apache License 2.0](LICENSE)
