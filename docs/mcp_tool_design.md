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
   - 持有 `pkg::tool_registry::mcp::McpClientRuntime` 依赖，由 runtime 管理 MCP client/session 生命周期；
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
└── mcp_server.rs                # McpServerPo + McpServerConfig

src/service/dao/
├── mcp_server/
│   ├── mod.rs                   # McpServerDao trait：纯持久化
│   ├── sqlite.rs                # SQLite 实现
│   └── sqlite_test.rs
├── tool/                        # 现有 ToolDao：只负责 ToolPo 持久化
└── tool_call/
    ├── mod.rs                   # ToolCallDao trait：通用 CoreTool 生产/包装/调用
    ├── impl.rs                  # 基础 ToolCallDaoImpl
    ├── mcp.rs                   # McpToolCallDaoImpl：协议增强实现，依赖 pkg::tool_registry::mcp::McpClientRuntime
    └── mcp_test.rs

src/service/dal/
├── mcp_server.rs                # McpServerDal：MCP Server 管理面读写与校验
├── mcp_tool.rs                  # McpToolDal：MCP 专属同步/组装/调用/按 server 管理
└── tool.rs                      # ToolDal：通用 Tool 基础能力，不承载协议膨胀逻辑

src/service/domain/finance/
├── mcp_server_provider.rs       # MCP Server 管理面：CRUD、校验、连接测试
├── mcp_tool_provider.rs         # MCP Tool 管理面：sync/list_by_server/disable/delete/call 编排
└── tool_provider.rs             # 现有通用 Tool 管理面

src/pkg/tool_registry/
└── mcp.rs                       # McpClientRuntime + McpCoreTool + create_mcp_tool(po, deps)
```

## DAO 初始化与连接初始化边界

MCP client/session 生命周期是**工具调用底层能力**，不作为上层可见的独立 DAO 暴露；当前内聚在 `pkg::tool_registry::mcp::McpClientRuntime`，由 `McpToolCallDaoImpl` 持有并负责失效/后续 session 管理。`McpToolDal` 只组合 DAO 单例，不自行创建 base `ToolCallDao` 或新的 `McpClientRuntime`。

```text
service::dao::mcp_server::init()
  初始化 McpServerDaoSqliteImpl 单例（纯持久化）

service::dao::tool_call::init()
  通过单个 OnceLock 原子初始化一次 McpToolCallDaoImpl(base, McpClientRuntime)
  其中 base 是基础 ToolCallDaoImpl
  同一个 McpToolCallDaoImpl 同时暴露为：
    - tool_call::dao()      -> Arc<dyn ToolCallDao>
    - tool_call::mcp_dao()  -> Arc<dyn McpToolCallDao + Send + Sync>

service::dal::mcp_tool::init()
  new(tool::dao(), mcp_server::dao(), tool_call::mcp_dao())
  不调用 tool_call::new()
  不调用 new_mcp_tool_call_dao(...)
  不创建第二份 McpClientRuntime

McpToolDal
  读取 ToolPo + McpServerPo
  调用同一个全局 McpToolCallDao 的 MCP 专属组装/失效方法
  得到可调用 McpCoreTool
```

因此：

```text
McpServerDao init = 持久化组件初始化
ToolCallDaoImpl = 通用工具调用基础实现，只创建一次并作为 MCP 增强 DAO 的 base
McpToolCallDaoImpl = ToolCallDao 的 MCP 协议增强实现，拥有 pkg::tool_registry::mcp::McpClientRuntime 生命周期
McpToolDal = MCP 专属 DAL，准备 server config 并调用全局 MCP ToolCall DAO，不拥有 runtime 生命周期
```

这样可以实现：

- 上层使用时只关心 `ToolDal` / `McpToolDal`，不关心 MCP client 如何连接；
- `ToolDal` 保持通用，不因 MCP/HTTP/未来协议膨胀；
- MCP client/session/process/cache 是工具调用方式底层，生命周期由 `McpToolCallDaoImpl` 统一管理；
- `McpToolDal.invalidate_server(server_id)` 与 MCP Tool 实际调用链路命中同一个 runtime/cache，避免双 runtime / 双缓存；
- MCP Tool 工厂仍由 registry 承载，但构造所需参数由 `McpToolDal` 准备并传入。

## MCP Server 管理面链路

### 创建/更新 Server

```text
Handler: create/update_mcp_server
  ↓
Finance Domain: McpServerProviderManage
  - 权限校验（第一版只在 Domain 层预留边界，不下沉 DAO）
  - transport/config 安全校验
  - detail/list 脱敏策略
  ↓
McpServerDal
  - 使用业务实体 `McpServer`，不向 Domain/Handler 暴露 `McpServerPo`
  - 执行创建/更新前的最小配置校验
  - 更新/删除成功后通知 MCP runtime invalidate server cache
  ↓
McpServerDao
  - 只负责 `mcp_servers` 纯持久化 CRUD/query/status
  ↓
mcp_servers 表
```

创建/更新只修改持久化配置，不在 DAO/DAL 初始化阶段启动连接。后续如果 server 配置变化，由 MCP tool call runtime 内部的 session/client 管理组件按 `server_id` 做 invalidate/refresh；连接失败不应污染持久化事务，使用单独的 `test_connection` 或 `sync_tools` 动作展示连接结果。

### MCP Server 管理面增量实施方案

本阶段按“DAL → Finance Domain → Handler/API”的顺序小步接入，避免一次性把管理面、同步、HTTP runtime、安全策略混在一起。

#### Batch 1：`McpServer` 业务实体 + `McpServerDal`

目标：让上层可以用业务实体管理 MCP Server，同时 DAO 仍只暴露纯持久化。

涉及文件：

- `src/models/mcp_server.rs`
  - 新增 `McpServer { po: McpServerPo }` 业务实体；
  - 提供 `new_stdio(...)` / `new_streamable_http(...)` 或 `from_po(...)` 等轻量构造/转换；
  - 保留 `McpServerPo` 作为存储细节。
- `src/service/dal/mcp_server.rs`
  - 新增 `McpServerDal` trait：`create/find_by_id/query/update/delete/set_status`；
  - 组合 `McpServerDao`，只做 DAL 级业务校验与 PO/entity 转换；
  - stdio 第一版要求 `command` 非空；
  - streamable HTTP 第一版在 SSRF/header/redirect 安全策略落地前拒绝创建/更新，runtime 继续保持 not implemented；
  - 更新/删除后调用 `McpToolCallDao.invalidate_mcp_server(server_id)`，不直接创建 runtime。
- `src/service/dal/mod.rs`
  - 挂载 `mcp_server` 模块、测试模块和 init。

首个 TDD 合约：`McpServerDal.create` 能创建合法 stdio server 并通过 `find_by_id` 返回业务实体；缺少 stdio `command` 时返回 `BadRequest` 且不落库。

#### Batch 2：Finance Domain MCP Server 管理面

目标：把 MCP Server 纳入 Finance Domain 的外部能力配置管理面。

涉及文件：

- `src/service/domain/finance/mod.rs`
  - 新增 `McpServerProviderManage` trait；
  - `FinanceDomain` 增加 `mcp_server_provider_manage()`；
  - `FinanceDomainImpl` 注入 `Arc<dyn McpServerDal>`。
- `src/service/domain/finance/mcp_server_provider.rs`
  - 实现 create/get/query/update/delete/set_status；
  - `sync_mcp_tools(server_id)` 调用 `McpToolDal.sync_from_server(ctx, server_id)`。

#### Batch 3：Handler/API 接入

目标：HTTP API 只作为用户 action 入口，不承载文件/持久化/运行时逻辑。

当前已接入 MCP Server 管理 API，路由遵循 Finance Domain 统一前缀：

```text
POST   /api/v1/finance/mcp-servers
GET    /api/v1/finance/mcp-servers/{id}
GET    /api/v1/finance/mcp-servers
PUT    /api/v1/finance/mcp-servers/{id}
PUT    /api/v1/finance/mcp-servers/{id}/status
DELETE /api/v1/finance/mcp-servers/{id}
```

Handler 仅做 DTO ↔ 业务实体/命令转换，然后调用 Finance Domain。`common/src/api/mcp_server.rs` 提供前后端共享 DTO，`common/src/enums/mcp_server.rs` 提供 API 枚举；Handler 返回 Domain/DAL 提供的管理面脱敏视图，不直接暴露原始 `McpServerPo.config`。`streamable_http` 创建/更新仍由 Domain/DAL 校验拒绝，直到 SSRF/header/redirect 等安全策略落地。

列表 API 使用统一分页契约：`ListMcpServersRequest.pagination: PaginationParams { limit, offset }`，DAO/DAL/Domain 的 `query` 返回 `PagedResult<T> { items, total }`。`total` 与 `items` 复用同一组查询过滤条件，分页只作用于 `items`，避免独立 count 查询条件漂移。默认查询排除软删除记录；显式 `status = Deleted` 时可查询已删除记录用于管理/测试。

软删除记录不可通过 update/set_status 复活：DAO 层写操作带 `status != Deleted` 条件，恢复必须通过后续单独设计的明确 restore action，而不能被普通状态更新旁路。

MCP Tool 同步/查询管理面已在后续增量接入，复用同一 Finance Domain 前缀：

```text
POST   /api/v1/finance/mcp-servers/{server_id}/tools/sync
GET    /api/v1/finance/mcp-servers/{server_id}/tools
```

其中 `sync-tools` 仅负责触发 `tools/list` 同步并返回本次同步数量；按 server 查询 tools 使用统一分页契约 `ListMcpToolsByServerRequest.pagination: PaginationParams { limit, offset }`，并返回 `ListMcpToolsByServerResponse { tools, total }`。

Handler 只做 DTO 映射，业务编排进入 Finance Domain，再由 `McpToolDal` 调用 `ToolDao` 与 MCP runtime；不会在 API/Handler 层读取或暴露 `McpServerPo.config` 中的 command/env/headers/url 等连接配置。

#### Batch 4：安全与可观测性补强

- detail/list/log/error 对 `env`、`headers`、URL query 做脱敏；管理面更新需保留 `[REDACTED]` 占位符对应的既有敏感值，避免 read-modify-update 覆盖真实密钥；
- stdio command allowlist 是否启用继续保留决策点；
- trace input/output/error 默认 `[REDACTED]`，后续由 tool-level trace policy 放宽；
- streamable HTTP runtime 在 SSRF/header/redirect 策略完成前继续显式 `not implemented`。

### 同步 MCP Tools

当前实现状态：stdio transport 已支持通过 rmcp 初始化 session 并调用 `tools/list`；streamable HTTP transport 在 SSRF/header 安全策略落地前仍显式返回 `not implemented`。

```text
Handler: sync_mcp_tools(server_id)
  ↓
Finance Domain: McpServerProviderDomain.sync_tools
  ↓
McpToolDal.sync_from_server(ctx, server_id)
  ↓
McpServerDao.find_by_id(ctx, server_id)
  ↓
McpToolCallDao.list_mcp_tools(server)
  ↓
McpClientRuntime.list_tools(server)
  ↓
MCP initialize + tools/list
  ↓
McpToolDal 将 remote tool metadata 映射/同步为 ToolPo
  ↓
ToolDao create/update tools 表
```

生成的 `ToolPo` 规则：

```rust
ToolPo {
    id: format!("mcp.{server_id}.{tool_name}"),
    name: format!("mcp.{server_id}.{tool_name}"),
    protocol: ToolProtocol::Mcp,
    control_mode: ControlMode::Manual,
    parameters_schema: input_schema_from_mcp,
    config: json!({
        "server_id": server_id,
        "tool_name": tool_name,
    }),
    tags: vec!["mcp", server_id, tool_name],
    ...
}
```

同步 upsert 约定：

- 不存在则创建新的标准 `ToolPo`；
- 已存在则先校验现有记录必须是 `ToolProtocol::Mcp`，且 `ToolPo.config.server_id/tool_name` 与本次同步目标一致；否则返回 `Conflict`，避免 id 碰撞覆盖其他工具；
- 校验通过后更新名称/描述/schema/config/tags 等可同步元数据；
- 已存在记录保留 `created_at`、`created_by` 和当前 `status`，`updated_by` 使用当前 `RequestContext.user_id`；
- `ToolPo.config` 只保存 `server_id/tool_name` 绑定关系，不复制 `McpServerPo.config` 中的 command、env、headers、url 等连接配置或敏感信息；
- `ToolDaoSqliteImpl::create_tool/update_tool` 必须持久化 `control_mode`，确保 MCP 默认 `Manual` 不会落库丢失。

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
McpToolDal.get_by_id(ctx, tool_id)
  ↓
ToolDao.get_by_id(ctx, tool_id) -> ToolPo
  ↓
McpServerDao.find_by_id(ctx, server_id) -> McpServerPo
  ↓
McpToolCallDao.assemble_mcp_core_tool(&po, &server)
  ↓
match po.protocol
  Builtin => builtin factory
  Http    => HttpToolFactory.create(po)
  Mcp     => pkg::tool_registry::mcp::create_mcp_tool(po, deps)
  ↓
Tool { po, our_tool }
```

`create_mcp_tool(po, deps)` / `McpToolBuilder` 负责：

1. 反序列化 `McpToolConfig`；
2. 校验 `server_id/tool_name` 非空；
3. 创建 `McpCoreTool { po, config, server, client_runtime }`；
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
    async fn get_by_id(
        &self,
        ctx: RequestContext,
        tool_id: String,
    ) -> Result<Option<Tool>, AppError>;

    async fn sync_from_server(
        &self,
        ctx: RequestContext,
        server_id: &str,
    ) -> Result<usize, AppError>;

    fn invalidate_server(&self, server_id: &str);
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
    po: ToolPo,
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
    ) -> Result<Vec<RemoteMcpTool>>;

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
McpToolDal.get_by_id(ctx, tool_id)
  ↓
ToolDao -> ToolPo(protocol=Mcp, config={server_id, tool_name})
  ↓
McpServerDao -> McpServerPo / McpServerConfig
  ↓
McpToolCallDao.assemble_mcp_core_tool(po, server)
  ↓
registry::mcp::create_mcp_tool(po, deps) -> McpCoreTool
  ↓
ToolCallDao.call_manual(ctx, &tool, args)
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

### Phase 1：MCP Server 持久化与管理面

状态：MCP Server DAO/DAL、Finance Domain 管理面、Handler/API 管理入口均已完成；MCP Tool 同步编排与连接健康检查继续后续阶段。

- ✅ 新增 `McpServerPo/McpServerConfig`；
- ✅ 新增 `mcp_servers` migration（active name 唯一索引、transport/status CHECK 约束）；
- ✅ 新增 `McpServerDao` + SQLite 实现；
- ✅ 覆盖 insert/find/query/update/delete、软删除默认过滤、显式查询 Deleted、offset-only 查询、软删除后重建同名记录等单元测试；
- ✅ 新增 `McpServer` 业务实体与 `McpServerDal`，完成 create/find/query/update/delete/set_status 与最小配置校验；
- ✅ Finance Domain 管理面接入 MCP Server CRUD/query/status，审计字段从 `RequestContext` 派生；
- ✅ Handler/API 接入 `/api/v1/finance/mcp-servers` 管理路由与共享 DTO/enums；
- ✅ 创建/更新/删除后的 runtime invalidate 通过 `McpToolCallDao.invalidate_mcp_server(server_id)` 触发，不在 DAO/DAL init 阶段启动连接；
- ✅ 管理面 detail/list 返回脱敏配置；更新时 `[REDACTED]` 占位符会保留数据库中的既有真实敏感值。

### Phase 2：MCP Tool config + registry stub

- 新增 `McpToolConfig { server_id, tool_name }`；
- 在 `pkg::tool_registry::mcp` 提供 `create_mcp_tool(po, deps)` 或 stub；
- registry 对 `ToolProtocol::Mcp` 先明确走 stub/依赖注入边界；
- 不在通用 `ToolDal` 膨胀 MCP 专属逻辑。

### Phase 3：McpToolDal / McpToolCallDaoImpl 骨架

状态：已完成。

- ✅ 新增 `McpToolDal` 骨架，支持 `get_by_id` 读取 `ToolPo + McpServerPo` 并组装可执行 `Tool`；
- ✅ 新增 `McpToolCallDaoImpl` 组合基础 `ToolCallDao`，通用方法转发；
- ✅ `McpToolCallDaoImpl` 组合 `pkg::tool_registry::mcp::McpClientRuntime`，client/session lifecycle 由工具协议 runtime 管理；
- ✅ generic `assemble_core_tool(&po)` 对 MCP 返回 `None`，强制 MCP 走 `assemble_mcp_core_tool(&po, &server)`；
- ✅ `McpClientRuntime` 已接入最小 stdio rmcp runtime，`McpCoreTool.call` 可通过 `tools/call` 执行 stdio MCP 工具。

### Phase 4：接入官方 rmcp 并同步 MCP Tools

状态：stdio 调用与 tools/list 同步子阶段已完成；Finance Domain 管理面编排仍待接入。

- ✅ 添加官方 `rmcp = "1.7"`，启用 `client` + `transport-child-process`；
- ✅ `McpClientRuntime::call_tool(server, tool_name, args)` 第一版只支持 `McpTransport::Stdio`；
- ✅ stdio 启动使用 `tokio::process::Command` + `command/args`，不做 shell 拼接；
- ✅ 默认 `env_clear()`，只注入 `McpServerConfig.env` 显式环境变量；
- ✅ 启动 stdio 子进程前先校验 tool arguments 必须为 JSON object，非法参数不会启动外部进程；
- ✅ session 初始化受 `connect_timeout_ms` 约束，`tools/call` 受 `timeout_ms` 约束；
- ✅ `tools/call` 成功、失败、超时后统一尝试 `client.close()`，避免 stdio session/子进程泄漏；
- ✅ `McpCoreTool.call` 委托同一个 `McpClientRuntime`，返回序列化后的 `CallToolResult`（`structuredContent/isError/content`）；
- ⏳ Finance Domain 编排 `sync_mcp_tools(server_id)`；
- ✅ `McpClientRuntime.list_tools(server)` 支持 stdio `tools/list`；
- ✅ `tools/list` 映射为标准 `ToolPo`；
- ✅ upsert `ToolProtocol::Mcp` tools，保留 audit/status 并拒绝 id/binding 碰撞；
- ✅ 默认 `ControlMode::Manual`，且 SQLite create/update 持久化该字段。

### Phase 5：安全、管理面和完整测试

- Finance Domain 编排 MCP Server/Tool 管理面；
- trace/error/result 脱敏与截断；
- streamable HTTP runtime（需继承 HTTP Tool SSRF/redirect/header 安全策略）；
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
