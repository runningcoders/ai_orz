# AI Orz - Agent 开发规范总览

> 🎯 **本文档供 AI 助手快速理解项目**：5 分钟了解项目是什么、代码怎么组织、开发遵循什么规范。
>
> 本文档只维护**架构规范与开发约定**；功能现状以 [docs/wiki/](./docs/wiki/) 为准，变更历史以 git log 为准，设计决策见 [docs/design/](./docs/design/)。

---

## 一、项目概览

### 1.1 项目是什么

**AI Orz** - 全栈 Rust 多 Agent 协作框架，以组织化形式管理和执行 AI 代理任务

- **后端**：Rust + Axum + SQLite + sqlx 0.8 + 原生 CortexDao（OpenAI 兼容）
- **前端**：Dioxus 0.7 (WebAssembly) + Tailwind CSS v4 + DaisyUI v5
- **技术特色**：严格分层架构、类型安全、1124 个测试 100% 通过率、clippy `-D warnings` 零容忍（后端 + 前端 wasm32）、cargo-llvm-cov 覆盖率门槛（PR 38% / main 45%）、30+ 主题切换

### 1.2 核心能力域概览

> 功能细节（页面、接口、组件实现）以 wiki 为准，本表只做域级速览。

| 能力域 | 覆盖范围 |
|--------|---------|
| 👥 组织与权限 | 多级组织、用户角色（Member → Admin → SuperAdmin 并查集继承）、JWT Cookie + Bearer 双模式认证、**用户偏好双源沉淀**（users.preferences 自报 + 知识图谱 `user_preference` tag 推断） |
| 🤖 Agent | 全生命周期（创建/配置/工具绑定/入职）、唤醒执行、**两阶段唤醒（IntentAnalyze 先理解再执行 + Awaken 正式执行）**、多回合循环控制、Auto/Manual 工具调用三层分发、Agent 间协作、**思考运行时可观测（runtime-status/cancel-thinking/runtime-list 接口 + 前端轮询面板）**、**策略引擎（Policy trait + PolicyGroup + policy_set! 宏，5 个内置策略 + 混合模式组合）** |
| 🧠 记忆 | 四层记忆（Core/Working/Short-term/Long-term）、休息沉淀机制（agent_rest 定时）、知识图谱可视化、**task_id 注意力聚焦 + trace_ids 强制写入**、**种子节点推荐 recommend_seed_nodes + 图谱遍历 traverse_knowledge_graph** |
| 💬 对话与消息 | 用户 ↔ Agent 双向对话、SSE 实时推送、消息渠道（飞书 WS 入站已上线，微信/Slack/邮件/Webhook 出站骨架）、**用户身份凭证中枢（AES-256-GCM 加密存储 identity_credentials）** |
| 📋 任务与项目 | 任务状态机 + 进度追踪（0-100）、execution_plan/execution_result、项目聚合对话上下文、统一附件与产物存储 |
| 🛠️ 工具与技能 | 统一工具调用架构、工具包/技能包（tag 分组）、5 份预置技能、MCP 服务器集成 |
| 🔌 外部 Agent | A2A 协议 Client/Server、异步结果回传（Push 回调 + 30 秒轮询兜底） |
| 🔎 搜索 | FTS5 关键词 + 向量语义 + 图谱关系三位一体混合搜索，LanceDB 默认 + 多后端（HNSW/InMemory/SQLite VSS）、**tags 语义过滤** |
| 📊 统计与监控 | DuckDB 多维统计（五维度）、运行时内存统计收集器、AOP 队列监控、系统健康仪表盘（7 维度含飞书 WS）、**AgentAwakeEvent exit_reason 字段（思考退出原因统计分析）** |
| 🚀 异步基础设施 | AOP 事件中心（纯框架零业务依赖）、消费者框架（8 类消费者：调度/Agent 循环/任务/消息/工具执行/日志/统计/思考轮次）、定时触发器（cron，启动幂等注入 2 条系统默认任务） |
| 🖥️ 系统运维 | 结构化日志（宏化）、后台进程管理（shell_exec 双露工具）、数据备份恢复、日志在线查询 |
| 🎨 前端 | Dioxus Router 41 条路由 + Tailwind v4 + DaisyUI v5 + 30+ 主题、HUD Canvas 可视化（图谱/图表/看板/仪表盘）、Markdown + Mermaid 全链路渲染、**文档中心动态加载 design/plan/archive/wiki** |
| 🧪 质量工程 | 1124 测试 100% 通过率（后端 984：897单元+87集成 / 前端 82 / common 58，DAO/DAL/Domain/Handler/Pkg全覆盖；集成 87个/19 targets：Auth/SysInit CoreCRUD MsgDelivery VectorDegradation A2AFow PresetSkills CronTriggers LarkIntegration MsgChannel AgentAwaken 宏集成）、clippy `-D warnings` 双端零容忍、cargo-llvm-cov 覆盖率门槛（PR 38%/main 45%）、分层模块 DAO25/DAL23/Domain7/Handler8 零闲置（每 domain 含 init_base_data 扩展点）、E2E Playwright（仅本地） |
| 📚 **知识体系 + RAG 自索引** | **4 类文档，单向引用模型（v2.1）：**① `docs/design/` 为什么 / ② `docs/plan/` 怎么做+结果（两者为历史快照，写定冻结，完成后精简归档）/ ③ `docs/wiki/zh/content/` 百科（8 大板块 353 篇）/ ④ `docs/wiki/knowledge/zh/` 54+ 张 RAG 原子知识卡（总结+索引，RAG 第一召回层）；③④ 为活文档，`<cite>` / `source_files[]` **单向指向** ①② 与源码（代码引用 `相对路径#L起始-L结束`，文档引用 `相对仓库根路径`）；①② **不反向链** ③④（冻结文档无法跟进 wiki 改版 = 永久断链）。**阅读链路（严格顺序，禁跳过 Wiki）**：④卡 → ③长文 → 源码 → ①Design → ②Plan。知识卡 YAML `scope[]` 按 glob 过滤关注文件集，`source_files[]` 必须含源码锚点 + ③ 长文（①② 有则写）。同主题多张平行卡：按 §2.1.3 图谱法则判定合并/拆分，禁止裸重叠。维护 Skill：ai-orz-wiki-maintainer（③+④ 活文档）+ ai-orz-doc-maintainer（①+② 全生命周期含归档） |

---

## 二、文档快速索引

> 📌 **按需要读取详细设计文档**

### docs 内容脉络（四类文档，单向引用模型 v2.1）

**标准阅读链路（强制执行）**：`④ RAG 知识卡 → ③ Wiki 百科长文 → 源码 → ① Design → ② Plan`。**禁止跳过 ③ Wiki 长文**直接从 ④ 跳源码。

| 目录 | 用途 | 约束 | 维护 Skill |
|------|------|------|-----------|
| ④ `docs/wiki/knowledge/zh/` | RAG 第一召回层：54+ 张原子卡（总结+索引）| YAML 5 字段 + 4 节固定；`source_files[]` 含源码锚点 + ③ 长文（≥1 条，④→③ 活文档区闭环）+ ①②（有则写）；同主题多卡按 §2.1.3 图谱法则判定 | ai-orz-wiki-maintainer |
| ③ `docs/wiki/zh/content/`（入口 `docs/wiki/`）| 百科：8 大板块 353 篇（是什么）| 10 节固定目录；`<cite>` 关联源码 + ④ RAG 卡（①② 有则写，单向引用）| ai-orz-wiki-maintainer |
| ① `docs/design/` | 决策快照（为什么）| 写定后不追代码；关联文档段仅可链 ② plan（冻结互链）+ 上层权威文档；**禁止新增指向 ③④ 的链接**；功能完成后精简归档（归档件不写跨象限引用）| ai-orz-doc-maintainer |
| ② `docs/plan/` | 落地快照（怎么做+结果）| 7 章骨架（无 checkbox/命令/代码快照）；关联文档段仅可链 ① design + 上层权威文档；**禁止新增指向 ③④ 的链接**；功能完成后精简归档 | ai-orz-doc-maintainer |
| `docs/archive/` | 历史归档 | 只进不出，文头加一句归档说明；归档件不写任何跨象限引用；按来源分子目录（`design-archive/` / `plan-archive/` / `superpowers-archive/YYYY-MM-DD/`），**根目录禁散放** | ai-orz-doc-maintainer |
| `docs/superpowers/*/` | 开发期执行蓝图（临时） | 功能完成 7 天内处置：→ plan 7 章模板 / → archive 封存 | ai-orz-doc-maintainer |
| `docs/ARCHITECTURE.md` | 核心概念与实体关系 | 唯一权威纲要，手工维护 | — |
| `docs/LAYERED_ARCHITECTURE_PRACTICE.md` | 分层实践与避坑 | Agent 必遵循，手工维护 | — |

**路径格式铁律（引用时统一）**：
- 跳代码 → `相对路径#L起始-L结束`（如 `src/pkg/logging.rs#L15-L42`，GitHub 原生高亮，IDE 文件级跳转）
- 跳文档 → **`相对仓库根路径`**（如 `docs/design/xxx.md`，IDE 可点 + GitHub 可解析）
- 引用方向：**只有 ③④（活文档）→ ①② / 源码 / 兄弟卡是受维护方向**；①② → ③④ 禁止新建（存量不要求回删）

**维护流程（单向，无交叉等待）**：
1. 代码变化 → 只同步 ③ + ④（缺一会导致 RAG 召回过期知识）；wiki/RAG 的 cite、source_files 单向指向 ①② 与源码
2. 新功能开发 → doc-maintainer 写 ① design + ② plan（只写自身内容 + ①② 互链）→ 功能完成 → 场景 D 精简归档 → 通知 wiki-maintainer 重定向 ③④ 中的旧路径

#### 文件落位与命名约定（强制执行，新建文档先查本表）

| 象限 | 落位目录 | 命名格式 | 示例 |
|------|---------|---------|------|
| ① 功能设计 | `docs/design/` | 英文 snake_case，`<topic>_design.md` | `runtime_design.md` |
| ① 长期规范（不归档） | `docs/design/` | 英文 snake_case，`<topic>_guide.md` / `<topic>_convention.md` | `sqlx_guide.md`、`api_protocol_convention.md` |
| ② 落地快照 | `docs/plan/` | **中文主题名**（无日期前缀，与功能语义同名） | `身份凭证Domain统一CRUD重构.md` |
| 历史决策归档 | `docs/archive/design-archive/` | 保留原文件名 | `docs/archive/design-archive/a2a_server_design.md` |
| 已完成 plan 归档 | `docs/archive/plan-archive/` | 保留原文件名 | `docs/archive/plan-archive/聊天MVP.md` |
| superpowers 蓝图处置 | `docs/archive/superpowers-archive/YYYY-MM-DD/` | 保留原蓝图名 | `docs/archive/superpowers-archive/2026-08-16/xxx.md` |

**红线**：
- ❌ 在 `docs/` 下自创新目录（如 `docs/specs/`、`docs/notes/`）——四象限之外无处安放
- ❌ plan 文件名带日期前缀（`2026-08-15-xxx.md` 是 superpowers 蓝图风格，进 `docs/plan/` 时必须去掉日期）
- ❌ 归档件散放在 `docs/archive/` 根目录——必须进对应子目录
- ❌ design 用中文命名 / plan 用英文命名（保持两目录既有风格一致性，便于 grep 与目录扫描）

---

### RAG 查询操作指令

**查询前准备**：本仓库已接入 Trae/IDE RAG，对 54+ 张 `docs/wiki/knowledge/zh/` 原子卡做向量语义 chunk 召回。按以下 5 步执行：

1. **scope 预过滤**：若当前上下文含「用户指定关注文件集」或已知 IDE 打开文件列表 → 先用每张卡 YAML `scope[]` glob 匹配，不匹配的卡直接丢弃（不参与向量打分）
2. **读命中卡 ④**：优先读 §4 硬约束（最高权重）→ §2 关键文件表 → §3 架构约定 → §1 概述
3. **强制跳对应 Wiki 长文 ③**：从卡 `source_files[]` 中找 ③ Wiki 长文相对仓库根路径（`docs/wiki/zh/content/...`）形式的链接，立即跳 §5 详细分析 + §8 故障排查（系统化上下文，短卡不够）
4. **跳源码锚点**：从长文 cite/章节来源段 OR 卡 `source_files[]`，按 `相对路径#Ln-Lm` 读真实代码
5. **按需补跳 ① Design / ② Plan**：① 找为什么/决策表；② 找扩展入口速查表 §4 + §七 4 步扩展模板

**同主题多张平行卡**：全部召回、并行阅读、不做去重、不删旧卡（语义相近 = 不同切面，信息互补）。⚠️ **但「完全重复版本」不属此类**——scope[] 互为子集、§4 硬约束重叠率 > 90%、只是措辞不同的重复卡，必须走「吸收合并 + 直接删除副卡」，绝不可当作「平行互补」保留。见下方 §2.1.3 图谱节点组织法则。

**RAG 元问题第一跳**（如何使用知识卡 / 召回不到 / scope 匹配 / source_files 写法）→ 命中：
- [RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配](docs/wiki/knowledge/zh/RAG%20%E7%9F%A5%E8%AF%86%E7%B4%A2%E5%BC%95%EF%BC%9A%E5%A6%82%E4%BD%95%E4%BD%BF%E7%94%A8%E7%9F%A5%E8%AF%86%E5%8D%A1%E7%89%87%E5%81%9A%E5%8F%AC%E5%9B%9E%E6%A3%80%E7%B4%A2%E3%80%81%E9%94%9A%E5%AE%9A%E4%B8%8E%20scope%20%E5%8C%B9%E9%85%8D/RAG%20%E7%9F%A5%E8%AF%86%E7%B4%A2%E5%BC%95%EF%BC%9A%E5%A6%82%E4%BD%95%E4%BD%BF%E7%94%A8%E7%9F%A5%E8%AF%86%E5%8D%A1%E7%89%87%E5%81%9A%E5%8F%AC%E5%9B%9E%E6%A3%80%E7%B4%A2%E3%80%81%E9%94%9A%E5%AE%9A%E4%B8%8E%20scope%20%E5%8C%B9%E9%85%8D.md)

### 文档索引

| 分类 | 文档 | 优先级 |
|------|------|--------|
| **架构总览** | [README.md](./README.md) / [docs/wiki/](./docs/wiki/)（③ Wiki 百科入口，8 大板块） / [docs/wiki/knowledge/zh/](./docs/wiki/knowledge/zh/)（④ RAG 第一召回层）/ [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) / [docs/CODE_WIKI.md](./docs/CODE_WIKI.md) | ⭐⭐⭐ |
| **分层实践** | [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md)（Agent 必遵循，含适配层架构原则） | ⭐⭐⭐ |
| **API 协议** | [docs/design/api_protocol_convention.md](./docs/design/api_protocol_convention.md)（common DTO 单一事实源） | ⭐⭐⭐ |
| **SQL 规范** | [docs/design/sqlx_guide.md](./docs/design/sqlx_guide.md)（SQLx 0.8 + SQLite、STRICT、FTS5、测试隔离） | ⭐⭐⭐ |
| **日志规范** | [docs/design/logging_design.md](./docs/design/logging_design.md)（统一宏、上下文检测） | ⭐⭐⭐ |
| **Runtime** | [docs/design/runtime_design.md](./docs/design/runtime_design.md)（Agent 唤醒、神经 vs 外骨骼工具二分） | ⭐⭐⭐ |
| **策略引擎** | [docs/design/thinking_task_policy_engine_design.md](./docs/design/thinking_task_policy_engine_design.md)（Policy trait + policy_set! 宏 + 思考运行时可观测） | ⭐⭐⭐ |
| **前端** | [docs/design/frontend_architecture.md](./docs/design/frontend_architecture.md) / [docs/design/ui_design_system.md](./docs/design/ui_design_system.md) | ⭐⭐⭐ / ⭐⭐ |
| **查询规范** | [docs/design/pagination_and_count_convention.md](./docs/design/pagination_and_count_convention.md)（分页 + 通用 count） | ⭐⭐ |
| **记忆设计** | [docs/memory_design.md](./docs/memory_design.md)（四层记忆系统、检索策略） | ⭐⭐ |
| **模块设计** | tool / message_interaction / message_channel / lark_cli_integration / consumer_architecture / task_scheduler / event / skill / vector_search_architecture / external_agent（均在 docs/design/ 下） | ⭐⭐ |
| **业务设计** | task / project / organization / attachment_storage（均在 docs/design/ 下） | ⭐ |

---

### §2.1 文档编写与维护规范（强制执行）

> 🎯 **铁律**：代码是 SSOT（字段级细节跳源码），文档只承载代码无法表达的信息：设计动机、模块边界、影响面、扩展路径。

---

#### 2.1.1 强制文件头（标题后紧跟）

```markdown
# [文档标题]

> 🎯 **定位**：[一句话说明角色/象限/精度]
> 状态：[草稿|定稿|vX.Y|归档 YYYY-MM-DD]
> 触发场景：[一句话告诉 Agent 何时打开，否则直接读代码]
>
> 关联文档：
> - [上层权威文档](../../AGENTS.md) — 关联点
> - [横向文档](../design/xxx.md) — 关联点
```

**状态枚举**：草稿（设计中）/ 定稿（不追代码）/ vX.Y（YYYY-MM-DD，多版本）/ 归档 YYYY-MM-DD（只进不出）
**归档清单项**：删除 writing-plans 标记（`REQUIRED SUB-SKILL` 行）与所有 `- [ ] Step N` checkbox

---

#### 2.1.2 路径引用统一规范（强制执行，三环境通跳）

> 🎯 **核心原则：一律写「相对仓库根的相对路径」，永不写本机绝对路径，永不写 `file://` / `file:///` 伪协议。行号用 `#Lx-Ly` fragment（GitHub 原生兼容；IDE 降级为文件级跳转，可接受）。**

| 引用类型 | 唯一合法格式 | 例子 |
|----------|------------|------|
| 代码（行范围） | `[描述](路径#L起始-L结束)` | `[日志初始化](src/pkg/logging.rs#L15-L42)` |
| 代码（单行） | `[描述](路径#L行)` | `[UserRole 定义](common/src/enums/user.rs#L8)` |
| 代码（无行号） | `[描述](路径)` | `[初始迁移](migrations/20260420000000_initial.sql)` |
| 文档互引 | `[描述](docs/...md)` | `[日志设计](docs/design/logging_design.md)` |
| Wiki 长文 | `[描述](docs/wiki/zh/content/...md)` | `[日志系统](docs/wiki/zh/content/功能模块/系统管理/日志管理系统.md)` |
| RAG 卡 | `[描述](docs/wiki/knowledge/zh/...md)` | `[日志宏卡](docs/wiki/knowledge/zh/日志系统/日志宏设计.md)` |
| 外部链接 | 直接写 http(s) | `[sqlx](https://docs.rs/sqlx)` |

**三环境行为**：
| 环境 | 行为 |
|------|------|
| GitHub 仓库页 | 相对链接自动解析为 `blob/<branch>/path#Lx-Ly` 并高亮行 ✅ |
| 本地 IDE | Cmd+Click 打开文件（fragment 被忽略，文件级跳转）⚠️ |
| 前端文档中心 | 渲染期后处理 + 点击拦截 → GitHub blob 新窗口 ✅ |

**注意**：md 链接目标里的空格必须写成 `%20`（如 `CoreTool%20trait`），中文字符原样保留。

**红线（tools/docs_lint CI 必 fail）**：
- ❌ 本机绝对路径（`file:///Users/...` 或裸 `/Users/...`）
- ❌ `file://` 伪协议前缀
- ❌ 行号写 legacy 冒号格式 `path:15-42`（存量已迁移归零；分类器兼容解析但新文禁写）

**契约型代码块规则（不变）**：trait 签名/struct 字段/enum 变体/SQL schema/ASCII 图可留代码块，紧邻下方附 `> 当前实现：[xxx.rs#L12-L50](src/xxx.rs#L12-L50)`；实现快照型（函数体/控制流/命令）删代码块，改 `> 逻辑见：[func](src/xxx.rs#L288-L352)`。

---

#### 2.1.3 知识图谱节点组织与 RAG 卡拆分合并强制规范（强制执行）

> 🎯 **定位**：治理「增量构建 Wiki / RAG 卡时，不查重直接新建，导致 30+ 张完全重复版本」的系统性问题。定义知识图谱节点「合并/拆分/关联」的判定标准与标准写法。
>
> **适用范围**：`docs/wiki/zh/content/` ③ Wiki 长文节点 + `docs/wiki/knowledge/zh/` ④ RAG 原子知识卡节点。**所有执行 wiki 同步 / 新增 RAG 卡的 Agent（ai-orz-wiki-maintainer 技能）必须先过本法则，再决定是「合并」还是「新建」。**

---

##### 2.1.3.1 黄金法则（两条，优先级最高）

1. **优先合并**：候选主题 T 到来 → 先扫现存节点做相似度匹配，命中已有节点就**先尝试合并**。只有合并失败（颗粒度爆炸、视角完全不兼容）才考虑拆分建新节点。
2. **明确拆分 + 必连线**：确有关联但不适合合并时，才分开展示。拆分后必须在两张卡之间**建立显式关联声明**（source_files[] 互引 + §3 说明关系类型）。拆分不连线 = 两张孤立重复命中 = FAIL。

---

##### 2.1.3.2 5 级决策算法（每个候选主题 T 必须走一次，不可跳过）

```
候选主题 T（来自代码改动或 Design 文档）
    │
    ▼
Step 0. 前置查重（必填分支，不可省略）
    扫 docs/wiki/knowledge/zh 全部 md：
      ① name 关键词模糊匹配
      ② category 精确匹配
      ③ scope[] glob 交集面积 >= 30%
    │
    ├── 命中 0 张 → 【Level 5：纯新主题】 → 直接新建卡 ✅
    │
    └── 命中 ≥ 1 张 → 按 Level 1~4 逐层判定（按顺序优先级从高到低）
            │
            ├── Level 1（完全重复）❌禁止新建，强制合并 + 删除副卡
            │   判定：scope[] 互为子集 AND §4 硬约束重叠率 > 90%
            │         AND 主题 T 是旧卡主题的同义词/不同措辞
            │   动作：把 T 独有源码锚点 → 旧卡 §2；T 独有硬约束 → 旧卡 §4；
            │         旧卡 YAML scope[]/source_files[] 取并集；
            │         T 的草稿副卡 → 直接删除（历史靠 git 追溯，不归档）；
            │         Design/Plan/Wiki 所有引用副卡路径 → 替换为旧卡路径。
            │
            ├── Level 2（主卡-子卡 层级包含）优先合并，合并不下才拆分
            │   判定：scope[T] ⊂ scope[旧卡]（真子集）
            │         （如「日志自动上下文字段注入」⊂「日志系统」）
            │   动作首选（合并）：T 内容合并进旧卡 §3 加编号小节 + §4 追加硬约束 +
            │                       §2 追加独有源码锚点
            │   动作备选（拆分，满足以下任一才允许）：
            │         a) 合并后主卡 §4 硬约束超过 15 条；或
            │         b) 旧卡 scope[] 与 T scope[] 实际是两个不相交主题的并集
            │   拆分后声明：
            │         主卡 §2 表格末行加：`细粒度拆解：[子卡名](docs/wiki/knowledge/zh/子卡名.md)`
            │         子卡 §3 开头加：`本卡是 [主卡名](docs/wiki/knowledge/zh/主卡名.md) 的 XX 模块细粒度展开`
            │         子卡 source_files[] 末尾追加主卡相对仓库根路径
            │
            ├── Level 3（总卡-视角细卡 分层/分视角）允许拆分，但必互相声明关联
            │   判定：scope[] 交集 30%-80%，是同一体系的不同层面
            │   典型模式：
            │     • 严格分层：身份凭证 Model 层 / Domain 层 / Handler 层（三张）
            │     • 协议三角：A2A 协议层 / Client 端 / Server 端（三张）
            │     • 双端视角：DuckDB 统计写入侧 / StatsHandler REST 查询侧（两张）
            │     • 双端实现：AOP 框架层 / Domain 事件消费者全链路业务层（两张）
            │   动作：允许每张独立建卡，不合并
            │   必做关联声明（每张都写，缺一张 = FAIL）：
            │     • 每张卡 source_files[] 末尾追加其余几张兄弟卡的绝对路径
            │     • 每张卡 §3 架构约定开头加一句：
            │       「本卡与 [兄弟卡A名](path) + [兄弟卡B名](path) 构成 XX 体系的
            │         YY / ZZ 互补视角；按 AGENTS §2.1.3 Level 3 保留平行卡」
            │
            └── Level 4（总卡-细卡 总分结构）保留主总卡 + 子细卡，并互相引用
                判定：scope[旧卡] ⊃ scope[T]（真超集），旧卡 §3 已有总分说明
                典型模式：Memory 搜索三合一（总卡）+ recommend_seed_nodes 三因子推荐
                        + knowledge_graph traverse BFS/DFS（两张细卡）
                动作：旧卡保留作为「总卡」，T 新建作为「细粒度子卡」
                必做关联声明：
                  • 总卡 §2 关键文件表末尾加一行：`细粒度拆解卡：[子卡名](docs/wiki/knowledge/zh/子卡名.md)`
                  • 子卡 §3 开头加：`本卡为 [总卡名](docs/wiki/knowledge/zh/总卡名.md) 描述的 XX 体系中
                                       YY 模块的细粒度独立召回卡`
                  • 子卡 source_files[] 追加总卡相对仓库根路径
```

**红线**：跳过 Step 0 前置查重 → 直接新建 RAG 卡 = FAIL。Level 1 完全重复场景下仍然新建卡（即使写了 Overlaps 说明）= FAIL。Level 2/3/4 拆分后**没写关联声明**= FAIL，视为两张孤立重复卡，需回退修正。

---

##### 2.1.3.3 四种节点关系类型声明速查表（强制对齐用词）

| 关系类型 | 判定 | 主卡/旧卡侧声明位置 + 固定句式 | 子卡/副卡/兄弟卡侧声明位置 + 固定句式 |
|---|---|---|---|
| **①完全重复**（Level 1） | scope 全等 + 主题同义 + §4 重叠率>90% | 主卡吸收合并，§2/§4 追加内容；无需额外声明。副卡**直接删除**（不归档，历史靠 git 追溯）。Design/Plan/Wiki 中所有副卡路径 → 替换为主卡路径。 | （副卡不再召回，已删除） |
| **②主卡-子卡**（Level 2） | scope[子] ⊂ scope[主]，真包含层级 | 主卡 §2 表格末行：`细粒度拆解：[子卡名](docs/wiki/knowledge/zh/子卡名.md)` | 子卡 §3 首句：`本卡是 [主卡名](docs/wiki/knowledge/zh/主卡名.md) 的 XX 模块细粒度展开`；子卡 `source_files[]` 末尾追加主卡相对仓库根路径 |
| **③总卡-视角细卡**（Level 3） | scope 交集 30-80%，同一体系不同层面/分层 | 每张卡 §3 首句统一句式：`本卡与 [兄弟卡A名](docs/wiki/knowledge/zh/A卡名.md) + [兄弟卡B名](docs/wiki/knowledge/zh/B卡名.md) + … 构成 XX 体系的 YY/ZZ/… 互补视角；按 AGENTS §2.1.3 Level 3 保留平行卡`；每张卡 `source_files[]` 末尾追加**所有**兄弟卡相对仓库根路径（互相完整闭环）| （与左列相同，Level 3 是对称兄弟关系，每张写法一致）|
| **④总卡-细卡**（Level 4） | scope[总] ⊃ scope[细]，总卡有总分结构 | 总卡 §2 表格末行：`细粒度拆解卡：[细卡1名](path) + [细卡2名](path) + …` | 细卡 §3 首句：`本卡为 [总卡名](docs/wiki/knowledge/zh/总卡名.md) 描述的 XX 体系中 YY 模块的细粒度独立召回卡`；细卡 `source_files[]` 追加总卡相对仓库根路径 |

---

##### 2.1.3.4 工具链约定（引导 Agent 正确使用技能）

增量构建 Wiki / RAG 卡时，**必须按以下顺序触发技能**，不可跳过前置：

1. **第一步：触发 `ai-orz-wiki-maintainer` 技能** — 该技能 Step 0 前置查重会自动按 §2.1.3.2 的 5 级算法做合并/拆分判定，把结果写进 SOP Step 5 产物。
2. **同步互引**：判定拆分后，该技能会强制写齐 §2.1.3.3 四种关系的双方声明（声明载体：RAG 卡 source_files[] 互引 + §3 首句，以及 Wiki 长文 §1 视角声明——均为 ③④ 活文档区内部，符合单向引用模型）。
3. **只有在新增 Design / Plan 文档时**，才触发 `ai-orz-doc-maintainer` 技能（写完即冻结，不写任何指向 ③④ 的链接，无回填流程）。

> **⚠️ ⛔ 明确禁止的反模式**：
> - ❌ 直接 Write 工具新建 md 文件，绕开 ai-orz-wiki-maintainer 技能 = 跳过 5 级决策 = FAIL。
> - ❌ 在 SOP 中写 "Overlaps OK" / "允许重叠" / "重叠不用管" 等措辞 = 绕过 Level 1/2 判定 = FAIL。本次 v2.1 升级后，**整个代码库所有文档中一律删除此类措辞**（保留的唯一合法重叠是 §2.1.3 Level 3 明确声明关联关系的互补视角平行卡）。
> - ❌ 新增 RAG 卡后，Wiki 长文中仍然只引用「自己这张新卡路径」，不补引兄弟关联卡 / 主卡 / 总卡路径 = 关联关系没入链路 = FAIL。
> - ❌ 在 design/plan（含归档件）中新增指向 wiki/RAG 卡的链接 = 违反单向引用模型 = FAIL（存量已有不要求回删，但任何新写的 ①② 文档不得再包含 ③④ 链接）。

---

##### 2.1.3.5 _module.yaml 驱动模块子卡的豁免条款（仅适用于 AI Orz 多模块工作区 / 多 Agent 执行框架 两组模块卡组）

**背景**：`docs/wiki/knowledge/zh/AI Orz 多模块工作区（后端服务 + Dioxus 前端 + 共享 crate）/` 与 `AI Orz 多 Agent 执行框架（Rust 后端 + Dioxus 前端）/` 两个目录下存在 `_module.yaml` 驱动的模块化知识树，通常包含：概述 / 架构设计 / 技术栈 / 编码规范 / 特殊配置与命令 / 子模块进一步的下钻 6 类平行子卡。它们天生 scope[] 多为顶层通配，scope 交集判定会 100% 命中，会与 §2.1.3.2 的 5 级决策算法冲突。

**豁免规则（仅对这两组目录生效）**：
- **5 级决策算法改为基于 `_module.yaml` 的 `role:` / `section:` 字段判定**：
  - 若 `_module.yaml` 中该子卡的 `role` / `section` 标签与现存任意子卡完全相同 → 判定为 Level 1 完全重复（同一层同一 role 不应两张卡），走合并+归档；
  - 若 `_module.yaml` 中该子卡的 `role` / `section` 标签为 6 类标准角色中现存没有的新角色 → 判定为 Level 3 互补视角平行卡，允许独立新建，但必须在每张子卡的 YAML `source_files[]` 末尾追加「同组其余 5 张子卡绝对路径」形成完整闭环互引。
- 现有 11 张模块子卡已人工核验通过角色去重，无需重判。

---

##### 2.1.3.6 Commit 消息「重复检查自我声明」签名规范（强制执行，wiki-maintainer 每次提交必带）

> 🎯 目的：**透明化 Step 0 判定结果，形成责任链闭环**。如果某次错判把「语义别名」标成了 Level 5，你通过 git log 能立刻定位到那次提交的自我声明，回溯判定过程，避免重复卡「悄悄进入仓库很久后才发现但找不到源头」。

**模板（必须原样粘贴到 wiki-maintainer 每次提交的 commit message 末尾，占独立一段；数字必须与本次 Step 0 实际执行结果一致，禁止写 0 蒙混）**：

```
—— 重复检查自我声明（AGENTS §2.1.3.6 v2.1）——
本次候选主题总数：<N> 个
Step 0 5 级判定结果 →
  Level 1（完全重复 → 合并删除副卡）：<X> 张
  Level 2（主卡-子卡 → 合并到主卡）：<M> 张
  Level 2（主卡-子卡 → 拆分新建子卡 + 声明）：<K> 张
  Level 3（视角兄弟卡 → 独立新建 + 互声明）：<P> 张
  Level 4（总卡-细卡 → 新建细卡 + 声明）：<Q> 张
  Level 5（纯新主题 → 直接新建）：<R> 张
  合计处理 RAG 卡：<X+M+K+P+Q+R> 次（含合并，不与新建重复计数）
——
  🔍 语义别名疑似清单（name/scope 不命中但语义读下来近似同主题）：
    • <疑似1：候选主题 T_name → 疑似对应现存卡 <现存卡名>；建议：人工确认后决定合并 or 新建>
    • <疑似2：... >
    • 0 条（如无则写这一句；6 大高重复领域被判 Level5 的必须至少写 1-2 条留痕，即使最后结论仍为 Level5）
——
```

**红线**：自我声明段 6 个分级数字（X/M/K/P/Q/R）加总必须等于候选主题总数 N（Σ = N）。语义别名疑似清单不得为了省事永远写 0——如果候选主题的领域是知识库已有密集覆盖的主题（如「日志 / 配置 / 统计 / 构建 / 前端样式」等本轮被识别出高重复率的 6 大领域），必须至少执行一次"通读现存卡 title 做语义比对"，写 1-2 条疑似项（即使最后结论是"判 Level 5 没问题"也要把这个判断过程留下来）。

---

#### 2.1.4 各象限章节模板

文件头统一按 2.1.1；章节内容用下列清单，字段级细节填相对路径链接（`路径#Lx-Ly`），**不贴实现快照代码块**。

##### 模板 A：`docs/design/*.md`（决策快照，为什么）

| 章节 | 内容要点 |
|------|---------|
| 一、设计目标 | 设计哲学 + **关键决策表**（问题/方案/原因 3 列）|
| 二、架构思路 | ASCII 分层/数据流图 + 关键结构定义源码链接 |
| 三、涉及文件清单 | 分层索引表（文件/角色/内容摘要）+ **零改动面**说明 |
| 四、关键边界 / 行为红线 | 编号列表，每条一句话（回归必保语义）|
| 五、扩展模式 | 按场景列步骤 + 每步参考文件链接 |

**红线**：写定后不追代码；发现契约与代码不一致 → 文头加一句 `注：本文件为决策快照，接口细节以代码为准`。
**参考实例**：[docs/design/thinking_task_policy_engine_design.md](./docs/design/thinking_task_policy_engine_design.md)

---

##### 模板 B：`docs/plan/*.md`（落地快照，怎么做+结果）

| 章节 | 内容要点 |
|------|---------|
| 一、目标 | 问题/解决方式表 + **收敛后效果**一句话（架构收益）|
| 二、架构思路 | ASCII 知识下沉路径 + **关键边界/行为红线** |
| 三、涉及文件清单 | 分层表（每行路径链接 + 变更摘要）+ **零改动面** |
| 四、分发点速查表 | 改动点 × N，每点：分支表 + 代码入口链接 |
| 五、验收清单 | checkbox（完成时更新状态）|
| 六、执行结果摘要 | 各模块验证结果表 + 与计划偏离项 |
| 七、后续扩展路径 | **4 步模板**：common 模型 / domain 分发 / handler 目录 / 前端 |

**红线（功能完成时强制执行）**：删除 writing-plans 产生的 Task/Step checkbox、失败测试代码块、所有实现代码块、`cargo test/build` 命令；只保留以上 7 节概述（目标 ≈ 150 行级）。
**参考实例**：[docs/plan/身份凭证Domain统一CRUD重构.md](./docs/plan/身份凭证Domain统一CRUD重构.md)

---

##### 模板 C：`docs/archive/*.md`（归档）

只在文件头加一句：
```
> 📦 归档标记（YYYY-MM-DD）：被 [新文档/SHA] 取代。保留原因：[xxx]。生效方案：[../design/new.md | commit abc123]
```
正文永不修改。

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
│   └── build.rs               # 构建脚本（自动 npm install + Tailwind CSS 编译）
│
└── docs/                       # 详细设计文档
```

#### 3.2.1 基础设施公共工具位置约定（强制执行）

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

### 3.3 命名规范

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

### 3.4 数据对象四层清晰定义

| 对象类型 | 定义位置 | 用途 |
|----------|----------|------|
| **API DTO** | `common/src/api/**` | HTTP 请求/响应，前后端复用；通用响应包装使用 `common::api::ApiResponse<T>` |
| **跨层共享模型** | `common/src/models/**` | DAO/DAL/Domain/API 共用的结果结构体（StatsInterval、TimeSeriesPoint、TokenSumResult 等） |
| **Command/Query** | `src/service/domain/*/mod.rs` | Domain 层输入，表达业务意图 |
| **业务实体** | `src/models/*.rs` | 核心业务对象，包含行为和状态 |
| **PO (持久化对象)** | `src/models/*.rs` | 数据库映射，1:1 对应表结构 |

### 3.5 PO 与业务实体分层边界规范（强制执行）

**核心原则：PO 仅在 DAO/DAL 层内部使用，绝对不对外暴露到 Domain 层及以上**

| 层级 | 可使用对象 | 数据传递方式 | 说明 |
|------|------------|------------|------|
| **DAO 层** | 仅 PO | PO ↔ 数据库 | 单一数据源 CRUD，SQL 拼接，无业务逻辑；含外部 API 出站调用 |
| **DAL 层** | 内部：PO，对外：业务实体 | PO ↔ 业务实体 双向转换 | 组合 DAO，完成业务级数据操作 |
| **Domain 层** | 仅业务实体 | 业务实体 ↔ Command | 核心业务逻辑编排，产生内部事件，无 PO 依赖 |
| **Adapter 层** | 业务实体 + DTO/外部结构 | DTO/外部结构 ↔ Command | HTTP Handler + AOP Producer，外部协议转换与校验 |

**业务实体内部设计**：业务实体内部持有 PO 字段（`pub struct Project { pub po: ProjectPo }`），DAL 层直接通过 `&xxx.po` 传递给 DAO，避免字段逐一映射。

**DAL 层接口签名**：统一使用业务实体，不使用 PO——写操作接收 `&Project` 引用，读操作返回 `Option<Project>` / `Vec<Project>`。

**RequestContext 跨层传递**：统一使用 `ctx.clone()`（内部 Arc 引用，clone 成本极低），避免所有权移动导致编译错误。

**软删除约定**：`status = 0` 视为软删除，常规查询默认过滤（如 `TaskStatus::Cancelled = 0`）；需要查询历史/恢复时用 `query` 方法绕过过滤。

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

用户相关信息从 `ctx.uid()` / `ctx.uname()` 获取，不再单独传参；内部私有方法可省略，只读操作也需要传递（便于日志记录）。

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

### 4.7 日志系统规范（强制执行）

**核心原则：项目内所有代码必须使用统一日志宏，禁止直接调用 tracing::*!**

| 级别 | 宏名 |
|------|------|
| INFO | `log_info!` |
| WARN | `log_warn!` |
| ERROR | `log_error!` |
| DEBUG | `log_debug!` |

**两种调用模式（宏自动检测）：**

```rust
// ✅ 模式 1：无上下文（系统级别，第一个参数是字符串）
log_info!("application started");
log_info!("config loaded from {}", path);

// ✅ 模式 2：带上下文（请求级别，&ctx + operation 字符串）
log_info!(&ctx, "create_memory", "created memory id={}", memory_id);
log_error!(&ctx, "update_project", "db error: {:?}", err);
```

**禁止的写法**：直接调用 `tracing::info!`；传 ctx 值而非 `&ctx`；Operation 传变量（必须是字符串字面量）。

> 💡 完整规范（检测机制、tracing 语法速查）见 [docs/design/logging_design.md](./docs/design/logging_design.md)

### 4.8 向量化实体规范（强制执行）

**核心原则：所有支持向量索引的 PO 必须实现 `Vectorizable` trait，禁止在 DAL 层手工拼接向量文本**

```rust
// src/models/vector.rs
pub trait Vectorizable: Send + Sync {
    /// 生成待向量化的文本内容（由 PO 自己决定哪些字段参与向量化）
    fn vectorize_text(&self) -> String;
    /// 向量集合名称（对应 vss_{collection} 表）
    fn vector_collection() -> &'static str where Self: Sized;
    // 以下为默认实现，通常无需覆盖
    fn vector_content_hash(&self) -> String { ... }
    fn vector_expire_at(&self) -> Option<i64> { ... }
    fn needs_reindex(&self, existing_hash: &str) -> bool { ... }
}
```

**调用规范**：

| 场景 | 正确写法 | 错误写法 |
|------|---------|---------|
| 索引场景（create/update/rebuild） | `embed_entity(ctx, cortex, po)` | `embed_text_for_search(ctx, cortex, &format!(...))` |
| 获取 collection 名 | `Po::vector_collection()` | 硬编码 `"namespace"` 字符串 |

**已实现 Vectorizable 的实体**：AgentPo（`agents`）、ToolPo/Tool（`tools`）、TaskPo（`tasks`）、SkillPo/Skill（`skills`）、ShortTermMemoryIndexPo（`memory:short_term`）、LongTermKnowledgeNodePo（`memory:knowledge_node`）。

**禁止的写法**：DAL 层手工 `format!` 拼接向量文本；在 PO 上添加独立 `vector_text()` 方法（应实现 trait）；硬编码 collection 名。

> 💡 **设计动机**：将"哪些字段参与向量化"的知识封装在 PO 内部（信息专家原则），DAL 层无需感知 PO 的字段结构。未来调整向量化字段组合，只需改 PO 的 `vectorize_text()` 一处。

### 4.9 查询分页与通用 count 规范（强制执行）

**核心原则：query 是核心查询能力，list 是语法糖；count 与 query 复用同一套过滤条件。** 完整实现模式见 [docs/design/pagination_and_count_convention.md](./docs/design/pagination_and_count_convention.md)。

- **query**（POST body，完整查询条件 + pagination）与 **list**（GET，只接受分页，内部固定默认过滤和排序）统一返回 `PagedResult<T> { items, total }`
- pagination 随 Query 结构体全链路透传，每层用 `PagedResult::map()` 转换内部类型
- DAO 层必须抽取 `push_query_filters`，COUNT 与 LIST 复用同一套 WHERE 条件
- 三层统一 `count(ctx, query) -> Result<u64>` 透传；特定 `count_by_xxx` 一律构造 Query 后调用通用 count

**禁止的写法**：
- ❌ list 接口接受查询字段（ids/status/keyword 等必须走 query）
- ❌ DAO query 方法返回 `Vec` 而非 `PagedResult`
- ❌ Handler 层把 `PagedResult` 当 `Vec` 用（应取 `.items`）
- ❌ count 独立拼 WHERE 不复用 `push_query_filters`；`count_by_xxx` 独立实现 SQL；query 后取 `len()` 当 count

### 4.10 两阶段初始化 + 基础数据注入规范（强制执行）

**核心原则：启动拆成两阶段 ——「基础设施就绪」与「基础数据注入」严格分离，绝不混在消费者注册代码里。**

**启动总顺序（`lib.rs::run()` 强制执行）**：

```
pkg::init_all()                  # 最底层：日志/存储/JWT/工具注册（一次性全局 OnceLock）
  → service::init()              # 阶段 ①（同步、纯内存）：DAO → DAL → Domain 单例注册，绝不碰 DB
  → producer::init() / consumer::init()  # AOP 基础设施（订阅者注册，绝不注入 DB 默认值！）
  → service::init_base_data().await      # 阶段 ②（异步、DB IO、幂等）：
      └─► domain::init_all_base_data()   #   派发到每个 domain 的 init_base_data()
  → AOP stats hook + aop::init_all()     # 事件总线调度器启动（真正开始轮询/消费）
  → HTTP 服务启动
```

**各层扩展点**：

| 想补什么默认数据 → 放在哪里 | 正确做法 | 错误做法（禁止） |
|---------------------------|---------|----------------|
| 某 domain 的系统默认 DB 行（cron triggers、默认角色等） | 在该 domain 的 `mod.rs` 加 `pub async fn init_base_data()`（try/warn 包裹的幂等检查：先查后插），在 `domain::init_all_base_data()` 追加一行 `.await` | 写到 consumer::init、HTTP handler、外部 migration 脚本 |
| 生产者/消费者 AOP 订阅者注册 | producer::init() / consumer::init() 内部调用 registry 注册 | 把业务代码塞到 init 函数里直接发事件 |

**Consumer 边界红线**：`consumer::init()` 只做一件事——把 Consumer 注册到 AOP Registry。写 DB 默认值、触发内部事件、调用改变全局状态的业务方法，一律禁止。

**测试环境同步对齐**：`tests/common/env.rs` 的 `init_full_test_env` 必须严格遵循真实启动顺序（基础设施 → service::init → producer::init → consumer::init → service::init_base_data），不要在测试里手动造「应该启动就有」的默认数据。

### 4.11 前后端 API 协议规范（强制执行）

**核心原则：`common` crate 是前后端 API 协议的单一事实源。** 详见 [docs/design/api_protocol_convention.md](./docs/design/api_protocol_convention.md)。

1. **禁止裸原始类型响应**：handler 即便只返回一个字段也必须用标准 Response 结构体（`ApiResponse<T>` 信封的 data 内禁止裸 bool/()/String）；无业务字段的操作用 `<Action>Response { success: bool }`。
2. **DTO 只定义在 common**：Request/Response 一律先定义在 `common/src/api/<域>.rs`；禁止 `frontend/src/api/` 本地镜像；禁止 handler 直接返回 DAL/Domain 内部结构体（DAL 需要时 re-export common 定义）。
3. **请求参数必须结构体化**：新增接口的请求参数（path / query / body）一律用结构体定义在 `common/src/api/<域>.rs`，通过 `#[derive(Params)]` + `#[param(source = "path"|"query")]` 注解声明参数来源；禁止在 handler 签名中散落 `Path<String>` / `Query<HashMap>` 等裸提取器。结构体即接口契约，便于扩展字段、前后端复用、文档生成。
4. **共享枚举禁止数字比较**：权限判断用 `UserRole` 枚举方法（has_permission/find_root），禁止 `role == 0`/`role >= 2` 类数字大小比较。
5. **前端复用后端结构体**：前端 API client 优先复用 `common::api::*` 中的 Request/Response 结构体作为参数和返回类型，减少前后端字段定义漂移；前端自定义结构体仅用于纯展示层（如聚合多个接口数据的 ViewModel）。
6. **前端兼容导入**：既有导入路径多的 api 模块用 `pub use common::api::{...}` re-export 保持路径；注意 frontend 是 bin crate，无人引用的 re-export 会触发 unused import，只 re-export 实际被引用的类型。

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

*本文档是 AI 助手的快速入门手册：规范与约定看本文，功能现状看 wiki，设计决策看 docs/design/，避坑经验看 docs/LAYERED_ARCHITECTURE_PRACTICE.md*
