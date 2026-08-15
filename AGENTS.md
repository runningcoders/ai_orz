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
| 📚 **知识体系 + RAG 自索引** | **4 类文档闭环：**① `docs/design/` 为什么 / ② `docs/plan/` 怎么做+结果 / ③ `docs/wiki/zh/content/` 百科（8 大板块 353 篇）/ ④ `docs/wiki/knowledge/zh/` 54+ 张 RAG 原子知识卡（总结+索引，RAG 第一召回层）；**四类显式双向互引**（代码引用 `file://相对路径`，文档引用 `file:///绝对路径`）。**阅读链路（严格顺序，禁跳过 Wiki）**：④卡 → ③长文 → 源码 → ①Design → ②Plan。知识卡 YAML `scope[]` 按 glob 过滤关注文件集，`source_files[]` 必须四类齐全。同主题多张平行卡：全部召回、并行阅读、不去重、不删旧卡。维护 Skill：ai-orz-wiki-maintainer（③+④）+ ai-orz-doc-maintainer（①+②） |

---

## 二、文档快速索引

> 📌 **按需要读取详细设计文档**

### docs 内容脉络（四类文档闭环，v2.0）

**标准阅读链路（强制执行）**：`④ RAG 知识卡 → ③ Wiki 百科长文 → 源码 → ① Design → ② Plan`。**禁止跳过 ③ Wiki 长文**直接从 ④ 跳源码。

| 目录 | 用途 | 约束 | 维护 Skill |
|------|------|------|-----------|
| ④ `docs/wiki/knowledge/zh/` | RAG 第一召回层：54+ 张原子卡（总结+索引）| YAML 5 字段 + 4 节固定；`source_files[]` **必须 4 类齐全**（含 ③ Wiki 长文 `file:///绝对路径` ≥1 条）；同主题允许多张平行卡 | ai-orz-wiki-maintainer |
| ③ `docs/wiki/zh/content/`（入口 `docs/wiki/`）| 百科：8 大板块 353 篇（是什么）| 10 节固定目录；`<cite>` 强制关联 ①/②/④ 绝对路径 | ai-orz-wiki-maintainer |
| ① `docs/design/` | 决策快照（为什么）| 写定后不追代码；「关联文档」段强制列 ③ Wiki + ④ RAG 占位/真实路径 | ai-orz-doc-maintainer |
| ② `docs/plan/` | 落地快照（怎么做+结果）| 7 章骨架（无 checkbox/命令/代码快照）；「关联文档」强制 ③+④ 路径（**plan 0 RAG = FAIL**）| ai-orz-doc-maintainer |
| `docs/archive/` | 历史归档 | 只进不出，文头加一句归档说明 | ai-orz-doc-maintainer |
| `docs/superpowers/*/` | 开发期执行蓝图（临时） | 功能完成 7 天内处置：→ plan 7 章模板 / → archive 封存 | ai-orz-doc-maintainer |
| `docs/ARCHITECTURE.md` | 核心概念与实体关系 | 唯一权威纲要，手工维护 | — |
| `docs/LAYERED_ARCHITECTURE_PRACTICE.md` | 分层实践与避坑 | Agent 必遵循，手工维护 | — |

**路径格式铁律（互引时统一）**：
- 跳代码 → `file://相对项目根路径`（可选 `:Ln-Lm`）
- 跳文档（①/②/③/④之间）→ **`file:///绝对完整路径`**（IDE 可点）
- 覆盖率底线：每类文档必须显式链接另外至少 1 条对应主题文档，禁止孤立

**维护流程顺序（防死锁：最后执行者回填占位）**：
1. 代码变化 → 同时同步 ③ + ④（缺一会导致 RAG 召回过期知识）
2. 新功能设计 → ① design 先写 ③+④ 精确占位路径 → ② plan 补齐占位 → ③+④ wiki 同步时回填真实路径 → 若 doc-maintainer 最后执行则 doc 回填

---

### RAG 查询操作指令

**查询前准备**：本仓库已接入 Trae/IDE RAG，对 54+ 张 `docs/wiki/knowledge/zh/` 原子卡做向量语义 chunk 召回。按以下 5 步执行：

1. **scope 预过滤**：若当前上下文含「用户指定关注文件集」或已知 IDE 打开文件列表 → 先用每张卡 YAML `scope[]` glob 匹配，不匹配的卡直接丢弃（不参与向量打分）
2. **读命中卡 ④**：优先读 §4 硬约束（最高权重）→ §2 关键文件表 → §3 架构约定 → §1 概述
3. **强制跳对应 Wiki 长文 ③**：从卡 `source_files[]` 中找 `file:///绝对路径` 形式的 ③ Wiki 链接，立即跳 §5 详细分析 + §8 故障排查（系统化上下文，短卡不够）
4. **跳源码锚点**：从长文 cite/章节来源段 OR 卡 `source_files[]`，按 `file://相对路径:Ln-Lm` 读真实代码
5. **按需补跳 ① Design / ② Plan**：① 找为什么/决策表；② 找扩展入口速查表 §4 + §七 4 步扩展模板

**同主题多张平行卡**：全部召回、并行阅读、不做去重、不删旧卡（语义相近 = 不同切面，信息互补）。

**RAG 元问题第一跳**（如何使用知识卡 / 召回不到 / scope 匹配 / source_files 写法）→ 命中：
- [RAG 知识索引：如何使用知识卡片做召回检索、锚定与 scope 匹配](file:///Users/aman/Technology/rust/ai_orz/docs/wiki/knowledge/zh/RAG%20%E7%9F%A5%E8%AF%86%E7%B4%A2%E5%BC%95%EF%BC%9A%E5%A6%82%E4%BD%95%E4%BD%BF%E7%94%A8%E7%9F%A5%E8%AF%86%E5%8D%A1%E7%89%87%E5%81%9A%E5%8F%AC%E5%9B%9E%E6%A3%80%E7%B4%A2%E3%80%81%E9%94%9A%E5%AE%9A%E4%B8%8E%20scope%20%E5%8C%B9%E9%85%8D/RAG%20%E7%9F%A5%E8%AF%86%E7%B4%A2%E5%BC%95%EF%BC%9A%E5%A6%82%E4%BD%95%E4%BD%BF%E7%94%A8%E7%9F%A5%E8%AF%86%E5%8D%A1%E7%89%87%E5%81%9A%E5%8F%AC%E5%9B%9E%E6%A3%80%E7%B4%A2%E3%80%81%E9%94%9A%E5%AE%9A%E4%B8%8E%20scope%20%E5%8C%B9%E9%85%8D.md)

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

#### 2.1.2 代码引用铁律

1. **契约型代码块可保留**（仅限：trait 签名/struct 字段/enum 变体/SQL schema/ASCII 图，不含 `{}` 实现体）→ 代码块紧邻下方附源码路径：
   `> 当前实现：[file.rs#L12-L50](file:///absolute/path/to/file.rs#L12-L50)`
2. **实现快照型代码块一律禁止**（函数体/测试/控制流/命令/脚本）→ 删除，改为路径引导：
   `> 逻辑见：[file::func](file:///.../file.rs#L288-L352)`
3. **首选引用格式（优先级 > 贴代码块）**：
   `[简短描述](file:///绝对路径/到文件.rs#L起始行-L结束行)`
4. **判断口诀**：粘到编辑器能直接编译/运行 → 属实现快照型，删掉；仅声明接口形状 → 属契约型，可留。

---

#### 2.1.4 各象限章节模板

文件头统一按 2.1.1；章节内容用下列清单，字段级细节填 `file:///` 链接，**不贴实现快照代码块**。

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
