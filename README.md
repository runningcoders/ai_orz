# ai_orz

**AI 代理协作框架** — 让多个 AI 代理像团队一样协作完成任务

![Tests](https://img.shields.io/badge/tests-892%20%E2%9C%94-brightgreen)
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
| **后端** | Rust + Axum + SQLite + rig-core，单二进制可部署 |
| **前端** | Dioxus 0.7 (WASM) + Tailwind CSS v4 + DaisyUI v5，15 条路由，30+ 主题切换 |
| **架构** | Adapter（Handler/Producer）→ Domain → DAL → DAO 四层严格单向依赖 |
| **测试** | 892 个测试，100% 通过（后端 796 + 前端 46 + common 50） |
| **实体覆盖** | Agent / Project / Task / Message / Memory / Skill / Tool / ModelProvider 全栈 |

核心能力已落地：

- 🤖 **Agent 全生命周期**：创建、入职（自动安装工具包）、绑定工具、唤醒执行
- 🔌 **A2A 协议完整支持**：
  - 作为 Client：可注册外部 Agent（CLI/Remote），通过 A2A 协议委派任务
  - 作为 Server：对外暴露 A2A 协议端点供外部调用
  - 异步结果回传：Push 回调端点 + 30秒轮询兜底双通道，适配层直接处理，外部协议不污染内部事件中心
- 🧠 **四层记忆**：Core / Working / Short-term / Long-term（含知识图谱），支持 FTS5 + 向量混合搜索
- 🛠️ **混合工具调用**：简单工具走 LLM auto，关键工具走自建 manual 可控链路
- 📨 **消息渠道系统**：飞书 P2P 私信已上线（WebSocket 长连接 + 出站推送），多渠道适配器架构就绪（微信/Slack/Webhook/邮件 待实现）
- 📋 **任务协作**：项目 + 任务 + Agent 间任务分配，状态机、进度追踪、依赖关系，支持委派给外部 A2A Agent
- 🔍 **综合搜索**：FTS5 关键词 + 向量语义 + 知识图谱三合一
- 📊 **多维统计**：Agent / Project / Task / Tool / ModelProvider 五维度统计，异步重建索引
- 🛠️ **系统管理**：数据备份与恢复、日志查询、基于角色的权限控制
- 📡 **AOP 事件中心**：统一生产-消费事件框架，支持同步/异步消费模式，内置内存队列；Producer 与 Consumer 分别注册，完全解耦
- 🏗️ **适配层架构**：HTTP Handler / 回调端点 / AOP Producer 同属适配层（Adapter），统一负责外部协议转换和校验，直接调用 Domain；外部协议数据不进入事件中心
- 🧩 **统一 IDL 宏**：`#[generate_http_handler]` + `#[derive(Params)]` + `#[param(source = "path/query")]` 一份结构体定义同时支持 HTTP API 和 LLM 工具调用，自动从 path/query/body 提取参数，自动生成 axum handler，支持 path-only / query-only / path+query / path+body / 空 struct 等多种参数组合

> 完整功能列表和开发规范请看 [AGENTS.md](./AGENTS.md)

---

## 快速开始

```bash
# 克隆并启动
git clone https://github.com/runningcoders/ai_orz
cd ai_orz
./start.sh prod     # 生产模式：自动编译 + 启动
```

服务监听 `0.0.0.0:3000`，浏览器打开 `http://localhost:3000` 即可使用。

**开发模式**（前端热重载）：

```bash
./start.sh dev      # 同时启动后端 cargo run + 前端 dx serve
# 后端: http://localhost:3000
# 前端: http://localhost:8080
```

**更多用法**：

```bash
./start.sh help     # 查看所有模式
./start.sh build    # 仅编译（前端 release + 后端 release）
./start.sh backend  # 只启动后端
./start.sh frontend # 只启动前端
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

- [AGENTS.md](./AGENTS.md) — AI 开发规范总览 + 完整文档索引
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — 完整架构说明
- [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md) — **开发必读**分层架构 7 个实践（含适配层架构原则）、反模式避坑
- [docs/external_agent_design.md](./docs/external_agent_design.md) — 外部 Agent 接入（CLI/Remote/A2A 异步回调轮询）
- [docs/a2a_server_design.md](./docs/a2a_server_design.md) — A2A Server 对外协议端点
- [docs/runtime_design.md](./docs/runtime_design.md) — Agent 唤醒与神经工具
- [docs/message_channel_design.md](./docs/message_channel_design.md) — 多渠道消息接入（出站推送）
- [docs/vector_search_architecture.md](./docs/vector_search_architecture.md) — 混合搜索架构

---

## License

[Apache License 2.0](LICENSE)
