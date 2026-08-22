# 共享工具凭据增强器设计（MCP / HTTP 工具接入身份凭证体系）

> 🎯 **定位**：① Design 决策快照——记录「共享工具凭据从配置内嵌原文改为类型级需求声明 + 凭据增强器模型（编排取数 → pkg 纯函数加工 → check 注入单次实例 → binding 纯放置）」的动机、(kind, platform) 匹配键、增强器 trait 契约与默认装配规则、显式增强器组合语义、supports 矩阵、OAuth 生命周期内聚、MCP env/headers 字段整体移除与 HTTP 敏感名拒绝策略
> 状态：定稿 v1.9（2026-08-21，经需求声明 → 凭据模板变量 → 凭据增强器 → 编排注入 → Lark 统一 → 入口统一 → tavily 兜底废除 → 工具参数闭环八轮迭代收敛；v1.4 取数上移编排层 + D22 实例生命周期 + D23 连接隔离；v1.5 **Lark 统一入模型**——生产端二元化（user dal / lark dal，D17），dal 产出的不只是凭据还有派生属性（D24 attributes），内置工具消费形态补 Internal binding（D25）；lark_cli 工厂化，per-tool resolver trait 三清零，`service::init` 凭据注册项全部删除；v1.6 **pkg 加工模块独立落位 `src/pkg/credential/`**——凭据域纯值加工不隶属 tool_registry，依赖方向 tool_registry → credential 单向，未来非工具消费方（渠道出站认证等）可直接引用；v1.7 **工具调用入口统一（D26）**——Agent 工具调用全部经 domain RuntimeToolExecution 单点编排（think_loop 切换 + 双删 DAL call_tool_by_id + ToolExecutionRequest 统一传参），顺带修复 Auto 模式 MCP 工具主循环不可执行；命名规避 models/cortex_types.rs 既有 ToolCallRequest（LLM 调用描述符），与 models/tool.rs 既有 ToolExecutionResult 成对；v1.8 **tavily 共享 config 兜底废除（D27）**——全局 config `[tavily]` 整段删除，TavilyKey 纯单轨与 GithubToken 一致，timeout_ms 挪 tavily 工具 PO config 带默认值，生产路由纯二元零兜底；v1.9 **工具参数 PO config 闭环 + readiness 数据驱动（D28）**——browser 三字段（command/timeout_ms/max_output_bytes）与 gh_cli/lark_cli 二进制名全部进各自 PO config，`BrowserConfig` 段删除全局 config 零工具参数，readiness 探测注册表消亡改为 domain `tool_readiness` 数据驱动（复用 resolve_tool_credentials），CLI 型统一 po.config.command 不变式）
> 触发场景：实现/修改 MCP Server 或 HTTP 工具的凭据注入、新增凭据类型（generic_token / oauth / user_password）、新增或修改凭据增强器、修改 supports 矩阵或敏感配置校验规则、排查「共享工具以谁的身份执行」问题时打开
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — §3 分层架构、§2.1 文档规范
> - [用户身份凭证独立表设计](user_credentials_design.md) — 凭据表结构 / 解析链 / 可见性语义的底层设计；本设计扩展其 D15/D16（默认唯一索引加 platform 维度）与消费侧
> - [网络搜索与浏览器工具设计](web_search_and_browser_tools_design.md) — 双轨授权先例（tavily 个人 key + 共享 config 兜底 + api_key_missing 引导）
> - [共享工具凭据增强器落地](../plan/共享工具凭据增强器落地.md) — 落地流程与执行结果快照

---

## 一、设计目标

### 1.1 设计哲学：凭据增强器

工具是组织共享的，凭据是用户私有的——这对矛盾决定了共享工具配置里**永远不能出现凭据实例**（credential_id 或原文）。绑定私人凭据 ID 等同把私人凭据公开：任何人调用该工具，都会以凭据主人的身份对外执行。

本设计把共享工具的凭据消费拆为两层：

- **配置层（静态声明）**：requirement 只声明匹配键 `(kind, platform, field?)`、增强器类型 `enhancer?`、注入点 `binding`——三者皆非敏感，不引用任何凭据实例；**增强器在工具配置中一并选定**（与最终使用位置绑定，配置即闭环）；
- **运行层（编排注入 + 实例消费）**：service 编排层读 PO 需求声明 → user dal `find_default` 取数 → pkg 纯函数加工（解密 + 增强器/canonical）→ 工具工厂实例化并经 check 生命周期注入——工具实例单次使用，凭据是对象状态（D22）；工具 call 内只做 binding 纯放置，零数据访问。

原始凭据获取到后的**衍生行为**（前缀拼接、多字段组装、OAuth 刷新换短期 token）统一建模为**凭据增强器**：pkg 中实现并封装好的行为规范，requirement 声明增强器类型（不声明取规范可用值），加工结果经 check 注入工具实例。与 lark_cli / gh_cli / tavily_search 的「工具无状态共享，身份在调用时注入」精神同构——本设计将其泛化为「DB 注册共享工具可配置 + 内置工具统一工厂化」。

增强器模型相对自由变换管道（前稿方案，已废弃）的结构优势：

1. **共享配置零自由模板**：不再有 `{{field}}` 模板字符串进共享配置，敏感字段被模板引用的风险与管道中段语义歧义一并消解；多字段拼接收敛为特定增强器类型（BasicAuth），内部参数在执行时由凭据对象代理填充；
2. **能力管控优于红线管控**：OAuth 凭据只装配 AccessToken 默认增强器，refresh_token / client_secret 是**无提取路径**（物理不可达），而非「规定不许」；
3. **工具配置与凭据形态解耦**：工具配置作者无需预知用户绑 PAT（generic_token）还是 OAuth 凭据——requirement 不声明 enhancer 时取规范可用值，两种凭据形态对工具透明。

现状 MCP 配置的 `env` / `headers` 字段（[McpServerConfig](../../src/models/mcp_server.rs#L101-L123)）允许凭据原文直接落库到共享配置，是本设计要根除的问题源；处置方式为**字段整体移除**，所有 env / header 注入唯一合法路径是 `credential_requirements`。

### 1.2 关键决策表

| # | 问题 | 方案 | 原因 |
|---|------|------|------|
| D1 | 共享工具如何关联凭据 | **类型级需求声明**：配置只存匹配键 + 增强器 + 注入点，运行时按 `ctx.user_id` 经 `find_default` 解析链匹配注入；配置中禁止 credential_id 与敏感原文 | 工具 org 共享而凭据个人私有；绑定实例 ID 等同公开私人凭据（见 §1.1） |
| D2 | 凭据类型如何扩展 | 新增 `CredentialKind::GenericToken`（detail `{ token }`）/ `OAuth`（§2.5.1）/ `UserPassword`（detail `{ username, password }`）；需求声明可引用任意既有 kind | generic_token 承载平台长尾（Notion/Linear PAT…）；user_password 是 BasicAuth 增强器的双字段宿主；避免按接入点分裂 kind（McpToken/HttpToken 冗余） |
| D3 | 同 kind 多平台凭据如何区分 | **匹配键升级为 `(kind, platform)` 二元组**：凭据表加 `platform TEXT NULL` 列（generic_token / oauth / user_password 必填，专用 kind 留空）；requirement 声明 `platform: Option<String>` 精确匹配（generic 类 kind 必填）；显示名（label）永不参与匹配 | 结构化约定可索引可归一；纯名称匹配改名即断链且无法归一；细分 kind 枚举爆炸、新平台要改 common + DB 值域 + 迁移 |
| D4 | 一条凭据多字段注入（app_id + app_secret 双 env） | requirement 带可选 `field: Option<String>`：None = 规范可用值，Some = detail 指定字段；同一凭据可被多个 requirement 以不同 field 注入 | lark 类双字段是现实场景；与 enhancer 互斥（D8），契约一次定形 |
| D5 | 原始凭据的衍生行为（前缀/拼接/派生）如何建模 | **凭据增强器 trait**（pkg 实现、自封装）：`CredentialEnhancer { kind, supports(kind), enhance(credential) }`；凭据对象代理执行——`enhance(kind)` 直接取结果，多字段拼接的内部参数从 detail 填充完毕；v1 内置 BearerToken / BasicAuth / AccessToken | 衍生行为是「为了方便使用的行为规范」，pkg 单点实现可校验可测试；凭据对象代理让工具侧零感知实现细节 |
| D6 | requirement 不声明 enhancer 时取什么 | **规范可用值（canonical value）**：OAuth 凭据 → 默认增强器 AccessToken（access_token）；user_password → 默认增强器 BasicAuth（`"Basic " + base64(user:pass)`）；generic_token → token 原文；专用 kind → `primary_secret()` / field 指定字段 | 工具配置作者不预知用户绑 PAT 还是 OAuth（Linear 两者皆可）；canonical 语义吸收凭据形态差异，工具配置与凭据解耦；两种复合形态凭据对称拥有默认增强器 |
| D7 | 敏感凭据的管控方式 | **默认增强器能力模型**：OAuth 凭据构造时只装配 AccessToken（refresh_token / client_secret 无任何提取路径）；user_password 构造时装配 BasicAuth（username / password 原文无默认注入路径）；generic_token 等单值 kind 不主动装配默认增强器 | 能力管控强于规则管控；敏感边界收敛在装配规则单点；对称模型：复合形态凭据的规范值必经默认增强器产出 |
| D8 | 增强器与 field 的关系 | **互斥**：requirement 要么声明 field 提取原始字段，要么声明 enhancer 取派生值，两者同显式配置期拒绝 | v1 无「增强器作用在指定字段上」的组合场景；互斥使校验与语义最简（组合需求出现时再放开，§五.8） |
| D9 | binding 的职责边界 | **纯放置**：`Env { name }` / `Header { name }` / `Query { name }` 只指注入位置；Bearer / Basic 等前缀拼接归增强器；自定义前缀留作未来 Prefix 增强器（§五.2） | 职责正交：增强器管「值怎么来」，binding 管「值放哪」；binding 混入拼装职责会重新模糊边界 |
| D10 | 显式增强器与默认增强器的组合语义 | **包裹规则**：`注入值 = 显式增强器( canonical(凭据) )`——显式增强器包裹规范值，默认增强器是规范值的生产者，链内隐式参与；物理上不存在同一增强器执行两次 | 消解「增强器不能重复」的顾虑：默认与显式是 wrap 关系非并列关系；配置层去重维持 `(kind, platform, 注入点名)` 三元组——同一凭据、不同增强器、注入不同位置合法且有用（access_token 进 env + Bearer 头进 header，两条 requirement） |
| D11 | 显式选择该凭据的默认增强器（如 oauth + AccessToken） | **幂等允许**：前端不提供该选项（避免困惑），后端接受且与不选等价（同一注入值）；无额外校验分支 | 宽松幂等优于强校验：语义本就等价，拒绝徒增规则与前端状态复杂度 |
| D12 | 增强器对各凭据类型的可用性 | **supports 矩阵 v1（精确表）**：BearerToken → generic_token + oauth；BasicAuth → user_password；AccessToken → oauth；**专用 kind（lark_app / github_token / tavily_key）零支持，前后端一致拒绝**；真实需求出现再放开（§五.2） | 两端同一套规则（前端禁用下拉 + 后端 validate 拒绝），配置数据干净；GitHub 类 Bearer 需求真实出现时放开矩阵即一行改动 |
| D13 | OAuth refresh→access 生命周期 | **内聚在 AccessToken 增强器**（pkg）：`get_access_token` 内存 TTL 缓存（命中且剩余 > 60s 直返；miss 则校验 endpoint SSRF → POST refresh_token → 解析 access_token/expires_in → 提前 60s 过期写缓存）；刷新失败不缓存失败结果 | 生命周期单点收敛；缓存避免每次调用都刷；提前过期防边界穿透；工具不感知刷新只消费结果 |
| D14 | 存量 env/headers 字段处置（MCP） | **字段整体移除，无兼容层**：`McpServerConfig` / `McpServerConfigDto` 删除 `env`、`headers`；stdio 子进程环境变量唯一来源是 Env binding 注入 | 测试阶段无生产包袱（沿 [user_credentials_design D10](user_credentials_design.md) 同款决策）；「字段不存在」比「字段存在但校验拒绝」的保证强一个量级 |
| D15 | HTTP 工具 headers/query 处置 | **保留字段（协议必需：Content-Type / Accept 等），配置期拒绝敏感名条目**——静态值与 `{{args.*}}` 模板同禁，命中 [is_sensitive_header](../../src/pkg/tool_registry/tool_security.rs#L173-L182) 判定一律报错 | HTTP 工具的 headers/query 一半是协议语义一半是凭据载体，无法一刀切；敏感注入唯一路径是 Header / Query binding |
| D16 | 运行时如何拿到调用者身份 | **编排层持有 ctx 取数，实例携带用户维度**：service 编排（domain）以 ctx.user_id 调 user dal 取数；工具实例经 check 持凭据值与 user 维度（stdio 连接隔离键，D23）；CoreTool::call(ctx, args) 的 ctx 保留给日志/审计，凭据路径不依赖它 | 取数发生在 service 层（本就有 ctx，正路调用 user dal）；pkg 零数据访问；连接隔离键随实例走 |
| D17 | 「找凭据」的归属与 pkg 的数据访问 | **pkg 零数据访问，取数上移编排层，生产端二元化**（用户决策 v1.4/v1.5）：三方分工——dal 生产凭据（**两个合法生产者**：user dal 凭据增删改查 `find_default(kind, platform)`；lark dal `resolve_credentials_for_user` 渠道路径产出 LarkApp 凭据 + 派生属性），pkg 凭据模块纯值加工（解密 / 增强器 / canonical / OAuth 刷新 / 校验），工具实例声明需求 + 消费值；编排链：`读 requirements → domain 生产路由（LarkApp → lark dal，其余 → user dal）→ pkg 加工 → 工厂 create + check(credentials) → call`；**无 per-tool resolver trait、无 CredentialDataProvider 端口、无 OnceLock 注入注册**（`service::init` 凭据注册三行全删——Gh/Tavily/Lark）；gh_cli / tavily / lark_cli 统一工厂化（D22）：静态声明 requirements，编排层统一取数注入（tavily 共享 config 兜底随 D27 废除，纯 user dal 单轨） | 编排层本就有 ctx 与合法 dal 访问权，端口反转依赖是绕远路；Lark「渠道复合语义」实为生产端逻辑（凭据选择内核同为 find_default，仅路径经渠道表 + mode 派生），消费端无特殊性，统一无障碍；工厂注册表现状已是 per-request create，零基础设施新增 |
| D18 | tools/list 同步用什么身份 | 操作者 ctx；声明了 requirements 但解析不到 → api_key_missing 结构化引导（绑个人凭据或设组织默认） | 同步是管理动作，与调用走同一解析链；所有 requirements 均为必选（YAGNI） |
| D19 | 缺凭据 / 解析失败的用户体验 | 统一复用 `api_key_missing_json` 结构化引导（error_code + 双路径 hint）；错误不回显 secret；MCP 底层错误继续走 `map_mcp_tool_error` 脱敏 | 与 tavily 引导同构，Agent 可读引导自愈 |
| D20 | platform 维度对解析链/唯一索引的波及 | `UserCredentialQuery` / `find_default` 增加 platform 过滤参数；默认唯一索引升级为 `(user_id, kind, platform)` / `(org_id, kind, platform)`（扩展 [user_credentials_design D15/D16](user_credentials_design.md)） | 匹配键变了，链序（个人默认 > 个人活跃 > 组织默认 > 组织 public）与软删除等语义原样保留，只加维度 |
| D21 | 非敏感 env 配置需求（NODE_ENV 等） | v1 不支持；真实需求出现再评估显式白名单（见 §五） | 为罕见需求保留双通道（静态 + 凭据）违背「唯一合法路径」原则 |
| D22 | 工具实例生命周期（凭据注入通道） | **CoreTool 增生命周期方法，实例单次使用**（用户决策 v1.4）：trait 增 `credential_requirements()`（共享工具从实例 config 读；内置工具静态声明）与 `check(&mut self, resolved)`（校验声明与注入匹配 + 存入对象字段）；编排层每次调用 `create 实例 → check 注入 → call`，实例不复用（凭据/用户维度是对象状态，跨调用复用会串号）；**内置工具三员统一工厂化**（gh_cli / tavily / lark_cli，v1.5）；[ToolRegistry](../../src/pkg/tool_registry/mod.rs#L45-L60) 现状已是工厂 + per-request create，零基础设施新增 | 对象存值天然承载上下文性质数据；单次实例免除并发/复用污染；备选「call 增 options 传参」被否——参数列表膨胀且状态散落 |
| D23 | 声明凭据需求的 stdio MCP 连接隔离 | **连接缓存键升级 `(server_id, user_id)`**：env 是进程级的，A 用户 token 不能进 B 用户进程——带 requirements 的 server 每用户各自子进程；无 requirements 的 server 保持全局共享连接（现状键 server_id） | 隔离凭据串号风险；无凭据 server 不付出进程膨胀代价；连接管理收敛在 McpClientRuntime 单点 |
| D24 | dal 派生属性（identity_mode 等）如何进模型 | **`FetchedCredential` / `ResolvedCredential` 增 `attributes: BTreeMap<String, String>`**（v1.5）：dal 生产凭据时可附带派生属性（lark 的 identity_mode 由 `identity_mode_of(channel)` 派生）；`canonical_value(field)` 查找链：detail 字段 → attributes → primary_secret 兜底；user dal 生产路径 attributes 为空集 | 派生属性是生产端知识（渠道决定身份模式），值消费统一走 field 提取；通用机制，未来 token 过期时间 / 账号 id 等派生属性同路；不为 identity_mode 单开通道 |
| D25 | 内置工具的凭据消费形态 | **`CredentialBinding` 增 `Internal { field: String }`**（v1.5）：值存工具实例字段（check 内 match 收集），非 env/header/query 外部注入——lark_cli 这类内置工具消费形态；Env/Header/Internal 各自对应消费端类型，编排层不感知差异 | 内置工具无 HTTP/子进程载体，「注入」即存字段供 call 内使用；保持 binding 枚举单一模型，不为内置工具另设通道 |
| D26 | 工具调用编排入口统一 | **所有 Agent 工具调用经 domain `RuntimeToolExecution` 单点编排**（用户决策 v1.7）：think_loop 的 control_mode 分发改走 domain（Auto → `call_tool` 协议路由；Manual → `dispatch_manual_tool` 转发，真实执行仍兜回 `call_manual_tool_for_agent` → `call_tool`，凭据在真实执行时编排）；`ToolDal::call_tool_by_id` / `McpToolDal::call_tool_by_id` 删除（全仓无生产调用方），`ToolDal::execute_auto` / `execute_manual` 随 think_loop 切换一并删除（唯一调用方消失，特殊 tool 转发逻辑上移 domain）；domain → DAL 统一传参 **`ToolExecutionRequest { tool, args, resolved }`**（落位 models/tool.rs，内部调用结构非 HTTP DTO 不进 common；命名规避 models/cortex_types.rs 既有 `ToolCallRequest`（LLM 调用描述符），与既有 `ToolExecutionResult` 成对）；DAL 内 per-call 重组装实例 → `check(resolved)` → `execute`（Builtin 不再复用 `agent.tools()` 装配实例，D22 单次实例） | 凭据编排挂在 domain call_tool 单点，主循环绕行直连 tool_dal 会整条漏注入；现状 think_loop Auto 路径的 MCP 工具是 ManagementOnlyTool 占位（`assemble_core_tool` 对 Mcp 刻意返回 None）主循环实际不可执行，切到 domain 协议路由顺带修复；结构体传参一次定形两个 DAL 签名 |
| D27 | tavily 共享 config 兜底废除 | **全局 config `[tavily]` 整段删除（api_key + timeout_ms），TavilyKey 纯单轨**（用户决策 v1.8）：凭据唯一来源 user dal `find_default(TavilyKey)`，与 GithubToken 完全一致；`timeout_ms` 挪 tavily 工具 PO config（读取缺省 15_000，存量 PO config=Null 零迁移）；`shared_key_configured` 状态字段随兜底一并删除（common DTO + domain + 前端两处分支 + 集成测试） | 共享 key 落全局 config 是双轨授权（[web_search_and_browser_tools_design](web_search_and_browser_tools_design.md)）时期遗留，与「凭据统一走凭证库」相悖；共享 key 挪 tool PO config 也不行——凭据原文落组织共享配置等同全员可见（D1 红线，同被废的 MCP env/headers）；timeout 是非敏感工具行为参数，跟工具走比留全局 config 内聚 |
| D28 | 工具参数归宿 + readiness 探测形态 | **工具环境参数 PO config 闭环 + readiness 数据驱动**（用户决策 v1.9）：① CLI 型工具（browser / gh_cli / lark_cli）的二进制名与行为参数（timeout/max_output）全部进各自 PO config（工厂 create_po 写默认值，存量 Null 读取时缺省兜底零迁移），`BrowserConfig` 段删除——**全局 config 零工具参数不变式**，工具页可改命令路径；② readiness 探测注册表（ToolReadinessProbe trait / PROBES / register_default_probes / 三个 probe）整体消亡，改为 domain `RuntimeToolExecution::tool_readiness(ctx, &Tool)` 数据驱动：CLI 型 = `po.cli_command()` → pkg `command_available` 纯函数，key 型 = requirements → 复用 `resolve_tool_credentials`（Some→Ready / None→NotReady / Err→Unknown），无要求 → Ready；TTL 缓存迁 domain 语义等价 | 内置工具是我们包装的，其使用的二进制与行为参数是工具自身属性，应在工具管理页闭环（用户论断：迁移时 toml 同样要改，config 无部署绑定优势）；readiness 与凭据解析同构（需 PO config + 用户凭据数据），D17 哲学第三次应用——取数上移 domain、pkg 纯函数，探测注册表与 resolver OnceLock 是同一种要消亡的注册机制；tavily probe 依赖 resolver（本设计 Step 4 删除）本就必死，重构是唯一正解 |

## 二、架构

### 2.1 配置期（管理面）

```
┌────────────────────────────────────────────────────────────────┐
│ MCP Server（org 共享，DB 注册）                                  │
│  config = { command, args, url, 超时参数,                       │
│             credential_requirements: [                         │
│               { kind: oauth, platform: linear,                 │
│                 field: null, enhancer: bearer_token,           │
│                 binding: header { name: authorization } }      │
│             ] }                                                │
│  （env / headers 字段已删除——注入唯一来源是 requirements）      │
├────────────────────────────────────────────────────────────────┤
│ HTTP 工具（org 共享，DB 注册）                                   │
│  config = { method, url, headers/query/body 模板,              │
│             credential_requirements: [                         │
│               { kind: user_password, platform: internal_api,   │
│                 field: null,                                   │
│                 enhancer: null,    ← canonical（默认 BasicAuth）│
│                 binding: header { name: authorization } }      │
│             ] }                                                │
└────────────────────────────────────────────────────────────────┘
│  配置期校验（创建/更新 handler，前端预校验同规则）：
│  · HTTP 敏感名条目（静态值或 {{args.*}} 模板）→ 拒绝
│  · binding ↔ 协议不匹配（stdio↔Header 等）→ 拒绝
│  · (kind, platform, 注入点名) 三元组重复 → 拒绝
│  · platform ↔ kind 匹配（generic 类必填、专用 kind 必空）
│  · field ↔ enhancer 互斥（D8）
│  · enhancer ↔ kind 匹配（supports 矩阵，D12；专用 kind 零支持）
│  · 显式选择默认增强器（oauth+access_token / user_password+basic_auth）
│    → 幂等允许，等价于不选（D11）
                    │ 落库：config 内零凭据原文、零 credential_id
                    ▼
```

### 2.2 运行时（编排取数 → pkg 加工 → check 注入 → 纯放置）

```
┌─ service 编排层（domain，每次工具调用，D17/D22）──────────────┐
│ ① 读需求   tool.credential_requirements()                     │
│      共享工具：实例 config（PO）；内置工具：静态声明           │
│      （gh / tavily / lark_cli 三员统一，v1.5）                │
│ ② 生产路由（两个合法生产者，D17）                             │
│      kind=LarkApp → lark dal resolve_credentials_for_user     │
│         （渠道路径 + attributes{identity_mode}，D24）         │
│      其余 kind   → user dal find_default(kind, platform)      │
│         链序：个人默认 > 个人活跃 > 组织默认 > 组织 public     │
│      tavily 兜底：未命中 + kind=tavily_key → 共享 config 合成 │
│      任一 requirement 未命中 → api_key_missing 结构化引导     │
│      （不构造实例，直接返回）                                  │
│ ③ 加工     pkg 凭据模块纯函数（组合规则 D10）：                │
│      解密 → ResolvedCredential（含 attributes）→              │
│      enhancer 声明 → credential.enhance(kind)                 │
│        BearerToken  = "Bearer " + 规范可用值                   │
│        BasicAuth    = "Basic " + base64(username:password)     │
│        AccessToken  = refresh → access_token（TTL 缓存）       │
│      无 enhancer → credential.canonical_value(field)           │
│        oauth → access_token；user_password → Basic 串；        │
│        其余 → detail 字段 → attributes（D24）→ primary_secret │
│ ④ 实例化   工厂 create(po) → instance.check(resolved)          │
│      check：校验声明与注入匹配 + 存对象字段（实例单次使用）    │
│ ⑤ 调用     instance.call(ctx, args)                           │
│      ctx 仅日志/审计；凭据路径不依赖 ctx                       │
└──────────────────────────────────────────────────────────────┘
                    │
                    ▼
┌─ 工具实例内部（binding 纯放置，D9）───────────────────────────┐
│ Env{name}       → 子进程环境变量（stdio MCP，键 (server,user)）│
│ Header{name}    → HTTP 请求头（http MCP / HTTP 工具）          │
│ Query{name}     → URL 查询参数（HTTP 工具）                    │
│ Internal{field} → 实例字段（内置工具，D25）                    │
│ connect_stdio_client（env_clear + 注入 env，D23 隔离）          │
│ / execute_http_call（模板渲染 + 注入 header/query + SSRF）     │
└──────────────────────────────────────────────────────────────┘
```

### 2.3 需求声明契约（common 模型层）

```rust
/// 共享工具的凭据需求声明（类型级声明，非实例级引用）
pub struct CredentialRequirement {
    /// 需要的凭据类型
    pub kind: CredentialKind,
    /// 平台标识（generic_token / oauth / user_password 必填，专用 kind 留空）
    pub platform: Option<String>,
    /// 提取字段：None = 规范可用值；Some = detail 指定字段；与 enhancer 互斥
    pub field: Option<String>,
    /// 增强器类型：None = 规范可用值（复合形态凭据走默认增强器）；
    /// 显式选择默认增强器幂等等价于 None（D11）；与 field 互斥
    pub enhancer: Option<CredentialEnhancerKind>,
    /// 注入点（纯放置）
    pub binding: CredentialBinding,
}

/// 增强器类型（requirement 配置声明用；可用性见 §2.5 supports 矩阵）
pub enum CredentialEnhancerKind {
    /// "Bearer " + 规范可用值
    BearerToken,
    /// "Basic " + base64(username:password)
    BasicAuth,
    /// OAuth refresh → access_token（oauth 默认装配）
    AccessToken,
}

/// 注入点（纯放置，零变换）
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CredentialBinding {
    /// 注入子进程环境变量（stdio MCP）
    Env { name: String },
    /// 注入 HTTP 请求头（http MCP / HTTP 工具）
    Header { name: String },
    /// 注入 URL 查询参数（HTTP 工具）
    Query { name: String },
    /// 存工具实例字段（内置工具消费形态，D25；field 为实例字段名）
    Internal { field: String },
}
```

> 当前实现：契约随本设计新建，落位于 [identity_credentials.rs](../../common/src/models/identity_credentials.rs)（与 CredentialKind 同文件，DTO 单一事实源）。

### 2.4 增强器契约与凭据对象（pkg 层）

```rust
/// 凭据增强器：原始凭据的衍生行为封装（pkg 实现、自包含）
#[async_trait]
pub trait CredentialEnhancer: Send + Sync {
    /// 增强器类型标识
    fn kind(&self) -> CredentialEnhancerKind;
    /// 可用性判定：该增强器适用于哪些凭据类型（配置期校验与装配共用，矩阵见 §2.5）
    fn supports(&self, kind: CredentialKind) -> bool;
    /// 执行增强：内部参数从凭据 detail 填充（多字段拼接的上下文即凭据自身）；
    /// 入参为规范可用值（组合规则 D10：显式增强器包裹 canonical）
    async fn enhance(&self, credential: &ResolvedCredential) -> Result<CredentialEnhancedValue>;
}

/// 增强结果（不同增强器返回形态不同，枚举拓展；v1 单值，Multi 为扩展位）
pub enum CredentialEnhancedValue {
    /// 单值（注入 env/header/query）
    Value(String),
}

/// 解析后的凭据对象（pkg 构造：detail + attributes + credential_id；detail 已解密）
pub struct ResolvedCredential { /* 字段私有 */ }

impl ResolvedCredential {
    /// 获取指定增强器的结果（凭据对象代理执行；supports 不匹配 → 错误）
    pub async fn enhance(&self, kind: CredentialEnhancerKind) -> Result<CredentialEnhancedValue>;
    /// 规范可用值：复合形态凭据走默认增强器（oauth→access_token、
    /// user_password→Basic 串）；单值 kind 查找链（D24）：
    /// detail 字段 → attributes 派生属性 → primary_secret
    pub async fn canonical_value(&self, field: Option<&str>) -> Result<String>;
}

/// 单条 requirement 的最终注入值（编排层产出，经 check 注入工具实例）
pub struct ResolvedRequirement {
    pub requirement: CredentialRequirement,
    /// 注入值（组合规则 D10：注入值 = 显式增强器( canonical )）
    pub value: String,
}

/// 编排层传入的凭据条目（dal 生产：credential_id + 加密态 detail + 派生属性）
pub struct FetchedCredential {
    pub credential_id: String,
    pub detail: CredentialDetail,
    /// dal 派生属性（D24：lark 的 identity_mode 等；user dal 生产路径为空集）
    pub attributes: BTreeMap<String, String>,
}

/// 纯函数加工入口（D17：pkg 零数据访问——凭据由编排层经 dal 生产路由取回传入；
/// 内部：按 kind 解密 → ResolvedCredential → enhancer/canonical 取值）
pub fn resolve_requirements(
    requirements: &[CredentialRequirement],
    fetched: &[FetchedCredential],
) -> Result<Vec<ResolvedRequirement>>;
```

> 当前实现：trait、三个内置增强器、`ResolvedCredential` / `resolve_requirements` 随本设计新建，落位于 src/pkg/credential/（门面 mod.rs；增强器与 OAuthTokenManager 在子模块 enhancer.rs）——凭据域纯值加工模块，不隶属 tool_registry（依赖方向 tool_registry → credential 单向）；**模块无 ctx / 无数据访问 / 无注入注册**——取数编排（生产路由 → 本函数 → tavily 兜底）在 domain `resolve_tool_credentials` 单点（见 §三 service 层清单）。

### 2.5 内置增强器一览（v1）

| 增强器 | 行为 | supports（矩阵 D12） | 默认装配 |
|--------|------|----------------------|----------|
| `BearerToken` | `"Bearer " + 规范可用值` | generic_token、oauth | 无 |
| `BasicAuth` | `"Basic " + base64(username:password)` | user_password | **user_password 默认**（D7） |
| `AccessToken` | refresh_token 刷新换 access_token（§2.5.1） | oauth | **oauth 默认**（D7） |
| （不选） | canonical：generic_token → token；专用 kind → 查找链 detail 字段 → attributes（D24）→ primary_secret | — | — |

**默认增强器与规范值**（对称模型，D6/D7）：

| kind | 默认增强器 | canonical 值 |
|------|-----------|--------------|
| oauth | AccessToken | `access_token` |
| user_password | BasicAuth | `"Basic " + base64(username:password)` |
| generic_token | 无 | `token` 原文 |
| 专用 kind（lark_app / github_token / tavily_key） | 无 | 查找链：detail 字段 → attributes（D24，lark_app 的 identity_mode 走此）→ primary_secret |

**组合示例**（包裹规则 D10）：

| requirement 声明 | 用户实际绑定 | 注入值 |
|------------------|-------------|--------|
| oauth + 无 enhancer | oauth 凭据 | `access_token` |
| oauth + BearerToken | oauth 凭据 | `"Bearer " + access_token` |
| oauth + BearerToken | （若用户绑 linear）| `"Bearer " + access_token`——工具配置不感知凭据形态 |
| generic_token + 无 enhancer | Notion PAT | `ntn_xxx` 原文 |
| generic_token + BearerToken | Notion PAT | `"Bearer " + ntn_xxx` |
| user_password + 无 enhancer | 内网账号密码 | `"Basic " + base64(user:pass)` |
| user_password + BasicAuth（显式选默认，幂等 D11） | 内网账号密码 | 同上，等价 |

#### 2.5.1 AccessToken 增强器（OAuth 生命周期内聚）

```rust
/// OAuth refresh_token → access_token（AccessToken 增强器的内部引擎）
pub struct OAuthTokenManager { /* cache: Mutex<HashMap<credential_id, CachedToken>> */ }

impl OAuthTokenManager {
    /// 取 access_token：缓存命中且剩余 > 60s 直返；miss/将过期则刷新
    pub async fn get_access_token(
        &self,
        credential_id: &str,
        oauth: &CredentialDetailOAuth,
    ) -> Result<String>;
}

/// OAuth 凭据 detail（common 模型层，敏感字段加密存储）
pub struct CredentialDetailOAuth {
    pub token_endpoint: String,
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub scope: Option<String>,
}
```

刷新流程（`get_access_token` 内聚）：缓存命中且剩余有效期 > 60s → 直返；否则校验 `token_endpoint` SSRF（红线 7）→ `POST {token_endpoint}`（`grant_type=refresh_token&client_id&client_secret&refresh_token[&scope]`，form-encoded）→ 解析 `access_token` / `expires_in` → 写缓存（过期时刻提前 60s）→ 返回。刷新失败不缓存失败结果。

> 当前实现：OAuthTokenManager 随本设计新建，落位于 src/pkg/credential/enhancer.rs（AccessToken 增强器内部）；HTTP 调用复用项目既有 client，日志/错误不含任何 token 值。

## 三、涉及文件清单

### common 契约层

| 操作 | 路径 | 内容 |
|------|------|------|
| 修改 | [identity_credentials.rs](../../common/src/models/identity_credentials.rs#L23-L30) | `CredentialKind` 增 `GenericToken` / `OAuth` / `UserPassword`；补 `JsonSchema` derive（DTO 嵌入需要） |
| 修改 | [identity_credentials.rs](../../common/src/models/identity_credentials.rs#L73-L124) | `CredentialDetail` / `CredentialDetailPatch` 增三变体（`{ token }` / §2.5.1 字段 / `{ username, password }`）；新增 `primary_secret()`；kind/validate/normalized/encrypt_sensitive/apply_patch 各 match 臂补齐 |
| 新增（同文件追加） | [identity_credentials.rs](../../common/src/models/identity_credentials.rs) | `CredentialRequirement` + `CredentialEnhancerKind` + `CredentialBinding` 契约（§2.3） |
| 修改 | [mcp_server.rs](../../common/src/api/mcp_server.rs#L12-L32) | `McpServerConfigDto` 删 `env` / `headers`，增 `credential_requirements`（HTTP 工具 config 为裸 JSON blob，无 DTO 结构改动） |

### 数据库迁移

| 操作 | 路径 | 内容 |
|------|------|------|
| 新建 | migrations/2026xxxx_add_credential_platform.sql | `user_credentials` 加 `platform TEXT NULL`；默认唯一索引重建为 `(user_id, kind, platform)` / `(org_id, kind, platform)` 部分索引（D20）；存量行 platform 置 NULL |

### models 层

| 操作 | 路径 | 内容 |
|------|------|------|
| 修改 | [mcp_server.rs](../../src/models/mcp_server.rs#L101-L167) | `McpServerConfig` 删 `env` / `headers`，增 `credential_requirements`；`default_stdio` / `default_streamable_http` / `redacted_for_management` 同步调整（脱敏仅剩 URL） |

### pkg 运行时层

| 操作 | 路径 | 内容 |
|------|------|------|
| 新建 | src/pkg/credential/（mod.rs + enhancer.rs） | **凭据域纯值加工模块，零数据访问**（D17），不隶属 tool_registry（依赖方向 tool_registry → credential 单向）：`decrypt_detail`（按 kind 解密，与 encrypt_sensitive 对称）+ §2.4 `CredentialEnhancer` trait + 三个内置增强器（BearerToken / BasicAuth / AccessToken 含 §2.5.1 OAuthTokenManager，OnceLock 仅内部缓存）+ `FetchedCredential`（含 attributes，D24）+ `ResolvedCredential`（enhance / canonical_value 查找链 detail→attributes→primary_secret + 默认装配规则）+ `resolve_requirements(requirements, fetched)`（纯函数：requirement + dal 生产凭据 → 注入值列表，内部解密+增强）+ `validate_requirements`（协议匹配 / 三元组去重 / platform↔kind / field↔enhancer 互斥 / enhancer↔kind supports 矩阵 / 显式默认幂等归一 / binding↔消费端类型含 Internal）+ `credential_missing_json`（编排层引导） |
| 修改 | [tool.rs](../../src/models/tool.rs) | `CoreTool` trait 增生命周期方法（D22）：`credential_requirements(&self) -> &[CredentialRequirement]`（默认空切片）+ `check(&mut self, resolved: &[ResolvedRequirement]) -> Result<()>`（默认空实现）；各协议实现覆写 |
| 修改 | [mod.rs](../../src/pkg/tool_registry/mod.rs#L90-L108) | `ToolRegistry::create_tool` 后编排辅助：`credential_requirements(&self, po) -> Vec<...>`（Builtin 查 factory 静态声明 / Mcp·Http 从 config 解析）——domain 读需求统一入口 |
| 修改 | [gh_cli.rs](../../src/pkg/tool_registry/gh_cli.rs#L50-L64) | **删除** `GhCredentialResolver` trait / RESOLVER OnceLock / set·get_credential_resolver；CoreTool 实现改 D22 生命周期：静态 requirements `[GithubToken]` + `check` 存 token 字段；`call` 内取数段（L350-L362）删除，改用实例字段（D17） |
| 修改 | [tavily_search.rs](../../src/pkg/tool_registry/tavily_search.rs#L48-L95) | **删除** `TavilyCredentialResolver` trait / OnceLock / setter·getter；`resolve_api_key` 双轨逻辑**上移编排层**（domain：find_default 未命中 + tavily_key → 共享 config 合成注入值）；CoreTool 实现改 D22 生命周期（静态 requirements `[TavilyKey]` + check 存 key） |
| 修改 | [lark_cli.rs](../../src/pkg/tool_registry/lark_cli.rs#L47-L62) | **删除** `LarkCredentialResolver` trait / RESOLVER OnceLock / set·get_credential_resolver（v1.5 统一）；CoreTool 实现改 D22 生命周期：静态 requirements `[LarkApp field=app_id (Internal{app_id}) / LarkApp field=app_secret (Internal{app_secret}) / LarkApp field=identity_mode (Internal{identity_mode})]`（同凭据三 requirement，D4 多字段模式）+ `check` 存三元组实例字段；`call` 内取数段（L296-L309）删除，改用实例字段；「解析器未就绪」分支一并消失（未绑定引导统一编排层） |
| 修改 | [mcp.rs](../../src/pkg/tool_registry/mcp.rs#L142-L268) | `McpCoreTool` 增 requirements（从 server config）+ `check`（存 env 注入值 + user 维度）；`connect_stdio_client` 删除静态 env 注入循环（L214-L216），改用实例注入值；`McpClientRuntime` 连接缓存键 `(server_id, user_id)`（有 requirements 时；无则 server_id，D23） |
| 修改 | [mcp.rs](../../src/pkg/tool_registry/mcp.rs#L72-L101) | `call_tool` / `list_tools` 增 ctx 参数（list 同步路径取数用，D18） |
| 修改 | [http.rs](../../src/pkg/tool_registry/http.rs#L42-L73) | `HttpToolConfig` 增 `credential_requirements` |
| 修改 | [http.rs](../../src/pkg/tool_registry/http.rs#L100-L220) | `HttpCoreTool` 增 requirements（从 config）+ `check`（存 header/query 注入值）；`execute_http_call` 模板渲染后叠加实例注入值 |
| 修改 | [http.rs](../../src/pkg/tool_registry/http.rs#L369-L392) | `validate_config` 增：敏感名条目拒绝（静态 + 模板，复用 `is_sensitive_header`）、requirements 校验（仅 Header/Query binding） |

### service 层

| 操作 | 路径 | 内容 |
|------|------|------|
| 修改 | [user_credential/mod.rs](../../src/service/dao/user_credential/mod.rs#L74-L83) | `UserCredentialQuery` / `find_default` 增 platform 过滤参数（D20） |
| 修改 | [user.rs](../../src/service/dal/user.rs#L301-L360) | **删除** Gh / Tavily 两个 Dal resolver（随 per-tool trait 一并消亡，无替代实现——pkg 零数据访问后无需 provider） |
| 修改 | domain/runtime/tool_execution.rs | **新增编排取数入口** `resolve_tool_credentials(ctx, requirements) -> Result<Option<Vec<ResolvedRequirement>>>`（D17 编排链 ①②③：**生产路由**——`kind=LarkApp → lark dal resolve_credentials_for_user`（附 attributes），`其余 → user dal find_default`（tavily 无兜底，D27 纯单轨）；→ pkg `resolve_requirements` 加工；任一未命中返回 None 供调用方出引导）。**落位 runtime 而非 finance domain（D26 收敛）**：唯一消费方是同模块 `call_tool` 编排，依赖经 RuntimeDomainImpl 字段构造注入（`user_dal` 既有 trait + `lark_credentials` 子 trait `LarkCredentialDal`，与 tool_dal 同模式，禁止方法内取全局单例），零 finance domain 依赖，规避 domain 同层互调 |
| 修改 | 工具调用编排（domain 调 `dal.call_tool` 处 + [mcp_tool.rs](../../src/service/dal/mcp_tool.rs#L236-L250)） | 调用链改造（D22）：domain 取 resolved → 传入 dal → dal `assemble_executable_tool` 创建实例后 `instance.check(&resolved)` → execute；`call_tool` / `execute` 签名增 resolved 参数 |
| 修改 | [mod.rs](../../src/service/mod.rs#L14-L24) | `service::init` 凭据相关注册**全部删除**（Gh/Tavily/Lark resolver 三行；不再新增任何注册项） |
| 修改 | [dal/lark.rs](../../src/service/dal/lark.rs) → 拆分 dal/lark/ 文件夹 | **trait 体系化 + 改名（用户决策）**：原 `LarkMessageChannelDal` 无 trait 且名不副实——拆为 `mod.rs`（总 trait `LarkDal: LarkCredentialDal + LarkListenerDal` + 单例/init）+ `credentials.rs`（子 trait `LarkCredentialDal`：resolve_credentials_for_user / resolve_channel_app_id / find_channels_by_credential_id）+ `listener.rs`（子 trait `LarkListenerDal`：ensure/release/sync/handover listener + listener_stats）+ `impl.rs`（`LarkDalImpl` 改名实现全部）；上层按需只持子 trait（runtime domain 只持 `Arc<dyn LarkCredentialDal>`，finance 持 `Arc<dyn LarkDal>`）。删除 `LarkDalCredentialResolver` 及 pkg trait impl 段（L47-L63）；`resolve_credentials_for_user`（L239-L295）**保留为合法生产端**——返回值适配 `FetchedCredential`（补 attributes{identity_mode}，D24），渠道选择语义不变 |
| 修改 | [tool_call/mcp.rs](../../src/service/dao/tool_call/mcp.rs#L118) | tools 同步路径 `list_tools(server)` → `list_tools(ctx, server)`（同步前编排层取数注入，D18） |
| 修改 | [common/src/config.rs](../../common/src/config.rs#L629-L655) | **D27**：`TavilyConfig` 段整体删除（api_key + timeout_ms），`Config` struct 的 `tavily` 字段移除 |
| 修改 | [tavily_search.rs](../../src/pkg/tool_registry/tavily_search.rs) | **D27**：timeout 读工具 PO config `timeout_ms` 缺省 15_000；`resolve_api_key` 双轨改单轨（仅用户凭证，随 Task 3.3 check 注入一并落地）；引导文案单路径（删「管理员配置共享 key」）；[tool_readiness.rs](../../src/pkg/tool_registry/tool_readiness.rs#L180-L210) TavilyKeyProbe 删共享 config 探测分支；`identity_credential.rs` tavily_integration_status 删 `shared_key_configured`（common DTO + 前端 identity_tavily.rs 两处分支 + 集成测试同步） |
| 修改 | [common/src/config.rs](../../common/src/config.rs#L657-L692) | **D28**：`BrowserConfig` 段整体删除（command/timeout_ms/max_output_bytes），`Config.browser` 字段移除——与 D27 tavily 段删除共同达成「全局 config 零工具参数」不变式 |
| 修改 | [browser.rs](../../src/pkg/tool_registry/browser.rs) / [gh_cli.rs](../../src/pkg/tool_registry/gh_cli.rs) / [lark_cli.rs](../../src/pkg/tool_registry/lark_cli.rs) | **D28**：CLI 命令与行为参数进各自 PO config（工厂 create_po 写默认值：browser `{command, timeout_ms, max_output_bytes}`、gh `{command:"gh"}`、lark `{command:LARK_CLI_BIN}`）；spawn 读 `self.po` config（GH_CLI_BIN/LARK_CLI_BIN 常量降级为缺省值来源）；[ToolPo](../../src/models/tool.rs) 增 `cli_command()` helper（CLI 型判定 + 命令读取单点，存量 Null 缺省兜底零迁移）；引导文案改「在工具配置中修改命令路径」 |
| 修改 | [tool_readiness.rs](../../src/pkg/tool_registry/tool_readiness.rs) | **D28**：`ToolReadinessProbe` trait / PROBES 注册表 / `register_default_probes` / BrowserCliProbe / FixedCliProbe / TavilyKeyProbe 整体删除（探测注册机制消亡）；保留纯函数 `command_available` / `cli_binary_readiness` / `cli_not_installed_json` / `credential_missing_json`（引导文案与各工具 spawn 内文案统一来源） |
| 修改 | domain/runtime/tool_execution.rs + [response.rs](../../src/handlers/finance/tool/response.rs#L23) | **D28**：`RuntimeToolExecution` 增 `tool_readiness(ctx, &Tool) -> RuntimeReady` 数据驱动——CLI 型 `po.cli_command()` → `command_available`；key 型 requirements → 复用 `resolve_tool_credentials`（Some→Ready / None→NotReady{api_key_missing} / Err→Unknown）；无要求→Ready；TTL 缓存（30s，key 型 tool\|user）迁 domain impl 语义等价；response.rs 唯一调用点 `tool_readiness::probe(&id, ctx)` → domain `tool_readiness(&ctx, &tool)` |

### handler 层

| 操作 | 路径 | 内容 |
|------|------|------|
| 修改 | [create_mcp_server.rs](../../src/handlers/finance/mcp_server/create_mcp_server.rs) | 创建时接入 requirements 校验 |
| 修改 | [update_mcp_server.rs](../../src/handlers/finance/mcp_server/update_mcp_server.rs) | 同上 |
| 修改 | [response.rs](../../src/handlers/finance/mcp_server/response.rs#L34-L60) | DTO ↔ Model 转换：删 env/headers 映射，补 `credential_requirements` |

### 前端

| 操作 | 路径 | 内容 |
|------|------|------|
| 修改 | [mcp_servers.rs](../../frontend/src/pages/finance/mcp_servers.rs#L48-L112) | 创建表单增「凭据需求」区，字段联动：kind 下拉 → platform（generic 类才显示）→ 增强器下拉（supports 矩阵过滤：专用 kind 禁用 + 提示「该凭据类型不适用增强器」；默认增强器不提供选项；选了 field 则禁用）→ 注入位置（按 transport 过滤 Env/Header）→ 注入名；前端预校验与后端同一套规则 |
| 修改 | [tools.rs](../../frontend/src/pages/finance/tools.rs) | HTTP 工具表单同款（Header/Query） |

### 测试

| 操作 | 路径 | 内容 |
|------|------|------|
| 修改 | [mcp_tests.rs](../../src/pkg/tool_registry/mcp_tests.rs) | env 注入命中 / 缺凭据引导 / platform 匹配用例；存量 env 用例改写 |
| 修改 | [http_tests.rs](../../src/pkg/tool_registry/http_tests.rs) | 敏感名静态值与模板拒绝 / header·query 注入 / BasicAuth canonical 注入用例 |
| 新增（内嵌 mod tests） | src/pkg/credential/{mod,enhancer}.rs | requirements 校验（协议匹配、三元组去重、platform↔kind、field↔enhancer 互斥、supports 矩阵含专用 kind 拒绝、显式默认幂等归一、Internal binding 合法性）/ resolve_requirements 纯函数（oauth → access_token、user_password → Basic 串、generic_token → token、field 提取、attributes 查找链 D24）/ BearerToken 包裹组合（"Bearer " + access_token）/ OAuthTokenManager（缓存命中、过期刷新、失败不缓存、SSRF 拒绝）用例——全纯函数构造，无需 DB / mock provider |
| 新增（domain 侧） | domain/runtime/tool_execution.rs（内嵌 tests） | resolve_tool_credentials 编排：生产路由（LarkApp 走 lark dal 含 attributes / 其余走 user dal）、find_default 逐条命中 / 未命中 None（stub 注入无需 DB）；**D28 tool_readiness**：CLI 型（po.config.command 可寻址 / 不可寻址）、key 型（Some→Ready / None→NotReady / Err→Unknown）、无要求→Ready、TTL 缓存命中与过期 |
| 修改 | [mcp_server_test.rs](../../src/models/mcp_server_test.rs)、[response_test.rs](../../src/handlers/finance/mcp_server/response_test.rs)、[list_mcp_servers_test.rs](../../src/handlers/finance/mcp_server/list_mcp_servers_test.rs) | env/headers 删除后构造与断言同步改写 |
| 修改 | [identity_credentials.rs](../../common/src/models/identity_credentials.rs#L326-L722)（内嵌 tests） | GenericToken / OAuth / UserPassword serde / 加密 / 补丁 / primary_secret / platform 匹配规则用例 |

## 四、边界与行为红线

1. **共享配置红线**：落库 config 禁 credential_id；MCP config 零 env / headers 字段；HTTP config 的 headers/query 禁敏感名条目（静态值与 `{{args.*}}` 模板同禁）；`credential_requirements` 仅含匹配键 + 增强器 + 注入点声明（非敏感，可直接展示）。
2. **注入红线**：最终注入值只进子进程 env / HTTP 请求头 / 查询串；binding 纯放置零变换（一切拼装/派生只发生在增强器，显式包裹 canonical，D10）；禁入工具入参 schema、禁入返回值（输出脱敏既有）、禁入日志、禁入错误信息（`map_mcp_tool_error` 既有）。
3. **解析红线**：链序单点收敛在 `UserCredentialDao::find_default`（含 platform 维度），消费侧禁止自实现；取数只发生在 service 编排层（domain，D17）——pkg 与工具实例零数据访问；secret 解密后仅存于当次调用栈与单次工具实例字段。
4. **能力红线**：OAuth 凭据只装配 AccessToken 默认增强器、user_password 只装配 BasicAuth 默认增强器——refresh_token / client_secret / 裸 username / 裸 password 无默认注入路径（D7）；OAuth 禁 field 提取（field 与 enhancer 互斥且 OAuth 仅 AccessToken 可用）；user_password 的 field 提取默认关闭（canonical 走 BasicAuth；裸字段提取需求出现时经评审放开）；增强器可用性以 `supports` 矩阵单点判定，配置期静态校验，前后端一致（D12）。
5. **校验红线**：binding ↔ 协议匹配（stdio→Env、http→Header、HTTP 工具→Header/Query）；(kind, platform, 注入点名) 三元组去重；platform ↔ kind 匹配（generic 类必填、专用 kind 必空）；field ↔ enhancer 互斥；enhancer ↔ kind 按 supports 矩阵；显式选择默认增强器幂等归一（等价于不选，D11）；配置期拒绝为主，运行时连接前保留防御性再校验。
6. **环境隔离红线**：`env_clear()` 白名单式注入保持——子进程环境变量仅含注入值，绝不继承系统环境。
7. **OAuth endpoint 红线**：`token_endpoint` 刷新调用前必须过 SSRF 校验（复用 `validate_target_url`，拒绝内网/环路地址）；刷新请求与响应的日志不含任何 token 值。
8. **既有 resolver 红线**（v1.5 全统一）：Gh / Tavily / Lark 三个 per-tool resolver trait 与 Dal 实现**整体删除**（D17）——取数上移编排层，lark 的 `resolve_credentials_for_user` 保留为合法生产端（渠道路径语义不变，仅适配返回值）；pkg 侧唯一 trait 是 `CredentialEnhancer`（值加工），无任何数据访问 trait / 端口 / 注入注册；dal 生产端仅两个——user dal `find_default` 与 lark dal `resolve_credentials_for_user`，禁止第三处自建取数；tavily 共享 config 兜底废除（D27），凭据无任何非凭证库来源。
9. **实例生命周期红线**（D22）：凭据注入值与用户维度是工具实例状态——实例每次调用经 `create → check → call` 单次使用，**禁止跨调用复用实例**（串号风险）；stdio 连接缓存键必须含 user 维度（D23），带 requirements 的 server 禁止全局共享连接。
10. **工具参数闭环红线**（D27/D28）：全局 config 零工具参数（`[tavily]` / `[browser]` 段已废）——工具行为参数与 CLI 命令唯一归宿是工具自身 PO config（非敏感、org 管理页可改）；凭据类参数唯一归宿是用户凭证库（D1/D27 双轨废弃）；新工具接入优先在自身 PO config 闭环，禁止再向全局 config 添加工具段。readiness 探测零注册机制（D28）——就绪状态是 Tool 数据的派生（CLI 型 po.config.command + key 型 requirements），新增工具自动获得就绪判定，无需注册 probe。

## 五、扩展模式

1. **新增凭据类型**：common 模型加 kind/detail/patch 变体 + `primary_secret` match 臂即可；共享工具侧零改动（需求声明直接引用新 kind）。
2. **新增增强器或放开 supports 矩阵**：`CredentialEnhancerKind` 加变体 + pkg 实现 `supports` / `enhance`（如 `Prefix { prefix }` 自定义前缀、`Hmac` 签名、返回 `Multi` 的多字段注入增强器）；矩阵放开专用 kind（如 GitHub Bearer）只改 supports 一处；工具配置声明即用，pkg 单点扩展。
3. **streamable_http MCP 落地**：Header binding 即插即用，注入链路与 HTTP 工具同构（当前 mcp.rs 对该 transport 报「not implemented」）。
4. **工具级覆盖 server 级**：当前 requirements 挂 MCP Server 配置（server 内全工具共享）；未来单工具差异化时在 ToolPo.config 加覆盖层（本设计不实现）。
5. **非敏感 env 白名单**：如未来出现真实非敏感 env 配置需求（NODE_ENV 等），评估显式白名单 reintroduce 静态 env（本设计不实现，D21）。
6. **OAuth 凭据失效标记**：刷新持续失败（如 refresh_token 已吊销）时将凭据标记失效的管理面告警（本设计不实现，仅预留）。
7. **凭据健康度体检**：requirements 声明 + find_default 可派生「声明了但 org 内无人可解析」的管理面告警（本设计不实现）。
8. **field × enhancer 组合**：若出现「增强器作用于指定字段」的组合需求（如 Bearer + app_id），放开 D8 互斥约束，增强器语义升级为「作用于 canonical_value(field)」（本设计不实现）。
