# AI Orz - Agent 开发规范总览

> 🎯 **本文档供 AI 助手快速理解项目**：3 分钟了解项目是什么、代码怎么组织、去哪找规范。
>
> 功能现状以 [docs/wiki/](./docs/wiki/) 为准；编码规范全文见 [docs/CODE_STANDARDS.md](./docs/CODE_STANDARDS.md)；文档维护细则见 [docs/DOCUMENTATION.md](./docs/DOCUMENTATION.md)。

---

## 一、项目速览

**AI Orz** — 全栈 Rust 多 Agent 协作框架，以组织化形式管理和执行 AI 代理任务。

- **后端**：Rust + Axum + SQLite + sqlx 0.8 + 原生 CortexDao（OpenAI 兼容）
- **前端**：Dioxus 0.7 (WebAssembly) + Tailwind CSS v4 + DaisyUI v5
- **质量**：1124 测试 100% 通过 · clippy `-D warnings` 双端零容忍 · cargo-llvm-cov 38%/45% 门槛

### 核心能力域

| 域 | 一句话说明 |
|----|-----------|
| 👥 组织权限 | 多级组织 + 角色并查集继承 + JWT 双模式 + 偏好双源沉淀 |
| 🤖 Agent | 全生命周期 + 两阶段唤醒（IntentAnalyze → Awaken）+ 策略引擎 |
| 🧠 记忆 | 四层记忆 + 休息沉淀 + 知识图谱 + 种子推荐 |
| 💬 对话消息 | SSE 实时推送 + 多渠道入站（飞书 WS）+ 5 类出站骨架 |
| 📋 任务项目 | 任务状态机 + 进度追踪 + TaskGraph DAG + 项目聚合上下文 |
| 🛠️ 工具技能 | 三层调用架构 + 5 份预置技能 + MCP 集成 |
| 🔌 外部 Agent | A2A 协议 Client/Server + 异步回调 |
| 🔎 搜索 | FTS5 + 向量 + 图谱三位一体混合搜索 |
| 📊 统计监控 | DuckDB 五维统计 + 运行时观测 + 系统健康仪表盘 |
| 🚀 异步基建 | AOP 事件中心 + 8 类消费者 + cron 定时触发器 |
| 🎨 前端 | Dioxus 41 路由 + HUD Canvas + Markdown/Mermaid 全链路 |

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

---

## 四、文档索引

| 分类 | 文档 | 场景 |
|------|------|------|
| **编码规范 SSOT** | [CODE_STANDARDS.md](./docs/CODE_STANDARDS.md) | 所有编码规则（命名/分层/宏/错误/日志等） |
| **架构总纲** | [ARCHITECTURE.md](./docs/ARCHITECTURE.md) | 实体关系、设计哲学、核心概念 |
| **分层实践** | [LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md) | 适配层架构 + 反模式避坑 |
| **文档维护** | [DOCUMENTATION.md](./docs/DOCUMENTATION.md) | 四类文档编写/引用/图谱法则 |
| **API 协议** | [api_protocol_convention.md](./docs/design/api_protocol_convention.md) | common DTO 单一事实源 |
| **SQL 规范** | [sqlx_guide.md](./docs/design/sqlx_guide.md) | SQLx 0.8 + SQLite 工程规范 |
| **日志规范** | [logging_design.md](./docs/design/logging_design.md) | 统一宏 + tracing 语法 |
| **Runtime** | [runtime_design.md](./docs/design/runtime_design.md) | Agent 唤醒 + 工具二分 |
| **策略引擎** | [thinking_task_policy_engine_design.md](./docs/design/thinking_task_policy_engine_design.md) | Policy trait + policy_set! 宏 |
| **前端架构** | [frontend_architecture.md](./docs/design/frontend_architecture.md) | Dioxus + 41 路由 |
| **查询规范** | [pagination_and_count_convention.md](./docs/design/pagination_and_count_convention.md) | 分页 + 通用 count |
| **记忆设计** | [memory_design.md](./docs/memory_design.md) | 四层记忆系统 |
| **Wiki 百科** | [docs/wiki/](./docs/wiki/) | 8 大板块 353 篇（功能实现细节）|
| **RAG 知识卡** | [docs/wiki/knowledge/zh/](./docs/wiki/knowledge/zh/) | 54+ 张原子卡（第一召回层）|

---

## 五、RAG 查询操作指令

**查询前 5 步**（强制执行）：

1. **scope 预过滤** → 用每张卡 YAML `scope[]` glob 匹配当前上下文关注的文件集
2. **读命中知识卡 ④** → §4 硬约束 → §2 关键文件表 → §3 架构约定 → §1 概述
3. **强制跳 Wiki 长文 ③** → 从 `source_files[]` 找 `docs/wiki/zh/content/...` 链接，读 §5 分析 + §8 故障排查
4. **跳源码锚点** → 按 `相对路径#Ln-Lm` 读真实代码
5. **按需补跳** → ① Design（为什么）/ ② Plan（怎么做）

**阅读链路**：`④ RAG 知识卡 → ③ Wiki 长文 → 源码 → ① Design → ② Plan`（禁止跳过 ③）

**同主题多卡**：全部召回、并行阅读、不做去重。完全重复（scope 互为子集 + §4 重叠率 > 90%）→ 走合并流程。

**元问题第一跳**：[RAG 知识索引卡](docs/wiki/knowledge/zh/RAG%20%E7%9F%A5%E8%AF%86%E7%B4%A2%E5%BC%95%EF%BC%9A%E5%A6%82%E4%BD%95%E4%BD%BF%E7%94%A8%E7%9F%A5%E8%AF%86%E5%8D%A1%E7%89%87%E5%81%9A%E5%8F%AC%E5%9B%9E%E6%A3%80%E7%B4%A2%E3%80%81%E9%94%9A%E5%AE%9A%E4%B8%8E%20scope%20%E5%8C%B9%E9%85%8D/RAG%20%E7%9F%A5%E8%AF%86%E7%B4%A2%E5%BC%95%EF%BC%9A%E5%A6%82%E4%BD%95%E4%BD%BF%E7%94%A8%E7%9F%A5%E8%AF%86%E5%8D%A1%E7%89%87%E5%81%9A%E5%8F%AC%E5%9B%9E%E6%A3%80%E7%B4%A2%E3%80%81%E9%94%9A%E5%AE%9A%E4%B8%8E%20scope%20%E5%8C%B9%E9%85%8D.md)

---

## 六、文档规范速记

**全量细则见 [docs/DOCUMENTATION.md](./docs/DOCUMENTATION.md)**：

- 路径引用一律 `相对仓库根路径 + #Lx-Ly`；禁 `file://` 伪协议、绝对路径、冒号行号
- 新增 Wiki/RAG 卡 → 先触发 `ai-orz-wiki-maintainer` 过 5 级查重
- 新增 design/plan → 触发 `ai-orz-doc-maintainer`
- 引用单向：③④ 活文档 → ①② 快照 + 源码
- 归档件只加归档头，正文永不修改

**文件落位**：

| 象限 | 目录 | 命名 |
|------|------|------|
| 功能设计 | `docs/design/` | 英文 snake_case（如 `runtime_design.md`）|
| 长期规范 | `docs/design/` | 英文 snake_case（`*_guide.md` / `*_convention.md`）|
| 落地快照 | `docs/plan/` | 中文主题名（如 `身份凭证Domain统一CRUD重构.md`）|
| 归档 | `docs/archive/{design,plan}-archive/` | 保留原文件名 |

**红线**：
- ❌ 在 `docs/` 下自创新目录
- ❌ plan 文件名带日期前缀
- ❌ 归档件散放在 `docs/archive/` 根目录
- ❌ design 用中文命名 / plan 用英文命名

---

*规范与约定看 [CODE_STANDARDS.md](./docs/CODE_STANDARDS.md)，功能现状看 [wiki](./docs/wiki/)，设计决策看 [docs/design/](./docs/design/)，文档维护细则看 [DOCUMENTATION.md](./docs/DOCUMENTATION.md)*
