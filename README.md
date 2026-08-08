# ai_orz

**AI 代理协作框架** — 让多个 AI 代理像团队一样协作完成任务

![Tests](https://img.shields.io/badge/tests-941%20%E2%9C%94-brightgreen)
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
| **前端** | Dioxus 0.7 (WASM) + Tailwind CSS v4 + DaisyUI v5，15 条路由，30+ 主题切换 |
| **架构** | Adapter（Handler/Producer）→ Domain → DAL → DAO 四层严格单向依赖；启动分两阶段（单例注册 + 基础数据注入） |
| **测试** | 949 个测试，100% 通过（后端 845 = 812 单元 + 33 集成 + 前端 54 + common 50） |
| **CI 质量** | clippy `-D warnings` 零容忍（后端 + 前端 wasm32） + cargo-llvm-cov 覆盖率门槛（PR 38% / main 45%） + 集成测试 3.7s |
| **实体覆盖** | Agent / Project / Task / Message / Memory / Skill / Tool / ModelProvider 全栈 |

核心能力已落地：

- 🤖 **Agent 全生命周期**：创建、入职（自动安装工具包）、绑定工具、唤醒执行
- 🔌 **A2A 协议完整支持**：
  - 作为 Client：可注册外部 Agent（CLI/Remote），通过 A2A 协议委派任务
  - 作为 Server：对外暴露 A2A 协议端点供外部调用
  - 异步结果回传：Push 回调端点 + 30秒轮询兜底双通道，适配层直接处理，外部协议不污染内部事件中心
- 🧠 **四层记忆**：Core / Working / Short-term / Long-term（含知识图谱），支持 FTS5 + 向量混合搜索；**定时沉淀**（agent_rest 4h）自动将短期记忆整理入长期知识图谱
- 🛠️ **统一工具调用架构**：execute_auto / execute_manual 三层分发（awakening 循环 → call_tool 直接执行 → ToolCallDao::execute + decorate 装饰器），Manual 通过特殊 internal 工具转发同步/异步调用
- 📨 **消息渠道系统**：飞书 P2P 私信已上线（WebSocket 长连接 + 出站推送），多渠道适配器架构就绪（微信/Slack/Webhook/邮件 待实现）
- 📋 **任务协作 + 执行计划/进度追踪**：项目 + 任务 + Agent 间任务分配，状态机、**execution_plan/execution_result** 显式记录、DAG 依赖、实时 progress summary，支持委派给外部 A2A Agent
- 📝 **前端 Markdown 渲染全覆盖 + Mermaid**：pulldown-cmark 统一渲染组件（原始 HTML 转义守护，XSS 安全），详情页/聊天气泡/记忆面板全链路 Markdown 展示；支持 Markdown 内嵌 ```mermaid 图与独立 task_graph 任务依赖图（vendor mermaid.js 懒加载，主题跟随 DaisyUI）
- 💬 **聊天页信息侧栏**：沟通页面右侧可收起信息面板（localStorage 记忆展开态），项目对话 总览/任务/产物/Agent 四 Tab + 默认对话 Agent/我 两 Tab；任务列表内展开懒加载详情，产物按项目级/任务级分组；手动刷新 + SSE 消息防抖自动刷新，移动端右侧抽屉
- ⏰ **两阶段系统初始化 + 2 条默认定时任务**：
  - 首次 `initialize_system`（HTTP）注入：组织 / 超级管理员 / Chat & Embedding Provider / 内置工具 / 5 份预置技能
  - 进程启动时 `service::init_base_data()`（幂等）自动注入 2 条系统级 cron triggers：**agent_rest（每 4 小时，全员沉淀记忆）** + **project_followup（每 1 小时，巡检所有进行中项目并唤醒 Owner 上报/巡检进度）**
  - 预置 5 份技能已更新：项目/任务技能新增执行计划与进度上报指引、工具/记忆/协作技能精简为类比+指南两层结构
- 🔍 **综合搜索**：FTS5 关键词 + 向量语义 + 知识图谱三合一
- 📊 **多维统计**：Agent / Project / Task / Tool / ModelProvider 五维度统计，异步重建索引
- 🛠️ **系统管理**：数据备份与恢复、日志查询、基于角色的权限控制、cron trigger 完整 CRUD/Pause/Resume
- 📡 **AOP 事件中心**：统一生产-消费事件框架，支持同步/异步消费模式，内置内存队列；Producer 与 Consumer 分别注册，完全解耦
- 🏗️ **适配层架构**：HTTP Handler / 回调端点 / AOP Producer 同属适配层（Adapter），统一负责外部协议转换和校验，直接调用 Domain；外部协议数据不进入事件中心
- 🧩 **统一 IDL 宏**：`#[generate_http_handler]` + `#[derive(Params)]` + `#[param(source = "path/query")]` 一份结构体定义同时支持 HTTP API 和 LLM 工具调用，自动从 path/query/body 提取参数，自动生成 axum handler，支持 path-only / query-only / path+query / path+body / 空 struct 等多种参数组合
- 🧪 **集成测试体系**：33 个集成测试覆盖 Auth/SysInit + Core CRUD + Message Delivery + Vector Degradation + A2A Flow + Preset Skills + Cron Triggers 全链路，3.7s 跑完；向量降级契约守护测试确保无 embedding provider 时主流程仍可用；CI 启用 clippy `-D warnings` 零容忍 + cargo-llvm-cov 差异化覆盖率门槛（PR 38% / main 45%）

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

> 📌 **内容脉络（四象限）**：wiki/ 回答「是什么」（现状百科，阅读第一站）；design/ 回答「为什么」（历史决策快照）；plan/ 回答「要去哪」；archive/ 存放已归档方案。

- [AGENTS.md](./AGENTS.md) — AI 开发规范总览 + 完整文档索引
- [docs/wiki/](./docs/wiki/) — **知识库入口**（代码现状百科，IDE 生成随代码演进）
- [docs/CODE_WIKI.md](./docs/CODE_WIKI.md) — 代码认知入口页（模块速查 + 文档导航）
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — 权威架构总纲
- [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md) — **开发必读**分层架构 7 个实践（含适配层架构原则）、反模式避坑

设计决策精选（design/ 目录全量见上方导航）：

- [docs/design/external_agent_design.md](./docs/design/external_agent_design.md) — 外部 Agent 接入（CLI/Remote/A2A 异步回调轮询）
- [docs/design/runtime_design.md](./docs/design/runtime_design.md) — Agent 唤醒与神经工具
- [docs/design/message_channel_design.md](./docs/design/message_channel_design.md) — 多渠道消息接入（出站推送）
- [docs/design/vector_search_architecture.md](./docs/design/vector_search_architecture.md) — 混合搜索架构

---

## License

[Apache License 2.0](LICENSE)
