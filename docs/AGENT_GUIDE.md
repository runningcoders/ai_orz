# Agent 实现需求指引书

> **这份文档是 Agent 承接功能需求时的唯一入口。**
> 它不重复架构细节——只回答三个问题：① 这个需求属于哪个模块？② 动手前必须读哪几篇？③ 哪些红线不能碰？
>
> 人读入口：[docs/README.md](./README.md)（文档地图）· 全库清单：[docs/modules/INDEX.md](./modules/INDEX.md)
> 机器入口：[docs/modules/INDEX.json](./modules/INDEX.json)（`make wiki-index` 重建）

---

## 一、五步流程（照做，不要跳）

```
1. 定位模块     → 用 §四 路由表；拿不准就查 INDEX.json 的 scope
2. 读「为什么」 → 该模块 A-设计 中 status=现行 的文档
3. 读「是什么」 → 该模块 C-知识 里覆盖同一块的现状长文
4. 动手         → 遵守 §六 红线 + docs/CODE_STANDARDS.md
5. 回填         → 代码变了必须改 C-知识，然后 make wiki-index
```

**第 2 步是最容易被跳过、也是最容易导致返工的一步。** 本项目大量模块的做法是「先有设计决策，再有实现」，跳过设计直接写码，通常会重新发明一遍已有约束，或者在已被否决的方案上再走一遍。

---

## 二、知识库三层与权威关系

| 层 | 回答的问题 | 什么时候读 | 什么时候改 |
|---|---|---|---|
| `A-设计` | **为什么**这么设计（决策快照，写定后不追现状） | 动手前 | 只有设计本身变了才改 |
| `B-落地方案` | **要去哪**（规划 / 进度 / 任务拆解） | 判断是否已有在途方案 | 状态推进时改 |
| `C-知识` | **是什么**（代码现状百科长文 + RAG 原子知识卡） | 落到具体文件/字段时 | 代码变了就改 |

**权威关系（冲突时的判据）：**

- **`C-知识` 是"现状"权威** —— 它描述代码现在的样子。
- **`A-设计` 是"意图"权威** —— 它解释为什么是现在这样、边界在哪。
- **两者冲突 → 以 `C-知识` 为准**（代码即事实），并回头把 `A-设计` 修掉或标注"已被取代"。
- **`B-落地方案` 没有权威性** —— 它是快照，进度可能已过期，只用来判断"这块是否已有在途方案"。

---

## 三、`status` 语义（读错会直接踩坑）

`status` 在 `A-设计` / `B-落地方案` 的 frontmatter 里，由 `make wiki-meta` 校验。**它是判断"这篇能不能当权威"的唯一机器信号。**

| status | 含义 | Agent 该怎么做 |
|---|---|---|
| `现行` | 设计在生效 | ✅ 按它实现 |
| `落地中` | 设计正在落地 | ⚠️ 先看对应 `B-落地方案` 进度，避免与在途改造撞车 |
| `草案` | 未定稿 / 评审中 | ⚠️ **不要当权威**，先确认是否已拍板；未拍板则向人确认 |
| `进行中` | 方案正在执行（B 层） | ⚠️ 可能已有未合入的改动，先对齐 |
| `已完成` | 方案已交付（B 层） | ✅ 可作为"这块已经这么做过了"的历史依据 |
| `已废弃` | 已被取代 | 🚫 不要照着实现；以替代文档或 `C-知识` 为准 |
| `未标注` | 作者没声明 | ⚠️ **必须以 `C-知识` 的现行实现为准**，并把该设计的 status 补上 |

> **横幅不等于 status。** 部分文档顶部有 `> ⚠️ 疑似过期 / 已重构：...` 横幅，那是 `make wiki-stale` 体检出的**失效代码路径引用**（文档正文提到的文件已改名/删除），只说明"正文里的路径要核对"，不代表整篇设计失效。设计是否失效看 `status`。

---

## 四、模块路由表（15 个能力域）

> 数据来自 `docs/modules/INDEX.json`。**「现行设计」列只列 `status=现行` 的文档**；带 ⚠️ 的见括注。

| 模块 | 现行设计（必读） | 无设计时读这个 |
|---|---|---|
| **00-总览与架构** | 无 A 层（本模块即跨模块规范本家） | `C-知识/分层架构设计.md`、`AOP 事件系统架构.md`、`安全架构设计.md` |
| **01-组织与权限** | [user_credentials_design](./modules/01-组织与权限/A-设计/user_credentials_design.md) | — |
| **02-Agent运行时** | [runtime_design](./modules/02-Agent运行时/A-设计/runtime_design.md)、[thinking_task_policy_engine_design](./modules/02-Agent运行时/A-设计/thinking_task_policy_engine_design.md)、[think_lifecycle_design](./modules/02-Agent运行时/A-设计/think_lifecycle_design.md) | ⚠️ `thinking_graph_engine_design`（落地中，配 [实施计划](./modules/02-Agent运行时/B-落地方案/思考图引擎改造.md)） |
| **03-记忆系统** | 无 A 层 | `C-知识`（四层记忆/沉淀/休息）+ 根目录 [memory_design.md](./memory_design.md) |
| **04-对话与消息** | [lark_channel_integration_design](./modules/04-对话与消息/A-设计/lark_channel_integration_design.md)、[message_recall_design](./modules/04-对话与消息/A-设计/message_recall_design.md) | ⚠️ `wechat_channel_integration_design`（草案） |
| **05-任务与项目** | [chat_project_agents_and_tool_call_identity_design](./modules/05-任务与项目/A-设计/chat_project_agents_and_tool_call_identity_design.md)（聊天侧栏 Agent 列表 + 工具调用身份） | `C-知识/任务状态机与项目聚合.md` 等 53 篇现状长文 |
| **06-工具与技能** | [tool_credential_requirement_design](./modules/06-工具与技能/A-设计/tool_credential_requirement_design.md)、[tool_output_boundary_design](./modules/06-工具与技能/A-设计/tool_output_boundary_design.md)、[web_search_and_browser_tools_design](./modules/06-工具与技能/A-设计/web_search_and_browser_tools_design.md) | ⚠️ `skill_import_methods_design`（草案） |
| **07-外部Agent(A2A)** | 无 A 层 | `C-知识/A2A 集成指南.md` 等 |
| **08-搜索** | [ontology_knowledge_sedimentation_design](./modules/08-搜索/A-设计/ontology_knowledge_sedimentation_design.md) | — |
| **09-统计与监控** | [workspace_topbar_metrics_design](./modules/09-统计与监控/A-设计/workspace_topbar_metrics_design.md) | — |
| **10-异步基建(AOP)** | [aop_producer_consumer_contract_design](<./modules/10-异步基建(AOP)/A-设计/aop_producer_consumer_contract_design.md>) | — |
| **11-前端应用** | [frontend_architecture](./modules/11-前端应用/A-设计/frontend_architecture.md)、[ui_design_system](./modules/11-前端应用/A-设计/ui_design_system.md) | — |
| **12-基础设施与部署** | [logging_design](./modules/12-基础设施与部署/A-设计/logging_design.md)、[sqlx_guide](./modules/12-基础设施与部署/A-设计/sqlx_guide.md)、[model_provider_access_mode_design](./modules/12-基础设施与部署/A-设计/model_provider_access_mode_design.md) | — |
| **13-数据模型** | [api_protocol_convention](./modules/13-数据模型/A-设计/api_protocol_convention.md)、[pagination_and_count_convention](./modules/13-数据模型/A-设计/pagination_and_count_convention.md)、[timestamp_convention](./modules/13-数据模型/A-设计/timestamp_convention.md) | — |
| **14-测试** | 无 A 层 | `C-知识`（测试指南 / Playwright E2E） |

**跨模块必读（不属于任何单一模块，但每次实现都适用）：**

| 主题 | 文档 |
|---|---|
| 编码规范 SSOT（命名/分层/宏/错误/日志/分页/枚举） | [CODE_STANDARDS.md](./CODE_STANDARDS.md) |
| 架构总纲（实体关系、设计哲学、核心概念） | [ARCHITECTURE.md](./ARCHITECTURE.md) |
| 分层实践 + 反模式避坑 | [LAYERED_ARCHITECTURE_PRACTICE.md](./LAYERED_ARCHITECTURE_PRACTICE.md) |
| 文档编写/引用/图谱法则 | [DOCUMENTATION.md](./DOCUMENTATION.md) |

---

## 五、在途改造清单（动手前先查，避免撞车）

以下方案处于 `进行中` / `草案`，**相关模块的改动可能已有未合入的分支工作**。碰这些模块前先读对应 B 层文档：

| 模块 | 在途方案 | 状态 |
|---|---|---|
| 01-组织与权限 | 联邦鉴权升级方案（静态 token → 组织公钥签名） | 进行中 |
| 01-组织与权限 | 组织组网与去中心化联邦方案 | 草案 |
| 02-Agent运行时 | 思考图引擎改造实施计划 | 进行中 |
| 04-对话与消息 | 微信 iLink 协议对齐重构方案 | 进行中 |
| 04-对话与消息 | 飞书链路对齐重构方案 | 进行中 |
| 06-工具与技能 | 浏览器工具与环境就绪提示（阶段二） | 进行中 |
| 06-工具与技能 | 网络搜索工具（阶段一）· 技能导入方式扩展落地 | 草案 |
| 08-搜索 | 向量搜索 Pre-Filter 改造（Payload 落库与谓词下推） | 进行中 |
| 08-搜索 | 本体论知识沉淀落地 | 草案 |
| 09-统计与监控 | Responses API 接入与推理统计扩展 | 进行中 |

---

## 六、反模式（这几条踩了基本都要返工）

1. **不读 A-设计 就重新设计。** 你想到的方案很可能已被否决过，理由就在设计文档里。
2. **把 `草案` / `未标注` 当权威实现。** `草案` 表示未定稿（如 `wechat_channel_integration_design`、`skill_import_methods_design`、`thinking_graph_engine_design`），先确认是否已拍板；`未标注` 一旦出现说明作者没声明状态，**必须以 `C-知识` 现行实现为准**。
3. **相信 stale 横幅后面的旧路径。** 横幅里列出的代码路径已改名/删除，正文提到它们的地方要按现行代码核对。
4. **改了代码不改 `C-知识`。** 这是知识库腐化的唯一来源；改完必须 `make wiki-index` 并提交派生索引。
5. **新增设计/计划文档不带 frontmatter。** `make wiki-meta` 会直接拦住（缺 `kind/module/name/title/status` 即失败）。
6. **跨层越权。** 严格单向 `Adapter → Domain → DAL → DAO → Models`，PO 不上浮，Domain 禁跨域——细节见 `LAYERED_ARCHITECTURE_PRACTICE.md`。

---

## 七、机器接口

```bash
make wiki-index    # 重建 3 类索引（INDEX.md / INDEX.json / 各 00-INDEX.md）
make wiki-check    # 门禁聚合：索引一致性 + 元数据合规（提交前跑这个）
make wiki-stale    # 诊断：列出文档里引用的失效代码路径（不阻塞）
```

**要查东西，用 `INDEX.json` 查，不要通读 `INDEX.md`。** 常用查询（按代码路径反查设计、只看现行设计、查在途方案等）已写在 [README §六 查询配方](./README.md)，直接照抄命令即可：

```bash
# 例：改 src/models/agent.rs 之前，该读哪几篇
jq -r '.modules[].docs[] | select(.scope and (.scope|tostring|contains("src/models/agent.rs")))
       | "\(.tier)\t\(.status)\t\(.path)"' docs/modules/INDEX.json
```

`docs/modules/INDEX.json` 结构（`schema: 1`）：

```json
{
  "total": 524,
  "modules": [
    {
      "id": "02-Agent运行时",
      "counts": { "A-设计": 4, "B-落地方案": 1, "C-知识": 19 },
      "docs": [
        {
          "tier": "A-设计",
          "path": "02-Agent运行时/A-设计/runtime_design.md",
          "name": "runtime_design",
          "title": "Runtime Domain 设计",
          "status": "现行",
          "scope": ["src/models/agent.rs", "..."],
          "related": ["02-Agent运行时/A-设计/thinking_task_policy_engine_design.md"]
        }
      ]
    }
  ]
}
```

- `path` 相对 `docs/modules/`，可直接拼成读取路径。
- `scope` = 该文档正文涉及的代码路径（自动抽取的初值，可人工校正）——用它反查"改这个文件该读哪篇设计"。
- `status` 只从文档显式信号推导，`未标注` 表示需人工确认。

---

## 八、落地后必做（回填契约）

代码改动合入前，按下表回填，否则知识库立刻开始腐化：

| 你改了什么 | 必须回填 | 备注 |
|---|---|---|
| 某模块的实现细节 | 对应 `C-知识` 长文 | 代码变了就改，这是唯一高频动作 |
| 新增了原子知识点 | 新建 `C-知识` RAG 卡 | 带 `kind/name/category/scope/source_files` frontmatter |
| 设计决策变更 | 对应 `A-设计`，或新建一篇并给旧篇标 `status: 已废弃` | 不要就地抹掉旧决策的理由 |
| 方案推进 | 对应 `B-落地方案` 的 `status` 与 checkbox | `进行中` → `已完成` |
| 任何增删改名 | `make wiki-index` 并提交派生索引 | `make wiki-lint` 会拦 |

**一次典型收尾：**

```bash
make wiki-index && make wiki-lint && make wiki-meta
```

---

## 九、已知缺口（诚实声明，不要假装完整）

- **4 个模块没有 A 层设计文档**：00-总览与架构、03-记忆系统、07-外部Agent(A2A)、14-测试。这些模块的"为什么"散在 `00-总览` 的 C-知识、根目录 `memory_design.md` 与各模块现状长文里。后续补设计时按 §八模板补齐即可。
- **`status` 完整性已达标（43/43，0 未标注）**，但它是**人工维护项**：新增设计/计划文档必须写明，否则 `make wiki-meta` 直接失败。若哪天出现 `未标注`，按 §三 表处理——不要默认当现行。
- **`make wiki-stale` 有 43 条失效代码路径引用**，分布在 33 篇带横幅的文档中，均为"确已删除/重构"的历史引用（横幅里已写明原因），属可接受残留，修一次即减少一批。
