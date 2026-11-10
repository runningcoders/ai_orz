# ai_orz

**AI 代理协作框架** — 让多个 AI 代理像团队一样协作完成任务

![Tests](https://img.shields.io/badge/tests-passing-brightgreen)
![Clippy](https://img.shields.io/badge/clippy--D%20warnings-zero-success)
![Coverage](https://img.shields.io/badge/coverage-PR%2038%25%2C%20main%2045%25%20threshold%20(llvm--cov)-yellow)
![Rust](https://img.shields.io/badge/Rust-1.99.1-000000?logo=rust)
![License](https://img.shields.io/github/license/runningcoders/ai_orz)

> Rust 版本由 [`rust-toolchain.toml`](./rust-toolchain.toml) **锁死具体版本**（非 `stable`），保证本地 / CI / pre-push 跑同一个 clippy。升级需人工验证后改该文件。

---

## 🧭 先看图 · 再看字

**① 在线文档站** — **<https://runningcoders.github.io/ai_orz/>**
文档地图 + 全部架构图的在线总入口，GitHub Pages 直出，**不用克隆仓库**。

**② 四张交互式架构图** — 点开即看，支持缩放、查找节点、追踪有向路径（图比文字快）：

| 图 | 讲什么 | 在线看（推荐） | 仓库内文件 |
|---|---|---|---|
| **架构总览 · 容器级** ⭐ | 系统由哪些容器组成、请求怎么流；细节下沉下面三张专题子图 | [打开](https://runningcoders.github.io/ai_orz/diagrams/ai-orz-architecture.html) | [本地](./docs/diagrams/ai-orz-architecture.html) |
| 思维轨迹与思考图 | Agent 怎么「想」：唤醒 → 三层预算思考循环 → 图引擎 → 轮级轨迹落盘 | [打开](https://runningcoders.github.io/ai_orz/diagrams/ai-orz-think-path.html) | [本地](./docs/diagrams/ai-orz-think-path.html) |
| 记忆子系统 | 四层记忆（Core / Working / Short-term / Long-term）+ 两条 JSONL + 向量与全文索引 + 睡眠沉淀 | [打开](https://runningcoders.github.io/ai_orz/diagrams/ai-orz-memory.html) | [本地](./docs/diagrams/ai-orz-memory.html) |
| 本体子系统 | 受控词表 TBox / 属性定义与五道门闩 / 骨架三元组 / 漂移与有效性治理 | [打开](https://runningcoders.github.io/ai_orz/diagrams/ai-orz-ontology.html) | [本地](./docs/diagrams/ai-orz-ontology.html) |

> ⚠️ **GitHub 仓库页对 `.html` 只显示源码、不渲染**（平台限制）。想看交互效果请走「在线看」列；或克隆后用自己的浏览器打开「本地」列文件，效果与在线版一致。

**③ 想读文字** → 跳到 [当前能力](#当前能力) · [快速开始](#快速开始) · [知识库](#知识库) · [文档索引](#文档)。
要**动手改代码**，直接进 [docs/AGENT_GUIDE.md](./docs/AGENT_GUIDE.md)（实现需求的唯一入口）。

---

## 我们想做的事

传统的 AI 应用把 LLM 当成函数调用——发个 prompt，收个回复，单次交互。

**ai_orz 把 AI 代理当作长期角色来管理**：

- 代理有名字、身份、记忆、技能、工具，就像团队成员
- 代理之间可以相互发消息、协作、委派任务
- 代理可以主动休息、沉淀经验、调用工具、操作外部系统
- 用户和代理在一个统一的消息流里沟通
- 多渠道接入：网页、飞书、微信、邮件

这不是"又一个 chatbot 框架"，而是 **AI 时代的工作流平台底层**。

---

## 当前能力

| 维度 | 现状 |
|------|------|
| **后端** | Rust + Axum + SQLite + 原生 CortexDao（OpenAI 兼容），单二进制可部署 |
| **前端** | Dioxus 0.7 (WASM) + Tailwind CSS v4 + DaisyUI v5，44 条路由，30+ 主题切换 |
| **架构** | Adapter（Handler/Producer）→ Domain → DAL → DAO 四层严格单向依赖；启动分两阶段（单例注册 + 基础数据注入） |
| **测试** | 全量测试 100% 通过（后端单元/集成 + 前端 + common） |
| **CI 质量** | clippy `-D warnings` 零容忍（后端 + 前端 wasm32） + cargo-llvm-cov 覆盖率门槛（PR 38% / main 45%） + E2E Playwright 仅本地 |
| **实体覆盖** | Agent / Project / Task / Message / Memory / Skill / Tool / ModelProvider 全栈 |

核心能力已落地：

- 🤖 **Agent 全生命周期**：创建、入职（自动补装基础技能/工具包）、绑定工具、唤醒执行；多回合循环控制与任务完成检测
- 🔌 **A2A 协议完整支持**：作为 Client 注册外部 Agent（CLI/Remote）委派任务；作为 Server 对外暴露协议端点；异步结果回传（Push 回调 + 30 秒轮询兜底）
- 🧠 **四层记忆**：Core / Working / Short-term / Long-term（含知识图谱），FTS5 + 向量混合搜索；定时沉淀（agent_rest）自动将短期记忆整理入长期图谱
- 🛠️ **工具与技能体系**：Auto（LLM 原生）/ Manual（提示词转发）双模式 + 调用追踪；12 份预置技能（7 个神经技能常驻 Prompt），技能摘要 + id 常驻、全文 get_skill 按需加载；实践沉淀可更新技能副本 / 创建新技能（自我进化）
- 🔗 **MCP 工具接入**：作为 MCP Client 连接外部 MCP Server（stdio / streamable HTTP 双传输），远端工具同步进统一工具注册中心与调用追踪，管理面支持 Server/Tool 的增删改查与同步刷新
- 📨 **消息渠道系统**：飞书 P2P 私信已上线（WebSocket 长连接 + 出站推送 + 退避重连），微信（iLink 长轮询）与邮件（IMAP 受管轮询）入站已接入，多渠道适配器架构就绪（Slack/Webhook 待实现）
- 🔑 **用户身份凭证**：用户级凭证中枢（加密存储），渠道仅存凭证引用；前端身份凭证管理页（飞书为首个凭证类型：应用绑定/OAuth 设备流/自动绑定）
- 📋 **任务协作 + 进度追踪**：项目 + 任务 + Agent 间任务分配，状态机、执行计划/执行结果显式记录、DAG 依赖、实时进度汇总，支持委派给外部 A2A Agent
- 💬 **对话体验**：用户 ↔ Agent 双向对话、SSE 实时推送、附件上传、聊天信息侧栏（总览/任务/产物/工具）、Markdown + Mermaid 全链路渲染
- ⏰ **定时触发器**：cron trigger 完整 CRUD/Pause/Resume；启动自动注入 2 条系统默认任务（Agent 记忆沉淀 + 项目进度巡检）
- 🔍 **综合搜索**：FTS5 关键词 + 向量语义 + 知识图谱三合一
- 📊 **多维统计与监控**：Agent / Project / Task / Tool / ModelProvider 五维度统计、AOP 队列监控、系统健康仪表盘
- 🛠️ **系统管理**：数据备份与恢复、日志在线查询、基于角色的权限控制、后台进程管理
- 📡 **AOP 事件中心**：统一生产-消费事件框架，同步/异步消费模式，Producer/Consumer 完全解耦（15 类消费者：渠道入站/任务/统计/联邦/定时）
- 🧪 **质量工程**：集成测试覆盖 Auth/SysInit + CRUD + 消息投递 + 向量降级 + A2A + 飞书集成全链路；E2E Playwright 仅本地

> 功能现状以 [docs/modules/](./docs/modules/) 为准，开发规范见 [AGENTS.md](./AGENTS.md)

---

## 快速开始

```bash
# 克隆并启动
git clone https://github.com/runningcoders/ai_orz
cd ai_orz
make prod           # 生产模式：编译 → 搬运到部署根 → 启动
```

服务监听 `0.0.0.0:3000`，浏览器打开 `http://localhost:3000` 即可使用。

生产实例的落点（**部署根** `~/.ai_orz`）包含三样：数据（SQLite / DuckDB / 向量库 / Agent 工作区 / 日志）
在 `~/.ai_orz/data`，二进制在 `~/.ai_orz/bin/ai_orz`，前端产物在 `~/.ai_orz/dist` ——
都不在 git 工作树里，`git clean` / 切分支 / 删仓库都不影响它，从任意 checkout 调用 `make prod`
指向同一个生产实例。开发态（`make dev`）仍使用仓库内 `.ai_orz` 以保持隔离。

生产链路分三步：`make build`（只编译，产物留仓库）→ `make install`（搬运到部署根）→ 启动；
`make prod` = 这三步一次做完。`make package` 只消费 build 产物打包分发，不触碰本机部署根。
详见 [scripts/README.md](./scripts/README.md)「部署根」一节。

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
├── common/              # 前后端共享 DTO/枚举/配置（common/config + common/src）
├── ai-orz-macros/       # 过程宏 crate（HTTP handler / 工具注册等双宏）
├── src/                 # 后端
│   ├── handlers/        # HTTP 接口层（适配层：面向用户 API + 外部回调）
│   ├── producer/        # 事件生产者（适配层：轮询 + 外部 WS 事件接入）
│   ├── consumer/        # 事件消费者（处理 Domain 产生的内部事件）
│   ├── service/
│   │   ├── dao/         # 数据访问（本地 DB CRUD + 外部 API 出站调用）
│   │   ├── dal/         # 业务数据访问（组合 DAO，PO↔Entity 转换）
│   │   └── domain/      # 业务编排（核心业务逻辑，产生内部事件）
│   └── pkg/
│       ├── aop/         # 事件中心纯框架（Event/Producer/Consumer/Registry）
│       ├── think_path/  # 思考图引擎（awaken 与沉淀链路的执行骨架）
│       ├── adapter/     # 通用适配器基础设施（消息渠道适配中台）
│       └── ...          # 其他基础设施（向量存储、日志、策略）
├── frontend/            # Dioxus 前端（Tailwind CSS v4 + DaisyUI v5）
│   ├── src/             # 前端源码（pages / components / api）
│   ├── styles/          # Tailwind CSS 入口
│   └── public/          # 构建产物（output.css）
├── tests/               # E2E（Playwright）与跨模块集成测试
├── tools/               # 仓库工具（wiki_index.py 知识库索引器 / split_inline_tests.py）
├── scripts/             # 运维脚本（构建 / 启动 / 门禁，见 scripts/README.md）
├── migrations/          # SQLx 迁移脚本
└── docs/                # 模块化知识库（见下文）
```

详细分层规范、命名约定、PO/Entity 边界、Context 传递、适配层原则等强制规范见 [AGENTS.md](./AGENTS.md)。

---

## 知识库

`docs/` 已按**能力域**拆分为 15 个模块，每个模块三层，是人与 Agent 共用的同一份事实源：

| 层 | 回答 | 谁维护 | 权威性 |
|---|---|---|---|
| `A-设计` | **为什么**这么设计（决策快照） | 设计阶段写、落地后冻结 | 意图权威 |
| `B-落地方案` | **要去哪**（规划 / 进度） | 状态推进时更新 | 无权威性（进度可能过期）|
| `C-知识` | **是什么**（代码现状百科 + RAG 原子卡） | 代码一变就增量修订 | **现状权威** |

冲突时**以 `C-知识` 为准**（代码即事实），并回头修 `A-设计`。完整规则见 [docs/AGENT_GUIDE.md §二](./docs/AGENT_GUIDE.md)。

**入口按角色分开：**

| 我是 | 从这进 |
|---|---|
| 要改代码 / 接需求（人或 Agent） | [docs/AGENT_GUIDE.md](./docs/AGENT_GUIDE.md) — 五步流程 + 模块路由表 + 反模式 |
| 要查某个功能的现状 | [docs/modules/INDEX.md](./docs/modules/INDEX.md)（人读）/ [INDEX.json](./docs/modules/INDEX.json)（机读，含 `status`/`scope`） |
| 要写文档 / 维护索引 | [docs/README.md](./docs/README.md) — 命令速查 + 查询配方 + 维护场景 |

**三条铁律：**

1. **三类索引全是派生物**（`INDEX.md` / `INDEX.json` / 各 `00-INDEX.md`）——改文档后跑 `make wiki-index`，禁止手改索引。
2. **`status` 决定能不能当权威**：只有 `status: 现行` 的设计可照做；`草案` / `落地中` / `未标注` 一律先确认。
3. **代码变了必须回填 `C-知识`** —— 这是知识库腐化的唯一来源。

```bash
make wiki-index    # 重建索引（新增/删除/改名文档后必跑）
make wiki-check    # 提交前门禁：索引一致性 + frontmatter 合规
make wiki-stale    # 诊断：文档里引用的失效代码路径
```

---

## 文档

> 📌 **内容脉络**：知识库按 15 个能力域拆成 `A-设计`（为什么）/ `B-落地方案`（要去哪）/ `C-知识`（是什么），统一在 [docs/modules/](./docs/modules/) 下；被取代的旧设计与旧方案按其能力域留在对应模块的 `A-设计` / `B-落地方案`，用 `status: 已废弃` / `已完成` 表达生命周期，不再单独设归档目录。旧的四象限目录 `docs/wiki/`、`docs/design/`、`docs/plan/` 已于 2026-11 全部并入模块体系。

- 文档站与四张架构图已上移到 **文首「先看图 · 再看字」**，此处只列文字文档。

**入口文档：**

| 文档 | 用途 |
|---|---|
| [docs/AGENT_GUIDE.md](./docs/AGENT_GUIDE.md) | **实现需求入口**：五步流程 + 15 模块路由表 + 在途改造清单 + 反模式 |
| [docs/README.md](./docs/README.md) | 知识库地图与运维手册（命令 / 查询配方 / 维护场景 / 元数据规范）|
| [AGENTS.md](./AGENTS.md) | AI 开发规范总览 + 完整文档索引 + **贡献流程（分支与 PR，禁直推 main）** |
| [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) | 权威架构总纲（实体关系 / 设计哲学 / 核心概念）|
| [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md) | **开发必读**：分层架构实践 + 反模式避坑 |
| [docs/CODE_STANDARDS.md](./docs/CODE_STANDARDS.md) | 编码规范 SSOT（命名 / 分层 / 宏 / 错误 / 日志 / 分页）|
| [docs/memory_design.md](./docs/memory_design.md) | 四层记忆系统设计（模块 03 的"为什么"）|
| [ai-orz-doc-maintainer.md](./docs/modules/12-基础设施与部署/C-知识/ai-orz-doc-maintainer.md) | **写文档 Skill**：A-设计 / B-落地方案 全生命周期 + superpowers 蓝图精简（注册版在 `.trae/skills/`）|
| [ai-orz-wiki-maintainer.md](./docs/modules/12-基础设施与部署/C-知识/ai-orz-wiki-maintainer.md) | **同步代码到知识库 Skill**：C-知识 长文 + RAG 卡 维护规范（注册版在 `.trae/skills/`）|
| [scripts/README.md](./scripts/README.md) | 运维脚本地图与「一处实现」规则 |

**设计文档精选**（完整清单见 [docs/modules/INDEX.md](./docs/modules/INDEX.md)，`status` 列标注是否生效）：

- [外部 Agent 接入 A2A](<./docs/modules/07-外部Agent(A2A)/C-知识/A2A 集成指南.md>) — Client/Server 双角色 + 异步回调轮询
- [Runtime Domain 设计](./docs/modules/02-Agent运行时/A-设计/runtime_design.md) — Agent 唤醒与工具二分
- [思考生命周期设计](./docs/modules/02-Agent运行时/A-设计/think_lifecycle_design.md) — 唤醒图引擎（`pkg/think_path`）的理想态与落地蓝图
- [飞书渠道集成设计](./docs/modules/04-对话与消息/A-设计/lark_channel_integration_design.md) — 用户级凭证中枢 + WS 重连
- [前端架构](./docs/modules/11-前端应用/A-设计/frontend_architecture.md) — Dioxus 0.7 + HUD 设计系统

---

## License

[Apache License 2.0](LICENSE)
