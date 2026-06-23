# MCP Tool Runtime 设计文档

## 概述

MCP（Model Context Protocol）工具不应被建模为单个裸 `mcp_call(server, tool, args)` 内置工具，而应被建模为：

```text
MCP Server 是外部能力 Provider；
MCP Tool 是 Provider 暴露出的具体工具实体；
运行时通过 MCP client runtime 连接 Server 并调用具体 Tool。
```

本设计延续现有 Tool 架构：`ToolProtocol` 表达工具来源/协议，`ControlMode` 表达调用方式。第一版 MCP Tool 默认走 `Manual`，不直接进入 Rig auto tool calling。

---

## 核心结论

1. 新增独立 `mcp_servers` 表保存 MCP Server 连接配置。
2. `tools` 表继续保存具体 MCP Tool，`ToolPo.config` 只保存 `server_id + tool_name` 等绑定信息。
3. `ToolDal` 保持通用基础 DAL，不承载 MCP/HTTP 等协议专属膨胀逻辑。
4. 第一版新增 `McpToolDal`，专门处理 MCP 相关能力：同步 tools、按 server 管理、读取 server config、组装带 MCP 依赖的可调用 `Tool` 实体。
5. MCP client/session 生命周期不单独暴露成上层可见的 `McpClientDao`；它是 MCP tool call 的底层执行能力。
6. 新增 `McpToolCallDaoImpl` 作为 `ToolCallDao` 的协议增强实现：
   - 组合基础 `ToolCallDao`，大部分通用方法直接转发；
   - 主要重写/扩展 MCP CoreTool 构造能力；
   - 持有 `pkg::mcp::McpClientRuntime` 依赖，由 runtime 管理 MCP client/session 生命周期；
   - 构造 MCP Tool 时支持传入 server config、client runtime 等额外参数。
7. MCP Tool 仍注册到 `tool_registry`；工厂构造 MCP Tool 时不强行复用单一 `create_tool(po)` 签名，而是采用 builder 模式或专用 `create_mcp_tool(po, deps)`，由 `McpToolDal` 按需准备并传入参数。

## 数据模型

### `mcp_servers` 表

MCP Server 是外部能力 Provider，需要独立持久化：

```sql
mcp_servers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  transport INTEGER NOT NULL CHECK (transport IN (0, 1)), -- 0=stdio, 1=streamable_http
  config TEXT NOT NULL,          -- JSON serialized McpServerConfig
  status INTEGER NOT NULL CHECK (status IN (0, 1, 2)), -- 0=Deleted, 1=Enabled, 2=Disabled
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  created_by TEXT,
  updated_by TEXT
)

UNIQUE INDEX idx_mcp_servers_active_name_unique ON mcp_servers(name) WHERE status != 0
```

### `McpServerConfig`

```rust
pub struct McpServerConfig {
    // stdio transport
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,

    // streamable http transport
    pub url: Option<String>,
    pub headers: BTreeMap<String, String>,

    // common runtime options
    pub timeout_ms: u64,
    pub connect_timeout_ms: u64,
    pub response_max_bytes: u64,
}

pub enum McpTransport {
    Stdio,
    StreamableHttp,
}
```

安全默认值：

- `env` 默认不继承系统环境，只允许显式配置；
- stdio `command` 不走 shell，只允许 `command + args` 数组；
- HTTP transport 默认拒绝 localhost / 私网 / 特殊地址；
- headers/env/detail/error/log 全部需要脱敏；
- 第一版仅管理员可创建/修改 MCP Server。

### `ToolPo.config` for MCP Tool

具体 MCP Tool 仍是标准 `tools` 表记录：

```rust
ToolPo {
    protocol: ToolProtocol::Mcp,
    control_mode: ControlMode::Manual,
    parameters_schema: input_schema_from_mcp,
    config: McpToolConfig JSON,
    ...
}
```

`ToolPo.config` 只保存工具绑定关系，不复制 server credential：

```rust
pub struct McpToolConfig {
    pub server_id: String,
    pub tool_name: String,
}
```

示例：

```json
{
  "server_id": "filesystem",
  "tool_name": "read_file"
}
```

---

## 代码结构规划

```text
src/models/
└── mcp_server.rs                # McpServerPo + McpServerConfig + McpToolConfig

src/service/dao/
├── mcp_server/
│   ├── mod.rs                   # McpServerDao trait：纯持久化
│   ├── sqlite.rs                # SQLite 实现
│   └── sqlite_test.rs
├── tool/                        # 现有 ToolDao：只负责 ToolPo 持久化
└── tool_call/
    ├── mod.rs                   # ToolCallDao trait：通用 CoreTool 生产/包装/调用
    ├── impl.rs                  # 基础 ToolCallDaoImpl
    ├── mcp.rs                   # McpToolCallDaoImpl：协议增强实现，依赖 pkg::mcp::McpClientRuntime
    └── mcp_test.rs

src/service/dal/
├── mcp_server.rs                # McpServerDal：MCP Server 管理面读写与校验
├── mcp_tool.rs                  # McpToolDal：MCP 专属同步/组装/调用/按 server 管理
└── tool.rs                      # ToolDal：通用 Tool 基础能力，不承载协议膨胀逻辑

src/service/domain/finance/
├── mcp_server_provider.rs       # MCP Server 管理面：CRUD、校验、连接测试
├── mcp_tool_provider.rs         # MCP Tool 管理面：sync/list_by_server/disable/delete/call 编排
└── tool_provider.rs             # 现有通用 Tool 管理面

src/pkg/mcp/
├── mod.rs
├── transport.rs                 # stdio / streamable_http transport 基础封装
├── types.rs                     # McpDiscoveredTool / McpCallResult 等 SDK-neutral 类型
└── redaction.rs                 # MCP 错误/配置/日志脱敏

src/pkg/tool_registry/
└── mcp.rs                       # McpCoreTool + builder/create_mcp_tool(po, deps)
```

## DAO 初始化与连接初始化边界

MCP client/session 生命周期是**工具调用底层能力**，不作为上层可见的独立 DAO 暴露；由 `pkg::mcp::McpClientRuntime` 管理，`McpToolCallDaoImpl` 只持有 runtime 依赖并负责协议路由/装饰。

```text
service::dao::mcp_server::init()
  初始化 McpServerDaoSqliteImpl 单例（纯持久化）

service::dao::tool_call::init()
  初始化基础 ToolCallDaoImpl
  初始化/组合 McpToolCallDaoImpl
  McpToolCallDaoImpl 持有 MCP client runtime 依赖，不直接管理 session 细节

McpToolDal
  读取 ToolPo + McpServerPo
  调用 McpToolCallDaoImpl 的 MCP 专属组装方法
  得到可调用 McpCoreTool
```

因此：

```text
McpServerDao init = 持久化组件初始化
ToolCallDaoImpl = 通用工具调用基础实现
McpToolCallDaoImpl = ToolCallDao 的 MCP 协议增强实现，组合 pkg::mcp::McpClientRuntime
McpToolDal = MCP 专属 DAL，准备 server config 并调用 MCP 增强组装/调用能力
```

这样可以实现：

- 上层使用时只关心 `ToolDal` / `McpToolDal`，不关心 MCP client 如何连接；
- `ToolDal` 保持通用，不因 MCP/HTTP/未来协议膨胀；
- MCP client 是工具调用方式底层，生命周期由 MCP tool call 实现管理；
- MCP Tool 工厂仍由 registry 承载，但构造所需参数由 `McpToolDal` 准备并传入。

## MCP Server 管理面链路

### 创建/更新 Server

```text
Handler: create/update_mcp_server
  ↓
Finance Domain: McpServerProviderDomain
  - 权限校验
  - transport/config 安全校验
  - detail/list 脱敏策略
  ↓
McpServerDal
  ↓
McpServerDao
  ↓
mcp_servers 表
```

创建/更新只修改持久化配置，不在 DAO/DAL 初始化阶段启动连接。后续如果 server 配置变化，需要由 MCP tool call runtime 内部的 session/client 管理组件按 `server_id` 做 invalidate/refresh；连接失败不应污染持久化事务，建议提供单独的 `test_connection` 或 `sync_tools` 动作展示连接结果。

### 同步 MCP Tools

```text
Handler: sync_mcp_tools(server_id)
  ↓
Finance Domain: McpServerProviderDomain.sync_tools
  ↓
McpToolDal.sync_from_server(server_id)
  ↓
McpServerDao.get_by_id(server_id)
  ↓
MCP tool call runtime / rmcp client list_tools(server_config)
  ↓
MCP initialize + tools/list
  ↓
Finance Domain 将 MCP tool metadata 映射为 ToolPo
  ↓
McpToolDal 将 MCP tool metadata 映射/同步为 ToolPo
  ↓
ToolDao upsert tools 表
```

生成的 `ToolPo`：

```rust
ToolPo {
    id: format!("mcp.{server_id}.{tool_name}"),
    protocol: ToolProtocol::Mcp,
    control_mode: ControlMode::Manual,
    parameters_schema: input_schema_from_mcp,
    config: json!({
        "server_id": server_id,
        "tool_name": tool_name,
    }),
    ...
}
```

---

## MCP Tool 实体组装链路

现有工具实体结构是：

```rust
pub struct Tool {
    pub po: ToolPo,
    pub our_tool: Box<dyn CoreTool + Send + Sync>,
}
```

MCP Tool 组装建议沿用现有链路：

```text
ToolDal.get_by_id(ctx, tool_id)
  ↓
ToolDao.get_by_id(ctx, tool_id) -> ToolPo
  ↓
ToolCallDao.assemble_core_tool(&po)
  ↓
match po.protocol
  Builtin => builtin factory
  Http    => HttpToolFactory.create(po)
  Mcp     => create_mcp_tool(po, deps) / McpToolBuilder
  ↓
Tool { po, our_tool }
```

`create_mcp_tool(po, deps)` / `McpToolBuilder` 负责：

1. 反序列化 `McpToolConfig`；
2. 校验 `server_id/tool_name` 非空；
3. 创建 `McpCoreTool { po, config }`；
4. 不在这里直接读取 `mcp_servers` 表。

---

## `McpToolDal` 设计

`McpToolDal` 第一版就新增，用来拆分 MCP 协议专属逻辑，避免通用 `ToolDal` 随协议数量增长而膨胀。

### 职责

`McpToolDal` 负责所有 MCP 相关的 DAL 级编排：

- 读取 `ToolPo.config` 中的 `server_id + tool_name`；
- 读取对应 `McpServerPo/McpServerConfig`；
- 调用 MCP tool call 增强实现组装带运行时依赖的 `McpCoreTool`；
- sync MCP tools：`tools/list` → upsert `ToolPo`；
- 按 server 查询/禁用/删除/stale reconcile；
- MCP 专属 manual call。

### 组合关系

```rust
pub trait McpToolDal: Send + Sync {
    async fn sync_from_server(
        &self,
        ctx: &RequestContext,
        server_id: String,
    ) -> Result<Vec<Tool>, AppError>;

    async fn get_by_id(
        &self,
        ctx: &RequestContext,
        tool_id: String,
    ) -> Result<Option<Tool>, AppError>;

    async fn call_by_tool_id(
        &self,
        ctx: &RequestContext,
        tool_id: String,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, ToolError>;
}

pub struct McpToolDalImpl {
    tool_dao: Arc<dyn ToolDao + Send + Sync>,
    mcp_server_dao: Arc<dyn McpServerDao + Send + Sync>,
    mcp_tool_call_dao: Arc<dyn McpToolCallDao + Send + Sync>,
}
```

### 与 `ToolDal` 的关系

- `ToolDal` 是通用基础 DAL，继续负责通用 Tool 查询/组装/调用；
- `McpToolDal` 是协议专属 DAL，处理 MCP 的特殊数据链路；
- 上层 Domain 可以根据 protocol 路由：普通工具走 `ToolDal`，MCP 相关操作走 `McpToolDal`；
- 不把 MCP 专属逻辑塞进通用 `ToolDal`，避免后续协议继续膨胀。


---

## MCP Tool 工厂构造方式

MCP Tool 与 Builtin/HTTP 的差异是：仅靠 `ToolPo` 不足以构造可调用实例，还需要 MCP server 配置与 client runtime/session 能力。因此 MCP 不强行复用单一：

```rust
create_tool(po)
```

推荐两种可选实现，第一版优先选择更直接的专用方法。

### 方案 A：专用构造方法（第一版推荐）

```rust
pub fn create_mcp_tool(
    po: &ToolPo,
    deps: McpToolDeps,
) -> Result<Box<dyn CoreTool + Send + Sync>>;

pub struct McpToolDeps {
    pub server: McpServerPo,
    pub client_runtime: Arc<McpClientRuntime>,
}
```

优点：

- 签名直观，清楚表达 MCP 需要额外依赖；
- 不污染通用 `ToolRegistry::create_tool(po)`；
- `McpToolDal` 可以明确准备 `server + client_runtime` 后再构造；
- 测试更容易直接构造 fake deps。

### 方案 B：Builder 模式（后续扩展）

当 MCP 构造参数继续增加时，可以改为 builder：

```rust
pub struct McpToolBuilder {
    po: ToolPo,
    server: Option<McpServerPo>,
    client_runtime: Option<Arc<McpClientRuntime>>,
    trace_policy: Option<ToolTracePolicy>,
}

impl McpToolBuilder {
    pub fn new(po: ToolPo) -> Self;
    pub fn server(mut self, server: McpServerPo) -> Self;
    pub fn client_runtime(mut self, runtime: Arc<McpClientRuntime>) -> Self;
    pub fn trace_policy(mut self, policy: ToolTracePolicy) -> Self;
    pub fn build(self) -> Result<Box<dyn CoreTool + Send + Sync>>;
}
```

优点：

- 参数可选/可扩展；
- 后续增加 trace policy、timeout override、server capability cache 时不用频繁修改函数签名；
- 更适合 MCP 这种依赖逐步增加的协议工具。

### 当前约定

第一版采用：

```rust
create_mcp_tool(po, deps)
```

如果后续 MCP 构造参数超过 3 个或出现多组可选参数，再迁移为 `McpToolBuilder`。通用 `create_tool(po)` 仍保留给只依赖 `ToolPo` 的协议；MCP 由 `McpToolDal` 调用专用构造路径。

## `McpToolCallDaoImpl` 设计

`McpToolCallDaoImpl` 是 `ToolCallDao` 的 MCP 协议增强实现。Rust 没有传统继承，这里采用组合/装饰器模式实现“继承基础实现 + 重写部分方法”。

```rust
pub trait McpToolCallDao: ToolCallDao {
    fn assemble_mcp_core_tool(
        &self,
        po: &ToolPo,
        server: &McpServerPo,
    ) -> Result<Option<Box<dyn CoreTool + Send + Sync>>>;

    async fn list_mcp_tools(
        &self,
        server: &McpServerPo,
    ) -> Result<Vec<McpDiscoveredTool>, ToolError>;

    fn invalidate_mcp_server(&self, server_id: &str);
}

pub struct McpToolCallDaoImpl {
    base: Arc<dyn ToolCallDao + Send + Sync>,
    mcp_client_runtime: Arc<McpClientRuntime>,
}
```

转发规则：

```rust
impl ToolCallDao for McpToolCallDaoImpl {
    fn assemble_core_tool(&self, po: &ToolPo) -> Result<Option<Box<dyn CoreTool + Send + Sync>>> {
        if po.protocol == ToolProtocol::Mcp {
            // MCP 需要 server config 等额外参数，不能只靠 ToolPo 构造；
            // 常规入口返回 None 或明确错误，由 McpToolDal 调 assemble_mcp_core_tool。
            return Ok(None);
        }
        self.base.assemble_core_tool(po)
    }

    fn wrap_for_rig(&self, tools: &[Tool], ctx: RequestContext) -> Vec<Box<dyn ToolDyn>> {
        self.base.wrap_for_rig(tools, ctx)
    }

    async fn call_manual(&self, ctx: RequestContext, tool: &Tool, args: Value)
        -> Result<(Value, ToolCallEntry), ToolError>
    {
        self.base.call_manual(ctx, tool, args).await
    }
}
```

MCP 专属构造：

```rust
impl McpToolCallDao for McpToolCallDaoImpl {
    fn assemble_mcp_core_tool(
        &self,
        po: &ToolPo,
        server: &McpServerPo,
    ) -> Result<Option<Box<dyn CoreTool + Send + Sync>>> {
        let deps = McpToolDeps {
            server: server.clone(),
            client_runtime: self.mcp_client_runtime.clone(),
        };
        pkg::tool_registry::mcp::create_mcp_tool(po, deps).map(Some)
    }
}
```

这样 MCP client/session 生命周期完全是 tool call 底层细节，上层只通过 `McpToolDal` 获得完整 `Tool` 或执行调用。

## 运行时调用链路

```text
LLM 输出 ToolCallRequest(tool_id, args)
  ↓
Message Consumer / Runtime Domain
  ↓
根据 ToolProtocol 路由：MCP 走 McpToolDal
  ↓
McpToolDal.call_by_tool_id(ctx, tool_id, args)
  ↓
ToolDao -> ToolPo(protocol=Mcp, config={server_id, tool_name})
  ↓
McpServerDao -> McpServerPo / McpServerConfig
  ↓
McpToolCallDao.assemble_mcp_core_tool(po, server)
  ↓
registry::mcp::McpToolBuilder / create_mcp_tool(po, deps) -> McpCoreTool
  ↓
ToolCallDao.call_manual(ctx, tool, args)
  ↓
McpCoreTool.call
  ↓
McpClientRuntime -> rmcp client -> MCP Server tools/call
  ↓
MCP result
  ↓
结果脱敏/截断/标准化
  ↓
ToolCallResult 写回消息链路
```

`McpCoreTool` 不直接查询 DAO；它在构造时已经由 `McpToolDal` / `McpToolCallDaoImpl` 注入了执行所需依赖：

```rust
pub struct McpCoreTool {
    po: ToolPo,
    config: McpToolConfig,
    server: McpServerPo,
    client_runtime: Arc<McpClientRuntime>,
}
```

调用时只做协议执行：

```rust
impl CoreTool for McpCoreTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value, ToolError> {
        self.client_runtime
            .call_tool(&self.server, &self.config.tool_name, args)
            .await
            .map_err(redact_mcp_error)
    }
}
```

## 连接生命周期

### MVP：按需连接 + 缓存

连接生命周期归属 `McpToolCallDaoImpl` 内部的 `McpClientRuntime`：

```text
McpCoreTool.call
  ↓
McpClientRuntime.get_or_connect(server)
  ↓
没有 session：使用 server config 连接并 initialize
  ↓
tools/call
  ↓
保留 session，供后续复用
```

### 更新/删除 Server

```text
update_mcp_server
  ↓
McpServerDao.update
  ↓
McpToolCallDao.invalidate_mcp_server(server_id)
  ↓
下次调用按新配置重连
```

```text
delete_mcp_server
  ↓
McpToolCallDao.invalidate_mcp_server(server_id)
  ↓
McpServerDao.delete
  ↓
McpToolDal.disable/delete tools by server_id
```

## 官方 SDK 选择

优先使用官方 Rust SDK：

```toml
rmcp = { version = "1.7", default-features = false, features = [
  "client",
  "transport-child-process",
  "transport-streamable-http-client-reqwest",
  "reqwest",
] }
```

选择原因：

- 官方仓库：`modelcontextprotocol/rust-sdk`；
- 同时支持 MCP client / server；
- 支持 stdio child process 与 streamable HTTP client；
- 后续如果 ai_orz 自身要暴露 MCP Server，也可以继续复用。

具体 feature 需要通过 spike 确认，避免一次性引入 server 侧不必要依赖。

---

## 安全与脱敏规则

MCP 安全边界比 HTTP Tool 更严格，因为 stdio MCP Server 等价于启动本地进程。

### stdio transport

- 禁止 shell 拼接；
- 使用 `command + args` 数组；
- 默认不继承完整环境变量；
- env 必须显式配置，detail/list/log/error 必须脱敏；
- 第一版建议 command allowlist，例如 `npx`、`uvx` 或系统预置路径；
- connect/call timeout 必须有默认值和硬上限；
- stdout/stderr/error 必须限制大小并脱敏。

### streamable HTTP transport

- 默认拒绝 localhost / 私网 / 特殊地址；
- 默认不泄漏 URL query/header；
- headers detail/log/error 脱敏；
- timeout 与响应大小限制；
- 后续参考 HTTP Tool 的 SSRF 防护策略。

### tool result

- MCP result 进入 trace/message 前必须限制大小；
- HTTP/MCP 这类外部工具默认 trace input/output/error 可配置为脱敏或截断；
- 第一版建议 MCP trace input/output/error 默认 `[REDACTED]`，等有 tool-level policy 后再放宽。

---

## 分阶段落地计划

### Phase 0：文档与 spike

- 补充本设计文档；
- 用 `rmcp` 做最小 spike：
  - stdio connect；
  - `tools/list`；
  - `tools/call`；
  - 确认 API、feature、错误类型与 async 生命周期。

### Phase 1：MCP Server 持久化

状态：DAO 持久化子阶段已完成。

- ✅ 新增 `McpServerPo/McpServerConfig`；
- ✅ 新增 `mcp_servers` migration（active name 唯一索引、transport/status CHECK 约束）；
- ✅ 新增 `McpServerDao` + SQLite 实现；
- ✅ 覆盖 insert/find/query/update/delete、软删除默认过滤、显式查询 Deleted、offset-only 查询、软删除后重建同名记录等单元测试；
- ⏳ `McpServerDal` 与管理面 create/update/detail/list/delete 后续单独接入，先不真实连接。

### Phase 2：MCP Tool config + registry stub

- 新增 `McpToolConfig { server_id, tool_name }`；
- 在 `pkg::tool_registry::mcp` 提供 `create_mcp_tool(po, deps)` 或 stub；
- registry 对 `ToolProtocol::Mcp` 先明确走 stub/依赖注入边界；
- 不在通用 `ToolDal` 膨胀 MCP 专属逻辑。

### Phase 3：McpToolDal / McpToolCallDaoImpl 骨架

- 新增 `McpToolDal` 处理 MCP 专属同步/按 server 管理；
- 新增 `McpToolCallDaoImpl` 组合基础 `ToolCallDao`，通用方法转发；
- `McpToolCallDaoImpl` 组合 `pkg::mcp::McpClientRuntime`，client/session lifecycle 由 pkg runtime 管理；
- 支持错误脱敏和 timeout 骨架。

### Phase 4：接入官方 rmcp 并同步 MCP Tools

- Finance Domain 编排 `sync_mcp_tools(server_id)`；
- `tools/list` 映射为 `ToolPo`；
- upsert `ToolProtocol::Mcp` tools；
- 默认 `ControlMode::Manual`。

### Phase 5：安全、管理面和完整测试

- Finance Domain 编排 MCP Server/Tool 管理面；
- `McpCoreTool` manual call 通过 MCP tool call runtime 调用 rmcp；
- trace/error/result 脱敏与截断；
- session cache、reconnect、health check；
- server update/delete 后 session invalidate；
- 并发调用策略。

### 后续增强：连接生命周期增强

- session cache；
- reconnect；
- health check；
- server update/delete 后 session invalidate；
- 并发调用策略。

---

## 待确认问题

1. 第一版是否只支持 `stdio`，还是同时支持 `streamable_http`？
2. stdio `command` 是否采用 allowlist？allowlist 放配置还是代码常量？
3. MCP Server 是否只允许管理员配置？
4. MCP Tool 同步时，远端删除的 tool 是禁用、软删除，还是保留 stale 状态？
5. MCP trace 默认全脱敏是否接受？是否需要按 server/tool 配置 trace policy？
6. `McpToolDal` 是否等 Phase 3 后再根据同步复杂度决定，而不是第一版立即新增？
