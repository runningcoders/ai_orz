---
kind: RAG 原子知识卡
name: Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配
category: 业务模块 / AI Agent
scope:
  - "common/src/api/agent.rs"
  - "common/src/api/skill.rs"
  - "common/src/api/tool.rs"
  - "src/handlers/hr/agent/association.rs"
  - "src/handlers/hr/agent/get_agent.rs"
  - "src/handlers/hr/agent/sync_packs.rs"
  - "src/service/domain/hr/agent.rs"
  - "src/service/domain/hr/mod.rs"
  - "src/service/domain/finance/tool_provider.rs"
  - "src/pkg/agent_runtime_state.rs"
source_files:
  # ===== 源码锚点 =====
  - common/src/api/agent.rs#L101-L113
  - common/src/api/agent.rs#L145-L189
  - src/handlers/hr/agent/association.rs#L1-L146
  - src/handlers/hr/agent/get_agent.rs#L138-L157
  - src/service/domain/hr/mod.rs#L433-L467
  - src/service/domain/hr/agent.rs#L654-L845
  - src/service/domain/finance/tool_provider.rs#L38-L45
  - src/handlers/finance/tool/response.rs#L16-L31
  - src/handlers/hr/agent/sync_packs.rs#L1-L25
  - src/service/domain/hr/agent.rs#L94-L133
  - src/service/domain/hr/agent.rs#L135-L290
  # ===== Wiki 长文 =====
  - docs/wiki/zh/content/功能模块/AI Agent 管理/Agent 生命周期管理.md
  # ===== 兄弟卡（Level 3 平行卡）=====
  - 【平行卡】docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md
---

## §1 概述

**本卡角色**：Agent 视角下工具与技能全景数据的"按需装配"机制知识卡。覆盖 DTO 结构 `AgentToolsOverview`（neural_tools / bound_tools / pack_groups 三分组互斥并集等于运行时注入全集）和 `AgentSkillsOverview`（neural_skills / pack_groups / standalone_skills）、Hr domain 产出 ID 分组的业务规则（neural → bound → pack 优先级去重）、Handler 层 `association.rs` 跨领域编排（调 finance domain 批量查工具实体 → runtime domain 就绪探测 → 复用专业领域 `to_list_item` 打包，避免硬编码 Unknown）。**定位：新增 Agent 全景展示、排查分组遗漏/重复、调试 runtime_ready 未正确传递时读。**

- **按需装配**：`GetAgentRequest.with_tools` / `with_skills` 为 Option 开关，关闭时跳过全部工具/技能查询并在响应中跳过对应 DTO 字段（`None` + `serde(skip_serializing_if)`），避免 Agent 详情页高频拉取时的冗余开销。
- **专业领域打包复用**：Handler 层不重复实现 DTO 转换逻辑——工具走 `finance::tool::response::to_list_item`（内含 `runtime_ready` 就绪状态，来自 runtime domain `probe_runtime_ready` 带 TTL 30s 缓存），技能走 `hr::skill::response::to_list_item`。domain 层只产出 ID 分组，打包职责归专业领域。
- **三分组去重规则同源**：工具分组（neural tags 过滤 internal → agent_tools 关联表 → 按 installed_tags 展开）与运行时唤醒装配逻辑完全同源（`src/service/domain/hr/agent.rs#L654-L845`），确保全景展示与实际注入一致。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| common/src/api/agent.rs | 全景 DTO + 请求参数 | `AgentToolsOverview` / `AgentSkillsOverview` 三分组结构、`AgentToolPackGroup` / `AgentSkillPackGroup` 分组条目、`GetAgentRequest.with_tools` / `with_skills` 按需开关 | `#L101-L189` |
| src/service/domain/hr/mod.rs | ID 视图 struct 定义 | `AgentToolGroups` / `AgentSkillGroups` / `AgentToolPackIds` / `AgentSkillPackIds` — domain → handler 之间传递分组结果，刻意不含 common DTO | `#L433-L467` |
| src/service/domain/hr/agent.rs | 三分组业务规则 | `get_agent_association_groups`：工具侧 neural→bound→pack 优先级去重 + internal 剔除；技能侧 neural→pack→standalone（技能讲究自进化副本，只查 author_id = agent_id） | `#L654-L845` |
| src/handlers/hr/agent/association.rs | 跨领域编排（新建模块） | `build_tools_overview` / `build_skills_overview`：汇总 ID → finance domain 批量查实体 → runtime domain 就绪探测 → 复用专业领域 `to_list_item` 打包 | `#L1-L146` |
| src/handlers/hr/agent/get_agent.rs | Handler 入口按需装配 | 读开关 → 调 domain 产出 ID 分组 → association 模块装配 DTO → 写入 `GetAgentResponse.tools_overview` / `skills_overview` | `#L138-L157` |
| src/service/domain/finance/tool_provider.rs | 工具实体批量查询 | `ToolProviderManage.query_tools`：按 ToolQuery.ids 一次性批量查完所有工具实体 | `#L38-L45` |
| src/handlers/finance/tool/response.rs | 就绪探测 + 工具 DTO 打包 | `probe_runtime_ready`（runtime domain `tool_readiness`，TTL 30s 缓存）+ `to_list_item`（含 `runtime_ready` 字段） | `#L16-L50` |
| [Agent 生命周期管理.md](docs/wiki/zh/content/功能模块/AI Agent 管理/Agent 生命周期管理.md) | Wiki 长文关联 | Agent 全景装配机制与生命周期状态机、运行时唤醒装配逻辑互补 | cite 区 |
| 【平行卡】[工具系统三层调用架构](docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md) | Level 3 兄弟卡 | 兄弟卡管运行时实际注入的三层调用架构与执行原语，本卡管 Agent 视角下的全景分组展示与按需装配 | 双向对称声明 |

**章节来源**
- [agent.rs#L101-L189](common/src/api/agent.rs#L101-L189)
- [association.rs#L1-L146](src/handlers/hr/agent/association.rs#L1-L146)
- [hr/agent.rs#L654-L845](src/service/domain/hr/agent.rs#L654-L845)
- [response.rs#L16-L50](src/handlers/finance/tool/response.rs#L16-L50)

---

## §3 架构约定

本卡聚焦 Agent 视角下工具与技能的「全景分组与按需装配」；运行时实际注入工具的「三层调用架构」由兄弟卡「工具系统三层调用架构」承载，两卡互补视角并行。

**分层职责边界**（严格单向）：
```
Handler 层 (association.rs)    → 跨领域编排：汇总 ID → 调专业领域查询实体 → 调专业领域打包
    ↑
Domain 层 (hr/agent.rs)       → 产出 ID 分组（neural → bound → pack 的业务规则）
    ↑
DAL 层 (agent_dal / tool_dal)  → 数据查询（被 domain 调，不被 handler 直连）
    ↑
DAO 层 (SQLite)               → CRUD
```

**跨领域编排模式**（association.rs 职责）：
| 环节 | 归属域 | 调用方法 | 说明 |
|------|--------|---------|------|
| 工具实体查询 | Finance | `tool_provider_manage.query_tools(ids)` | 按汇总 ID 批量一次查完，避免按组 N 次往返 |
| 运行时就绪探测 | Runtime | `probe_runtime_ready(&ctx, &tools)` | TTL 30s 缓存，列表高频调用无重复开销 |
| 工具 DTO 打包 | Finance handler | `finance::tool::response::to_list_item(tool, runtime_ready)` | 复用专业领域逻辑，runtime_ready 是真实值而非硬编码 Unknown |
| 技能实体查询 + 打包 | HR | `skill_manage.list_for_agent` + `hr::skill::response::to_list_item` | 技能只查 Agent 自身副本（author_id = agent_id） |

**三分组去重规则**（同源运行时装配）：
- **工具侧**：① neural（tags 含 neural，过滤 internal，全部启用）→ ② bound（agent_tools 关联表，已排除 neural 命中项）→ ③ pack（按 runtime_config.installed_tags 逐个 tag 展开，跳过 neural、跳过前两组已命中）。internal 标签工具全程剔除。
- **技能侧**：仅查 Agent 自身技能副本（author_id = agent_id，排除 Expired）。① neural（tags 含 neural）→ ② pack（按 installed_skill_packs 顺序展开，排除 neural 命中）→ ③ standalone（不在任何 pack、也非 neural 的剩余项）。

---

## §4 硬约束（9 条）

1. **domain 层只产 ID 分组，不含 common DTO**：`AgentToolGroups` / `AgentSkillGroups` 刻意只传 `Vec<String>` 和 `AgentXxxPackIds { tag, xxx_ids }`——打包（ToolListItem / SkillListItem）必须由 handler 层 `association.rs` 调专业领域方法完成。禁止在 domain 层出现 ToolListItem / SkillListItem。
2. **专业领域打包方法必须复用，禁止硬编码 Unknown**：工具 DTO 打包必须走 `finance::tool::response::to_list_item`（runtime_ready 由 `probe_runtime_ready` 真实填充），不能自己 new ToolListItem 把 runtime_ready 写死 Unknown；技能同理走 `hr::skill::response::to_list_item`。
3. **ID 必须先汇总去重再批量查询**：association.rs 的 `build_tools_overview` 先把 neural_ids + bound_ids + 所有 pack 的 tool_ids 汇总到一个 Vec，sort + dedup 后才调 `query_tools(ids)`。禁止按组分别调三次 query_tools（N 次往返 + 工具实体在多组重复出现）。
4. **neural → bound → pack 优先级与运行时唤醒完全同源**：三分组的去重顺序、internal 剔除、enabled_only 过滤，必须与 runtime 唤醒时 `hr/agent.rs` tag_filter 的装配逻辑保持一致（`get_agent_association_groups` 与 `get_agent` 内 `with_tools=true` 分支的规则同源）。改了一处忘了另一处 = 全景展示与实际注入不一致。
5. **internal 标签全程剔除**：任何分组（neural / bound / pack）在加入前都必须过滤 `tags.contains("internal")`。internal 工具为系统自用，不出现在 Agent 全景展示。
6. **技能全景只查 Agent 自身副本**：技能侧全程限定 `author_id = agent_id` + `exclude Expired`。技能讲究「安装且自进化」，即便 neural 技能也必须先安装到自身目录才能在全景展示中出现——不能直接从全局技能池拉数据。
7. **按需开关两侧均关闭时必须短路**：`with_tools=false && with_skills=false` → `get_agent_association_groups` 直接返回 `(None, None)`，handler 层跳过全景装配，`GetAgentResponse.tools_overview` / `skills_overview` 为 None。禁止在开关关闭时仍执行工具/技能查询。
8. **runtime_ready 就绪探测带 TTL 缓存**：`probe_runtime_ready` 内部的 `tool_readiness` 对每个工具的 CLI/凭据型探测结果有 30s TTL 缓存。列表页高频调用 Agent 全景时，同一工具不会被反复探测。修改探测逻辑后需确认 TTL 未被意外绕过。
9. **sync_agent_packs 是 create_agent 入职绑定唯一入口**：Agent 创建拆 2 显式步骤（基础信息持久化 → sync_agent_packs 补装），create_agent 不再逐个 handler 调 install；sync_agent_packs 内部两阶段：① BASE_AGENT_PACKS（neural/skill_management/tool_management）工具+技能包缺失补装；② 已安装技能包增量补全（检测 tag 下新增已发布技能，reinstall 刷新副本内容）。返回 SyncAgentPacksResponse 带 installed_tool_tags / installed_skill_packs / refreshed_skill_packs 三计数，**禁止**硬编码 tag 列表；同步必须复用 SkillDomain.find_by_tag + ToolDomain.query。
