---
kind: RAG 原子知识卡
name: 共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例
category: 基础设施 / 凭据体系
scope:
- src/pkg/credential/**
- src/pkg/tool_registry/*.rs
- src/service/domain/runtime/**
- common/src/models/identity_credentials.rs
- src/handlers/finance/tool/**
- src/handlers/finance/mcp_server/**
- frontend/src/components/credential*.rs
- frontend/src/components/create_http_tool.rs
source_files:
- common/src/models/identity_credentials.rs#L587-L795
- common/src/models/tool.rs#L29-L52
- src/pkg/credential/mod.rs#L22-L237
- src/pkg/credential/enhancer.rs#L26-L210
- src/pkg/tool_registry/mod.rs#L142-L160
- src/service/domain/runtime/tool_execution.rs#L36-L101
- src/service/domain/runtime/tool_execution.rs#L393-L450
- src/models/tool.rs#L17-L96
- src/pkg/tool_registry/http.rs#L113-L160
- docs/design/tool_credential_requirement_design.md
- docs/plan/共享工具凭据增强器落地.md
- docs/wiki/zh/content/基础设施/工具注册表/共享工具凭据增强器.md
- docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md
---

## §1 概述

**本卡角色**：共享工具凭据增强器的全链路知识卡。覆盖工具凭据消费的两层模型——配置层类型级需求声明（`CredentialRequirement { kind, platform, field?, enhancer?, binding }`，皆非敏感零凭据实例）+ 运行层编排注入（domain `resolve_tool_credentials` 生产路由二元化 → pkg `src/pkg/credential/` 纯值加工（解密 + 增强器包裹 canonical）→ `CoreTool::check` 注入单次实例 → call 内 binding 纯放置）。**定位：给 MCP Server / HTTP 工具接凭据、新增凭据类型或增强器、排查「共享工具以谁的身份执行」、排查凭据未注入 / 敏感名被拒 / Builtin 改配置被拒时读。**

- **根除问题源**：工具是组织共享的、凭据是用户私有的——共享配置里出现 credential_id 或原文等同公开私人凭据。MCP config 的 `env` / `headers` 字段已整体删除（D14 无兼容层）；HTTP 工具 headers/query 保留协议必需字段但配置期拒绝敏感名条目（D15）。
- **生产端二元化**（D17）：合法凭据生产者仅两个——user dal `find_default(kind, platform)`（链序：个人默认 > 个人活跃 > 组织默认 > 组织 public）与 lark dal `resolve_credentials_for_user`（渠道路径 + attributes{identity_mode}，D24）。取数上移 domain 编排层，pkg 与工具实例零数据访问，无 per-tool resolver / 无数据端口 / 无 OnceLock 注入注册（Gh / Tavily / Lark 三 resolver 已删，`service::init` 凭据注册清零）。
- **增强器模型**（D5/D7）：衍生行为（前缀拼接 / Basic 组装 / OAuth 刷新）建模为 pkg 增强器 trait；组合规则（D10）`注入值 = 显式增强器( canonical(凭据) )`；能力管控优于红线管控——OAuth 只装配 AccessToken（refresh_token / client_secret 物理不可达）、user_password 只装配 BasicAuth。
- **入口统一**（D26）：所有 Agent 工具调用经 domain `RuntimeToolExecution::call_tool` 单点编排（think_loop 的 Auto → `call_tool` / Manual → `dispatch_manual_tool`）；DAL `call_tool_by_id` 与 `execute_auto` / `execute_manual` 已删；`ToolExecutionRequest { tool, args, resolved }` 统一传参。
- **PO config 闭环**（D27/D28）：tavily 共享 config 兜底废除（`[tavily]` / `[browser]` 全局段删除）；CLI 型工具（browser / gh_cli / lark_cli）二进制名与行为参数全部进各自 PO config；readiness 数据驱动（domain `tool_readiness` 复用 `resolve_tool_credentials`，探测注册表消亡）——全局 config 零工具参数。

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| common/src/models/identity_credentials.rs | 契约单一事实源 + 校验单点 | `CredentialRequirement` / `CredentialEnhancerKind` / `CredentialBinding` / `CredentialRequirementScope` + `enhancer_supports` 矩阵 + `default_enhancer` + `is_sensitive_credential_name` 敏感名单点 + `validate_requirements` 六规则单点（含 `binding_allowed` / `binding_name` / `mcp_transport_scope` / `as_str`；前后端委托，双端零漂移）（D2/D5/D12 单点；`CredentialKind` 六变体与 `requires_platform` 同文件） | `#L587-L795` |
| common/src/models/tool.rs | 工具 config 校验单点 | `validate_builtin_tool_config`（command 非空 / 数字正整数 / 未知字段宽松 / 非对象通过）+ `is_supported_http_method`（GET/POST 大小写不敏感）；后端 handler 与前端表单共用 | `#L29-L52` |
| src/pkg/credential/mod.rs | pkg 纯值加工门面 | `ResolvedCredential`（enhance 代理 + canonical_value 查找链 detail→attributes→primary_secret）+ `decrypt_detail` 解密单点 + `resolve_requirements` 纯函数 + `validate_requirements` 校验包装（本体在 common 单点）+ `credential_missing_json` 引导（零数据访问零注入注册） | `#L22-L237` |
| src/pkg/credential/enhancer.rs | 增强器实现 | `CredentialEnhancer` trait + BearerToken / BasicAuth / AccessToken 三实现 + `OAuthTokenManager`（TTL 缓存命中剩余 >60s 直返；miss 则 SSRF 校验 → refresh → 提前 60s 过期写缓存；失败不缓存） | `#L26-L210` |
| src/pkg/tool_registry/mod.rs | registry 需求聚合 | `ToolRegistry::credential_requirements(&po)` 统一读取——Builtin 查工厂静态声明 / Mcp·Http 从 config 解析（domain 读需求唯一入口） | `#L142-L160` |
| src/service/domain/runtime/tool_execution.rs | domain 单点编排 | `call_tool`（读需求 → 取数加工 → 协议路由 / 引导）+ `dispatch_manual_tool`（Manual 转发，D26）+ `tool_readiness` 数据驱动（D28，TTL 30s）+ `resolve_tool_credentials` 生产路由二元化（D17，依赖经 RuntimeDomainImpl 字段注入禁全局单例） | `#L36-L101` / `#L271-L392` / `#L393-L450` |
| src/models/tool.rs | CoreTool 生命周期 + 统一传参 | trait 增 `credential_requirements()`（默认空）+ `check(&mut self, resolved)`（默认空实现）；`ToolExecutionRequest { tool: ToolPo, args, resolved }`（tool 为 PO 载体，实例由 DAL per-call 重组装；`ToolPo::cli_command()` CLI 型命令读取单点） | `#L17-L96` |
| src/pkg/tool_registry/http.rs | HTTP 工具 check 注入 | `HttpCoreTool` 从 config 读 requirements + `check` 存 header/query 注入值（叠加在模板渲染之后）；`validate_no_sensitive_template_keys` 敏感名拒绝（静态/模板同判，D15） | `#L113-L160` / `#L469-L500` |
| src/pkg/tool_registry/mcp.rs | MCP check 注入 + 连接隔离 | `McpCoreTool` requirements 从 server config + `check` 收集 Env 注入值；`connect_stdio_client` per-operation 连接 + `env_clear` 白名单注入（D23 结构性隔离） | 见 mcp.rs |
| src/pkg/tool_registry/{gh_cli,tavily_search,lark_cli}.rs | 内置三员工厂化 | 模块级 `credential_requirements()` 单点声明（工厂与实例同源，一致性测试锁定）：gh `[GithubToken]`、tavily `[TavilyKey]` + timeout_ms 缺省 15_000（D27）、lark 三条 Internal（app_id/app_secret/identity_mode，D4/D24/D25）+ create_po 默认 config（D28） | gh `#L165` / tavily `#L85,L139` / lark `#L117,L162` |
| src/handlers/finance/mcp_server/{create,update}_mcp_server.rs + tool/update_tool.rs | 配置期校验 + 工厂字段保护 | 创建/更新接 `validate_requirements`（scope 经 common `mcp_transport_scope` 单点推导）；Builtin 更新 diff 式 guard——工厂字段（name/description/protocol/control_mode/parameters_schema/tags）不可改，config/status 放行（校验本体在 common `validate_builtin_tool_config`） | update_tool `#L107-L119` |
| frontend/src/components/credential_form.rs + credential_requirements.rs + create_http_tool.rs | 前端预校验 + 展示（common 单点薄委托门面） | `validate_requirements_scoped` / `is_sensitive_name` / `binding_name` / `mcp_transport_scope` / `enhancer_to_value` 全部委托 common 同一实现 + `injection_value_preview` 注入值形态预览 + `recommended_binding_name` 惯用名建议；`CredentialRequirementsTable` 只读组件（MCP/工具详情复用）；headers/query 敏感名即时预检 + 方法白名单 common 单点 | form `#L69-L106` / table `#L39` / http `#L117` |
| **Wiki 长文**：docs/wiki/zh/content/基础设施/工具注册表/共享工具凭据增强器.md | 百科长文 | 10 节全链路详解（概念 / 契约 / 编排流程 / 九条红线 / 实现分析 / 扩展点 / 3 个故障排查路径：凭据未注入 / 敏感名被拒 / Builtin 改配置被拒） | 全文 |

**章节来源**
- [tool_credential_requirement_design.md:L36-L67](docs/design/tool_credential_requirement_design.md#L36-L67)
- [tool_execution.rs:L36-L101](src/service/domain/runtime/tool_execution.rs#L36-L101)
- [identity_credentials.rs:L587-L669](common/src/models/identity_credentials.rs#L587-L669)

## §3 架构约定与调用顺序图

本卡与 [工具系统三层调用架构卡](docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md) 构成 **凭据注入编排 / 工具调用路由** 互补视角（本卡管凭据需求声明与注入链路、兄弟卡管三层路由与执行原语）；按 AGENTS §2.1.3 Level 3 保留平行卡。

**运行时编排链**（domain 每次工具调用）：

```
domain RuntimeToolExecution::call_tool(ctx, tool, args)
  ├─ ① ToolRegistry::credential_requirements(&po)      ← Builtin 工厂静态声明 / Mcp·Http config 解析
  ├─ ② resolve_tool_credentials(ctx, requirements)     ← 生产路由二元化（D17）
  │      kind=LarkApp → lark dal resolve_credentials_for_user（渠道路径 + attributes）
  │      其余 kind   → user dal find_default(kind, platform)（链序：个人默认 > 个人活跃 > 组织默认 > 组织 public）
  ├─ ③ pkg::credential::resolve_requirements            ← 纯值加工：解密 → 增强器包裹 canonical（D10）
  ├─ ④ 任一未命中 → credential_missing_json 结构化引导（D19，Agent 可读自愈）
  └─ ⑤ 全命中 → dal.call_tool(ToolExecutionRequest { tool, args, resolved })
                  └─ DAL per-call 重组装实例 → CoreTool::check(resolved) → call 内 binding 纯放置
```

**binding ↔ 消费端矩阵**（配置期校验，前端预校验同源）：

| binding | 合法消费端 | 注入形态 |
|---------|-----------|---------|
| `Env { name }` | stdio MCP | 子进程环境变量（env_clear 白名单 + per-operation 连接隔离） |
| `Header { name }` | http MCP + HTTP 工具 | HTTP 请求头（模板渲染之后叠加） |
| `Query { name }` | HTTP 工具 | URL 查询参数（模板渲染之后叠加） |
| `Internal { field }` | Builtin 内置工具 | 工具实例字段（check 内 match 收集） |

**supports 矩阵 v1**（前后端一致）：BearerToken → generic_token + oauth；BasicAuth → user_password；AccessToken → oauth；专用 kind（lark_app / github_token / tavily_key）零支持。默认装配：oauth → AccessToken、user_password → BasicAuth、单值 kind 无。canonical 查找链：detail 字段 → attributes 派生属性（D24）→ primary_secret。

**组合规则**（D10）：`注入值 = 显式增强器( canonical(凭据) )`——oauth + BearerToken 注入 `"Bearer " + access_token`；user_password 不选增强器注入 `"Basic " + base64(user:pass)`；显式选择默认增强器幂等等价于不选（D11）。

## §4 硬约束与回归红线（8 条）

1. **共享配置禁凭据实例**：落库 config 禁 credential_id 与凭据原文；MCP config 零 env / headers 字段（整体删除无兼容层）；`credential_requirements` 仅含匹配键 + 增强器 + 注入点（非敏感可直接展示）；tavily 共享 config 兜底已废（D27）——凭据无任何非凭证库来源。
2. **敏感 header/query 名拒绝**：HTTP 工具 headers/query 命中 `is_sensitive_header`（含连字符归一，静态值与 `{{args.*}}` 模板同判）一律配置期报错；敏感注入唯一合法路径是 Header / Query binding 的 requirements；规则本体单点 common `is_sensitive_credential_name`，前后端均委托（双端零漂移，改一处即生效）。
3. **binding ↔ scope 矩阵强校验**：Env→stdio MCP、Header→http MCP + HTTP 工具、Query→HTTP 工具、Internal→Builtin；跨协议声明配置期拒绝；`validate_requirements` 六规则（矩阵 / 注入名非空 / platform↔kind / field↔enhancer 互斥 / supports / 三元组去重）本体在 common 单点，前后端同一实现。
4. **D22 单次实例**：凭据注入值与用户维度是工具实例状态——每次调用 create → check → call，**check 注入的实例禁止缓存复用**（跨调用复用会串号）；带 requirements 的 stdio server 禁止全局共享连接（per-operation 连接 + 实例级注入）。
5. **入口统一（D26）**：think_loop 禁直连 tool_dal / mcp_tool_dal——所有 Agent 工具调用必须经 domain `RuntimeToolExecution::call_tool` 单点编排（绕行直连 DAL 会整条漏凭据注入）；`ToolExecutionRequest { tool, args, resolved }` 是 domain → DAL 唯一传参形态。
6. **工厂所有权字段保护**：Builtin 工具更新仅 config / status 可改——name / description / protocol / control_mode / parameters_schema / tags 是工厂所有不可改；config 变化不触发向量重索引。
7. **前端预校验与后端同源**：`validate_requirements_scoped`（六规则）/ 敏感名 / 注入名 / transport→scope 映射 / 方法白名单 / Builtin config 校验全部双端委托 common 单点（identity_credentials.rs + tool.rs）——规则改动只改 common 一处，前端模块仅薄委托门面。
8. **生产端二元化 + PO config 闭环**：dal 生产凭据仅 user dal `find_default` 与 lark dal `resolve_credentials_for_user` 两处，禁止第三处自建取数（per-tool resolver / CredentialDataProvider / OnceLock 注册全仓零残留）；全局 config 零工具参数——工具行为参数与 CLI 命令唯一归宿是工具自身 PO config（CLI 型 `po.cli_command()` 不变式），新工具接入禁止再向全局 config 添加工具段。
