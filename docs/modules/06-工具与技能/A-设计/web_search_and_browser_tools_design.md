---
kind: 设计决策
module: 06-工具与技能
name: web_search_and_browser_tools_design
title: 网络搜索与浏览器工具设计
status: 现行
scope:
- common/src/config.rs
- common/src/models/identity_credentials.rs
- frontend/src/pages/finance/identity_tavily.rs
- src/handlers/finance/tavily_integration/
- src/pkg/tool_registry/browser.rs
- src/pkg/tool_registry/tavily_search.rs
- src/pkg/tool_registry/tool_readiness.rs
- tests/integration/browser_tool_test.rs
- tests/integration/web_search_tool_test.rs
related: []
---
> ⚠️ 疑似过期 / 已重构：本文档引用的以下代码路径在当前代码库中已失效（`tools/wiki_index.py --stale` 体检发现）。使用前请核对现行实现：
> - `frontend/src/pages/finance/identity_tavily.rs`：Tavily 集成已移除（前端 identity_tavily.rs 与后端 handlers/finance/tavily_integration/ 均不存在）
> - `src/handlers/finance/tavily_integration/`：Tavily 集成已移除（前端与后端 handlers 均不存在）

# 网络搜索与浏览器工具设计

> 🎯 **定位**：① 决策快照 — 为 Agent 增加网络搜索与浏览器自动化两类内置工具的选型、授权双轨制、环境就绪提示决策
> 状态：定稿（2026-08-18）
> 触发场景：新增搜索 provider 工具、扩展浏览器子命令、为工具接入授权（API key/token）、处理外部 CLI 未安装提示时先读本文
>
> 关联文档：
> - [plan 阶段一：网络搜索工具](docs/modules/06-工具与技能/B-落地方案/网络搜索工具.md) — 阶段一落地路径
> - [plan 阶段二：浏览器工具与环境就绪提示](docs/modules/06-工具与技能/B-落地方案/浏览器工具与环境就绪提示.md) — 阶段二落地路径
> - [AGENTS.md](AGENTS.md) — 分层架构与工具注册规范
> - [docs/LAYERED_ARCHITECTURE_PRACTICE.md](docs/LAYERED_ARCHITECTURE_PRACTICE.md) — pkg 层无业务感知原则

---

## 一、设计目标

为 Agent 补齐「查得到、看得见」的网络能力：网络搜索（获取实时信息）+ 浏览器自动化（JS 渲染页与页面交互，弥补 `http_fetch` 只能抓静态页的缺口）。同时确立两条横切原则：**工具授权双轨制**（工具级共享配置 vs 用户级身份凭证）与**环境就绪提示体系**（绑定了工具但授权/CLI 缺失时全链路可见）。

### 关键决策表

| # | 问题 | 方案 | 原因 |
|---|------|------|------|
| 1 | 工具授权（API key/token）放哪里？ | **授权双轨制**：① 免费/实例共享 → 工具级 config（如 `[tavily].api_key`）② 个人提供 → 用户身份凭证库（identity_credentials，AES-256-GCM 加密）。调用时解析顺序：**用户凭证优先 → 工具共享 config 兜底**，两者皆缺失才报引导错误（双路径指引：绑个人凭证或配共享 key） | 授权来源决定归属：人的凭证进人的身份体系（可审计/可轮换/不外泄给他人），共享能力进实例配置；与 lark_cli/gh_cli 既有实践一致——pkg 定义 Resolver trait、DAL 实现、`service::init` 注入，pkg 层保持无业务感知 |
| 2 | 搜索是否抽象通用 provider 封装（SearchProvider trait + 统一 Result）？ | **不抽象**，按工具名独立注册：`tavily_search` 等每家一个内置工具 | 各家独有参数（Tavily 的 `search_depth`、博查的 `freshness`）即能力自描述，通用封装会吃掉差异；工具 schema 让模型自行择优、可并行交叉验证；工具名与行为恒定绑定，防跨实例行为漂移 |
| 3 | 首个搜索后端选哪家？授权怎么落双轨？ | Tavily；`CredentialKind` 新增 `TavilyKey`（个人 key，走用户凭证库）+ `[tavily].api_key`（共享兜底） | 专为 LLM 设计返回干净摘要；免费 1000 次/月，个人可自注册 key，天然适配双轨；后续博查/SearXNG 按需以独立工具名 + 独立 CredentialKind 补充 |
| 4 | 浏览器走哪条路线？ | 包装 [agent-browser](https://github.com/vercel-labs/agent-browser) CLI 为内置工具 | Rust 原生二进制零 Node 运行时依赖（对比 MCP playwright）；daemon 常驻架构页面状态跨调用保持，天然适配 Agent 多回合循环；`snapshot`（@eN 引用无障碍树）+ `read`（agent 可读正文）纯文本模型友好；与 `lark_cli`/`gh_cli` 外部 CLI 包装模式同构。放弃 chromiumoxide 自研（工作量大收益低）。浏览器为本地 CLI 能力，无授权字段，不涉双轨 |
| 5 | browser 工具粒度？ | 单工具 + 整串 `command` 参数，默认开放 CLI 全部子命令 | 对齐 `lark_cli`/`gh_cli` 模式，控制工具清单膨胀；**定位是配置管理而非功能封禁**——CLI 天然具备的能力一律开放，仅管理员可在 `disabled_commands` 黑名单里屏蔽明确有风险的命令（详见红线 18） |
| 6 | 授权缺失 / CLI 未安装，在哪些接触点提示？ | **三层就绪提示体系**：① 工具清单/查询接口附带 `runtime_ready` 预检标志（绑定与选择时前端可见）② 前端 Agent 详情「工具与技能」页对未就绪工具渲染警示 badge ③ 调用时返回结构化引导错误兜底（`cli_not_installed` / `api_key_missing`，含双路径指引） | 只靠调用时报错 = 用户绑定时毫无感知，Agent 跑到一半才失败；三层覆盖「选工具 → 绑定 → 执行」全链路；工具清单稳定不隐藏（绑定关系不因环境漂移） |
| 7 | `runtime_ready` 预检怎么做？ | 轻量只读探测 + TTL 缓存，**带用户上下文**：CLI 型 → 二进制可寻址（config 绝对路径优先，PATH 兜底）；key 型 → 共享 config key 非空 **或** 当前用户凭证库含对应 kind。结果枚举 `ready / not_ready{reason, hint} / unknown`，探测异常一律 `unknown` | key 型授权是用户相关的（双轨制），就绪状态必须按当前查看者判定；只读探测无副作用；TTL 缓存避免每次列表都探测；`unknown` 不阻塞接口（best-effort） |
| 8 | 控制模式？ | `tavily_search` = Auto；`browser` = Manual | 搜索只读低危可自动执行；浏览器可产生真实网络副作用（登录/提交/下载）需人工确认 |
| 9 | 浏览器 daemon 会话隔离？ | 固定 `--session ai_orz_agent_{agent_id}`（owner 取 agent_id→user_id→default，仅保留字母数字、其余折叠为 `_`） | 不与用户手工 agent-browser 会话互踩；Agent 之间相互隔离；owner 退化为邮箱/UUID 时 key 仍合法 |
| 10 | 截图产物怎么返回？ | 落统一附件产物存储（ArtifactManage），返回产物引用 | 复用现有存储/下载链路；避免 base64 大 blob 膨胀消息体 |
| 11 | 就绪探测/引导逻辑放哪？ | `src/pkg/tool_registry/tool_readiness.rs` 统一模块：探测器（CLI 二进制 / 授权）+ TTL 缓存 + 引导错误构造 | CLI 工具（lark_cli/gh_cli/browser）与 key 型工具（tavily）共用；消除 lark_cli/gh_cli 现存泛化 spawn 报错；pkg 层无业务感知（探测与提示文本，不含业务逻辑；用户凭证可达性探测经 Resolver 接口注入，与授权解析同模式） |

## 二、架构思路

两类工具均为 Builtin 协议，走现有注册表分发链路，零框架改动：

```
Agent 思考 → 工具调用请求（tool_id + params）
    │
    ▼
ToolRegistry::create_tool()  ── 按 ToolPo.protocol 分发
    │                              ├─ Builtin → 内置工厂（本设计新增 2 个）
    │                              ├─ Http    → HTTP 工具
    │                              └─ Mcp     → MCP 工具
    ▼
┌─ tavily_search（Auto）─────────────────┐   ┌─ browser（Manual）──────────────┐
│ params: query/depth/max_results         │   │ params: 整串 command            │
│   ↓                                     │   │   ↓                             │
│ 授权解析（双轨，决策 1）：                │   │ tool_readiness 二进制探测        │
│   ① TavilyCredentialResolver            │   │   缺失 → cli_not_installed 引导  │
│      （pkg trait，DAL 实现注入）          │   │   ↓                             │
│      → 用户凭证库 TavilyKey（个人）       │   │ tokio::Command spawn            │
│   ② [tavily].api_key（共享兜底）          │   │   --session ai_orz_agent_{aid}  │
│   皆缺 → api_key_missing 双路径引导       │   │   ↓                             │
│   ↓                                     │   │ stdout/stderr + 超时 + 截断      │
│ reqwest POST api.tavily.com             │   │   ↓                             │
│   ↓                                     │   │ screenshot → 产物存储 → 引用返回 │
│ 结构化结果(title/url/snippet)            │   └─────────────────────────────────┘
└─────────────────────────────────────────┘

就绪预检（读路径，带用户上下文）：
list/query/search tools handler → to_list_item
    → tool_readiness::probe(tool, ctx.uid)（TTL 缓存，只读）
        CLI 型: 二进制可寻址
        key 型: 共享 config 就绪 OR 该用户凭证库含对应 kind
    → ToolListItem.runtime_ready: ready | not_ready{reason,hint} | unknown
    → 前端 agent_detail「工具与技能」页 badge 提示
```

授权双轨制的既有先例（新工具照此模式接入）：

> 当前实现：[CredentialKind 枚举（LarkApp/GithubToken）](../../common/src/models/identity_credentials.rs#L159-L163)、[LarkCredentialResolver：pkg trait 定义 + DAL 实现](../../src/service/dal/lark.rs#L49-L56)、[GhCredentialResolver 同款](../../src/service/dal/user.rs#L190-L197)、[IdentityCredentialManage Domain 管理](../../src/service/domain/finance/mod.rs#L290-L328)

配置段设计（对齐 `[a2a_server]` 先例，serde default 保证旧配置文件兼容）：

> 当前实现：[A2aServerConfig 默认值先例](../../common/src/config.rs#L576-L585)

```toml
[tavily]
api_key = ""            # 共享兜底（个人 key 优先走用户凭证库）；皆空则 not_ready
timeout_ms = 15000

[browser]
command = "agent-browser"  # 二进制名或绝对路径，优先绝对路径
timeout_ms = 60000
max_output_bytes = 262144
```

## 三、涉及文件清单

已有文件（改动）：

| 文件 | 角色 | 改动 |
|------|------|------|
| [CoreTool trait](../../src/models/tool.rs#L16-L34) | 统一工具接口 | 零改动（新工具实现它） |
| [内置工厂注册表](../../src/pkg/tool_registry/builtin.rs#L29-L39) | 工具注册 | 追加 2 个工厂 + tags 断言测试扩展 |
| [ToolProtocol 分发](../../src/pkg/tool_registry/mod.rs#L77-L104) | 协议分发 | 零改动（Builtin 通道复用） |
| [ToolProtocol 枚举](../../common/src/enums/tool.rs#L14-L21) | 协议枚举 | 零改动 |
| [配置模块](../../common/src/config.rs) | 配置定义 | 新增 `[tavily]`/`[browser]` 段 + 默认配置模板同步 |
| [CredentialKind/Detail](../../common/src/models/identity_credentials.rs#L159-L206) | 用户凭证模型 | 新增 `TavilyKey` 变体 + 默认凭证槽位 + 解析方法 |
| [用户 DAL Resolver 先例](../../src/service/dal/user.rs#L190-L197) | 凭证解析注入 | 新增 `TavilyDalCredentialResolver`（对齐 Gh 先例）+ `service::init` 注册 |
| [ToolListItem DTO](../../common/src/api/tool.rs#L215-L242) | 工具列表契约 | 新增 `runtime_ready` 字段（`has_config` 字段先例） |
| [工具列表组装](../../src/handlers/finance/tool/response.rs#L9) | DTO 组装 | 组装时附带 readiness 探测结果 |
| [lark_cli spawn 报错](../../src/pkg/tool_registry/lark_cli.rs#L347-L355) | CLI 包装先例 | 泛化报错改走 tool_readiness 统一引导 |
| [gh_cli](../../src/pkg/tool_registry/gh_cli.rs) | CLI 包装先例 | 同上统一 |
| [MCP 命令路径解析](../../src/pkg/tool_registry/mcp.rs#L249-L267) | PATH 探测先例 | 逻辑参考，提取复用至 tool_readiness |
| [ArtifactManage](../../src/service/domain/project/mod.rs#L365) | 产物存储 | 截图写入复用，零改动 |
| [工具清单页](../../frontend/src/pages/finance/tools.rs#L215-L226) | 前端清单 UX | 新增「就绪」列渲染 runtime_ready badge（对齐既有 protocol/启用 badge 模式；`ToolListItem` 复用 common 结构体，字段零镜像） |
| [Agent 详情工具绑定页](../../frontend/src/pages/hr/agent_detail.rs#L340) | 前端绑定 UX | 已绑定工具中未就绪者渲染警示 badge + hint 文案 |
| [身份凭证聚合页](../../frontend/src/pages/finance/identity.rs#L21) | 前端凭证 UX | 嵌入 Tavily 凭证区块（对齐 IdentityGithubSection 组件模式） |
| `frontend/src/pages/finance/identity_tavily.rs` | 前端凭证 UX | 新建：Tavily key 凭证区块组件（列表/创建/设默认/删除 + status 聚合端点，对齐 identity_github.rs） |
| [github_integration handlers](../../src/handlers/finance/github_integration/mod.rs) | 凭证 CRUD 先例 | 新建 tavily 凭证 CRUD handler 目录（对齐此模式） |

新增文件（规划路径）：

| 文件 | 角色 |
|------|------|
| `src/pkg/tool_registry/tool_readiness.rs` | 就绪探测器（CLI 二进制 / 授权可达）+ TTL 缓存 + `cli_not_installed`/`api_key_missing` 引导错误构造 |
| `src/pkg/tool_registry/tavily_search.rs` | Tavily 搜索内置工具（含 `TavilyCredentialResolver` trait 定义） |
| `src/pkg/tool_registry/browser.rs` | agent-browser 包装内置工具 |
| `src/handlers/finance/tavily_integration/` | Tavily 个人凭证 CRUD handlers（对齐 github_integration 目录） |
| `tests/integration/web_search_tool_test.rs` / `tests/integration/browser_tool_test.rs` | 集成测试（沿用现有集成测试布局） |

## 四、关键边界 / 行为红线

1. **授权双轨铁律**：新工具引入任何授权（key/token）必须同时评估两轨——共享走工具 config、个人走用户凭证库（新 CredentialKind + Resolver 注入）；**禁止**只做工具 config 单轨而把个人凭证塞进共享配置。
2. 授权解析顺序固定：**用户凭证优先 → 共享 config 兜底**；皆缺失时 `api_key_missing` 引导必须给双路径（绑个人凭证 / 配共享 key），由用户选择。
3. **禁止**新建 SearchProvider/通用搜索抽象层；新增搜索后端 = 新增独立工具文件（+ 独立 CredentialKind，如需授权）。
4. `tavily_search` 只读、Auto 模式；`browser` 一律 Manual 人工确认。
5. spawn 错误必须分类：`NotFound` → `cli_not_installed`（含安装命令与配置指引）；`PermissionDenied` → 权限提示；禁止统一泛化报错。
6. readiness 探测**只读**（不 spawn 长驻进程、不发真实网络请求；授权可达 = 共享 key 非空或用户凭证库含 kind，不解密不验证有效性）；key 型探测必须带用户上下文；结果带 TTL 缓存；探测异常返回 `unknown`，不得让列表接口失败。
7. `runtime_ready` 是附加信息而非硬约束：未就绪不阻止绑定（绑定关系稳定），只做提示。
8. browser 子命令默认**不设黑名单**，CLI 全部能力开放（含 eval/batch/tab/cookies）；仅管理员在工具配置 `disabled_commands` 显式列黑名单时才屏蔽命中命令。不经 shell 直接 argv 拼接（分词由 `tokenize_command` 做，支持引号）。详见红线 18。
9. browser 默认 headless（配置页可填 `AGENT_BROWSER_HEADED`，风险自担）；不主动向 Agent 暴露可视化参数。
10. 输出超时与截断上限沿用 config 默认值；stdout/stderr 合并返回。**超预算截断必须非静默**（对齐 [tool_output_boundary_design.md](./tool_output_boundary_design.md) 红线 1）：全量写 `tools/browser/logs/{YYYYMMDD}/{call_id}.log`，回传 `truncated: true` + `full_output_bytes` + `log_path` + 取回提示，Agent 用 `fs_read` 自助取全文（框架不代劳）。截断按 UTF-8 字符边界收缩（`combine_output`），禁止字节切片切坏多字节字符。落盘失败不否定命令结果，但如实回传「未留存」提示。
11. 截图一律落产物存储返回引用，禁止 base64 内嵌返回。
12. 新配置段必须 serde default，且默认配置模板（`DEFAULT_CONFIG_EMBEDDED`）同步更新，保证旧配置文件无损升级。
13. `tool_readiness` 与各 `CredentialResolver` 保持 pkg 层无业务感知：探测/解析经 trait 接口注入实现，pkg 不 import DAL/DAO。
14. **启动环境变量走统一 `env` 字段，唯一硬约束是前缀**：`BrowserConfig.env: Option<HashMap<String,String>>` 承接管理员配置的 agent-browser 启动变量。写入规则仅「`AGENT_BROWSER_` 前缀 + 字符串值」，前后端共用 `validate_browser_env`；spawn 侧再校验一次兜底（防绕过页面直改 DB 注入 `PATH`/`LD_PRELOAD` 劫持子进程——这是字段语义边界，不是功能封禁）。`common::models::tool::BROWSER_ENV_CATALOG` 是**常用中文兜底 + 引导文案**，CLI 新增变量无需改代码即可配置。危险项（headed / session / CDP / 云 provider / 加密密钥等）**照常可配**，仅由 `BROWSER_ENV_BLOCKED`/`BROWSER_ENV_DENY_TOKENS` 在配置页附风险提示，由管理员自行取舍——CLI 天然支持这些能力，硬禁只会逼人走 shell_exec 绕过。Agent 调用参数层不暴露 env。
15. **配置引导数据动态探测，common 仅兜底**：详情页 `get_tool` 经 `attach_browser_env_probe` 挂 best-effort 探测——`pkg::tool_registry::browser_env_probe` 解析 agent-browser 自带 `skills get core --ref commands` 的 Environment Variables 块（版本匹配，免人工维护），叠加本机浏览器路径（`is_file()` 扫描）与服务进程代理（带凭据丢弃）作为推荐值，合并为 `BrowserEnvProbeDTO` 返回。子进程带 2~3s 短超时、结果 60s TTL 单条缓存（与 command 无关，防无界增长）；任何失败留 `None`，前端回落静态目录，详情接口绝不因此失败。探测只服务配置引导，**不得参与就绪判定或自愈**（对齐 `cli_lookup` 红线），也不回显任何凭据。CLI 自带变量表只有一张（16 项），静态目录里其余项标为内置精选，不谎称实时来源。

16. **env 写入只校验前缀，风险项可见但可填**：agent-browser 新增的 `AGENT_BROWSER_*` 自动可配，安全边界只有前缀约束（防注入系统环境变量）。`BROWSER_ENV_BLOCKED`/`BROWSER_ENV_DENY_TOKENS` 退化为**风险提示清单**——经 `browser_env_blocked_reason` 在配置页标 ⚠️ + 理由，但**不拦截**；组合约束（`ALLOWED_DOMAINS` + `STATE`/`RESTORE`）也不再禁止，由管理员自行权衡。探测到的风险项进 DTO 且 `configurable=true`，前端照常可填（保留字段兼容），`insert_browser_env` 不再丢弃。前端 / `update_tool` / spawn 前共用同一校验。

17. **`open` URL 只做入口字面检查，不冒充完整 SSRF 边界**：浏览器的威胁模型与 `http_fetch` 不同——后者是「服务端替外部输入发请求」，SSRF 三件套（校验 + DNS pin + 禁重定向/代理）缺一不可；前者是 Agent 自己在浏览，302 / 子资源 / JS fetch / 点击跳转都由 Chromium 发起，入口校验碰不到。因此 `pkg::http::validate_browser_open_url` 只做无网络 I/O 的静态检查：scheme 限 http/https；云元数据端点（169.254.169.254 / 169.254.170.2 / 100.100.100.200 / metadata.google.internal / fd00:ec2::254）**无条件拒绝**；localhost / 字面内网地址**默认放行**（前期效率优先，测本机 dev server 开箱可用），仅管理员显式把 `BrowserConfig.allow_local_network` 设为 `false` 时收紧；Agent 调用参数无法覆盖。**明确不解析 DNS**：入口解析结论对后续请求无效（DNS rebinding），给了反而制造虚假安全感。拒绝时返回结构化 `success:false` + 引导文案，不 spawn。完整边界靠浏览器级 `--allowed-domains`（agent-browser containment，能拦页面流量 / WebSocket / WebRTC）与部署层 egress 隔离，二者不因此项而省略；页面内容的提示注入与副作用审批属于独立风险面，不受本红线覆盖。`file://` 是 scheme 白名单的唯一例外，规则见红线 19。

18. **browser 工具定位是配置管理而非功能封禁**：把需要人配的（命令路径 / 超时 / 启动环境变量 / 可选子命令屏蔽）做成配置页，而不是替用户决定「CLI 哪些功能不许用」。论据：CLI 天然具备 eval/batch/tab/cookies 等能力，硬禁只挡住老实走工具的人——想绕的人走 shell_exec 一样能跑裸命令，白名单形同虚设。故子命令约束用**黑名单** `disabled_commands: Option<Vec<String>>`，默认 `None` = 不屏蔽（开放 CLI 全部）；管理员只把明确有风险的命令（如 eval）列进来屏蔽，不必枚举其余命令，CLI 升级新增命令也默认可用——这比白名单更契合「默认开放 + 配置管理」的定位。skills 参数校验 / `close --all` 拒绝 / screenshot 路径限制全部移除（CLI 原生形态透传，截图给了路径就用、未给才临时路径且仍归档）；`--full` 撑上下文从禁止降为 skill 里的引导。保留的真实边界仅：URL 入口（红线 17）、env 前缀（红线 14）、session 隔离、产物归档、输出截断/超时——这些是字段语义 / 资源管理 / SSRF 纵深，不是功能封禁。`ControlMode::Manual` 经 `dispatch_manual_tool → request_tool_call` 同步转发立即返回，**不是**人工审批。

19. **`file://` 完全对齐 `fs_read`/`fs_write`（同一套作用域校验 + 授权语义）**：本地 HTML 需要 Agent 自行打开并截图。边界不靠"默认禁用"的总开关，而是**路径作用域**——既然 `fs_read` 默认就能读整个数据根、且没做额外功能封禁，浏览器渲染同一批文件不构成新的外带面，做额外限制没意义。故 browser 的 `file://` 分支直接复用 fs 的 `tool_security::fs::resolve_and_validate_path`（与 fs_read 同一函数，同一 `FsAccessMode::Read` 档），获得完全一致的语义：① `canonicalize` 展开符号链接 + 词法消 `..` + `starts_with` 校验，作用域 = `base_data_path` + `allowed_file_dirs`（对齐 fs 的 `additional_allowed_paths`）；② 敏感文件名（`.env`/`.git`/`.ssh` 等）与符号链接按读档规则拒；③ **作用域外不是硬拒**，而是返回 `require_confirmation` + message，让 Agent 停下来向用户请求显式授权——用户同意后把目录加进 `allowed_file_dirs` 即放行，与 fs 的授权路径一致；④ 不接受 `file://host/path` 远程形态（`to_file_path` 失败）。`BrowserConfig.allow_file_url`（默认 `None/true` 放行）仅作**完全禁用**的总开关：管理员显式设 `false` 时任何 `file://` 直接硬拒，Agent 参数不可覆盖。`validate_browser_open_url`（红线 17）只管 http(s) 字面检查，scheme 分派在 browser 工具层：file 走 fs 校验，其余走字面检查。刻意不另起一套 file 校验实现——单一 SSOT，避免与 fs 漂移。


20. **`desktop` 工具：agent-desktop CLI 包装壳，与 browser 同族但配置独立**：新增内置工具 `desktop`（`src/pkg/tool_registry/desktop.rs`）包装 [agent-desktop](https://github.com/vercel-labs/agent-desktop)（Vercel Labs 开源、Apache-2.0、Rust 实现 + C FFI），让 Agent 经 OS 无障碍树（macOS AX / Windows UIA / Linux AT-SPI）操作桌面原生应用，headless 默认不抢光标/焦点。与 browser 同属 agent-browser.dev 家族、命令风格/`skills get`/JSON envelope/snapshot-ref 循环一致，故包装壳完全同构——整串 `command` 透传 + shell 风格分词（`tokenize_command`）+ argv 直拼不经 shell + `--session ai_orz_agent_{agent}` 隔离 + 子命令黑名单 + UTF-8 安全截断全量落盘到 `tools/desktop/logs/` + 截图产物归档（复用 browser 注册的 `ScreenshotStorer`）。**配置独立**：`DesktopConfig`（common/src/config.rs）只含 command/timeout_ms/max_output_bytes/install_hint/disabled_commands 五字段，不共享 BrowserConfig 字段——desktop 无 URL 入口、无 `file://` 场景，故无 env / allow_local_network / allow_file_url。转译引导写进 description（`skills get desktop` / `skills get platform`），不建独立 skill（同红线 18 的取舍）。**tag = `["browser", "desktop"]`**：首 tag 与 browser 相同 → 前端按 `tool_group_key()` 取首 tag 归入同一「browser」分组（一组工具包），次 tag 保留语义区分，将来拆组只改首 tag 一处。与 browser 的技术分水岭：accessibility-tree/DOM 路线（快、省 token、稳定，依赖应用暴露语义）vs screenshot-coordinate 路线（通用但慢、贵、坐标易漂移）；浏览器场景优先 browser（结构化 DOM），desktop 作无 API 原生应用与 canvas/legacy 界面的 fallback，遵循 Plan→Act→Verify 与敏感操作人工确认。
## 五、扩展模式

**场景 A：新增搜索 provider（如博查）**
1. `common/src/config.rs` 加 `[bocha]` 共享段（serde default）→ 2. `common/src/models/identity_credentials.rs` 加 `BochaKey` CredentialKind → 3. 新建 `bocha_search.rs`（含 Resolver trait 定义，对齐 tavily_search）→ 4. `dal/user.rs` 加 `BochaDalCredentialResolver` + init 注入 → 5. `builtin.rs` 注册 + tag `search` → 6. tool_readiness 注册授权探测 → 凭证页/清单/调用三层自动获得。

**场景 B：browser 支持新子命令**
1. CLI 升级后无需改代码——`skills get core` 即为最新命令集，整串 `command` 直接透传 → 2. 黑名单 `disabled_commands` 只列被屏蔽的风险命令，新命令默认可用、无需同步配置 → 3. description 不必枚举命令（引导 Agent 读官方 skill 即可）。

**场景 C：新增外部 CLI 型内置工具（如 ffmpeg_cli）**
1. 新建工具文件（对齐 [gh_cli 包装模式](../../src/pkg/tool_registry/gh_cli.rs)）→ 2. tool_readiness 注册二进制探测 → spawn 前取统一未安装引导 → 3. 注册工厂，清单/调用/绑定三层提示自动获得；如需授权再按场景 A 接双轨。
