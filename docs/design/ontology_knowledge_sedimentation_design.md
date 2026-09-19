# 本体论驱动知识沉淀设计（Ontology-Driven Knowledge Sedimentation）

> 🎯 **本文档定位**：将本体论（Ontology）落地为知识图谱沉淀方法论的整体设计快照——本体骨架（TBox）与知识图谱（ABox）合一、三段沉淀闭环、惰性解析、漂移记账 + 人工归纳、双层统计；设计评审定稿，接口细节以实际代码为准。
> 状态：v1.0（2026-09-18 设计评审定稿，未开始实现）
> 触发场景：需要理解本体与知识图谱的关系取舍、本体表结构与词表维护机制、漂移判定为何不物化、DAL 组合方向、published 认证门禁语义、DuckDB 漂移事件设计时打开；字段级实现直接读代码。
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目整体分层架构与开发规范
> - [CODE_STANDARDS.md](../CODE_STANDARDS.md) — 编码规范 SSOT（§6 STRICT/FTS5、§11 两阶段初始化、§14.2 双端复用下沉）
> - [ARCHITECTURE.md](../ARCHITECTURE.md) — 实体关系与设计哲学（Agent → Brain → Memory 四层）
> - [LAYERED_ARCHITECTURE_PRACTICE.md](../LAYERED_ARCHITECTURE_PRACTICE.md) — 分层红线与反模式避坑（DAL 组合 DAO 先例）
> - [memory_design.md](../memory_design.md) — 四层记忆系统设计（本设计的领域上游）
> - 暂无对应 plan 文档（实施启动时补写 docs/plan/ 本体论知识沉淀落地篇）

---

## 一、问题背景与设计目标

### 1.1 现状与病根：知识沉淀的"漂移"

知识图谱（长期记忆）的沉淀由 Agent 在 settle/save 链路中临场完成，节点类型与关系词缺少稳定、共享、可校验的约定。代码查证显示漂移已经发生：

| 证据 | 位置 | 说明 |
|------|------|------|
| `node_type` 是开放 `String` | [memory.rs#L242](src/models/memory.rs#L242) | 注释词表（concept/event/preference/skill）与枚举 `KnowledgeNodeType` 并不一致 |
| 关系词枚举带 custom 兜底 | [memory.rs#L130](common/src/enums/memory.rs#L130)、[memory.rs#L183](common/src/enums/memory.rs#L183) | 15 个规范关系词 + custom；未知词原样透出（测试锚定），兜底即漂移入口 |
| `weight` 字段已存在 | [memory.rs#L321-L324](src/models/memory.rs#L321-L324) | `KnowledgeNodeRelationPo.weight: Option<f32>`，关系加权已有落点 |
| `is_published` 非可见性控制位 | [memory.rs#L249-L253](src/models/memory.rs#L249-L253) | 语义是重要性标记，蜂巢内全可见不由此字段决定 |

**病根**：本体（实体类 + 关系词的受控词表）以"散文 + 枚举"形态散落——枚举是编译期快照（改词表要发版），skill 模板是自然语言描述（不可机读校验，且 skill 副本自我进化后必然分裂）。Agent 沉淀时只能临场猜测词表，猜测不一致即漂移。

### 1.2 核心立场：本体与知识图谱合一（TBox/ABox）

本体与知识图谱**不分家**，采用 TBox/ABox 分层：

- **TBox（模式层/共相）**：本体——实体类（class）、关系类型（relation）、同义映射（synonym）。回答"世界上有哪些词是规范的"。
- **ABox（实例层/殊相）**：知识图谱——具体节点与边。回答"这个 Agent 认识哪些具体事物"。

Palantir 式本体的启示：本体 = 把数据库的值投射为有业务含义的对象的语义模型。映射到本项目：**图谱表的行就是 ABox，本体表是 TBox**；本体层的价值不是自动防火墙，而是为漂移治理提供**控制落点**。

### 1.3 设计目标

1. **词表可信**：关系词/节点类型的规范词表进入数据库（TBox），运行时以表为准
2. **沉淀受引导**：Agent 沉淀前能感知词表、沉淀时被提示词引导、沉淀后可被校验认证
3. **漂移可观测**：漂移不静默——记账、看板、指标三件套，支撑人工归纳决策
4. **进化自动愈合**：本体修订后，历史"漂移"自动重新解释，无需数据迁移
5. **零侵入既有链路**：`MemoryDal` 零改动，图谱表结构零改动（漂移主体就是存原文的字段）

### 1.4 关键决策表

| # | 问题 | 决策 | 理由 |
|---|------|------|------|
| 1 | 本体骨架放哪 | **独立全局本体表（DB 即 SSOT）**，不写 skill.md | skill 副本自我进化会分裂出多套词表；散文不可机读校验；违反单一事实源 |
| 2 | Agent 新词提案机制 | **不做**。漂移统计 + 人工归纳替代 | 本体是包容的抽象，不应由 Agent 临场提案制造噪音；应让反常积累（漂移数据）支撑人工抽象决策 |
| 3 | 漂移标记是否物化落库 | **不物化**。惰性解析纯函数，以当前本体为参数现场计算 | 本体进化后历史漂移自动"愈合"；避免映射标记与本体双写不一致 |
| 4 | 漂移统计是否跨表 JOIN | **不用 JOIN**。GROUP BY 词频留 MemoryDao（SQL 的家），词表全量查入内存，解析在 DAL 完成 | DAL 层无直接 SQL 权限；本体词表数据量有限，全量载入成本可忽略；对齐 [dal/memory.rs#L641](src/service/dal/memory.rs#L641) `traverse_knowledge_graph` 组合先例 |
| 5 | DAL 间引用方向 | **OntologyDal 组合 OntologyDao + MemoryDao**；`MemoryDal` 零改动零感知 | DAL 之间禁止互引；DAL → DAO 合法；把依赖放在"观察者"一侧，被观察者无感知 |
| 6 | 本体表是否带 organization_id | **不带**。3 张全局表，对齐 memory 先例 | 本地单部署单库，DB 即作用域边界；本体是"词典"不是"租户配置"；`term_key` 语义锚点保证未来多租户迁移便宜 |
| 7 | published 字段语义 | **升级为"通过本体校验的质量认证"**，软门禁 | 校验通过即发（与是否"事实"无关，事实性由沉淀流程保证）；蜂巢全可见不变；未过校验标 `needs_review` 而非拦截 |
| 8 | 解析纯函数位置 | **common/src/ontology.rs**（单文件模块） | 三个消费方跨 crate（domain 校验、DAL 看板、统计消费者），符合 CODE_STANDARDS §14.2 双端复用下沉 |
| 9 | 既有枚举 `KnowledgeRelationType` 去留 | **降级为 seed 初始词表来源 + 展示回落词表** | 运行时词表以本体表为准（DB 即作用域）；枚举保留类型安全的静态兜底，不再承担运行时判定 |
| 10 | DuckDB 漂移事件记什么 | **只记 raw_term 原文，不解析** | 写路径零耦合（"写只观察、读只解释"）；事件流是不可变审计日志，解析结论会随本体进化过期，原文不会 |
| 11 | 关系权重新增表？ | **不新增**。复用图谱边上的 `weight` 字段 | [memory.rs#L321-L324](src/models/memory.rs#L321-L324) 已有 `weight: Option<f32>`；本体侧提供 `weight_base` 基线，具体值由沉淀流程按基线校准 |
| 12 | 后端域归属（hr 还是 system） | **hr 域**，与 agent / skill 平级 | ABox（记忆/图谱）接口全在 [handlers/hr/agent/](src/handlers/hr/agent) 下，TBox 跟着 ABox 走；skill 先例证明全局资产归 hr（hr=组织资产域，system=运维设施域）；seed 是注入器不是归属判据（它同样注入归 hr 的 agents/skills）；前端 tab 在 hr 导航下，前后端同域零歧义 |
| 13 | 注入与监控的域边界 | **hr 提供"方法与数据"，跨域设施只做"管道"** | 注入方法（幂等 upsert）由 OntologyDomain 提供；seed 快照设施（system 域）与 `record_event!`（pkg 层）/ 事件消费者（[src/consumer/](src/consumer) 顶层）是跨域基础设施，hr 是调用方不是提供方。注入触发时机见决策 #15——不进启动链路（[init_all_base_data](src/service/domain/mod.rs#L42-L46) 派发的是基础设施自检，非 seed） |
| 14 | 手动同步策略（管理页同步按钮） | **仅补缺（OnlyMissing 单策略），不提供覆盖重置** | 形态对齐 [sync_preset_skills.rs](src/handlers/system/seed/sync_preset_skills.rs) preview/sync 先例，策略有意收窄：管理页是词表唯一修改入口（红线 6），Overwrite 会静默覆盖人工维护的描述 / required_fields；退役词在管理页可随时重新启用，无需"恢复误删"；词表是活的共享约定，不是模板副本 |
| 15 | 词表注入触发时机 | **跟 seed 走：初始化组织 + 手动同步，不做启动自动注入** | 启动链路（init_base_data）在项目里只承担基础设施自检：organization 联邦密钥（[organization/mod.rs#L29](src/service/domain/organization/mod.rs#L29)）、system cron triggers、finance 内置工具同步（[finance/mod.rs#L65](src/service/domain/finance/mod.rs#L65)，工具是缺了断链路且无管理页的系统目录）；预置内容资产（技能 / Agent）从不自动注入。词表是管理员治理的共享约定（管理页唯一修改入口），内容变更应显式采纳；老实例升级后词表过时的信号由漂移看板自然给出（新词漂移堆积 → 提示同步） |
| 16 | 写后认证与打点的触发机制 | **单管道：写路径只 publish 一个 AOP 事件，计算与打点全在消费者** | runtime 尾部唯一动作 `aop::publish(MemoryTermsWritten)`（对齐 [awakening.rs#L306](src/service/domain/runtime/awakening.rs#L306) 同模块先例），不打点、不解析、不调 domain/dal——**业务与统计彻底解耦**；OntologyCertifyConsumer（Async 模式不阻塞写路径）内完成两件事：调 OntologyDomain.certify（消费者→Domain 合法方向，[task_event_consumer.rs#L74](src/consumer/task_event_consumer.rs#L74) 先例）+ 记 OntologyDriftEvent 统计（消费者内打 stats 先例：[tool_exec_stats_consumer.rs#L67-L70](src/consumer/tool_exec_stats_consumer.rs#L67-L70)，其注释即"替代直打点 Decorator"的迁移实证；消费侧 ctx storage 全局兜底，stats 可用 [request_context.rs#L603-L609](src/pkg/request_context.rs#L603-L609)）。软门禁本就不拦截不撤回，异步最终一致语义无损；存量直调 stats 的打点统一迁移到 AOP 消费是全项目专题，不在本设计范围 |

---

## 二、核心架构

### 2.1 全景：三段沉淀闭环

```
┌────────────────────────────────────────────────────────────────────────┐
│  沉淀前（感知）                                                          │
│  ├── 神经技能提示词注入当前本体词表（实体类 / 关系词 / 同义映射样例）        │
│  │   └── OntologyDomain.list_lexicon(ctx) → 注入 prompt builder          │
│  └── 检索命中也按词表解释（漂移词回落到规范词展示）                          │
├────────────────────────────────────────────────────────────────────────┤
│  沉淀中（引导）                                                          │
│  └── 记忆沉淀技能（seed 预置技能模板）要求：关系词优先取词表；               │
│      词表外新词须原样书写（不强行套用），交由漂移记账发现                    │
├────────────────────────────────────────────────────────────────────────┤
│  沉淀后（校验 + 记账，写路径零耦合）                                       │
│  ├── runtime 沉淀链路尾部：原文落库（图谱零改动），唯一动作 publish：        │
│  │   └── aop::publish(MemoryTermsWritten) → AOP 事件中心 → 异步队列       │
│  └── OntologyCertifyConsumer（Async）→ 计算 + 打点全在消费侧：             │
│      ├── OntologyDomain.certify：resolve 命中 → is_published 置位         │
│      │     未命中 → needs_review（软门禁，不拦截写入）                     │
│      └── 记 OntologyDriftEvent 统计 → DuckDB（只记原文，不解析）           │
└────────────────────────────────────────────────────────────────────────┘
```

### 2.2 本体数据模型：3 张全局表

对齐 CODE_STANDARDS §6：STRICT 模式、`status` 软删除、INTEGER 秒级时间戳；对齐 memory 先例**无 organization_id**。

```sql
-- 实体类词表（TBox：节点类型）
CREATE TABLE IF NOT EXISTS ontology_classes (
    id               TEXT PRIMARY KEY,
    term_key         TEXT NOT NULL,
    display_name     TEXT NOT NULL,
    description      TEXT NOT NULL,
    required_fields  TEXT NOT NULL DEFAULT '[]',
    "status"         INTEGER NOT NULL DEFAULT 1,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    UNIQUE(term_key)
) STRICT;

-- 关系类型词表（TBox：边类型）
CREATE TABLE IF NOT EXISTS ontology_relation_types (
    id               TEXT PRIMARY KEY,
    term_key         TEXT NOT NULL,
    display_name     TEXT NOT NULL,
    description      TEXT NOT NULL,
    domain_classes   TEXT NOT NULL DEFAULT '[]',
    range_classes    TEXT NOT NULL DEFAULT '[]',
    weight_base      REAL NOT NULL DEFAULT 1.0,
    inverse_key      TEXT,
    "status"         INTEGER NOT NULL DEFAULT 1,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    UNIQUE(term_key)
) STRICT;

-- 同义映射（漂移修复手段：旧词/别名 → 规范词）
CREATE TABLE IF NOT EXISTS ontology_synonym_mappings (
    id               TEXT PRIMARY KEY,
    raw_term         TEXT NOT NULL,
    target_kind      TEXT NOT NULL,
    target_key       TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    UNIQUE(raw_term, target_kind)
) STRICT;
```

> 落地实现：迁移 SQL 放 [src/service/dao/ontology/sqlite.rs](src/service/dao/ontology/sqlite.rs)（待创建，对齐 [dao/memory/sqlite.rs](src/service/dao/memory/sqlite.rs) 布局）

**字段语义要点**：

| 字段 | 语义 |
|------|------|
| `term_key` | 语义锚点（snake_case 规范词），跨表/跨环境引用一律用它，不用代理 id——多租户演进、跨库同步时代价最小 |
| `required_fields` | JSON 数组；实体类必备字段清单，写后校验的判据之一 |
| `domain_classes` / `range_classes` | JSON 数组；关系的头/尾实体类约束（尾可空 = 不约束） |
| `weight_base` | 关系权重基线，映射到图谱边 `weight` 的校准参考 |
| `inverse_key` | 逆向关系词（如 depends_on ↔ enables），可空 |
| `status` | 1 正常 / 0 退役。**退役 ≠ 删除**：历史图谱中的存量引用仍需可解释 |
| `target_kind` | `class` / `relation`，一条映射只归一类 |

### 2.3 惰性解析：漂移判定是纯函数

**核心原则**：漂移不是写路径落库的状态位，而是"以当前本体词表为参数的纯函数计算结果"。解析器与查找结构下沉 common（第三个跨 crate 复用先例，参照 mention.rs 单文件形态）。

```rust
// common/src/ontology.rs（待创建）

/// 词表查找结构：启动/查询时由 OntologyDal 全量构建（词表量小，全量载入）
pub struct OntologyLexicon {
    pub class_keys:     std::collections::HashSet<String>,
    pub relation_keys:  std::collections::HashSet<String>,
    pub synonyms:       std::collections::HashMap<String, String>, // raw_term → term_key
}

pub enum ResolvedTerm {
    Canonical { term_key: String },          // 规范词命中
    ViaSynonym { term_key: String, raw_term: String }, // 同义映射命中
    Drift { raw_term: String },              // 词表外 = 漂移
}

/// 解析入口：resolve(&lexicon, kind, raw_term) -> ResolvedTerm
/// 纯函数：无 IO、无时钟、无副作用；同一输入 + 同一词表 ⇒ 同一结论
```

> 落地实现：[common/src/ontology.rs](common/src/ontology.rs)（待创建）

**自动愈合**：某词 V1 曾是漂移 → 管理员将其抽象为新关系词（或加同义映射）→ 下一次解析时历史漂移数据自动重新解释为规范/同义。无需回填任何数据，这就是不物化的全部理由。

### 2.4 写路径：沉淀打点与写后认证

**单管道（零耦合，只依赖 pkg/aop）**：runtime 沉淀链路（[domain/runtime/memory.rs](src/service/domain/runtime/memory.rs)）在原文落库后的唯一动作是 `aop::publish(MemoryTermsWritten)`——不打点、不解析、不调任何 domain/dal。不判断是否漂移——判断是读路径的事。

**写后认证与打点（published 软门禁，消费者异步执行）**：认证编排归 OntologyDomain，调用方是消费者（禁同层互调，决策 #16）；统计打点也在消费侧完成（对齐 [tool_exec_stats_consumer.rs](src/consumer/tool_exec_stats_consumer.rs) "业务发事件 + 消费者打 stats" 先例）：

```
settle / save_long_term 完成（runtime 写路径尾部）
    └── aop::publish(MemoryTermsWritten)    → AOP 事件中心 → 异步队列
          └── OntologyCertifyConsumer.on_event（Async 模式，不阻塞写路径）
                ├── OntologyDomain.certify_memory_terms(ctx, agent_id, 新词条)
                │     ├── resolve 命中 Canonical / ViaSynonym
                │     │     └── 校验通过 → is_published 置位（质量认证）
                │     └── resolve 得 Drift
                │           └── 标记 needs_review，不拦截、不撤回（软门禁）
                └── record OntologyDriftEvent → DuckDB（只记原文，解析结论不入库）
```

**published 语义修正**（对齐 [memory.rs#L249-L253](src/models/memory.rs#L249-L253) 现状）：
- 判据是"**通过本体校验**"，不是"是事实"——事实性由沉淀流程（两阶段唤醒 + settle）保证，认证只回答"结构是否符合共享约定"
- 蜂巢可见性不变：published 与否不影响图谱全可见
- 存量迁移：已有图谱节点**不做批量回填**，采用 grandfather（存量默认视为已认证）；新写入才走认证。避免一次性全量重解析的迁移风险

### 2.5 本体内容维护：seed 快照 + 管理页 CRUD

词表内容的生命周期（对齐 CODE_STANDARDS §11 两阶段初始化 + seed 快照机制）：

```
内置默认词表（枚举 15 关系词 + 4 节点类转换为 JSON，进 SeedSnapshot ontology 段）
    └── 初始化组织时注入（POST /organization/initialize，用户首次部署手动触发；
        [initialize_system.rs](src/handlers/organization/initialize_system.rs#L261) run_steps 追加词表注入步骤，
        对齐其调用 apply_preset_skills 的既有模式）：
        └── seed 设施 apply_preset_ontology → 逐条调 OntologyDomain 注入方法
            （幂等 upsert，term_key 已存在跳过）
            ↓
（手动同步，管理页同步按钮）SeedSnapshot（[defs.rs#L19](src/service/domain/system/seed/defs.rs#L19)）扩展 ontology 段
    └── system seed 管道（src/handlers/system/seed/sync_preset_ontology.rs 🆕，
        对齐 [sync_preset_skills.rs](src/handlers/system/seed/sync_preset_skills.rs) 形态）
        读内嵌快照（[default.rs](src/service/domain/system/seed/default.rs) embedded_default_snapshot）
        → 调用 hr 域同一注入方法（详细数据流见下方"手动同步入口"）
            ↓
组织管理页 CRUD（handlers/hr/ontology/，与 agent/ skill/ 平级）
    └── 新增 / 编辑 / 退役（status=0 软删除）/ 同义映射管理
            ↓
运行时词表 = 本体表为准
    └── 枚举 KnowledgeRelationType 降级为：seed 初始数据来源 + 展示回落词表
```

**约束**：seed 注入只做**新增**（term_key 不存在才插入），不做修改/删除——管理页是词表的唯一修改入口，避免两处写路径打架。

**手动同步入口（管理页同步按钮，对齐技能/Agent 既有先例）**：seed 内置内容只在初始化导入一次，版本升级后新增/修订的词表不会流入已运行实例（存在理由见 [sync_preset_skills.rs#L3-L6](src/handlers/system/seed/sync_preset_skills.rs#L3-L6) 文件头）。本体管理 tab 提供同步按钮，数据流形态完全对齐技能先例——页面归 hr、路由挂 /system（Admin 鉴权）、数据与方法分离：

```
前端（本体管理 tab 同步按钮 + Modal）
    ├── 打开弹窗 → GET /api/v1/system/seed/preset-ontology/preview
    │       └── src/handlers/system/seed/sync_preset_ontology.rs（🆕）
    │             ├── embedded_default_snapshot().ontology ← seed 数据（编译期内嵌）
    │             └── OntologyDomain 逐 term_key 对比（只读）→ 将新增 N 项清单
    └── 确认 → POST /api/v1/system/seed/preset-ontology/sync
            └── 逐条调用 hr 域注入方法（幂等 upsert）→ 同步返回结果
```

与技能先例的两处有意差异：策略**仅补缺**（决策 #14）；**不走后台任务**——词表量小（几十条）upsert 为毫秒级，同步返回即可，词表膨胀后再升级为 `pkg::background_task` 轮询形态（技能走后台是因为技能多 + 副本同步耗时长）。

**前端落位（双 tab 一体）**：知识图谱已是独立页面（`/hr/knowledge-graph`，`HrKnowledgeGraph`），将其改造为父壳 + 页面级 tab：

- **「知识图谱」tab**（默认）：现状内容整体平移（搜索 + canvas 可视化 + 详情 + 种子推荐），零重写
- **「本体管理」tab**：词表 CRUD（实体类 / 关系类型）+ 同义映射管理 + 漂移看板（Top N / 覆盖率 / 趋势）

TBox（本体）与 ABox（图谱）在同一入口一体呈现，路由与导航**零迁移**。tab 实现对齐 [agent_memory_panel.rs#L16](frontend/src/pages/hr/agent_memory_panel.rs#L16) 的 `MemoryTab` 枚举 + signal 切换 + use_effect 自动 fetch 先例。域归属：前后端同域 **hr**——后端 [handlers/hr/ontology/](src/handlers/hr/ontology)（待创建，与 agent/、skill/ 平级）+ 前端 api/hr.rs；TBox 与 ABox（图谱接口在 [handlers/hr/agent/](src/handlers/hr/agent) 下）同域对称，零歧义。注入边界见决策 #13：**注入方法与词表数据归 hr**（`apply_default_lexicon`），system seed 设施只做注入管道（内置快照提供方）；触发时机见决策 #15——初始化组织（[initialize_system.rs#L261](src/handlers/organization/initialize_system.rs#L261) run_steps 追加词表注入步骤，对齐其调用 apply_preset_skills 的既有模式）+ 管理页手动同步，**不进启动链路**——设施在 system，方法在业务域。

### 2.6 漂移归纳闭环（无提案队列）

Agent 不提案。漂移数据积累到看板，管理员按四条清单做抽象决策：

**看板指标**（SQLite 读路径惰性聚合，"现在时"视角）：

| 指标 | 口径 | 来源 |
|------|------|------|
| Top N 漂移词 | 词表外 raw_term 按关系/边数量降序 | MemoryDao GROUP BY 词频 + OntologyLexicon 内存解析 |
| 词表覆盖率 | 规范词关系数 / 全部关系数 | 同上 |
| 漂移节点数 | node_type 词表外的节点数 | 同上 |
| 漂移趋势 | 各指标随时间变化 | DuckDB 事件流（"历史时"视角，§2.7） |

**新词决策清单**（管理员面对一个高频漂移词时依次自问）：

1. **能否同义映射？** 若它只是既有规范词的别名 → 加 synonym mapping，零抽象成本
2. **是否不可归约？** 若压缩进既有词会丢失必要语义（如"师徒"强压进"协作"丢失方向与传承义）→ 值得新词
3. **结构是否稳定？** 若只在单一 Agent / 单一会话出现 → 观望，等漂移趋势说话
4. **是否多源？** 若多个 Agent 独立产出同一词 → 强抽象信号，优先处理

**闭环**：反常积累（漂移上涨）→ 看板可见 → 人工归纳（新增词表项或映射）→ 惰性解析自动愈合 → 指标回落。这正是"范式修订"的工程化：数据揭示反常，人来完成抽象。

### 2.7 双层统计：SQLite 惰性聚合 + DuckDB 事件流

套用项目既有"双层互补"模式：

```
读路径（看板，"现在时"——随本体进化自动愈合）
  Handler → OntologyDomain → OntologyDal
      ├─ OntologyDao.list_all() ──────────→ 内存构建 OntologyLexicon
      ├─ MemoryDao.word_freq(...) ────────→ GROUP BY 词频聚合（SQL 的家）
      │     └─ common::ontology::resolve ─→ Top N / 覆盖率 / 漂移节点数
      └─ MemoryDao.detail(raw_term,...) ──→ 明细下钻（解析结果作参数回传）

写路径（打点，"历史时"——不可变审计，零解析；单事件管道，统计在消费侧）
  runtime 沉淀链路（原文落库后）
      └─ aop::publish(MemoryTermsWritten) → AOP 事件中心 → OntologyCertifyConsumer
            ├─→ OntologyDomain.certify（读路径解释 + 置位）
            └─→ OntologyDriftEvent → Stats collector → DuckDB（只记原文）
```

**方向纪律**：读路径的"观察者"是 OntologyDal（组合 OntologyDao + MemoryDao）；写路径的 MemoryDal / MemoryDao **零改动零感知**——依赖单向指向被观察者。

**监控链路归属**：`record_event!` 宏在 pkg 层、事件消费者在 [src/consumer/](src/consumer) 顶层——两者是**跨域基础设施**（不属于任何 domain，system 域同样只是打点方之一）。hr 域的角色是打点方（写路径调用宏）与读路径提供方（DuckDB 趋势查询接口走 handlers/hr/ontology），**不自建 consumer 管道**——这是"写只观察"原则在设施维度的延伸：观察数据归业务域，观察设施归基础设施层。

**DuckDB 事件定义**（对齐 [pkg/stats/mod.rs#L89](src/pkg/stats/mod.rs#L89) `record_event!` 模板与 [dao/agent/stats_duckdb.rs](src/service/dao/agent/stats_duckdb.rs) 先例）：

```rust
// src/pkg/stats/ontology_drift.rs（待创建）
pub struct OntologyDriftEvent {
    pub agent_id: String,   // 谁沉淀的
    pub kind:       String, // "relation" | "class"
    pub raw_term:   String, // 只记原文，不解析、不翻译
}
// DuckDB tags 3 键：{ agent_id, kind, raw_term }（≤15 键约束内）
```

**为什么只记原文**：解析结论（是否漂移、归属哪个规范词）会随本体进化过期，落库即腐化；原文是不可变事实。历史趋势分析在 DuckDB 侧用当时的词表快照重放解析即可。

---

## 三、分层文件清单

> ✅ = 已存在（锚点有效）；🆕 = 待创建（仅约定落位，落地后补 `#Ln-Lm` 锚点）

| 层 | 文件 | 状态 | 职责 |
|----|------|:---:|------|
| common（共享） | [common/src/enums/memory.rs](common/src/enums/memory.rs#L130) | ✅ | `KnowledgeRelationType` 15 词枚举 → 降级为 seed 初始词表来源 + 展示回落 |
| common（共享） | [common/src/enums/memory.rs](common/src/enums/memory.rs#L183) | ✅ | custom 兜底与未知词原样透出（漂移入口，保留不动） |
| common（共享） | common/src/ontology.rs | 🆕 | `OntologyLexicon` + `resolve` 纯函数（§2.3），三消费方共用 |
| common（API） | common/src/api/ontology.rs | 🆕 | 本体 CRUD / 看板 DTO（DTO 单一事实源，对齐 api/seed.rs 形态） |
| models（PO） | [models/memory.rs](src/models/memory.rs#L242) | ✅ | 图谱 PO `node_type` 开放字段（漂移主体，零改动） |
| models（PO） | [models/memory.rs](src/models/memory.rs#L321-L324) | ✅ | `weight` 边权重（复用，零改动） |
| models（PO） | src/models/ontology.rs | 🆕 | 3 张本体表 PO + Entity 转换 |
| dao（SQLite） | src/service/dao/ontology/sqlite.rs | 🆕 | 3 张表 DDL + CRUD；全量词表查询 `list_all` |
| dao（SQLite） | [dao/memory/sqlite.rs](src/service/dao/memory/sqlite.rs) | ✅ | 新增词频 GROUP BY 聚合方法（漂移统计 SQL 的家） |
| dao（DuckDB） | src/service/dao/ontology/stats_duckdb.rs | 🆕 | 漂移事件表 DDL + 查询（对齐 [dao/agent/stats_duckdb.rs](src/service/dao/agent/stats_duckdb.rs)） |
| dal | src/service/dal/ontology.rs | 🆕 | 组合 OntologyDao + MemoryDao：词表加载、看板聚合、认证执行 |
| dal | [dal/memory.rs](src/service/dal/memory.rs#L262) | ✅ | `MemoryDalImpl` **零改动**（被观察，无感知） |
| dal | [dal/memory.rs](src/service/dal/memory.rs#L198) | ✅ | `traverse_knowledge_graph` 先例：DAL 组合内存解析的既定模式 |
| domain | src/service/domain/hr/ontology.rs | 🆕 | OntologyDomain：词表 CRUD 编排、`certify_memory_terms` 认证（供消费者调用）、看板查询编排、`apply_default_lexicon` 幂等注入（与 agent.rs / skill.rs 平级） |
| domain | [domain/hr/mod.rs](src/service/domain/hr/mod.rs) | ✏️ | 仅声明 `pub mod ontology;` 挂载子模块（对齐 agent/skill 先例）；**不新建 init_base_data**——词表注入走初始化组织 + 手动同步（决策 #15），对齐预置内容资产不进启动链路的既有约定 |
| domain | [domain/runtime/memory.rs](src/service/domain/runtime/memory.rs) | ✅ | 沉淀链路尾部唯一动作 `aop::publish(MemoryTermsWritten)`（对齐 [awakening.rs#L306](src/service/domain/runtime/awakening.rs#L306) 先例）——唯一侵入点，追加式，不碰 stats / domain / dal（决策 #16） |
| consumer | src/consumer/ontology_certify_consumer.rs | 🆕 | 订阅 MemoryTermsWritten（Async 模式）：调 OntologyDomain.certify_memory_terms（消费者→Domain 合法方向，[task_event_consumer.rs#L74](src/consumer/task_event_consumer.rs#L74) 先例）+ 记 OntologyDriftEvent 统计（消费者内打 stats 先例：[tool_exec_stats_consumer.rs#L67-L70](src/consumer/tool_exec_stats_consumer.rs#L67-L70)） |
| domain（seed 管道） | [domain/system/seed/defs.rs](src/service/domain/system/seed/defs.rs#L19) | ✅ | `SeedSnapshot` 扩展 `ontology` 段（classes / relation_types / synonym_mappings）；分发/迁移场景由该管道调用 hr 域注入方法 |
| domain（seed） | [domain/system/seed/default.json](src/service/domain/system/seed/default.json) | ✅ | 内置默认词表 JSON（由现有枚举转换） |
| domain（seed） | [domain/system/seed/diff.rs](src/service/domain/system/seed/diff.rs) | ✅ | 快照 diff 支持本体段（词表可随快照分发/迁移） |
| handlers | src/handlers/hr/ontology/ | 🆕 | 词表 CRUD + 同义映射 + 漂移看板接口（与 [handlers/hr/skill/](src/handlers/hr/skill) 平级，含看板查询） |
| handlers（seed 管道） | src/handlers/system/seed/sync_preset_ontology.rs | 🆕 | 本体预置同步 preview + sync（对齐 [sync_preset_skills.rs](src/handlers/system/seed/sync_preset_skills.rs) 形态：读内嵌快照 → 调 hr 域注入方法） |
| handlers | [handlers/hr/agent/settle_memory.rs](src/handlers/hr/agent/settle_memory.rs) | ✅ | 沉淀入口（不改签名；校验在 domain 尾部发生） |
| pkg（stats） | [pkg/stats/mod.rs](src/pkg/stats/mod.rs#L89) | ✅ | `record_event!` 宏（打点入口，不改） |
| pkg（stats） | src/pkg/stats/ontology_drift.rs | 🆕 | `OntologyDriftEvent` 定义 + StatTable 注册（4 步模板第 1-2 步） |
| pkg（stats） | [pkg/stats/collector.rs](src/pkg/stats/collector.rs) | ✅ | 事件注册（4 步模板第 3 步，追加注册行） |
| 前端 | [pages/hr/knowledge_graph.rs](frontend/src/pages/hr/knowledge_graph.rs) | ✅ | 改造为父壳：页面级 tab（图谱 / 本体）；图谱现状内容平移为默认 tab |
| 前端 | frontend/src/pages/hr/ontology_panel.rs | 🆕 | 本体管理面板：词表 CRUD + 同义映射 + 漂移看板（Top N / 覆盖率 / 趋势） |
| 前端 | [frontend/src/api/hr.rs](frontend/src/api/hr.rs) | ✅ | 追加本体 API 调用（词表 CRUD / 看板指标），与后端同域 hr |

---

## 四、边界与行为红线

1. **DAL 互引禁令**：OntologyDal 只允许组合 OntologyDao + MemoryDao（DAL → DAO）；任何 DAL 之间不得互相引用；`MemoryDal` 保持零改动。
2. **写只观察、读只解释**：写路径只允许原样落库 + 发布事件；一切"是否规范词"的判定与统计打点只发生在消费侧 / 读路径（认证、看板、展示回落）。写路径出现 `resolve` 调用或直接打 stats 即违规；消费者打点仍只记原文（决策 #10）。
3. **不物化漂移**：数据库中不得出现"mapped / drift_flag / resolved_key"之类的物化列或表。漂移是纯函数计算结果，本体进化后必须自动愈合。
4. **全局无租户维度**：3 张本体表不得添加 organization_id；作用域由部署单元（DB）承担。多租户演进必须走 §5.2 的显式迁移路径，不允许悄悄加列。
5. **published 非可见性**：认证置位/`needs_review` 不得参与任何可见性过滤；蜂巢内图谱保持全可见。软门禁：认证失败不拦截写入、不撤回数据。
6. **seed 单向写入**：seed 注入只新增（term_key 不存在才插），不修改不删除；管理页是词表唯一修改入口。两处写路径并存时以管理页为准。
7. **枚举降级边界**：`KnowledgeRelationType` 等枚举不得再作为运行时词表判定依据，只保留 seed 初始数据与展示回落两个职责；未注册词原样透出的既有行为（测试锚定）不得破坏。
8. **事件原样性**：DuckDB tags 中 raw_term 原样记录（trim 后），不做清洗、翻译、截断；单键超长时按现有 tags 约束处理，不得为"好看"而改写原文。
9. **STRICT 与软删除**：新表全部 STRICT 模式；退役一律 `status=0`，禁止物理删除被图谱引用的词表项。
10. **DTO 单一事实源**：本体 API 结构体定义在 common/src/api/ontology.rs，双端共用；PO 不出 DAL。

---

## 五、扩展模式

### 5.1 新增本体段（如流程类 ontology_processes）
1. 新增第 4 张表（沿用 term_key + status + STRICT 惯例），PO/DAO 落位对齐 §三
2. SeedSnapshot 增加对应段 + default.json 填内置数据（seed 只新增语义天然幂等）
3. OntologyLexicon 增加对应查找集合，resolve 增加 kind 分支；三个消费方自动获得新维度

### 5.2 多组织/多租户演进
1. `term_key` 是全部引用的锚点（图谱边/看板/映射都不持有本体表 id），迁移只需"建新表 + 按 term_key 回填"
2. 显式迁移：3 张表各加 organization_id 列 → 现有数据回填默认组织 → UNIQUE 改 (organization_id, term_key)
3. 词表分发：跨部署共享词表走 seed 快照导出/导入（diff.rs 已支持），不走 DB 直连

### 5.3 看板深化（历史时分析）
1. DuckDB 侧按时间窗聚合 OntologyDriftEvent（SQL 自由度远高于 SQLite 惰性聚合），产出"漂移速度""词表追赶曲线"
2. 需要历史某时点的漂移结论时：取该时点词表快照（seed diff 可复原）+ 事件流重放 resolve
3. 事件表膨胀后按现有 stats 表清理惯例治理，raw_term 高基数特性纳入保留期评估

### 5.4 沉淀引导强化
1. 神经技能提示词注入词表：OntologyDomain.list_lexicon(ctx) 结果进 prompt builder，Token 预算超限时按"关系词 > 实体类 > 同义样例"顺序裁剪
2. 引导效果度量：同一时间窗内 Drift 占比变化即引导有效性，无需新增埋点
