# AI Orz - Agent 开发规范总览

> 🎯 **本文档供 AI 助手快速理解项目**：3 分钟了解项目是什么、代码怎么组织、去哪找规范。
>
> 功能现状以 [docs/modules/](./docs/modules/) 为准；**实现功能需求前先读 [docs/AGENT_GUIDE.md](./docs/AGENT_GUIDE.md)**（流程 / 模块路由表 / 设计 status 语义 / 反模式）；编码规范全文见 [docs/CODE_STANDARDS.md](./docs/CODE_STANDARDS.md)；文档维护细则见 [docs/DOCUMENTATION.md](./docs/DOCUMENTATION.md)。

---

## 一、项目速览

**AI Orz** — 全栈 Rust 多 Agent 协作框架，以组织化形式管理和执行 AI 代理任务。

- **后端**：Rust + Axum + SQLite + sqlx 0.8 + 原生 CortexDao（OpenAI 兼容）
- **前端**：Dioxus 0.7 (WebAssembly) + Tailwind CSS v4 + DaisyUI v5
- **质量**：全量测试 100% 通过 · clippy `-D warnings` 双端零容忍 · cargo-llvm-cov 38%/45% 门槛

### 核心能力域

| 域 | 一句话说明 |
|----|-----------|
| 👥 组织权限 | 多级组织 + 角色并查集继承 + JWT 双模式 + 偏好双源沉淀 |
| 🤖 Agent | 全生命周期 + 两阶段唤醒（IntentAnalyze 输入理解 → 图引擎 awaken）+ 策略引擎 |
| 🧠 记忆 | 四层记忆 + 休息沉淀 + 知识图谱 + 种子推荐 |
| 💬 对话消息 | SSE 实时推送 + 多渠道入站（飞书 WS / 微信 iLink / 邮件 IMAP）+ 5 类出站骨架 |
| 📋 任务项目 | 任务状态机 + 进度追踪 + TaskGraph DAG + 项目聚合上下文 |
| 🛠️ 工具技能 | 三层调用架构 + 12 份预置技能（7 个神经技能常驻）+ 技能副本自我进化 + MCP 集成 |
| 🔌 外部 Agent | A2A 协议 Client/Server + 异步回调 |
| 🔎 搜索 | FTS5 + 向量 + 图谱三位一体混合搜索 |
| 📊 统计监控 | DuckDB 五维统计 + 运行时观测 + 系统健康仪表盘 |
| 🚀 异步基建 | AOP 事件中心 + 15 类消费者 + cron 定时触发器 |
| 🎨 前端 | Dioxus 44 路由 + HUD Canvas + Markdown/Mermaid 全链路 |

---

## 二、架构速览

### 分层架构（严格单向调用）

```
Adapter (适配层)    → 协议解析 / 鉴权 / DTO↔Command 转换
    ↓
Domain (领域层)     → 核心业务逻辑编排 / 跨领域事务 / 产生内部事件
    ↓
DAL (业务数据层)    → 组合多个 DAO / PO↔Entity 双向转换
    ↓
DAO (数据访问层)    → 单一数据源 CRUD / 外部 API 出站调用
    ↓
Models (PO)
```

**核心红线**：禁止跨层调用、禁止同层互调、PO 不暴露到 Domain 层及以上。

> 完整分层职责表 + 避坑实例 → [LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md)

### 实体关系

```
Organization ──┬── User
                ├── ModelProvider
                └── Agent
                     └── Brain
                          ├── Cortex（思考执行，绑定 ModelProvider）
                          └── Memory
                               ├── Core（角色设定、能力清单）
                               ├── Working（当前会话工作记忆）
                               ├── Short-Term（最近会话摘要索引）
                               └── Long-Term（长期沉淀知识图谱）
```

---

## 三、开发规范路由表

> 📌 **按需读取**：每种场景只需要读对应的规范文档即可，无需通读全量。

| 你要做什么 | 读取文档 |
|-----------|---------|
| **命名 / 函数前缀**（`get_`/`query_`/`search_`/`embed_` 等） | [CODE_STANDARDS.md §1](./docs/CODE_STANDARDS.md) |
| **数据对象分层**（DTO / Entity / PO / Command 边界） | [CODE_STANDARDS.md §2](./docs/CODE_STANDARDS.md) |
| **Trait 定义位置 / Domain 层约定** | [CODE_STANDARDS.md §3](./docs/CODE_STANDARDS.md) |
| **RequestContext 参数传递** | [CODE_STANDARDS.md §4](./docs/CODE_STANDARDS.md) |
| **枚举类型安全**（DB 映射型 / 纯领域型） | [CODE_STANDARDS.md §5](./docs/CODE_STANDARDS.md) |
| **SQLite + SQLx 规范**（STRICT / FTS5 / 测试隔离） | [CODE_STANDARDS.md §6](./docs/CODE_STANDARDS.md) |
| **Handler 双宏标注**（`generate_http_handler` + `register_handler_tool`） | [CODE_STANDARDS.md §7](./docs/CODE_STANDARDS.md) |
| **日志宏使用**（`log_info!` / `log_error!`） | [CODE_STANDARDS.md §8](./docs/CODE_STANDARDS.md) |
| **向量化实体**（`Vectorizable` trait / `embed_entity`） | [CODE_STANDARDS.md §9](./docs/CODE_STANDARDS.md) |
| **查询分页 + 通用 count**（`PagedResult` / `push_query_filters`） | [CODE_STANDARDS.md §10](./docs/CODE_STANDARDS.md) |
| **两阶段初始化 + 基础数据注入** | [CODE_STANDARDS.md §11](./docs/CODE_STANDARDS.md) |
| **统一错误处理**（`err!` / `bail_err!` / `ensure_err!`） | [CODE_STANDARDS.md §12](./docs/CODE_STANDARDS.md) |
| **统计事件**（`record_event!` / `StatsEvent` derive） | [CODE_STANDARDS.md §13](./docs/CODE_STANDARDS.md) |
| **前后端 API 协议**（DTO 单一事实源 / 结构体化参数） | [CODE_STANDARDS.md §14](./docs/CODE_STANDARDS.md) |
| **基础设施工具位置**（FTS5/向量/日志/JWT 放 pkg/） | [CODE_STANDARDS.md §14.1](./docs/CODE_STANDARDS.md) |
| **双端复用逻辑下沉 common**（校验规则 / 值映射 / 白名单矩阵） | [CODE_STANDARDS.md §14.2](./docs/CODE_STANDARDS.md) |

---

## 四、文档索引

| 分类 | 文档 | 场景 |
|------|------|------|
| **实现需求入口** | [AGENT_GUIDE.md](./docs/AGENT_GUIDE.md) | 实现功能需求前的五步流程 + 15 模块路由表 + 设计 status 语义 + 反模式 |
| **知识库运维手册** | [docs/README.md](./docs/README.md) | 常用命令 / 查询配方（按 `scope` 反查设计）/ 维护场景 / 元数据规范 |
| **知识库机器索引** | [INDEX.json](./docs/modules/INDEX.json) | 全库三层索引（含 status/scope），反查「改这个文件该读哪篇设计」 |
| **编码规范 SSOT** | [CODE_STANDARDS.md](./docs/CODE_STANDARDS.md) | 所有编码规则（命名/分层/宏/错误/日志等） |
| **架构总纲** | [ARCHITECTURE.md](./docs/ARCHITECTURE.md) | 实体关系、设计哲学、核心概念 |
| **分层实践** | [LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md) | 适配层架构 + 反模式避坑 |
| **文档维护规范** | [DOCUMENTATION.md](./docs/DOCUMENTATION.md) | 三层文档（A-设计 / B-落地方案 / C-知识）编写/引用/图谱法则 |
| **设计/方案文档 Skill** | [ai-orz-doc-maintainer.md](./docs/modules/12-基础设施与部署/C-知识/ai-orz-doc-maintainer.md) | A-设计/B-落地方案 全生命周期 + superpowers 蓝图精简（`.trae/skills/` 注册版）|
| **活文档维护 Skill** | [ai-orz-wiki-maintainer.md](./docs/modules/12-基础设施与部署/C-知识/ai-orz-wiki-maintainer.md) | C-知识 长文 + RAG 卡 代码同步（`.trae/skills/` 注册版）|
| **API 协议** | [api_protocol_convention.md](./docs/modules/13-数据模型/A-设计/api_protocol_convention.md) | common DTO 单一事实源 |
| **SQL 规范** | [sqlx_guide.md](./docs/modules/12-基础设施与部署/A-设计/sqlx_guide.md) | SQLx 0.8 + SQLite 工程规范 |
| **日志规范** | [logging_design.md](./docs/modules/12-基础设施与部署/A-设计/logging_design.md) | 统一宏 + tracing 语法 |
| **Runtime** | [runtime_design.md](./docs/modules/02-Agent运行时/A-设计/runtime_design.md) | Agent 唤醒 + 工具二分 |
| **策略引擎** | [thinking_task_policy_engine_design.md](./docs/modules/02-Agent运行时/A-设计/thinking_task_policy_engine_design.md) | Policy trait + policy_set! 宏 |
| **前端架构** | [frontend_architecture.md](./docs/modules/11-前端应用/A-设计/frontend_architecture.md) | Dioxus + 44 路由 |
| **查询规范** | [pagination_and_count_convention.md](./docs/modules/13-数据模型/A-设计/pagination_and_count_convention.md) | 分页 + 通用 count |
| **记忆设计** | [memory_design.md](./docs/memory_design.md) | 四层记忆系统 |
| **脚本地图** | [scripts/README.md](./scripts/README.md) | 运维脚本入口、职责划分与「一处实现」规则 |
| **模块知识** | [docs/modules/](./docs/modules/) | 15 个能力域模块，每模块 A-设计/B-落地方案/C-知识；全量索引 [docs/modules/INDEX.md](./docs/modules/INDEX.md)（自动生成）|
| **RAG 知识卡** | [docs/modules/](./docs/modules/) | 原子卡现位于各模块 C-知识/（第一召回层）|

---

## 五、知识库检索操作指令

**查询前 5 步**（强制执行）：

1. **scope 预过滤** → 用 `docs/modules/INDEX.json` 的 `scope[]`（或知识卡 YAML 的 `scope[]`）匹配当前关注的文件集；`jq` 现成命令见 [docs/README.md §六 查询配方](./docs/README.md)
2. **读命中 RAG 知识卡**（`C-知识/<卡名>/<卡名>.md`）→ §4 硬约束 → §2 关键文件表 → §3 架构约定 → §1 概述
3. **强制跳模块长文** → 从卡的 `source_files[]` 找同模块 `C-知识/*.md` 现状长文，读 §5 分析 + §8 故障排查
4. **跳源码锚点** → 按 `相对路径#Lx-Ly` 读真实代码
5. **按需补跳** → `A-设计`（为什么，**只认 `status: 现行`**）/ `B-落地方案`（做到哪了，无权威性）

**阅读链路**：`RAG 知识卡 → C-知识长文 → 源码 → A-设计 → B-落地方案`（禁止跳过第 3 步）

> **第 3 步为什么不能跳**：RAG 卡是原子切片，只看卡会漏掉跨切片的调用链与故障排查；长文是唯一把"局部正确"串成"系统正确"的地方。

**同主题多卡**：全部召回、并行阅读、不做去重。完全重复（scope 互为子集 + §4 重叠率 > 90%）→ 走合并流程（见 [DOCUMENTATION.md §3](./docs/DOCUMENTATION.md)）。

**元问题第一跳**：[RAG 知识索引卡](<docs/modules/08-搜索/C-知识/RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配/RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配.md>)

---

## 六、文档规范速记

**操作手册见 [docs/README.md](./docs/README.md)**（命令 / 查询配方 / 维护场景 / 元数据规范）；**编写与查重全量细则见 [docs/DOCUMENTATION.md](./docs/DOCUMENTATION.md)**：

- 路径引用一律 `相对仓库根路径 + #Lx-Ly`；禁 `file://` 伪协议、绝对路径、冒号行号
- **`A-设计` / `B-落地方案` 顶部必带 YAML frontmatter**（`kind/module/name/title/status/scope/related`），缺则 `make wiki-meta` 直接失败
- 新增 RAG 知识卡（各模块 `C-知识/`）→ 先按 [DOCUMENTATION.md §3](./docs/DOCUMENTATION.md) 的 5 级算法查重，再决定合并还是新建
- 引用单向：`C-知识`（活文档）→ `A-设计`/`B-落地方案`（快照）+ 源码；**新增快照文档不得反向链接 `C-知识`**

**文件落位**：

| 类型 | 目录 | 命名 |
|------|------|------|
| 功能设计（为什么） | `docs/modules/<模块>/A-设计/` | 英文 snake_case（如 `runtime_design.md`）|
| 长期规范（编码/协议约定） | `docs/modules/<模块>/A-设计/` | 英文 snake_case（`*_guide.md` / `*_convention.md`）|
| 落地方案（做到哪了） | `docs/modules/<模块>/B-落地方案/` | 中文主题名（如 `身份凭证Domain统一CRUD重构.md`）|
| 现状长文 + RAG 卡（是什么） | `docs/modules/<模块>/C-知识/` | 中文主题名；RAG 卡自带同名子目录 |
| 跨模块总纲 | `docs/` 根目录 | `ARCHITECTURE.md` / `CODE_STANDARDS.md` / `LAYERED_ARCHITECTURE_PRACTICE.md` 等 |

**改完必跑的收尾**：

```bash
make wiki-index && make wiki-check    # 重建索引 + 门禁（索引产物一起提交）
```

**红线**：
- ❌ 在 `docs/` 下自创新目录（模块 = 能力域，不是随手建文件夹）
- ❌ **手改 `INDEX.md` / `INDEX.json` / 各 `00-INDEX.md`**（全是 `make wiki-index` 的派生物）
- ❌ **改了代码不回填对应 `C-知识`**（知识库腐化的唯一来源）
- ❌ plan 文件名带日期前缀
- ❌ `A-设计` 用中文命名 / `B-落地方案` 用英文命名

---

## 七、贡献流程（分支与 PR）

`main` 是受保护分支：**禁止直推**，所有改动一律经 Pull Request 合入。

### 分支

| 层 | 分支名 | 用法 |
|---|---|---|
| L0 | `main` | 只 `git pull` 取基准，**不在 main 上开发或提交** |
| L1 | `feature/<slug>` / `bugfix/<slug>` | 从最新 `origin/main` 切出；**一个主题一条**，改动全部先汇到这条 |
| L2 | `dev/<agent>-<slug>` | 多 Agent 并行时的个人工作分支，本地合回 L1 即可，不必推远端 |

单人小改动只用 L1 就够；多人 / 多 Agent 并行才需要 L2。

### 提交前

```bash
make ci    # = lint（fmt + clippy + clippy-fe + docs-lint + inline-test-lint + wiki-check）+ 全量测试
```

### 提 PR

```bash
git push -u origin feature/<slug>                 # 推自己的功能分支，永不推 main
gh pr create --base main --head feature/<slug>     # 标题与正文写清「为什么改」，关联任务号
```

PR 需满足：四项必需检查 `fmt` / `lint` / `backend` / `frontend` 全绿；分支与 `main` 保持同步（strict）；不夹带本主题之外的改动。

### 合并

- 合并方式**统一 squash**：一条 PR 对应 `main` 上一条提交，历史保持线性可读。
- 合并后删除源分支。
- **merge 决策归仓库维护者**；贡献者负责推功能分支 + 开 PR，不做合并。

### 红线

- ❌ 直推 `main`（即使技术上能绕过保护规则，也**不得**这么做）
- ❌ 一条 PR 混装多个主题
- ❌ 带红门禁提 PR，或跳过 `make ci`

---

*实现需求先读 [AGENT_GUIDE.md](./docs/AGENT_GUIDE.md)；规范与约定看 [CODE_STANDARDS.md](./docs/CODE_STANDARDS.md)；功能现状看 [docs/modules/](./docs/modules/)；设计决策看各模块 `A-设计/`；文档操作看 [docs/README.md](./docs/README.md)，编写细则看 [DOCUMENTATION.md](./docs/DOCUMENTATION.md)*
