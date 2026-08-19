# 网络搜索与浏览器工具设计

> 🎯 **定位**：① 决策快照 — 为 Agent 增加网络搜索与浏览器自动化两类内置工具的选型、授权双轨制、环境就绪提示决策
> 状态：定稿（2026-08-18）
> 触发场景：新增搜索 provider 工具、扩展浏览器子命令、为工具接入授权（API key/token）、处理外部 CLI 未安装提示时先读本文
>
> 关联文档：
> - [plan 阶段一：网络搜索工具](../plan/网络搜索工具.md) — 阶段一落地路径
> - [plan 阶段二：浏览器工具与环境就绪提示](../plan/浏览器工具与环境就绪提示.md) — 阶段二落地路径
> - [AGENTS.md](../../AGENTS.md) — 分层架构与工具注册规范
> - [docs/LAYERED_ARCHITECTURE_PRACTICE.md](../LAYERED_ARCHITECTURE_PRACTICE.md) — pkg 层无业务感知原则

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
| 5 | browser 工具粒度？ | 单工具 + `command` 子命令参数，子命令白名单校验 | 对齐 `lark_cli`/`gh_cli` 模式，控制工具清单膨胀；白名单防危险子命令（如脚本执行类）透传 |
| 6 | 授权缺失 / CLI 未安装，在哪些接触点提示？ | **三层就绪提示体系**：① 工具清单/查询接口附带 `runtime_ready` 预检标志（绑定与选择时前端可见）② 前端 Agent 详情「工具与技能」页对未就绪工具渲染警示 badge ③ 调用时返回结构化引导错误兜底（`cli_not_installed` / `api_key_missing`，含双路径指引） | 只靠调用时报错 = 用户绑定时毫无感知，Agent 跑到一半才失败；三层覆盖「选工具 → 绑定 → 执行」全链路；工具清单稳定不隐藏（绑定关系不因环境漂移） |
| 7 | `runtime_ready` 预检怎么做？ | 轻量只读探测 + TTL 缓存，**带用户上下文**：CLI 型 → 二进制可寻址（config 绝对路径优先，PATH 兜底）；key 型 → 共享 config key 非空 **或** 当前用户凭证库含对应 kind。结果枚举 `ready / not_ready{reason, hint} / unknown`，探测异常一律 `unknown` | key 型授权是用户相关的（双轨制），就绪状态必须按当前查看者判定；只读探测无副作用；TTL 缓存避免每次列表都探测；`unknown` 不阻塞接口（best-effort） |
| 8 | 控制模式？ | `tavily_search` = Auto；`browser` = Manual | 搜索只读低危可自动执行；浏览器可产生真实网络副作用（登录/提交/下载）需人工确认 |
| 9 | 浏览器 daemon 会话隔离？ | 固定 `--session ai-orz-agent-{agent_id}` | 不与用户手工 agent-browser 会话互踩；Agent 之间相互隔离 |
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
│ params: query/depth/max_results         │   │ params: command(白名单)+args    │
│   ↓                                     │   │   ↓                             │
│ 授权解析（双轨，决策 1）：                │   │ tool_readiness 二进制探测        │
│   ① TavilyCredentialResolver            │   │   缺失 → cli_not_installed 引导  │
│      （pkg trait，DAL 实现注入）          │   │   ↓                             │
│      → 用户凭证库 TavilyKey（个人）       │   │ tokio::Command spawn            │
│   ② [tavily].api_key（共享兜底）          │   │   --session ai-orz-agent-{aid}  │
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
| `tests/web_search_tool_test.rs` / `tests/browser_tool_test.rs` | 集成测试（沿用现有集成测试布局） |

## 四、关键边界 / 行为红线

1. **授权双轨铁律**：新工具引入任何授权（key/token）必须同时评估两轨——共享走工具 config、个人走用户凭证库（新 CredentialKind + Resolver 注入）；**禁止**只做工具 config 单轨而把个人凭证塞进共享配置。
2. 授权解析顺序固定：**用户凭证优先 → 共享 config 兜底**；皆缺失时 `api_key_missing` 引导必须给双路径（绑个人凭证 / 配共享 key），由用户选择。
3. **禁止**新建 SearchProvider/通用搜索抽象层；新增搜索后端 = 新增独立工具文件（+ 独立 CredentialKind，如需授权）。
4. `tavily_search` 只读、Auto 模式；`browser` 一律 Manual 人工确认。
5. spawn 错误必须分类：`NotFound` → `cli_not_installed`（含安装命令与配置指引）；`PermissionDenied` → 权限提示；禁止统一泛化报错。
6. readiness 探测**只读**（不 spawn 长驻进程、不发真实网络请求；授权可达 = 共享 key 非空或用户凭证库含 kind，不解密不验证有效性）；key 型探测必须带用户上下文；结果带 TTL 缓存；探测异常返回 `unknown`，不得让列表接口失败。
7. `runtime_ready` 是附加信息而非硬约束：未就绪不阻止绑定（绑定关系稳定），只做提示。
8. browser 子命令**白名单**校验（open/read/snapshot/click/fill/type/press/scroll/screenshot/wait/close 等原子操作），脚本执行类（eval 等）不开放；不经 shell 直接 argv 拼接。
9. browser 固定 headless，不向 Agent 暴露 headed/可视化参数。
10. 输出超时与截断上限沿用 config 默认值；stdout/stderr 合并返回。
11. 截图一律落产物存储返回引用，禁止 base64 内嵌返回。
12. 新配置段必须 serde default，且默认配置模板（`DEFAULT_CONFIG_EMBEDDED`）同步更新，保证旧配置文件无损升级。
13. `tool_readiness` 与各 `CredentialResolver` 保持 pkg 层无业务感知：探测/解析经 trait 接口注入实现，pkg 不 import DAL/DAO。

## 五、扩展模式

**场景 A：新增搜索 provider（如博查）**
1. `common/src/config.rs` 加 `[bocha]` 共享段（serde default）→ 2. `common/src/models/identity_credentials.rs` 加 `BochaKey` CredentialKind → 3. 新建 `bocha_search.rs`（含 Resolver trait 定义，对齐 tavily_search）→ 4. `dal/user.rs` 加 `BochaDalCredentialResolver` + init 注入 → 5. `builtin.rs` 注册 + tag `search` → 6. tool_readiness 注册授权探测 → 凭证页/清单/调用三层自动获得。

**场景 B：browser 放开新子命令**
1. 白名单数组追加子命令名 → 2. description 同步枚举说明 → 3. 参数透传逻辑零改动（子命令即参数）。

**场景 C：新增外部 CLI 型内置工具（如 ffmpeg_cli）**
1. 新建工具文件（对齐 [gh_cli 包装模式](../../src/pkg/tool_registry/gh_cli.rs)）→ 2. tool_readiness 注册二进制探测 → spawn 前取统一未安装引导 → 3. 注册工厂，清单/调用/绑定三层提示自动获得；如需授权再按场景 A 接双轨。
