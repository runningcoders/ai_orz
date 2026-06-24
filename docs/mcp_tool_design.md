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

## 下一步实施方案：MCP Tool 运行面最小闭环

当前 MCP 管理面已经完成：MCP Server 可管理，MCP Tool 可通过 `tools/list` 同步为标准 `ToolPo`，并可按 server 查询。下一步优先验证并补齐运行面闭环，目标不是继续增加管理 API，而是证明“同步出来的 MCP Tool 可以被运行时执行”。

### 目标

打通 stdio MCP Tool 的最小执行链路：

```text
McpServerPo(stdio config)
  ↓
McpToolDal.sync_from_server(ctx, server_id)
  ↓
ToolPo(protocol=Mcp, config={server_id, tool_name})
  ↓
McpToolDal.get_by_id / call_tool_by_id
  ↓
McpToolCallDao.assemble_mcp_core_tool(po, server)
  ↓
McpCoreTool.call
  ↓
McpClientRuntime.call_tool(server, tool_name, args)
  ↓
stdio MCP Server tools/call
  ↓
标准 ToolCallEntry + result
```

### 架构边界决策

1. 不让 `ToolDal` 注入或直接调用 `McpToolDal`，避免 DAL 同层互调。
2. MCP 可执行 Tool 的组装仍由 `McpToolDal` 负责，因为只有它同时拥有 `ToolDao + McpServerDao + McpToolCallDao`。
3. 通用 `ToolDal.get_by_id/query/list_tools_for_agent_full` 对 MCP Tool 可继续返回 management-safe `Tool::from_po_for_management(po)`，用于列表、绑定、Prompt 展示；真正执行 MCP Tool 必须走 `McpToolDal` 或更上层 Domain/Runtime 路由。
4. 后续 Runtime/Message Consumer 需要执行工具时，由 Domain/Runtime 层按 `ToolProtocol` 路由：Builtin/HTTP 走通用 `ToolDal`，MCP 走 `McpToolDal`。不要把协议分发下沉到 DAO。
5. `McpToolCallDaoImpl` 继续拥有唯一 `McpClientRuntime` 生命周期；`McpToolDal` 只复用该 DAO，不自行创建 runtime 或 base `ToolCallDao`。
6. 第一版仍只支持 stdio；`streamable_http` 在 SSRF/header/redirect 安全策略完成前继续显式 `not implemented`。

### Batch A：McpToolDal 执行 API + E2E 测试

状态：已完成。DAL 层已经补齐 MCP 专属 manual call 入口，并通过 stdio MCP E2E 测试证明 sync 后的 MCP Tool 能被调用。

涉及文件：

- `src/service/dal/mcp_tool.rs`
  - 在 `McpToolDal` trait 新增：
    - `call_tool_by_id(ctx, tool_id, args) -> Result<Value, ToolError>`
    - `call_manual(ctx, &Tool, args) -> Result<(Value, ToolCallEntry), ToolError>`
  - 实现逻辑：
    - `call_tool_by_id` 调用 `self.get_by_id(...)` 组装完整 `McpCoreTool`；
    - 找不到 tool 返回 `ToolError::ToolCallError("Tool not found: ...")`；
    - 组装失败/非 MCP/缺 server 统一转换为不泄漏 config 的 `ToolError`；
    - `call_manual` 委托同一个 `mcp_tool_call_dao.call_manual(ctx, tool, args)`，复用现有 tracing decorator。
- `src/service/dal/mcp_tool_test.rs`
  - 新增 stdio MCP test server fixture；
  - 已覆盖流程：create server → sync tools → call synced tool by id → assert result；
  - 已覆盖 JSON object 参数成功路径；
  - 待补充错误路径：非 object 参数返回明确错误；
  - 待补充错误路径：缺失 server / 缺失 tool 的错误不包含 command/env/headers/url。

建议首个 RED 测试：

```text
sync_then_call_stdio_mcp_tool_by_id_returns_result
```

断言重点：

- `sync_from_server` 返回 1；
- persisted tool 的 `protocol == ToolProtocol::Mcp`；
- persisted config 只有 `server_id/tool_name`；
- `mcp_tool_dal.call_tool_by_id(ctx, "mcp.echo-server.echo", {"text":"hi"})` 返回 MCP `CallToolResult` 序列化 JSON；
- 返回内容包含 echo 结果；
- 不依赖通用 `ToolDal` 注入 MCP DAL。

### Batch B：运行面协议路由设计落点

状态：已完成。Runtime Domain 已新增 `RuntimeToolExecution::call_tool_by_id` 作为后续 Message Consumer 调用工具的上层能力入口；协议分发停留在 Domain 层，不下沉到 DAO，也不让 `ToolDal` 依赖 `McpToolDal`。

当前调用链路：

```text
RuntimeDomain.tool_execution().call_tool_by_id(ctx, tool_id, args)
  ↓
ToolDal.get_by_id(ctx, tool_id) 读取标准 Tool 元信息
  ↓
match tool.po.protocol
  Builtin | Http => ToolDal.call_tool_by_id(ctx, tool_id, args)
  Mcp            => McpToolDal.call_tool_by_id(ctx, tool_id, args)
```

当前落地文件：

- `src/service/domain/runtime/mod.rs`
  - `RuntimeDomain` 新增 `tool_execution()` 能力入口；
  - `RuntimeToolExecution` 新增 `call_tool_by_id(ctx, tool_id, args)`；
  - `RuntimeDomainImpl` 组合 `ToolDal` 与 `McpToolDal`，但只在 Domain 层做协议路由。
- `src/service/domain/runtime/tool_execution.rs`
  - 先通过 `ToolDal.get_by_id` 读取通用 Tool 元信息；
  - `ToolProtocol::Mcp` 转发到 `McpToolDal.call_tool_by_id`，并在 Runtime 边界将下层错误统一映射为安全错误，避免 command/env/headers/url/credential 等 server config 派生信息透出；
  - `ToolProtocol::Builtin | ToolProtocol::Http` 转发到 `ToolDal.call_tool_by_id`。
- `src/service/domain/runtime/tool_execution_test.rs`
  - 覆盖 MCP 工具路由到 `McpToolDal`；
  - 覆盖 Builtin/HTTP 工具路由到通用 `ToolDal`；
  - 覆盖 MCP 下层错误脱敏：Runtime 返回值保留 tool_id 与通用失败原因，不包含 command/env/url/credential 等敏感细节；
  - 使用 stub DAL 验证调用次数，避免依赖全局单例和真实外部 runtime。

验收结论：

- Domain 层负责协议路由，符合 handler→domain→dal→dao 单向分层；
- DAL 之间不互调，`ToolDal` 不注入、不调用 `McpToolDal`；
- DAO 仍只做持久化；
- MCP server config 不进入 Runtime Domain 返回值；Runtime 对 MCP 下层错误做 fail-closed 脱敏，当前测试与实现不输出 command/env/headers/url/credential；
- 第一阶段不新增 HTTP API，不接 LLM 自动调用，只为消息消费者后续编排准备纯能力入口。

### Batch C：Agent 绑定与 Prompt 可见性验证

目标：证明 MCP Tool 作为标准 Tool 可以被绑定到 Agent，并能在运行时工具列表/Prompt 展示中出现，但仍默认 `Manual`，不直接进入 Rig auto tool calling。

实现结论：

- `ToolPo::to_tool_prompt()` 负责输出模型可见的工具说明，只包含 `id/name/description/protocol/control_mode/parameters_schema`，不输出 `ToolPo.config`；
- `to_tool_prompt()` 会对远端 MCP metadata 来源的 `description/parameters_schema` 做模型可见脱敏，避免 `command/env/url/headers/authorization/token/secret/password/credential` 等敏感词原样进入 Prompt；
- `PromptBuilder::agent_tools(agent)` 只组合 Agent 当前绑定工具中的 `Manual` 工具安全 Prompt 文本，遵循「PO 负责格式化、Builder 纯组合」，并补充 ai_orz 工具调用方式说明：下方列出的 Manual 工具应发送工具调用消息，由消息机制处理；已经注册到 Rig 的 Auto 工具不在该列表中，仍使用模型默认 Rig/function calling 调用方式；
- `build_conversation_prompt()` 与 `RuntimeAwakening::awaken()` 已把 Agent 绑定工具加入 Prompt，因此 MCP Tool 可在唤醒上下文中被模型看到；
- MCP Tool 仍保持 `ControlMode::Manual`，`ToolDal.wrap_for_rig()` 不会把它暴露给 Rig auto tool calling；Finance Domain 在 create/update 写入侧拒绝 HTTP/MCP `Auto`，不依赖运行时兜底。

测试重点：

- sync 后 bind MCP tool to agent；
- `list_agent_tools` / `list_tools_for_agent_full` 不过滤掉 `ToolProtocol::Mcp`；
- MCP Tool 展示只使用 `ToolPo` 的 name/description/schema/tags，不需要 server config；
- `ControlMode::Manual` 默认保持；
- `wrap_for_rig` 暂不把 MCP 自动暴露给 Rig，除非后续明确设计 Auto 策略。

验收结论：

- Agent 绑定 MCP Tool 后，运行时工具列表可返回该 Tool；
- Prompt 中能看到 MCP Tool 的安全元信息和参数 schema；
- Prompt 中的工具列表只展示需要消息模式调用的 Manual 工具，并明确 Auto 工具仍走 Rig 默认调用方式；
- Prompt 不包含 server `command/env/url/headers/credential`，也不包含 MCP tool binding config 的 `server_id/tool_name` 字段名；
- MCP Tool 默认 `Manual` 且不进入 Rig auto tool calling。

### Batch D：Manual MCP ToolCallResult 回调消息闭环

状态：已完成最小闭环。Message Domain 已提供 `send_tool_call_request` / `send_tool_call_result` Command API；`MessageHandlerImpl` 已支持测试依赖注入，并在收到 `ToolCallRequest` 后编排 Runtime Domain 执行工具，再通过 Message Domain 回写 `ToolCallResult`。当前覆盖成功、Runtime 失败、非法请求、非 ToolCallRequest 系统消息忽略四类 Consumer 单元测试。

目标：把 Manual MCP Tool 从“Prompt 可见”推进到“异步消息调用闭环”。Manual 工具调用不是 Rig/function calling 的同步返回，而是 ai_orz 自建的消息协议：Agent 发出 `ToolCallRequest` 消息，工具执行器消费并调用 Runtime Domain，执行结果再作为 `ToolCallResult` 回调消息发送给 Agent，随后由消息机制重新唤醒 Agent 继续推理。

核心语义：

```text
Agent 推理并决定调用 Manual MCP Tool
  ↓
MessageDomain.delivery().send_tool_call_request(...)
  ↓
Message Domain 保存 ToolCallRequest 并发布消息事件
  ↓
ToolCallRequest Consumer 消费请求
  ↓
RuntimeDomain.tool_execution().call_tool_by_id(ctx, tool_id, args)
  ↓
按 ToolProtocol 路由并调用 McpToolDal / ToolDal
  ↓
Consumer 得到 result / error
  ↓
MessageDomain.delivery().send_tool_call_result(...)
  ↓
Message Domain 保存 ToolCallResult 并发布消息事件
  ↓
消息机制重新唤醒 Agent
  ↓
Agent 在下一轮 Prompt 中看到工具回调结果并继续完成用户任务
```

分层边界：

- **Consumer 管编排**：消费 `ToolCallRequest`，调用 Runtime Domain 执行工具，然后调用 Message Domain 发送结果；
- **Runtime Domain 管执行**：负责工具执行入口与协议路由，`ToolProtocol::Mcp` 路由到 `McpToolDal.call_tool_by_id`，Builtin/HTTP 路由到通用 `ToolDal.call_tool_by_id`；
- **Message Domain 管发送**：负责把工具执行结果转换为 `ToolCallResult` 回调消息，保存、发布事件、触发后续投递/唤醒；
- **Message DAL/DAO 只做持久化**：Consumer 不应直接依赖 `MessageDal` 写入结果消息，避免绕过 Message Domain 的发送语义、事件发布和唤醒流程；
- **Domain 不同层互调**：不要让 `RuntimeDomain` 直接调用 `MessageDomain`。Consumer 作为上层入口/应用编排层，可以同时协调 Runtime Domain 与 Message Domain。

当前实现落点：

- `src/consumer/message.rs`
  - `MessageHandlerImpl` 持有 `Arc<dyn RuntimeDomain>` 与 `Arc<dyn MessageDomain>`；
  - 生产环境通过 `MessageHandlerImpl::new()` 使用全局 Domain 单例；
  - 测试环境通过 `MessageHandlerImpl::new_for_test(...)` 注入 mock，避免 Consumer 单元测试绑定全局单例；
  - `MessageType::ToolCallRequest` 被解析为 `ToolCallMessage`，`args` 缺省时按 `Value::Null` 传入 Runtime；
  - Consumer 基于 ToolCallMessage 中的 `from_id/project_id/task_id` 构造 `RequestContext`，调用 `runtime_domain.tool_execution().call_tool_by_id(ctx, tool_id, args)`；
  - 执行成功映射为 `ToolCallExecutionOutcome::Success`，执行失败映射为 `ToolCallExecutionOutcome::Failure`，再统一调用 `message_domain.delivery().send_tool_call_result(...)`。
- `src/consumer/message_tests.rs`
  - 使用 `RecordingRuntimeDomain` / `RecordingMessageDomain` 记录调用，不直接依赖真实 Runtime/Message 全局单例；
  - 覆盖 Consumer 成功回调、失败回调、非法 JSON 返回错误（由上层 nack）、非 ToolCallRequest 系统消息忽略。

已新增 Message Domain 能力：

```rust
pub enum ToolCallExecutionOutcome {
    Success {
        result: serde_json::Value,
        result_file_meta: Option<FileMeta>,
    },
    Failure {
        // 由 Runtime 边界提供已脱敏错误文本
        error_message: String,
    },
}

pub struct SendToolCallResultCommand<'a> {
    pub request_message: &'a Message,
    pub outcome: ToolCallExecutionOutcome,
}

pub struct SendToolCallRequestCommand<'a> {
    pub request_id: &'a str,
    pub tool_id: &'a str,
    pub tool_name: &'a str,
    pub from_agent_id: &'a str,
    pub to_executor_id: &'a str,
    pub project_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub reply_to_id: Option<&'a str>,
    pub args: serde_json::Value,
}

#[async_trait::async_trait]
pub trait MessageDelivery: Send + Sync {
    async fn send_tool_call_request(
        &self,
        ctx: RequestContext,
        cmd: SendToolCallRequestCommand<'_>,
    ) -> Result<Message, AppError>;

    async fn send_tool_call_result(
        &self,
        ctx: RequestContext,
        cmd: SendToolCallResultCommand<'_>,
    ) -> Result<Message, AppError>;
}
```

`send_tool_call_request` 的职责：

- 基于 Command 构造 `ToolCallMessage::new_request(...)` 并序列化到 `message.content`；
- 固定消息语义为 `Agent -> System`、`MessageType::ToolCallRequest`、`Pending`；
- 保留 `request_id/tool_id/tool_name/project_id/task_id/reply_to_id/args`，用于工具执行与后续结果关联；
- 通过 Message Domain 保存消息并发布事件，避免调用方直接拼装/写入 `MessageDal`。

`send_tool_call_result` 的职责：

- 基于原始 `ToolCallRequest` 生成 `ToolCallResult`；
- 保持同一个 `request_id`，用于请求/结果关联；
- 自动反转 `from_id/to_id`，表现为系统/工具执行器回调给 Agent；
- 继承 `project_id/task_id/tool_id/tool_name` 等上下文；
- 对错误结果使用 Runtime 边界脱敏后的安全错误文本；
- 通过 Message Domain 现有发送流程保存消息并发布事件，使 Agent 被重新唤醒；
- 为大结果预留 `result_file_meta` / attachment 方案，避免过大内容直接塞入 message content。

Batch D 测试重点：

- ✅ Consumer 收到 `ToolCallRequest` 后调用 `RuntimeDomain.tool_execution()`，而不是直接调用 DAL；
- ✅ 工具成功时，Consumer 调用 `MessageDomain.delivery().send_tool_call_result(...)` 发送成功回调；
- ✅ 工具失败时，Consumer 发送失败回调；错误脱敏由 Runtime 边界负责，Consumer 不拼接 MCP server `command/env/url/headers/credential` 等配置细节；
- ✅ Consumer 不直接依赖 `MessageDal` 写入 `ToolCallResult`；
- ✅ `ToolCallResult` 写入沿用 Message Domain 发送语义，后续事件发布/唤醒链路不被 Consumer 绕过；
- ⏭️ “只允许调用当前 Agent 已绑定的 `Manual` 工具；Auto 工具不走这个消息模式”由 Runtime/Finance 工具绑定与 Prompt 可见性规则共同约束，后续可在授权/绑定校验 Batch 中补更细断言。

### Batch E：可观测性与安全补强

状态：已完成 Runtime MCP 下层错误脱敏、MCP tool call trace 默认脱敏，以及第一组安全错误语义映射；其余 runtime 生命周期补强继续作为后续 Batch。

已完成：

- ✅ `RuntimeDomain.tool_execution().call_tool_by_id(...)` 已在 Runtime 边界对 `McpToolDal.call_tool_by_id(...)` 的下层错误做 fail-closed 映射；
- ✅ 对外错误只保留安全上下文（例如 `tool_id`），不传播 stdio command、env、headers、URL、credential、rmcp/process 原始错误文本；
- ✅ 已补充 stub DAL 单元测试，验证 MCP 协议只路由到 `McpToolDal`，Builtin/HTTP 只路由到通用 `ToolDal`，并验证敏感错误片段不会出现在 Runtime 返回错误中；
- ✅ `LoggingDecorator` 对 `ToolProtocol::Mcp` 与 `ToolProtocol::Http` 的 trace `input/output/error` 采用 fail-closed 默认脱敏，避免外部工具参数、返回值或错误文本进入 tool call JSONL trace；
- ✅ 已补充 MCP trace 脱敏回归测试，验证 `placeholder-value`、URL host、`credential` 等敏感片段不会出现在序列化后的 trace entry 中；
- ✅ Runtime MCP 错误语义已做最小安全分类：timeout、server not found、server disabled、tool disabled/tool not found 会映射为只含 `tool_id` 的安全错误文案，其余未知错误继续 fail-closed 映射为通用 `MCP tool call failed for tool_id: ...`；
- ✅ `McpToolDal.get_by_id/call_tool_by_id` 已在组装执行工具前拒绝 disabled MCP tool 与 disabled MCP server，避免继续连接外部 runtime；DAO 仍只负责持久化，状态语义检查停留在 DAL/Runtime 边界；
- ✅ stdio `tools/list` / `tools/call` 成功后关闭 session 失败时，不再传播 rmcp/process 下层错误文本，仅返回安全文案 `MCP stdio session close failed after ... on server ...`，避免 command/env/credential 等细节外泄；
- ✅ 当前 stdio runtime 采用每次 `tools/list` / `tools/call` 独立连接、执行、关闭的无共享 session 策略，同一 MCP server 的并发调用各自使用独立 stdio session，可并发执行且不持有跨 await 的共享锁；
- ✅ server update/status/delete 触发的 runtime invalidation marker 已验证会在下一次成功 stdio 调用后被消费；当前 per-operation session 策略下等价于下一次调用按最新持久化 server config 重新连接，后续若引入 session cache，该 marker 将扩展为关闭/丢弃旧 session。

后续继续补：

- session cache / health check / reconnect 的增强实现；
- 更完整的 MCP 错误路径测试：tool/server 缺失、非 object args、错误文本不泄漏 command/env/headers/url。

### 验证命令

每个 Batch 都遵循 TDD：RED → GREEN → VERIFY → REVIEW。最小验证集：

```bash
cargo fmt --all -- --check
git diff --check
cargo test -q mcp_tool -- --nocapture
cargo test -q mcp_tool_handler -- --nocapture
cargo check -q
```

注意：过滤测试必须确认输出中 `running N tests` 且 `N > 0`，不能接受 0-test 通过。

### 暂不做事项

- 暂不启用 `streamable_http` runtime；
- 暂不新增更多管理 API（例如 stale reconcile/delete synced tool/detail schema）；
- 暂不把 MCP Tool 直接注册为 Rig auto tool；
- 暂不把 MCP 执行路由下沉到 DAO 或让 `ToolDal` 依赖 `McpToolDal`。

## 连接生命周期

### MVP：按需连接 + 无共享 session

连接生命周期归属 `McpToolCallDaoImpl` 内部的 `McpClientRuntime`。当前第一版先采用 per-operation stdio session，不缓存跨调用 session，优先保证实现简单、并发安全和关闭语义明确：

```text
McpCoreTool.call
  ↓
McpClientRuntime.call_tool(server, tool_name, args)
  ↓
使用当前持久化 server config 启动 stdio process 并 initialize
  ↓
tools/call
  ↓
client.close()，不保留共享 session
```

并发策略：同一个 MCP server 的并发 `tools/call` 会各自创建独立 stdio session；runtime 只维护轻量 invalidation marker，不持有跨 `.await` 的共享 session lock，因此不会因为同 server 调用串行化或产生 MutexGuard across await 风险。

### 更新/删除 Server

```text
update_mcp_server
  ↓
McpServerDao.update
  ↓
McpToolCallDao.invalidate_mcp_server(server_id)
  ↓
下次成功调用消费 invalidation marker，并按当前持久化配置重新启动 stdio session
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

当前 per-operation session 策略下，invalidate 不需要主动关闭长连接；它作为生命周期契约 marker，证明管理面变更已经触达 runtime。后续如引入 session cache，`invalidate_mcp_server(server_id)` 必须扩展为关闭并移除对应 cached session，下一次调用再重连。

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

状态：stdio 调用、tools/list 同步子阶段、Finance Domain/Handler 管理面编排均已完成；下一步进入运行面最小闭环。

- ✅ 添加官方 `rmcp = "1.7"`，启用 `client` + `transport-child-process`；
- ✅ `McpClientRuntime::call_tool(server, tool_name, args)` 第一版只支持 `McpTransport::Stdio`；
- ✅ stdio 启动使用 `tokio::process::Command` + `command/args`，不做 shell 拼接；
- ✅ 默认 `env_clear()`，只注入 `McpServerConfig.env` 显式环境变量；
- ✅ 启动 stdio 子进程前先校验 tool arguments 必须为 JSON object，非法参数不会启动外部进程；
- ✅ session 初始化受 `connect_timeout_ms` 约束，`tools/call` 受 `timeout_ms` 约束；
- ✅ `tools/call` 成功、失败、超时后统一尝试 `client.close()`，避免 stdio session/子进程泄漏；
- ✅ `McpCoreTool.call` 委托同一个 `McpClientRuntime`，返回序列化后的 `CallToolResult`（`structuredContent/isError/content`）；
- ✅ Finance Domain/Handler 编排 `sync_mcp_tools(server_id)`，暴露 `POST /api/v1/finance/mcp-servers/{server_id}/tools/sync`；
- ✅ `McpClientRuntime.list_tools(server)` 支持 stdio `tools/list`；
- ✅ `tools/list` 映射为标准 `ToolPo`；
- ✅ upsert `ToolProtocol::Mcp` tools，保留 audit/status 并拒绝 id/binding 碰撞；
- ✅ 默认 `ControlMode::Manual`，且 SQLite create/update 持久化该字段。

### Phase 5：MCP Tool 运行面最小闭环

状态：Batch A、Batch B、Batch C 已完成，后续继续补 Message Consumer 接入与更完整安全策略。

- ✅ `McpToolDal.call_tool_by_id/call_manual`：sync 后按标准 Tool ID 执行 MCP Tool；
- ✅ DAL 级 E2E 测试：create server → sync tools → call synced tool → assert result；
- ✅ Runtime Domain 协议路由：MCP 走 `McpToolDal`，Builtin/HTTP 走通用 `ToolDal`，禁止 DAL 同层互调；
- ✅ Runtime MCP 错误边界脱敏：MCP 下层错误统一映射为安全错误，不输出 command/env/headers/url/credential；
- ⏳ 错误路径测试：tool/server 缺失、非 object args、错误文本不泄漏 command/env/headers/url；
- ⏳ Message Consumer 接入 Runtime 工具执行入口；
- ✅ Agent 绑定与 Prompt 可见性验证：MCP Tool 可作为标准 Tool 绑定展示，但默认 `Manual`，暂不进入 Rig auto tool calling；

### Phase 6：安全、管理面和完整测试

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

1. stdio `command` 是否采用 allowlist？allowlist 放配置还是代码常量？
2. MCP Server 是否只允许管理员配置？
3. MCP Tool 同步时，远端删除的 tool 是禁用、软删除，还是保留 stale 状态？
4. MCP trace 默认全脱敏是否接受？是否需要按 server/tool 配置 trace policy？
5. MCP Tool 进入 Agent 工具列表后，Manual tool 的 Prompt 展示格式是否需要区别于可自动调用工具？
