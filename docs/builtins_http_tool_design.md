# HTTP Tool Runtime 设计文档

## 概述

HTTP 工具不设计为一个固定暴露给 Agent 的裸 `http_get` / `http_post` 内置工具，而设计为一套**通用 HTTP Tool Runtime**：

- 系统在代码中提供统一 HTTP 请求执行能力；
- 用户通过管理页面注册具体 HTTP 类型工具；
- 每个 HTTP 工具以标准 `ToolPo + HttpToolConfig` 存储在数据库；
- 运行时根据 `ToolProtocol::Http` 动态组装 `HttpCoreTool` 并执行；
- 调用方式由 `ControlMode` 独立控制，第一版 HTTP 工具默认走 `Manual` 消息链路。

核心结论：

```text
HTTP Runtime 是代码内置能力；
HTTP Tool 是数据库驱动的用户/系统注册工具。
```

---

## 核心设计原则

### 1. ToolProtocol 与 ControlMode 正交

`ToolProtocol` 表达工具来源/协议，不决定调用方式：

| ToolProtocol | 含义 |
|---|---|
| `Builtin` | 代码内置工具，由内置工厂创建 |
| `Http` | HTTP 协议工具，由 `ToolPo.config` 驱动 |
| `Mcp` | MCP 协议工具，后续扩展 |

`ControlMode` 表达谁来调用：

| ControlMode | 含义 |
|---|---|
| `Auto` | 进入 Rig tools，由 Rig 原生 tool calling 自动调用 |
| `Manual` | 不进入 Rig，走自建 `ToolCallRequest` / `ToolCallResult` 消息链路 |

因此：

```text
Builtin 不等于 Auto；
Http 不等于 Manual；
是否进入 Rig 只看 ControlMode。
```

第一版用户注册 HTTP 工具默认：

```rust
protocol = ToolProtocol::Http;
control_mode = ControlMode::Manual;
```

### 2. 不暴露裸 HTTP 客户端给 Agent

不推荐让 Agent 直接调用：

```json
{
  "url": "https://...",
  "headers": {...},
  "body": "..."
}
```

推荐用户注册业务语义明确的工具：

```text
github_search_repositories(query, limit)
get_weather(city)
create_linear_issue(title, description)
```

Agent 只提供 `parameters_schema` 中定义的业务参数，URL、method、headers、body 模板由 `ToolPo.config` 固定。

### 3. 构建逻辑统一收敛在工具中心

HTTP Runtime 代码放在工具中心：

```text
src/pkg/tool_registry/http.rs
```

该模块负责：

- 定义 `HttpToolConfig`；
- 定义 `HttpCoreTool`；
- 为每次调用创建带 timeout、redirect policy、DNS pinning、禁用代理的 `reqwest::Client`；
- 根据 `ToolPo` 创建 `Box<dyn CoreTool>`；
- 执行模板渲染、参数 schema 校验、安全校验、HTTP 请求、流式响应上限和响应裁剪。

`ToolCallDao` 不直接构造 HTTP 请求，只继续通过统一入口：

```rust
get_registry().create_tool(po)
```

---

## 目录结构

```text
src/pkg/tool_registry/
├── mod.rs              # ToolRegistry，根据 ToolProtocol 分发创建 CoreTool
├── builtin.rs          # BuiltinToolFactory
├── http.rs             # HttpToolConfig + HttpCoreTool + HTTP Runtime
├── mcp.rs              # MCP 后续扩展
└── handler_adapter/    # Handler 适配工具（如有）

src/service/dao/tool_call/
├── mod.rs              # ToolCallDao trait
└── impl.rs             # assemble_core_tool / execute
```

推荐依赖方向：

```text
ToolCallDao
  → ToolRegistry.create_tool(po)
    → match po.protocol
      → Builtin id factory
      → HTTP protocol factory (`HttpToolFactory.create(po)`)
      → MCP 专用 factory/builder（由 MCP DAL 准备 deps）
```

HTTP Tool 是数据库注册、配置驱动的协议工具，不需要按 tool id 注册 factory；`ToolRegistry` 持有一个协议级 `HttpToolFactory`，默认实现为 `DefaultHttpToolFactory`，用于依赖注入和测试替换。

---

## 用户注册型 HTTP Tool 数据模型

### ToolPo

HTTP 工具仍然是标准 Tool 记录，`HttpToolConfig` **不单独建表、不新增专属字段**，而是作为结构化 JSON 放在 `ToolPo.config` 中：

```rust
ToolPo {
    id,
    name,
    description,
    protocol: ToolProtocol::Http,
    control_mode: ControlMode::Manual,
    parameters_schema, // Agent 可填写的业务参数 schema
    config,            // JSON serialized HttpToolConfig
    status,
    ...
}
```

运行时读取 `ToolPo.config` 并反序列化为 `HttpToolConfig`：

```rust
let config: HttpToolConfig = serde_json::from_value(po.config.clone())?;
```

### HttpToolConfig

第一版字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpToolConfig {
    pub method: String,
    pub url: String,

    pub headers: Option<serde_json::Value>,
    pub query: Option<serde_json::Value>,
    pub body: Option<serde_json::Value>,

    pub timeout_ms: Option<u64>,
    pub response_max_bytes: Option<usize>,

    pub allowed_status_codes: Option<Vec<u16>>,
    pub response_json_pointer: Option<String>,

    pub allowed_domains: Option<Vec<String>>,
    pub blocked_domains: Option<Vec<String>>,
    pub allow_local_network: Option<bool>,
}
```

示例：

```json
{
  "method": "GET",
  "url": "https://api.github.com/search/repositories",
  "query": {
    "q": "{{args.query}}",
    "per_page": "{{args.limit}}"
  },
  "headers": {
    "Accept": "application/vnd.github+json"
  },
  "timeout_ms": 10000,
  "response_max_bytes": 65536,
  "allowed_status_codes": [200],
  "response_json_pointer": "/items"
}
```

Agent 看到的 `parameters_schema`：

```json
{
  "type": "object",
  "properties": {
    "query": { "type": "string", "description": "Search keywords" },
    "limit": { "type": "integer", "description": "Max result count" }
  },
  "required": ["query"]
}
```

---

## 调用链路

### 注册链路

```text
用户页面创建 HTTP Tool
  ↓
Finance Tool 管理面 Handler
  ↓
Finance Domain / ToolProviderManage
  ↓
ToolDal.create_tool()
  ↓
ToolDao 写入 tools 表
  ↓
用户绑定工具到 Agent
```

注册/更新链路会在持久化前执行 HTTP config 安全校验：固定 scheme/authority/host 必须可解析且不可模板化；固定目标命中 localhost/私网/特殊地址、`blocked_domains`，或不满足 `allowed_domains` 时直接拒绝；`headers` / `query` / `body` 模板占位符必须与运行时渲染规则完全一致。

### 执行链路（Manual）

```text
Agent Prompt 展示 manual tools
  ↓
LLM 输出 ToolCallRequest(tool_id, args)
  ↓
Message Consumer 识别 ToolCallRequest
  ↓
ToolDal.call_tool_by_id()
  ↓
ToolDao 读取 ToolPo
  ↓
ToolCallDao.assemble_core_tool()
  ↓
ToolRegistry.create_tool(po)
  ↓
ToolProtocol::Http → HttpToolFactory.create(po) → HttpCoreTool
  ↓
ToolCallDao.execute()
  ↓
HttpCoreTool.call(ctx, args)
  ↓
ToolCallResult 消息写回 Agent
```

### Rig 注入规则

`wrap_for_rig` 只看 `ControlMode`：

```rust
if tool.po.control_mode != ControlMode::Auto {
    continue;
}
```

因此 HTTP 工具只要是 `Manual`，即使实现位于代码内部，也不会进入 Rig。

---

## HttpCoreTool 职责

`HttpCoreTool` 实现 `CoreTool`：

```rust
pub struct HttpCoreTool {
    po: ToolPo,
    config: HttpToolConfig,
}
```

调用流程：

```text
1. 校验 args 符合 parameters_schema
2. 渲染 url/query/headers/body 模板
3. 校验 method/domain/URL 安全策略
4. DNS 解析并做 SSRF 防护，同时将已校验地址 pin 到本次请求的 `reqwest::Client`
5. 使用默认不跟随 redirect 的 reqwest::Client 发请求
6. 流式读取响应并限制响应大小，超过 `response_max_bytes` 立即失败
7. 按 response_json_pointer 裁剪响应
8. 脱敏敏感 header / metadata
9. 返回标准 JSON 结果
```

标准成功返回：

```json
{
  "status": 200,
  "headers": {
    "content-type": "application/json"
  },
  "content_length": 12345,
  "body": { }
}
```

失败返回通过 `ToolError` 表达，由 `ToolCallDao.execute()` 记录到 `ToolCallEntry`。

---

## 安全设计

### SSRF 防护

默认禁止访问非公网 / SSRF 风险地址：

| 范围 | 说明 |
|---|---|
| `127.0.0.0/8` | Localhost |
| `10.0.0.0/8` | Private network |
| `172.16.0.0/12` | Private network |
| `192.168.0.0/16` | Private network |
| `100.64.0.0/10` | Shared address space |
| `169.254.0.0/16` | Link-local / metadata 风险 |
| `0.0.0.0/8` | Invalid/current network |
| `224.0.0.0/4` 等保留/组播/文档/benchmark 网段 | 非公网地址 |
| `::1/128` | IPv6 localhost |
| `fc00::/7` | IPv6 unique local addr |
| `fe80::/10` | IPv6 link-local |
| IPv4-mapped IPv6 | 按映射后的 IPv4 地址重新校验 |
| `64:ff9b::/96`、`2002::/16`、`2001::/32` | IPv6 transition / 映射地址，默认按风险地址处理 |

本地/内网访问采用“默认安全 + 显式授权”策略：

- `blocked_domains` 命中时始终拒绝，优先级最高；
- `localhost`、回环地址、私网地址、link-local / metadata、共享地址、保留/组播等非公网风险地址默认拒绝；
- 域名匹配前会统一 lowercase、去除尾随 `.` 和 IPv6 方括号，避免 `internal.example.com.` 绕过 `blocked_domains`；
- 如果用户注册的 HTTP Tool 本身就是本地服务，必须在 `ToolPo.config` 中显式配置 `"allow_local_network": true`；
- 不建议仅依赖 `allowed_domains = ["localhost"]` 放行本地访问，`allowed_domains` 只表达域名白名单，不表达 SSRF 风险确认；
- `blocked_domains` 与 `allow_local_network=true` 同时出现时，仍以 `blocked_domains` 拒绝为准。

当前运行时已实现的校验优先在发起请求前完成：

```text
URL parse host
  ↓
blocked_domains 命中 → 拒绝
  ↓
localhost/私网/IP 风险地址 且 allow_local_network != true → 拒绝
  ↓
allowed_domains 存在但未命中 → 拒绝
  ↓
DNS resolve 所有 IP
  ↓
任一解析 IP 命中 localhost/私网/link-local/metadata/保留网段等非公网风险网段，且 allow_local_network != true → 拒绝
  ↓
将已校验地址 pin 到本次 reqwest Client，禁用代理后发起请求
```

因此即使配置 URL 使用普通域名，只要 DNS 解析结果落到本地/私网/metadata/保留网段等非公网风险地址，也会在 `request.send().await` 前失败；通过 `resolve_to_addrs` 将已校验地址 pin 到本次请求，避免校验与实际连接之间发生 DNS rebinding；`allow_local_network=true` 只放行本地/私网风险，不能覆盖 `blocked_domains`。

### 参数与模板校验

- 调用前会校验 `parameters_schema.required`；
- 对 `properties` 中声明的基础 JSON Schema 类型（`string/integer/number/boolean/object/array/null`）与 `enum` 做轻量校验；
- `additionalProperties: false` 时拒绝未声明参数；
- URL/query/header/body 模板渲染后如果仍存在任何 `{{...}}` 占位符（包括暂未支持的模板源），会在发起网络请求前失败。

### Redirect 安全策略

HTTP Runtime 默认**不跟随重定向**，`reqwest::Client` 使用 `redirect::Policy::none()`：

- 避免初始 URL 通过校验后，`302/301/307/308 Location` 跳转到 localhost、私网或 metadata 风险地址；
- 3xx 响应会作为普通响应返回并进入 `allowed_status_codes` 校验；第一版默认允许状态码不包含 3xx，因此通常返回 `unexpected http status code: 302`；
- 如果未来确实需要支持跳转，应作为显式配置单独设计，并对每一次跳转目标重复执行 URL/domain/DNS/SSRF 校验。

### 默认限制

| 限制 | 默认值 |
|---|---:|
| 允许 method | `GET`, `POST` |
| 请求超时 | `30s` |
| 响应返回上限 | 默认 `1MB`，配置硬上限 `10MB`，创建工具时拒绝 `0` 或超过硬上限 |
| 最大跳转次数 | `0`（`redirect::Policy::none()`） |

### 脱敏策略

以下字段不得原样写入日志或返回给前端：

```text
Authorization
Cookie
Set-Cookie
X-Api-Key
X-Auth-Token
*token*
*secret*
*password*
```

管理面延续现有策略：

```text
写入接口可以接收 config；
列表响应不返回 config 原文，仅返回 has_config；
详情响应可返回脱敏后的 config：敏感 header/body 字段值统一替换为 [REDACTED]，URL 中的 userinfo 移除，URL query 的所有值统一替换为 [REDACTED]；
parameters_schema 可以返回。
```

管理面 create/update 会在持久化前执行 HTTP Tool 安全校验：仅允许第一版 Manual 控制模式，且 config 必须通过与运行时一致的 method、URL 边界、headers/query/body 模板形状、状态码、JSON Pointer、timeout、响应大小等校验。运行时对外错误信息不得包含渲染后的 URL、header、query/body 值或密钥；HTTP Tool 调用追踪日志中 input/output 也按 [REDACTED] 记录，避免二次落盘泄漏。

---

## 和内置工具的关系

系统仍然可以预置 HTTP 工具，但推荐通过标准 Tool 数据完成：

```text
系统初始化写入 tools 表
  protocol = Http
  control_mode = Manual
  config = HttpToolConfig
```

这类工具与用户页面创建的 HTTP 工具使用完全相同的运行时链路。

不推荐第一版提供通用裸工具：

```text
http_get(url, headers)
http_post(url, body, headers)
```

除非未来明确需要调试型/管理员专用工具，并且加上严格权限与白名单。

---

## 实施计划

### Phase 1：文档与接口确认

- [x] 明确 HTTP Runtime 与 HTTP Tool 的区别；
- [x] 明确 `ToolProtocol` 与 `ControlMode` 正交；
- [x] 明确 HTTP Runtime 放在 `src/pkg/tool_registry/http.rs`；
- [x] 确认 `HttpToolConfig` 第一版字段。

### Phase 2：HTTP Runtime 基础实现

- [x] 在 `src/pkg/tool_registry/http.rs` 实现 `HttpToolConfig`；
- [x] 为每次调用创建带 DNS pinning 的 `reqwest::Client`；
- [x] 实现 `HttpCoreTool`；
- [x] `ToolRegistry.create_tool()` 支持 `ToolProtocol::Http`。

### Phase 3：安全与测试

- [x] SSRF 防护；
- [x] timeout / response size limit；
- [x] header 脱敏；
- [x] GET / POST 测试；
- [ ] `Manual` HTTP 工具不进入 `wrap_for_rig` 测试。

### Phase 4：管理面联调

- [ ] 页面/Handler 创建 HTTP Tool；
- [ ] 绑定到 Agent；
- [ ] 通过 ToolCallRequest 执行；
- [ ] ToolCallResult 回写后，按统计模块轮次预算触发 Agent 下一轮唤醒。

## 管理面脱敏与校验补充

- 写入接口允许接收 HTTP `config`，但 create/update 必须在持久化前执行与运行时一致的保守校验。
- 列表响应不返回 `config` 原文，仅返回 `has_config`。
- 详情响应只返回脱敏后的 `config`：URL userinfo 移除，URL query 所有值统一替换为 `[REDACTED]`；HTTP `headers` / `query` / `body` 的值默认全量替换为 `[REDACTED]`，仅保留字段结构，避免自定义 `auth` / `access_key` / `client_key` 等字段名漏判。
- 运行时对外错误与 tool trace 日志不得包含渲染后的 URL、header、query/body 值或密钥；HTTP Tool trace 的 input/output/error 均记录为 `[REDACTED]`。
