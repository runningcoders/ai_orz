---
kind: RAG 原子知识卡
name: 工具系统三层调用架构：CoreTool trait + Builtin/HTTP/MCP 三协议路由 + register_handler_tool 宏
  + 神经工具免绑定三层校验
category: 基础设施 / 工具注册表
scope:
- src/service/dao/tool/**
- src/service/dao/tool_call/**
- src/service/dao/mcp_tool/**
- src/service/dao/mcp_server/**
- src/service/dal/tool*.rs
- src/service/domain/finance/tool_provider.rs
- ai-orz-macros/src/register_handler_tool/**
- src/handlers/**/*tool*.rs
source_files:
- 'src/service/dao/tool/mod.rs '
- 'src/service/dao/tool_call/mod.rs '
- 'src/service/dao/tool_call/impl.rs#L1-L120 '
- 'src/service/dao/tool_call/mcp.rs '
- 'src/service/dao/mcp_server/mod.rs '
- 'src/service/domain/finance/tool_provider.rs#L1-L80 '
- 'src/handlers/finance/tool/create_http_tool.rs#L1-L40 '
- 'ai-orz-macros/src/register_handler_tool.rs '
- src/service/domain/runtime/tool_execution.rs#L36-L101
- src/models/tool.rs#L17-L96
- src/service/domain/hr/agent.rs#L100-L140
- docs/archive/design-archive/tool_design.md
- docs/archive/design-archive/mcp_tool_design.md
- docs/archive/design-archive/builtins_http_tool_design.md
- docs/archive/design-archive/generic_builtin_tools_design.md
- docs/archive/design-archive/handler-tool-registration-macro.md
- docs/design/tool_credential_requirement_design.md
- docs/archive/plan-archive/进程管理与shell_exec修复.md
- docs/archive/plan-archive/前端工具与进程管理.md
- docs/archive/plan-archive/移除rig依赖与向量存储后端解耦.md
- docs/wiki/zh/content/功能模块/工具生态系统/工具生态系统.md
- docs/wiki/zh/content/项目概述/核心功能特性/统一工具调用架构/统一工具调用架构.md
- docs/wiki/zh/content/功能模块/工具生态系统/工具注册与发现.md
- docs/wiki/zh/content/基础设施/工具注册表/工具注册表.md
- docs/wiki/zh/content/前端应用/页面模块/Finance 管理页面/工具管理/工具管理.md
- docs/wiki/zh/content/基础设施/工具注册表/共享工具凭据增强器.md
- 【平行卡 1】docs/wiki/knowledge/zh/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等
  Tag 分发 + Agent 入职绑定 + Prompt Token 熔断/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack
  幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断.md
- 【平行卡 2】docs/wiki/knowledge/zh/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry
  全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8
  类业务消费者注册.md
- 【平行卡 3】docs/wiki/knowledge/zh/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例.md
- 【平行卡 4】docs/wiki/knowledge/zh/Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配/Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配.md

---

## §1 概述

**本卡角色**：工具系统的整体架构知识卡。覆盖 ToolDao（PO/CRUD/向量搜索）/ ToolCallDao（统一 execute 双实现 Builtin+MCP）/ McpServerDao（MCP 服务器注册表）三层、domain runtime `RuntimeToolExecution` 的入口统一编排 + 三协议路由、`neural`/`internal` tag 加载过滤规则（hr/agent.rs）、`register_handler_tool!` 宏把 Handler 函数暴成神经工具、HTTP 工具 create_form method 白名单、shell_exec/fs 命令黑名单。**定位：新增工具协议、排查工具调用返回权限错误、调试工具注册宏失败时读。**

- **三层调用架构严格单向**（2026-08-21 D26 入口统一后现状）：think_loop → domain `RuntimeToolExecution::call_tool`（Auto）/ `dispatch_manual_tool`（Manual）单点编排 → DAL `call_tool(ToolExecutionRequest { tool, args, resolved })` → ToolCallDao（`assemble_core_tool` per-call 重组装 + `CoreTool::check` 凭据注入 + `execute` 统一原语）→ 分 Builtin impl.rs / MCP mcp.rs / HTTP（HTTP 工具按存储配置发起请求）。跨层禁止：think_loop 禁直连 tool_dal / mcp_tool_dal（绕行整条漏凭据注入）；Handler 管理面（CRUD/绑定）走 Finance `ToolProviderManage`，internal tag 绑定拒绝在 bind_tool_to_agent 校验。
- **凭据编排并入调用链**（2026-08-21，D22/D26）：domain `call_tool` 先读 `ToolRegistry::credential_requirements(&po)` → `resolve_tool_credentials` 取数加工 → 未命中返回 `credential_missing_json` 结构化引导（不构造实例不进 DAL）；命中经 `ToolExecutionRequest.resolved` 传 DAL，由 `check` 注入单次实例（禁止缓存复用）。Agent 工具加载（绑定 + neural 免绑定 - internal 剔除）现位于 hr/agent.rs 与 runtime/tool_execution.rs。凭据需求声明/生产路由/增强器全貌见平行卡「共享工具凭据增强器」。
- **tag 加载过滤三层免绑定规则**（2026-07-12 增强，工具设计文档 §工具包机制；2026-08-21 加载位置更新）：① `tag = "neural"` 的工具在 Agent 唤醒加载（hr/agent.rs，SQL 层 tag_filter）时不用 agent.tool_bindings 就自动加入「神经工具集」（核心思考工具如记忆写入沉淀，避免入职漏绑）；② `tag = "internal"` 的工具加载时从 Agent 可绑定列表过滤掉 + ToolProviderManage.bind_tool_to_agent 绑定拒绝，Manual 调用经 domain `call_manual_tool_for_agent` 授权（绑定/neural/已装包三来源校验，control_mode 必须为 Manual）（用于后台任务/备份恢复等用户直操作工具，不给 Agent 直接用）；③ 普通工具按 tool_packs.tag 分组（"tool_memory", "tool_file", "tool_project"），Agent 入职 install_skill_pack 后再 bind 对应 tag。
- **4 个通用内置工具参数规范**（generic_builtin_tools_design.md）：① `shell_exec(command, cwd)` — 命令黑名单 rm -rf /、sudo、；② `fs_read(path)` — 必须在 data_dir 下，禁止 `..` 穿越；③ `fs_write(path, content)` — 同路径安全校验，先校验再分块写；④ `http_fetch(url, method, headers, body)` — 默认禁止访问 `169.254.169.254`（元数据服务）+ `localhost:*` 本地端口，method 只许 GET/POST。HTTP 工具创建接口（create_http_tool.rs）的白名单在 DAO 层再双校验一次，防止前端绕过。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| dao/tool/mod.rs ToolDao trait | 工具 CRUD + 分组 | create_tool / update_tool / query（tags_contains、status、category）/ search（FTS5+向量混合）；Vectorizable 对应 vss_tools | 见 ToolDao trait |
| dao/tool_call/mod.rs ToolCallDao trait | 统一执行原语 + 实例组装 | `assemble_core_tool(&po)` per-call 经 registry 重组装新实例（D22 单次实例，禁缓存复用）；`execute(ctx, &tool, args)` 成功返回 (Value, ToolCallEntry)，entry.call_id 为本方法生成的真实 call_id；旧 `call_manual` 已改名为 `execute` | `#L28-L45` |
| dao/tool_call/impl.rs Builtin 实现 | LLM 内置工具 4 件套 | shell_exec 子进程 spawn + 超时 kill；fs_read/fs_write 路径安全校验（canonicalize 前缀对比 data_dir）；http_fetch reqwest Client 不带默认系统 proxy 防止 SSRF；所有 Builtin 工具都有独立单元测试 | `:L1-L120` |
| dao/tool_call/mcp.rs MCP 实现 | MCP 工具调用桥 | run_mcp server lookup 先 enabled check；server_name 必须在 McpServerDao.list_enabled 返回集合内；args JSON 校验 + mcp result stdout 转字符串 2000 字截断 | 见 mcp.rs |
| dao/mcp_server/mod.rs MCP 服务器注册表 | 服务器 CRUD + 描述符缓存 | create_mcp_server（command、args、env 变量加密存）/ update_enabled / get_descriptor（cache TTL=300s 防重复发 list_tools） | 见 McpServerDao trait |
| domain/finance/tool_provider.rs | 工具管理面 CRUD + 绑定管理 | ToolProviderManage：create/update/delete_tool（validate_tool_management_policy）+ bind_tool_to_agent（internal tag 绑定拒绝）+ list_agent_tools / sync_builtin_tools；原「协议路由 + tag 过滤」职责已上移 domain runtime（tool_execution.rs）与 hr/agent.rs | `:L13-L101` |
| domain/runtime/tool_execution.rs | domain 执行编排单点（D26） | `call_tool`：读凭据需求 → resolve_tool_credentials → 协议路由（Mcp→mcp_tool_dal / Builtin·Http→tool_dal）；`dispatch_manual_tool` Manual 转发；`call_manual_tool_for_agent` 手动授权（绑定/neural/已装包三来源 + control_mode 校验）；`tool_readiness` 数据驱动探测（TTL 30s） | `#L36-L101` / `#L162-L245` |
| models/tool.rs | CoreTool 生命周期契约 + 统一传参 | trait 增 `credential_requirements()`（默认空）+ `check(&mut self, resolved)`（默认空实现，D22）；`ToolExecutionRequest { tool, args, resolved }` 为 domain → DAL 唯一传参形态（tool 为 PO 载体） | `#L17-L96` |
| handlers/finance/tool/create_http_tool.rs HTTP 工具创建 Handler | method 白名单 + header 安全过滤 | create_form.method 白名单 enum 只枚举 GET/POST/PUT/PATCH/DELETE；headers 禁止携带 "Authorization"、"Cookie"（防止用户写死管理员 JWT 到工具里）；URL parse 校验是合法 HTTP/HTTPS | `:L1-L40` |
| ai-orz-macros register_handler_tool.rs | Handler 转神经工具宏 | `#[register_handler_tool(name="install_skill_pack", description="按 tag 安装技能包")]` 自动生成 CallableTool 的 name/description + 参数结构转 JSON Schema；宏展开 6 步见 handler-tool-registration-macro 设计文档 | 见宏过程定义 |

**章节来源**
- [tool_design.md:L30-L70](docs/archive/design-archive/tool_design.md#L30-L70)
- [impl.rs:L1-L120](src/service/dao/tool_call/impl.rs#L1-L120)
- [tool_provider.rs:L13-L101](src/service/domain/finance/tool_provider.rs#L13-L101)
- [tool_execution.rs:L36-L101](src/service/domain/runtime/tool_execution.rs#L36-L101)
- [tool_credential_requirement_design.md](docs/design/tool_credential_requirement_design.md)

---

## §3 架构约定与调用顺序图

本卡与 [共享工具凭据增强器卡](docs/wiki/knowledge/zh/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例.md) 构成 **工具调用路由 / 凭据注入编排** 互补视角（本卡管三层路由与执行原语、兄弟卡管凭据需求声明与注入链路）；同时与 [Agent 关联全景与工具技能分组装配卡](docs/wiki/knowledge/zh/Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配/Agent 关联全景与工具技能分组装配：三分组互斥去重 + 专业领域打包复用 + 按需装配.md) 构成 **运行时实际注入 / Agent 视角全景展示** 互补视角（本卡管三层调用架构与执行原语、兄弟卡管 Agent 详情页三分组按需装配）；按 AGENTS §2.1.3 Level 3 保留平行卡。

**完整 7 步调用链**（Agent 唤醒后想调 install_skill_pack 神经工具，2026-08-21 D26 入口统一后现状）：

```
1. Agent 思考 → 产出 ToolCall(tool_name="install_skill_pack", args={"tag":"memory"})
2. 唤醒期工具加载（hr/agent.rs）：agent.tool_bindings（DAO 查绑定表）
   + neural tag 免绑定追加（SQL 层 tag_filter）- internal 剔除
3. think_loop 按 tool.po.control_mode 分发（D26 入口统一）：
   Auto → RuntimeToolExecution::call_tool / Manual → dispatch_manual_tool
4. domain call_tool 凭据编排先行 → 协议路由：
   ├─ ToolRegistry::credential_requirements(&po) → resolve_tool_credentials
   │  → ToolExecutionRequest { tool, args, resolved }
   ├─ Mcp → mcp_tool_dal.call_tool(request)（per-operation 连接 + env_clear 白名单注入）
   └─ Builtin/Http → tool_dal.call_tool(request) → assemble_core_tool(po)
      → check(resolved) 注入单次实例 → ToolCallDao.execute(ctx, &tool, args)
5. 同步 Result<String, ToolCallError> 结果，记录 trace_id（entry.call_id 为真实 call_id）
6. → AOP 发布 tool.executed 事件
7. → ToolExecLogConsumer（写 messages.tool_result 表）+ ToolExecStatsConsumer（DuckDB ToolCall 统计表）
```

**协议选择铁律**（业务落工具时必须读）：
| 场景 | 选 Builtin | 选 HTTP | 选 MCP |
|------|-----------|--------|--------|
| 跟本地文件系统/进程交互 | ✅ shell_exec/fs_* | ❌ SSRF 风险 | ❌ |
| 固定第三方 REST API（GitHub、Jira、飞书） | ❌ 写死实现无法维护 | ✅ create_http_tool 模板化配置 | 若对方已提供 MCP Server 优先 |
| 对方生态有现成 MCP 适配器（browser_use、db-query） | ❌ | ❌ 还要写解析 | ✅ 直接接 McpServerDao |
| 想把 HTTP Handler 暴露给 Agent 当工具（install_skill_pack/send_to_agent） | ✅ register_handler_tool! 宏零代码 | ❌ | ❌ |

---

## §4 硬约束与回归红线（7 条）

1. **入口统一 D26：think_loop 禁直连 tool_dal / mcp_tool_dal**：所有 Agent 工具调用必须经 domain `RuntimeToolExecution::call_tool` / `dispatch_manual_tool` 单点编排——绕行直连 DAL 会整条漏凭据注入；`ToolExecutionRequest { tool, args, resolved }` 是 domain → DAL 唯一传参形态。Handler 管理面（CRUD/绑定）走 Finance `ToolProviderManage`（分层 AGENTS.md §3.1 红线）；单元测试用 mock 测 domain 编排不调真实 DAO。
2. **shell_exec 命令黑名单永不删**：`rm -rf /`、`sudo`、`su`、`ssh root@`、`chmod 777 /etc` 共 5 条硬匹配 + 子串匹配（黑名单列表在 impl.rs 顶部 const BLACKLIST）；新增命令须在 BLACKLIST 单元测试里补一条「匹配成功→返回错误」的测试。
3. **fs_read / fs_write 路径安全必校验**：`std::fs::canonicalize(path)?` 后用 `starts_with(data_dir)`，不能用简单 contains，防止 `/data_dir2/../../data_dir/file` 绕过；单元测试要覆盖 `..` 穿越场景（写入失败）。
4. **HTTP 工具 method 白名单双端校验**（前端 + DAO 层都要有）：只允许 GET/POST/PUT/PATCH/DELETE；禁止 CONNECT/TRACE（HTTP 走私）；创建/更新 HTTP 工具接口在 DAO 层再 enum 校验一次，不能信任前端 DTO 里传的 method 字符串（DTO 是 struct 不是 enum，防止绕过）。
5. **internal tag 工具不出现在 Agent 可用列表**：Agent 唤醒加载输出集合（hr/agent.rs tag 过滤后）中若包含 tag 为 "internal" 的工具 ID = fail；ToolProviderManage.bind_tool_to_agent 对 internal tag 绑定请求必须拒绝；前端"工具绑定"页面也同样过滤（双端一致）。
6. **register_handler_tool 宏生成的 CallableTool 参数名必须与 Handler fn 参数 1:1**：参数名错会导致 Agent 传的 args JSON 无法被 Handler 接收（400）；修改 Handler 参数列表后要运行对应集成测试的「工具 JSON Schema 生成」断言，否则 schema 与 fn 签名漂移。
7. **凭据实例单次性（D22）**：DAL per-call 经 `assemble_core_tool` 重组装新实例 + `check(resolved)` 注入——check 注入的实例禁止缓存复用（跨调用复用会串号）；带 requirements 的 stdio MCP server 禁止全局共享连接（per-operation 连接）。凭据需求声明 / 生产路由 / 敏感名拒绝等八条红线归平行卡「共享工具凭据增强器」§4 管辖，本卡只锁调用链侧不变式。
