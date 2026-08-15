# 技能系统增强（tag 过滤 + 技能包 + 唤醒注入）设计

> 🎯 **本文档定位**：技能系统三方面增强的设计决策（为什么 SQL 层做 tag 过滤、技能包的幂等安装原则、技能唤醒注入的 token 边界；字段级实现细节读代码）
>
> 状态：定稿（2026-07-15 功能落地）
>
> 查阅场景：新增技能包、理解技能唤醒注入机制、优化工具/技能 tag 过滤性能时打开。
>
> 关联文档：
> - [skill_design.md](./skill_design.md) — 技能系统基础设计
> - [tool_design.md](./tool_design.md) — 工具系统基础设计（tag 过滤通用优化）
> - [runtime_design.md](./runtime_design.md) — 唤醒流程与 PromptBuilder

---

## 一、设计目标与关键决策

### 问题背景

技能系统已有基础 CRUD 和向量搜索，但存在三个核心缺口：

| 缺口 | 影响 |
|-----|------|
| tag 过滤缺失 | SkillQuery/ToolQuery 不支持按 tag 精确过滤；技能包概念无法落地；每次唤醒全量工具到内存再 Rust 遍历过滤 |
| 批量安装机制缺失 | 只能逐个 `install_to_agent`；场景化技能（如「项目管理包」「写作包」）安装体验差 |
| 技能不参与唤醒 | PromptBuilder 预留了 `agent_skills` 字段但从未填充；Agent 不知道自己有哪些技能可用 |

### 关键决策表

| # | 决策问题 | 选择方案 | 选择原因 |
|---|---------|---------|---------|
| 1 | tag 过滤实现位置 | **SQL 层 `json_each` 精确匹配，不在内存中 Rust 遍历** | Skill/Tool 数量增长后内存过滤 O(N) 退化；SQL 层走 `EXISTS (SELECT 1 FROM json_each(tags) WHERE value = ?)`，JSON 字段走表达式索引；两实体实现同构 |
| 2 | 工具和技能 tag 过滤是否统一 | **统一实现模式** | Tool 场景更紧急（每次唤醒都要过滤神经工具），做完 Skill 直接抄模板；SQL 结构完全同构 |
| 3 | 技能包卸载语义 | **卸载只移除 tag 关联，不删除 Agent 已有技能副本** | 用户对单个技能可能有本地修改；卸载技能包不破坏已有定制；再次重装做覆盖更新 |
| 4 | 安装幂等性 | **parent_skill_id + author_id 双重判定副本是否存在** | 同一 Published 技能对同一 Agent 只能有一个副本；重复安装跳过，不创建重复 |
| 5 | 重装覆盖策略 | **content_hash 对比：源技能更新才覆盖** | 避免不必要的文件 IO；用户本地修改过的副本（理论上 hash 不同但当前实现未对副本 hash，保守策略：重装一律覆盖 Published 源最新内容） |
| 6 | 唤醒注入内容粒度 | **只注入技能摘要（名称 + 描述），不注入完整 skill.md** | 完整 skill.md 动辄数百行，Token 膨胀指数级；Agent 需要完整内容时走 `search_skill` + 文件 API 按需读取 |

---

## 二、架构思路

```
用户侧管理 API ──────────┬──────────────────────────────────────┐
                         │ install_skill_pack / uninstall / list │
                         ▼                                    │
                 HrDomain.skill 领域层                        │
                   ├─ install_skill_pack(tag)                 │
                   │    ├─ SkillDal.list_published_by_tag    │
                   │    ├─ 幂等：parent_skill_id 副本检查     │
                   │    ├─ SkillDal.install_to_agent (批量)   │
                   │    └─ 记录 installed_skill_packs tag     │
                   ├─ uninstall_skill_pack → 仅移除 tag       │
                   ├─ reinstall_skill_pack → content_hash 覆  │
                   └─ list_installed_skill_packs → 返回 tag   │
                                                            │
Agent 唤醒流程（awaken）                                │
  Step 1：加载内建工具（ToolQuery.tags = ["neural"]）  │
    │  SQL 层 json_each 过滤（不走内存过滤）              │
    ▼                                                   │
  Step 2：加载 Agent 已安装技能副本                      │
    │  SkillDal.list_for_agent(agent_id)                │
    ▼                                                   │
  PromptBuilder.agent_skills(skills) ◄──────────────────┘
    │  格式：每条技能 = 名称 + 描述（两行级，不含正文）
    ▼
  注入系统 Prompt（放在「内建工具」之后，「用户上下文」之前）

        ┌──────────────────────────────────────┐
        │ Agent 运行中按需获取技能全文           │
        │   search_skill 神经工具               │
        │     └─ 关键词搜索 + tag 过滤          │
        │        └─ 返回技能摘要 + skill_id     │
        │           └─ 需要正文再走文件 API      │
        └──────────────────────────────────────┘
```

---

## 三、涉及文件清单

| 文件 | 角色 | 变更摘要 |
|------|------|---------|
| **DAO 层 tag 过滤（Skill + Tool 通用模式）** | | |
| [src/service/dao/skill/mod.rs](../../src/service/dao/skill/mod.rs) | SkillQuery | 新增 `tags: Option<Vec<String>>`（OR 语义） |
| [src/service/dao/skill/sqlite.rs](../../src/service/dao/skill/sqlite.rs) | SkillDao query SQL | 追加 `EXISTS (SELECT 1 FROM json_each(tags) WHERE value IN (...))`；关键词 LIKE 扩展到 tags 字段 |
| [src/service/dao/tool/mod.rs](../../src/service/dao/tool/mod.rs) | ToolQuery | 新增 `tags: Option<Vec<String>>` |
| [src/service/dao/tool/sqlite.rs](../../src/service/dao/tool/sqlite.rs) | ToolDao query SQL | 同 Skill 模板；load_builtin_tools 改为 SQL 层过滤 |
| **模型层** | | |
| [src/models/agent.rs](../../src/models/agent.rs) | AgentRuntimeConfig | 新增 `installed_skill_packs: Vec<String>` + install/uninstall/has 三个幂等操作封装 |
| [src/models/skill.rs](../../src/models/skill.rs) | SkillPo | 新增 `to_prompt_summary()`：名称 + 描述格式化 |
| **DAL / Domain 技能包逻辑** | | |
| [src/service/dal/skill.rs](../../src/service/dal/skill.rs) | SkillDal | 新增 list_published_by_tag / find_agent_skill_copies / install_to_agent 幂等增强 |
| [src/service/domain/hr/skill.rs](../../src/service/domain/hr/skill.rs) | HrDomain skill | 新增 4 个技能包方法：install/uninstall/reinstall/list（tag 维度） |
| **唤醒注入** | | |
| [src/service/domain/runtime/awakening.rs](../../src/service/domain/runtime/awakening.rs) | awaken 流程 | Step 1.5 新增：list_for_agent 加载技能 → 注入 PromptBuilder |
| [src/service/domain/runtime/context_assembly.rs](../../src/service/domain/runtime/context_assembly.rs) | PromptBuilder | 完善 `agent_skills()` 方法：有技能输出「可用技能」段落；无技能不输出 |
| **Handler + 路由** | | |
| [src/handlers/hr/agent/](../../src/handlers/hr/agent/) | 3 新 Handler | install_skill_pack / uninstall_skill_pack / list_installed_skill_packs |
| [src/handlers/hr/skill/search_skill.rs](../../src/handlers/hr/skill/search_skill.rs) | search_skill | 注册为 neural 工具；关键词 + tag 搜索；返回摘要不含正文 |
| [common/src/api/skill.rs](../../common/src/api/skill.rs) | API DTO | 技能包请求/响应结构体 |
| **零改动面** | | |
| Skill/Tool 文件存储机制、Published/Draft 状态机、技能副本文件复制逻辑、前端技能编辑器 | 零改动 | 安装/卸载逻辑不触及已有 CRUD 内部实现 |

---

## 四、关键边界（行为红线）

1. **Tag 过滤 OR 语义**：`tags = ["a", "b"]` 匹配含任意一个 tag 的实体（交集需上层过滤）；禁止默认 AND（AND 语义几乎匹配不到数据）
2. **安装/卸载只记录 tag，不反向追溯个体技能来源**：卸载 `project_management` 包后，用户后来手动安装的同名 tag 技能不受影响（tag 是多对多引用，不是所有权）
3. **唤醒注入永不包含完整 skill.md**：PromptBuilder 禁止任何情况下写入技能文件完整内容；Token 安全网——摘要格式硬编码为 name + description 两行
4. **load_builtin_tools 必须走 SQL 层过滤**：禁止回退到「全量加载 + filter_builtin_tools 内存过滤」（除非单测场景）；性能红线——唤醒工具加载耗时 < 10ms
5. **create_memory 降级后不得重新加 neural flag**：与记忆系统增强一致，专用工具拆分后通用接口不向 Agent 暴露

---

## 五、扩展模式

### 场景 1：新增技能包 tag（如「数据分析工具包」）

1. 技能库侧：给对应 Published 技能打上 `"data_analysis"` tag（现有 tag 字段直接追加，无需代码改动）
2. 验证生效：[HrDomain :: install_skill_pack(ctx, agent_id, "data_analysis")](../../src/service/domain/hr/skill.rs) → 自动按 tag 查 Published 技能并批量安装；无需新增 Domain/DAO 代码
3. （可选）前端：技能包下拉列表追加 tag 选项（前端静态枚举扩展即可）

### 场景 2：唤醒时 Prompt 中技能摘要追加更多字段（如「版本号」「作者」）

1. 模型层：[src/models/skill.rs](../../src/models/skill.rs) 的 `to_prompt_summary()` 追加 `version`/`author` 字段
2. 注意 Token 总量红线：`to_prompt_summary()` 输出总长度单条控制在 200 字符内；超过 5 个技能时只列前 5 个 + 省略提示

### 场景 3：Tool 新增「按项目安装工具包」能力

1. 与 Skill 技能包同构实现：ToolQuery.tags SQL 层过滤已具备；新增 ProjectRuntimeConfig.installed_tool_packs
2. DAL/Domain 层：ToolDal 新增 list_published_by_tag + install_to_project；HrDomain → ProjectDomain 对应方法，直接抄 skill 模板改域
