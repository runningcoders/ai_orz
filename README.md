# ai_orz

**AI 代理协作框架** — 让多个 AI 代理像团队一样协作完成任务

![Tests](https://img.shields.io/badge/tests-708%20%E2%9C%94-brightgreen)
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
| **前端** | Dioxus 0.7 (WASM)，15 条路由，Mistral 暖色设计系统 |
| **架构** | Handler → Domain → DAL → DAO 四层严格单向依赖 |
| **测试** | 708 个单元测试，100% 通过 |
| **实体覆盖** | Agent / Project / Task / Message / Memory / Skill / Tool / ModelProvider 全栈 |

核心能力已落地：

- 🤖 **Agent 全生命周期**：创建、入职（自动安装工具包）、绑定工具、唤醒执行
- 🧠 **四层记忆**：Core / Working / Short-term / Long-term（含知识图谱），支持 FTS5 + 向量混合搜索
- 🛠️ **混合工具调用**：简单工具走 LLM auto，关键工具走自建 manual 可控链路
- 📨 **消息渠道系统**：飞书 P2P 私信已上线，多渠道架构就绪（微信/Slack/Webhook 待实现）
- 📋 **任务协作**：项目 + 任务 + Agent 间任务分配，状态机、进度追踪、依赖关系
- 🔍 **综合搜索**：FTS5 关键词 + 向量语义 + 知识图谱三合一
- 📊 **多维统计**：Agent / Project / Task / Tool / ModelProvider 五维度统计，异步重建索引
- 🛠️ **系统管理**：数据备份与恢复、日志查询、基于角色的权限控制

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
│   ├── handlers/        # HTTP 接口层
│   ├── service/
│   │   ├── dao/         # 数据访问
│   │   ├── dal/         # 业务数据访问
│   │   └── domain/      # 业务编排
│   ├── consumer/        # 异步消费者
│   └── pkg/             # 基础设施（向量存储、消息适配、日志）
├── frontend/            # Dioxus 前端
└── docs/                # 详细设计文档
```

详细分层规范、命名约定、PO/Entity 边界、Context 传递等强制规范见 [AGENTS.md](./AGENTS.md)。

---

## 文档

- [AGENTS.md](./AGENTS.md) — AI 开发规范总览 + 完整文档索引
- [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) — 完整架构说明
- [docs/runtime_design.md](./docs/runtime_design.md) — Agent 唤醒与神经工具
- [docs/vector_search_architecture.md](./docs/vector_search_architecture.md) — 混合搜索架构
- [docs/message_channel_design.md](./docs/message_channel_design.md) — 多渠道消息接入

---

## License

[Apache License 2.0](LICENSE)
