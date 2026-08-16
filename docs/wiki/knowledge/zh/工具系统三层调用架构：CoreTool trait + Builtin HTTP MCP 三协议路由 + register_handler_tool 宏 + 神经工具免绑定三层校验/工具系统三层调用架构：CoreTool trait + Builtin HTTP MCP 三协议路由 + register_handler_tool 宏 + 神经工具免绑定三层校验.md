---
kind: RAG 原子知识卡
name: 工具系统三层调用架构：CoreTool trait + Builtin/HTTP/MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验
category: 基础设施 / 工具注册表
scope:
  - "src/service/dao/tool/**"
  - "src/service/dao/tool_call/**"
  - "src/service/dao/mcp_tool/**"
  - "src/service/dao/mcp_server/**"
  - "src/service/dal/tool*.rs"
  - "src/service/domain/finance/tool_provider.rs"
  - "ai-orz-macros/src/register_handler_tool/**"
  - "src/handlers/**/*tool*.rs"
source_files:
  - src/service/dao/tool/mod.rs (ToolDao trait：CRUD + Vectorizable skill 向量搜索集成 + tool_packs 按 tag 分组)
  - src/service/dao/tool_call/mod.rs (ToolCallDao trait：统一 execute 原语；Builtin impl / MCP impl 双实现)
  - src/service/dao/tool_call/impl.rs#L1-L120 (Builtin ToolCall 实现：shell_exec / fs_read / fs_write / http_fetch 四个内置工具 + 参数校验白名单 + request_context cross_boundary)
  - src/service/dao/tool_call/mcp.rs (MCP ToolCall 实现：run_mcp server_name + tool_name + args JSON；server 注册表动态加载)
  - src/service/dao/mcp_server/mod.rs (McpServerDao：server CRUD + enabled/disabled + descriptor JSON 缓存；连接健康检查)
  - src/service/domain/finance/tool_provider.rs#L1-L80 (Finance Domain ToolProvider：Builtin/HTTP/MCP 三协议路由 + tag=neural 免绑定 / tag=internal 加载过滤 两层)
  - src/handlers/finance/tool/create_http_tool.rs#L1-L40 (HTTP 工具创建：create_form method 白名单 GET/POST + 端点 URL 校验 + header 安全注入)
  - ai-orz-macros/src/register_handler_tool.rs (register_handler_tool! 过程宏：把 Handler 函数包装成 register_tool CallableTool；参数结构自动→JSON Schema)
  - docs/archive/design-archive/tool_design.md（§三层调用架构职责表 + §工具包 tag 免绑定三层校验 + §CoreTool trait 扁平化演进）
  - docs/archive/design-archive/mcp_tool_design.md（§MCP 服务器启动幂等 + §描述符 JSON 缓存 + §run_mcp 参数 schema）
  - docs/archive/design-archive/builtins_http_tool_design.md（§create_form method 白名单 GET/POST + §header 禁止 Authorization Cookie 穿透）
  - docs/archive/design-archive/generic_builtin_tools_design.md（§4 个通用内置工具参数规范 + §文件读写范围校验 §shell_exec 命令黑名单）
  - docs/archive/design-archive/handler-tool-registration-macro.md（§register_handler_tool! 6 步宏展开 + §CallableTool trait 契约）
  - docs/archive/plan-archive/进程管理与shell_exec修复.md（§shell_exec 白名单 + 进程双暴露 shell_list HTTP+ToolCall）
  - docs/archive/plan-archive/前端工具与进程管理.md（§前端工具管理页面：HTTP/MCP/Builtin 三 Tab 维护）
  - docs/archive/plan-archive/移除rig依赖与向量存储后端解耦.md（§ToolCallDao.call_manual→execute 重命名 + BrainDal→CortexDaoRegistry 扁平化）
  - docs/wiki/zh/content/功能模块/工具生态系统/工具生态系统.md（工具系统全景：注册→分组→执行→统计四步用户故事）
  - docs/wiki/zh/content/项目概述/核心功能特性/统一工具调用架构/统一工具调用架构.md（Builtin/HTTP/MCP 三协议路由说明 + 工具包 tag 分组机制）
  - docs/wiki/zh/content/功能模块/工具生态系统/工具注册与发现.md（CoreTool trait 契约 + 注册表单例加载流程）
  - docs/wiki/zh/content/基础设施/工具注册表/工具注册表.md（工具注册表全景 + 内置工具系统/MCP 工具/HTTP 工具三个子板块入口）
  - docs/wiki/zh/content/前端应用/页面模块/Finance 管理页面/工具管理/工具管理.md（前端工具管理页面：三 Tab 视图 + create_http_tool 表单）
  - 【平行卡 1】docs/wiki/knowledge/zh/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断/Skill 系统增强：5 套 TEMPLATE 预置包 + install_skill_pack 幂等 Tag 分发 + Agent 入职绑定 + Prompt Token 熔断.md（技能 install_skill_pack 完成后，Agent 再通过 ToolProvider 安装工具包 tag）
  - 【平行卡 2】docs/wiki/knowledge/zh/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册.md（ToolExecLogConsumer + ToolExecStatsConsumer 两个业务消费者，消费 tool.executed 事件写日志+打统计）
---

## §1 概述

**本卡角色**：工具系统的整体架构知识卡。覆盖 ToolDao（PO/CRUD/向量搜索）/ ToolCallDao（统一 execute 双实现 Builtin+MCP）/ McpServerDao（MCP 服务器注册表）三层、Finance Domain `ToolProvider` 的三协议路由 + `neural`/`internal` tag 加载过滤规则、`register_handler_tool!` 宏把 Handler 函数暴成神经工具、HTTP 工具 create_form method 白名单、shell_exec/fs 命令黑名单。**定位：新增工具协议、排查工具调用返回权限错误、调试工具注册宏失败时读。**

- **三层调用架构严格单向**：Handler → Finance ToolProvider（路由+tag过滤）→ ToolCallDao.execute（统一原语）→ 分 Builtin impl.rs（LLM神经+系统工具）/ MCP impl.rs（run_mcp）/ HTTP（HTTP工具按存储配置发起请求）。跨层禁止：Handler 不能直接 use dao/tool_call，必须走 ToolProvider。
- **tag 加载过滤三层免绑定规则**（2026-07-12 增强，工具设计文档 §工具包机制）：① `tag = "neural"` 的工具在 ToolProvider.load_agent_tools() 时不用 agent.tool_bindings 就自动加入「神经工具集」（核心思考工具如记忆写入沉淀，避免入职漏绑）；② `tag = "internal"` 的工具 load 时从 Agent 可绑定列表过滤掉，只在 ToolDal.execute_manual（前端操作同步调用）通过 registry.create_tool 调（用于后台任务/备份恢复等用户直操作工具，不给 Agent 直接用）；③ 普通工具按 tool_packs.tag 分组（"tool_memory", "tool_file", "tool_project"），Agent 入职 install_skill_pack 后再 bind 对应 tag。
- **4 个通用内置工具参数规范**（generic_builtin_tools_design.md）：① `shell_exec(command, cwd)` — 命令黑名单 rm -rf /、sudo、；② `fs_read(path)` — 必须在 data_dir 下，禁止 `..` 穿越；③ `fs_write(path, content)` — 同路径安全校验，先校验再分块写；④ `http_fetch(url, method, headers, body)` — 默认禁止访问 `169.254.169.254`（元数据服务）+ `localhost:*` 本地端口，method 只许 GET/POST。HTTP 工具创建接口（create_http_tool.rs）的白名单在 DAO 层再双校验一次，防止前端绕过。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| dao/tool/mod.rs ToolDao trait | 工具 CRUD + 分组 | create_tool / update_tool / query（tags_contains、status、category）/ search（FTS5+向量混合）；Vectorizable 对应 vss_tools | 见 ToolDao trait |
| dao/tool_call/mod.rs ToolCallDao trait | 统一执行原语 | 旧 `call_manual` 已改名为 `execute`（工具设计文档 §Rig 移除+命名清理）；参数 tool_def + tool_call_args + ctx | 见 trait 签名 |
| dao/tool_call/impl.rs Builtin 实现 | LLM 内置工具 4 件套 | shell_exec 子进程 spawn + 超时 kill；fs_read/fs_write 路径安全校验（canonicalize 前缀对比 data_dir）；http_fetch reqwest Client 不带默认系统 proxy 防止 SSRF；所有 Builtin 工具都有独立单元测试 | `:L1-L120` |
| dao/tool_call/mcp.rs MCP 实现 | MCP 工具调用桥 | run_mcp server lookup 先 enabled check；server_name 必须在 McpServerDao.list_enabled 返回集合内；args JSON 校验 + mcp result stdout 转字符串 2000 字截断 | 见 mcp.rs |
| dao/mcp_server/mod.rs MCP 服务器注册表 | 服务器 CRUD + 描述符缓存 | create_mcp_server（command、args、env 变量加密存）/ update_enabled / get_descriptor（cache TTL=300s 防重复发 list_tools） | 见 McpServerDao trait |
| domain/finance/tool_provider.rs | 协议路由 + tag 过滤 | ToolProvider.get_tools_for_agent(ctx, agent_id)：先 agent.tool_bindings → 再追加 neural tag 工具 → 再剔除 internal 工具；分三类 Builtin/HTTP/MCP 各自 from(tool_def) → CallableTool 对象 | `:L1-L80` |
| handlers/finance/tool/create_http_tool.rs HTTP 工具创建 Handler | method 白名单 + header 安全过滤 | create_form.method 白名单 enum 只枚举 GET/POST/PUT/PATCH/DELETE；headers 禁止携带 "Authorization"、"Cookie"（防止用户写死管理员 JWT 到工具里）；URL parse 校验是合法 HTTP/HTTPS | `:L1-L40` |
| ai-orz-macros register_handler_tool.rs | Handler 转神经工具宏 | `#[register_handler_tool(name="install_skill_pack", description="按 tag 安装技能包")]` 自动生成 CallableTool 的 name/description + 参数结构转 JSON Schema；宏展开 6 步见 handler-tool-registration-macro 设计文档 | 见宏过程定义 |

**章节来源**
- [tool_design.md:L30-L70](docs/archive/design-archive/tool_design.md#L30-L70)
- [impl.rs:L1-L120](src/service/dao/tool_call/impl.rs#L1-L120)
- [tool_provider.rs:L1-L80](src/service/domain/finance/tool_provider.rs#L1-L80)

---

## §3 架构约定与调用顺序图

**完整 7 步调用链**（Agent 唤醒后想调 install_skill_pack 神经工具）：

```
1. Agent 思考 → 产出 ToolCall(tool_name="install_skill_pack", args={"tag":"memory"})
2. Runtime.tool_execution 模块 → ToolProvider.get_tools_for_agent(ctx, agent_id)
   ├─ agent.tool_bindings (DAO 查绑定表)
   ├─ + ToolDao.query(tags_contains["neural"]) (追加免绑定工具)
   └─ .filter(|t| !t.tags.contains("internal")) (剔除不给 Agent 用的工具)
3. 匹配 tool_name → 拿到 CallableTool instance
4. 区分 tool.protocol_kind：
   ├─ Builtin → ToolCallDao.execute(ctx, tool_def, args) → dao/tool_call/impl.rs
   ├─ HTTP → 根据 HTTP tool_def.url/method/headers 拼 reqwest::RequestBuilder → send
   └─ MCP → McpServerDao.get(server_name) 拿 enabled → run_mcp server 地址 + args
5. 同步 Result<String, ToolCallError> 结果，记录 trace_id
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

## §4 硬约束与回归红线（6 条）

1. **禁止 Handler 直接 use dao::tool_call::execute**：所有工具调用必须走 Finance::ToolProvider（分层 AGENTS.md §3.1 红线）；Handler 只做参数校验、ctx 构造、调用 ToolProvider → return ApiResponse。单元测试用 mock 测 Provider 不调真实 DAO。
2. **shell_exec 命令黑名单永不删**：`rm -rf /`、`sudo`、`su`、`ssh root@`、`chmod 777 /etc` 共 5 条硬匹配 + 子串匹配（黑名单列表在 impl.rs 顶部 const BLACKLIST）；新增命令须在 BLACKLIST 单元测试里补一条「匹配成功→返回错误」的测试。
3. **fs_read / fs_write 路径安全必校验**：`std::fs::canonicalize(path)?` 后用 `starts_with(data_dir)`，不能用简单 contains，防止 `/data_dir2/../../data_dir/file` 绕过；单元测试要覆盖 `..` 穿越场景（写入失败）。
4. **HTTP 工具 method 白名单双端校验**（前端 + DAO 层都要有）：只允许 GET/POST/PUT/PATCH/DELETE；禁止 CONNECT/TRACE（HTTP 走私）；创建/更新 HTTP 工具接口在 DAO 层再 enum 校验一次，不能信任前端 DTO 里传的 method 字符串（DTO 是 struct 不是 enum，防止绕过）。
5. **internal tag 工具不出现在 Agent 可用列表**：ToolProvider.get_tools_for_agent 输出集合中若包含 tag 为 "internal" 的工具 ID = fail；前端"工具绑定"页面也同样过滤（双端一致）。
6. **register_handler_tool 宏生成的 CallableTool 参数名必须与 Handler fn 参数 1:1**：参数名错会导致 Agent 传的 args JSON 无法被 Handler 接收（400）；修改 Handler 参数列表后要运行对应集成测试的「工具 JSON Schema 生成」断言，否则 schema 与 fn 签名漂移。
