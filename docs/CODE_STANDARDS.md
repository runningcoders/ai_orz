# AI Orz - 编码规范

> 🎯 **本文档定位**：所有编码规范的唯一事实源（SSOT）。AGENTS.md 中的规范速查指向本文档。
>
> 状态：v1.0（2026-08-20 最后更新）
>
> 关联文档：
> - [AGENTS.md](../AGENTS.md) — Agent 快速入门手册
> - [ARCHITECTURE.md](./ARCHITECTURE.md) — 架构总纲（实体关系、设计哲学）
> - [LAYERED_ARCHITECTURE_PRACTICE.md](./LAYERED_ARCHITECTURE_PRACTICE.md) — 分层实践与避坑
> - [DOCUMENTATION.md](./DOCUMENTATION.md) — 文档编写与维护规范

---

## 一、命名规范

### 1.1 元素命名

| 元素 | 规范 | 示例 |
|------|------|------|
| **变量/函数/方法** | snake_case | `user_id`, `create_agent`, `get_user_by_id` |
| **类型/结构体/枚举/Trait** | PascalCase | `AgentPo`, `RequestContext`, `AgentDao` |
| **常量** | SNAKE_CASE | `MAX_SIZE`, `LOG_ID`, `DEFAULT_TIMEOUT` |
| **文件名/目录名** | snake_case | `agent.rs`, `request_context.rs`, `sqlite_test.rs` |

### 1.2 函数/方法前缀

| 操作 | 前缀 | 示例 |
|------|------|------|
| 获取数据（有参数） | `get_` | `get_agent_by_id`, `get_user_name` |
| 获取单例/无参数 | 直接命名 | `agent_dao()`, `uid()` |
| 创建/新增 | `new_`, `create_` | `new_agent()`, `create_user()` |
| 修改/更新 | `update_` | `update_agent()` |
| 删除（软删除） | `delete_` | `delete_agent()` |
| 列表/批量 | `find_all`, `find_by_` | `find_all_agents()`, `find_by_org()` |
| 布尔判断 | `is_`, `has_`, `can_` | `is_deleted()`, `has_permission()` |
| **查询（带过滤/分页）** | `query_` | `query_agents(ctx, query) -> PagedResult<Agent>`，**必须复用 `find_by_` 的 WHERE 条件**（见 §6）|
| **统计计数** | `count_` | `count_agents(ctx, query) -> Result<u64>`，**与 query 复用同一套过滤**（见 §6）|
| **搜索（FTS/向量/图谱）** | `search_` | `search_knowledge(ctx, keyword) -> Vec<SearchResult>`，支持混合检索 |
| **向量嵌入** | `embed_` | `embed_entity(ctx, cortex, po)` / `embed_text(ctx, cortex, text)` |
| **向量检索** | `find_nearest_` | `find_nearest_vectors(ctx, table, embedding, top_k)` |
| **图谱遍历** | `traverse_` | `traverse_knowledge_graph(ctx, node_id, depth)` |
| **事件/状态查询** | `list_` | `list_events(ctx, agent_id, since)`（时间序列查询，非分页列表）|

### 1.3 集合变量与 Trait 命名

- **集合变量**：使用复数形式 `agents`, `user_ids`
- **Trait 不加 `Trait` 后缀**：`trait AgentDao { ... }`
- **实现类加 `Impl` 后缀**：`struct AgentDaoSqliteImpl`

### 1.4 DTO 命名约定

| 操作 | Request 命名 | Response 命名 | 说明 |
|------|------------|-------------|------|
| 获取单个 | `Get{Entity}Request` | `Get{Entity}Response` | 如 `GetAgentRequest` / `GetAgentResponse` |
| 创建 | `Create{Entity}Request` | `Create{Entity}Response` | Response 通常含 `id` |
| 更新 | `Update{Entity}Request` | `Update{Entity}Response` | Response 可复用 Get |
| 删除 | `Delete{Entity}Request`（可选） | `Delete{Entity}Response` | Response 仅 `{ success: bool }` |
| 列表（语法糖） | `List{Entities}Request` | `List{Entities}Response` | 只含 pagination |
| 查询（完整） | `{Entity}QueryRequest` | `PagedResult<{Entity}ListItem>` | POST + body，完整过滤 |
| 搜索（语义） | `Search{Entities}Request` | `PagedResult<{Entity}ListItem>` | FTS5 + 向量混合 |

---

## 二、数据对象分层

### 2.1 四层数据对象

| 对象类型 | 定义位置 | 用途 |
|----------|----------|------|
| **API DTO** | `common/src/api/**` | HTTP 请求/响应，前后端复用；通用响应包装使用 `common::api::ApiResponse<T>` |
| **跨层共享模型** | `common/src/models/**` | DAO/DAL/Domain/API 共用的结果结构体（StatsInterval、TimeSeriesPoint、TokenSumResult 等） |
| **Command/Query** | `src/service/domain/*/mod.rs` | Domain 层输入，表达业务意图 |
| **业务实体** | `src/models/*.rs` | 核心业务对象，包含行为和状态 |
| **PO (持久化对象)** | `src/models/*.rs` | 数据库映射，1:1 对应表结构 |

### 2.2 PO 与业务实体边界

**核心原则：PO 仅在 DAO/DAL 层内部使用，绝对不对外暴露到 Domain 层及以上**

| 层级 | 可使用对象 | 数据传递方式 |
|------|------------|------------|
| **DAO 层** | 仅 PO | PO ↔ 数据库 |
| **DAL 层** | 内部：PO，对外：业务实体 | PO ↔ 业务实体 双向转换 |
| **Domain 层** | 仅业务实体 | 业务实体 ↔ Command |
| **Adapter 层** | 业务实体 + DTO/外部结构 | DTO/外部结构 ↔ Command |

**业务实体内部设计**：业务实体内部持有 PO 字段（`pub struct Project { pub po: ProjectPo }`），DAL 层直接通过 `&xxx.po` 传递给 DAO。

**DAL 层接口签名**：统一使用业务实体——写操作接收 `&Project` 引用，读操作返回 `Option<Project>` / `Vec<Project>`。

**软删除约定**：`status = 0` 视为软删除，常规查询默认过滤；需要查询历史/恢复时用 `query` 方法绕过过滤。

---

## 三、Trait 定义位置规范

| 层级 | Trait 定义位置 | 实现位置 | 示例 |
|------|---------------|---------|------|
| **DAO** | 子模块目录 `mod.rs`（如 `dao/agent/mod.rs`） | 各存储实现文件（如 `sqlite.rs`、`stats_duckdb.rs`） | `AgentDao` 定义在 `dao/agent/mod.rs`，实现在 `dao/agent/sqlite.rs` |
| **DAL** | 各自文件中（如 `dal/agent.rs`） | 同文件内 | `AgentDal` trait + impl 都在 `dal/agent.rs` |
| **Domain** | 主模块 `mod.rs`（如 `domain/message/mod.rs`） | 子模块文件中 | `MessageDelivery` trait 在 `domain/message/mod.rs`，`impl MessageDelivery for MessageDomainImpl` 在 `domain/message/delivery.rs` |

**Domain 层具体约定**：
- 主模块 `mod.rs` 中定义总 trait 和所有子能力 trait
- 子模块文件中写 `impl SubTrait for DomainImpl`，不定义新的 struct 包装器
- DomainImpl 结构体定义在主模块 `mod.rs` 中，子模块通过 `use super::DomainImpl` 引入

---

## 四、RequestContext 参数

**所有 service 层（DAO/DAL/Domain）公共方法的第一个参数必须是 `ctx: RequestContext`**

```rust
// ✅ 正确
fn wake_cortex(&self, ctx: RequestContext, provider: &ModelProvider, prompt: &str) -> Result<String>;

// ❌ 错误 - 缺少 ctx
fn wake_cortex(&self, provider: &ModelProvider, prompt: &str) -> Result<String>;
```

用户相关信息从 `ctx.uid()` / `ctx.uname()` 获取；内部私有方法可省略，只读操作也需要传递。

---

## 五、枚举类型安全

**所有存储在数据库中的枚举状态/角色字段，必须使用 Rust 枚举类型**（统一定义在 `common/src/enums/`），禁止裸 `i32`。

### 5.1 DB 映射型

适用：TaskStatus、UserRole、ProjectStatus 等

```rust
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type))]
pub enum TaskStatus {
    #[default] Pending = 1,
    InProgress = 2,
    Completed = 3,
    Cancelled = 0,
}
```

**强制要求**：
- `#[cfg_attr(feature = "sqlx", ...)]` — WASM 前端编译必需
- 默认变体标 `#[default]`
- 实现 `From<i32>`（未知值落 `Default`，不 panic）
- 实现 `From<i64>`、`From<Self> for i32`
- 提供 `from_i32()`/`to_i32()` 辅助方法
- **权限类枚举例外**：UserRole 未知值落最低权限 `Member`，防提权

参考实现：[task.rs](../common/src/enums/task.rs)

### 5.2 纯领域型

适用：MemoryRole、KnowledgeRelationType 等（不映射 DB 列）

- 无 repr/sqlx 派生
- 实现 `From<String>` 字符串匹配

参考实现：[memory.rs](../common/src/enums/memory.rs)

### 5.3 SQL 侧配合

枚举列查询写 `status as "status: TaskStatus"`，SQL 关键字必须转义。

---

## 六、SQLite + SQLx 规范

- **所有表必须启用 `STRICT` 模式**
- **SQL 关键字必须转义**：`status` → `"status"`
- **枚举字段显式标注**：`status as "status: TaskStatus"`
- **软删除约定**：已删除 `status = 0`，查询默认过滤
- **`.sqlx` 目录必须纳入版本控制**
- **测试使用 `#[sqlx::test]`**，每个测试独立内存数据库
- **测试隔离原则**：无状态组件可使用单例（OnceLock）；有状态内存组件必须每次新建实例

详见：[sqlx_guide.md](./design/sqlx_guide.md)

---

## 七、Handler 拆分规范

### 7.1 文件组织

- 按业务域分组（organization、user、agent、task 等）
- **每个业务方法一个独立文件**，单个文件只放一个 handler 函数
- `mod.rs` 只保留模块导出，不存放实现
- 所有 DTO 从 `common/src/api/` 导入；通用响应包装统一使用 `common::api::ApiResponse<T>`

### 7.2 双宏标注（强制）

所有 handler 必须用两个宏标注：

```rust
#[register_handler_tool(
    id = "query_agents",
    name = "query_agents",
    description = "Query agents with full filtering support",
    params = "common::api::AgentQueryRequest",
    tags = "collaboration"
)]
#[generate_http_handler]
pub async fn query_agents(ctx: RequestContext, params: AgentQueryRequest) -> Result<PagedResult<AgentListItem>> { ... }
```

**`#[generate_http_handler]`**：自动生成 Axum 路由（方法 + 路径 + 中间件），handler 签名固定为 `(ctx: RequestContext, params: Params) -> Result<T>`

**`#[register_handler_tool]`**：同时注册为内置工具，Agent 可通过工具调用方式触发该 handler

---

## 八、日志系统规范

**核心原则：项目内所有代码必须使用统一日志宏，禁止直接调用 `tracing::*!`**

| 级别 | 宏名 |
|------|------|
| INFO | `log_info!` |
| WARN | `log_warn!` |
| ERROR | `log_error!` |
| DEBUG | `log_debug!` |

**两种调用模式**：

```rust
// 模式 1：无上下文（系统级别）
log_info!("application started");
log_info!("config loaded from {}", path);

// 模式 2：带上下文（请求级别）
log_info!(&ctx, "create_memory", "created memory id={}", memory_id);
log_error!(&ctx, "update_project", "db error: {:?}", err);
```

**禁止的写法**：直接调用 `tracing::info!`；传 ctx 值而非 `&ctx`；Operation 传变量（必须是字符串字面量）。

详见：[logging_design.md](./design/logging_design.md)

---

## 九、向量化实体规范

**核心原则：所有支持向量索引的 PO 必须实现 `Vectorizable` trait，禁止在 DAL 层手工拼接向量文本**

```rust
pub trait Vectorizable: Send + Sync {
    fn vectorize_text(&self) -> String;
    fn vector_collection() -> &'static str where Self: Sized;
    fn vector_content_hash(&self) -> String { ... }
    fn vector_expire_at(&self) -> Option<i64> { ... }
    fn needs_reindex(&self, existing_hash: &str) -> bool { ... }
}
```

**调用规范**：

| 场景 | 正确写法 | 错误写法 |
|------|---------|---------|
| 索引场景 | `embed_entity(ctx, cortex, po)` | `embed_text_for_search(ctx, cortex, &format!(...))` |
| 获取 collection 名 | `Po::vector_collection()` | 硬编码 `"namespace"` 字符串 |

**已实现 Vectorizable 的实体**：AgentPo（`agents`）、ToolPo（`tools`）、TaskPo（`tasks`）、SkillPo（`skills`）、ShortTermMemoryIndexPo（`memory:short_term`）、LongTermKnowledgeNodePo（`memory:knowledge_node`）。

---

## 十、查询分页与通用 count 规范

**核心原则：query 是核心查询能力，list 是语法糖；count 与 query 复用同一套过滤条件。**

- **query**（POST body，完整查询条件 + pagination）与 **list**（GET，只接受分页，内部固定默认过滤和排序）统一返回 `PagedResult<T> { items, total }`
- pagination 随 Query 结构体全链路透传，每层用 `PagedResult::map()` 转换内部类型
- DAO 层必须抽取 `push_query_filters`，COUNT 与 LIST 复用同一套 WHERE 条件
- 三层统一 `count(ctx, query) -> Result<u64>` 透传

**禁止的写法**：
- ❌ list 接口接受查询字段（ids/status/keyword 等必须走 query）
- ❌ DAO query 方法返回 `Vec` 而非 `PagedResult`
- ❌ Handler 层把 `PagedResult` 当 `Vec` 用（应取 `.items`）
- ❌ count 独立拼 WHERE；`count_by_xxx` 独立实现 SQL

详见：[pagination_and_count_convention.md](./design/pagination_and_count_convention.md)

---

## 十一、两阶段初始化 + 基础数据注入

**核心原则：启动拆成两阶段——「基础设施就绪」与「基础数据注入」严格分离。**

### 11.1 启动总顺序（`lib.rs::run()` 强制执行）

```
pkg::init_all()                  # 最底层：日志/存储/JWT/工具注册（一次性全局 OnceLock）
  → service::init()              # 阶段 ①（同步、纯内存）：DAO → DAL → Domain 单例注册，绝不碰 DB
  → producer::init() / consumer::init()  # AOP 基础设施（订阅者注册，绝不注入 DB 默认值！）
  → service::init_base_data().await      # 阶段 ②（异步、DB IO、幂等）：
      └─► domain::init_all_base_data()   #   派发到每个 domain 的 init_base_data()
  → AOP stats hook + aop::init_all()     # 事件总线调度器启动
  → HTTP 服务启动
```

### 11.2 扩展点

| 想补什么默认数据 → 放在哪里 | 正确做法 | 错误做法 |
|---------------------------|---------|---------|
| 某 domain 的系统默认 DB 行 | 在该 domain 的 `mod.rs` 加 `pub async fn init_base_data()`（try/warn 包裹） | 写到 consumer::init、HTTP handler、外部 migration 脚本 |
| 生产者/消费者 AOP 订阅者注册 | producer::init() / consumer::init() 内部调用 registry 注册 | 把业务代码塞到 init 函数里直接发事件 |

### 11.3 Consumer 边界红线

`consumer::init()` 只做一件事——把 Consumer 注册到 AOP Registry。写 DB 默认值、触发内部事件、调用改变全局状态的业务方法，一律禁止。

### 11.4 测试环境同步对齐

`tests/common/env.rs` 的 `init_full_test_env` 必须严格遵循真实启动顺序。

---

## 十二、统一错误处理规范

**核心原则：所有错误必须通过 `common::error` 的三个宏构造，禁止手写 `Error::new()`**

| 宏 | 用途 | 示例 |
|------|------|------|
| `err!` | 构造错误（不返回） | `err!(NotFound, "Agent {} not found", id)` |
| `bail_err!` | 构造并立即 `return Err(...)` | `bail_err!(InvalidRequest, "参数不合法: {}", e)` |
| `ensure_err!` | 条件检查，不满足则 `bail_err!` | `ensure_err!(age >= 18, InvalidRequest, "未成年禁止")` |

### 12.1 常用 ErrorCode 分类

| 分类 | 典型变体 | HTTP 状态码 |
|------|---------|------------|
| **用户输入** | `InvalidRequest`, `NotFound`, `Unauthorized`, `Forbidden` | 400/404/401/403 |
| **业务逻辑** | `Conflict`, `RateLimited`, `QuotaExceeded` | 409/429 |
| **系统** | `Internal`, `DatabaseError`, `Timeout` | 500/504 |
| **外部集成** | `LarkApiError`, `A2aProtocolError`, `ToolExecutionFailed` | 502 |
| **身份凭证** | `CredentialEncryptionFailed`, `CredentialDecryptionFailed` | 500 |

### 12.2 高级用法

```rust
// 带 JSON 字段（用于前端展示）
err!(InvalidRequest, "字段校验失败", field: { field: "email", reason: "invalid format" });

// 带 source（包装底层错误，保留原始错误链）
.map_err(|e| err!(DatabaseError, "查询失败: {}", source: e))?;
```

**禁止的写法**：
- ❌ `Error::new(ErrorCode::NotFound, "...")` — 必须用 `err!` 宏
- ❌ `.map_err(|e| anyhow::anyhow!(...))` — 必须映射为项目 `Error`
- ❌ 裸字符串错误作为公共 API 返回值

---

## 十三、统计事件规范

**核心原则：所有业务事件统计必须通过 `record_event!` 宏写入，禁止直接操作 DuckDB 连接**

### 13.1 定义与记录

```rust
// 1. 定义事件结构体
#[derive(Debug, Clone, StatsEvent)]
pub struct ModelCallEvent {
    pub timestamp: i64,
    pub agent_id: Option<String>,
    pub model_provider_id: String,
    pub tokens_input: u64,
    pub tokens_output: u64,
}

// 2. 在业务代码中记录事件
record_event!(&ctx, ModelCallEvent {
    timestamp: now,
    agent_id: Some(agent_id.to_string()),
    model_provider_id: provider_id.to_string(),
    tokens_input: input,
    tokens_output: output,
}).await?;
```

### 13.2 特性

- 自动从 `ctx` 获取 Stats 实例，无需手动传连接
- 事件结构体类型自动路由到对应的 StatTable
- 支持内存版（`RuntimeStatsCollector`，重启重置）和持久化版（DuckDB，跨重启）

### 13.3 内置统计事件

`AgentAwakeEvent`、`ModelCallEvent`、`ToolCallEvent`、`TaskEvent`、`ProjectEvent`

---

## 十四、前后端 API 协议规范

**核心原则：`common` crate 是前后端 API 协议的单一事实源。**

1. **禁止裸原始类型响应**：handler 即便只返回一个字段也必须用标准 Response 结构体
2. **DTO 只定义在 common**：Request/Response 一律先定义在 `common/src/api/<域>.rs`
3. **请求参数必须结构体化**：通过 `#[derive(Params)]` + `#[param(source = "path"|"query")]` 注解
4. **共享枚举禁止数字比较**：权限判断用 `UserRole` 枚举方法
5. **前端复用后端结构体**：前端 API client 优先复用 `common::api::*` 中的结构体
6. **前端兼容导入**：既有导入路径多的 api 模块用 `pub use common::api::{...}` re-export

### 14.1 基础设施公共工具位置

**核心原则：通用工具函数必须放在基础设施层，禁止散落在业务 DAO 中造成跨 DAO 依赖。**

| 工具类型 | 存放位置 | 示例 |
|----------|----------|------|
| **FTS5 全文搜索工具** | `src/pkg/storage/fts5.rs` | `escape_fts5_keyword` |
| **向量存储抽象** | `src/pkg/storage/vector.rs` | `VectorStore` trait |
| **日志宏** | `src/pkg/logging.rs` + `ai-orz-macros` | `log_info!`, `log_error!` |
| **统计事件宏** | `src/pkg/stats/` + `ai-orz-macros` | `record_event!` |
| **运行时统计基础设施** | `src/pkg/stats/runtime/` | `RuntimeStatsCollector<K>` |
| **JWT 工具** | `src/pkg/jwt.rs` | `encode_token`, `decode_token` |
| **出站 HTTP 客户端** | `src/pkg/http/client.rs` | `HttpClientOptions`, `build_client` |
| **出站 HTTP 预设** | `src/pkg/http/presets.rs` | `llm()`, `outbound()`, `ssrf_guarded()` |
| **出站安全（SSRF）** | `src/pkg/http/ssrf.rs` | `validate_target_url`, `read_limited_response_body` |
| **子进程执行原语** | `src/pkg/process/exec.rs` | `exec()`, `ExecOptions`, `ExecOutput` |
| **子进程注册中心** | `src/pkg/process/mod.rs` | `registry()`, `ProcessEntry` |
| **WS 长连接管理器** | `src/pkg/ws/` | `WsClientAdapter`, `serve_server` |

**反模式（禁止）**：
- ❌ 在某个业务 DAO 中定义通用工具函数，其他 DAO 直接 import
- ❌ 为了复用在每个 DAO 中复制粘贴相同代码
- ❌ 把业务逻辑相关的工具放到 pkg 层（pkg 层必须无业务感知）

### 14.2 双端复用逻辑下沉 common

**核心原则：前后端都要用的一切逻辑（契约、枚举方法、校验规则、值映射、白名单矩阵）优先放 `common` crate——这是前后端同为 Rust 的最大红利，双端复刻即双端漂移。**

1. **判定标准**：只要前端（wasm）与后端（pkg / handler / domain）需要同一套行为，落位就是 common——不仅限 DTO 结构体，还包括校验函数、匹配矩阵、值域映射、脱敏规则等纯函数
2. **落位文件**：与既有类型同址（契约函数跟契约走，如 `identity_credentials.rs` 的 `enhancer_supports` / `validate_requirements`；工具校验跟领域文件走，如 `models/tool.rs` 的 `validate_builtin_tool_config`）
3. **依赖约束**：下沉函数必须是纯函数（`Result<(), String>` 返回文案或 `bool`），禁依赖可变状态——各端自行包装为本地错误类型（后端 `err!(InvalidRequest, ...)`、前端直接展示）
4. **消费形态**：规则本体在 common 单点，消费方为薄委托（后端包装 / 前端模块作门面），禁止任何一端保留规则复刻

**已下沉单点（参考先例）**：
- `common::models::validate_requirements`（凭据需求六规则）+ `binding_allowed` / `binding_name` / `mcp_transport_scope` / `is_sensitive_credential_name`
- `common::models::validate_builtin_tool_config`（Builtin 工具 config 校验）+ `is_supported_http_method`（HTTP 方法白名单）

### 14.3 出站 HTTP 基建（pkg/http）

**核心原则：出站 HTTP 是基建层职责。业务层只声明「要哪种客户端」，一律经 `pkg/http` 预设构建，禁止手写 reqwest 配置。**

1. **唯一构建入口**：任何 `reqwest::Client` 都必须经 `pkg::http::build_client`（或其预设）构建；**禁止**业务层出现 `reqwest::Client::new()` / `builder()` / `default()`
2. **永不产出无超时客户端**：`effective_timeout()` 硬约束——未指定或 0 → `DEFAULT_TIMEOUT`(30s)，超 `MAX_TIMEOUT`(600s) → 截断。`Client::new()` 无超时，网络抖动时请求永久挂起，是历史 bug 根源
3. **构建失败 fail-fast**：禁用 `unwrap_or_else(|_| Client::new())` 这类回退裸客户端的降级路径（曾有两处此类 fail-open bug）
4. **预设优先**：`llm()`（LLM 推理 120s）、`outbound()`（一般出站 30s）、`ssrf_guarded()`（DNS pinning + 禁重定向 + 禁代理三件套，用于目标地址来自用户/工具配置的场景）；不满足时用 `HttpClientOptions` 叠加
5. **SSRF 防护成套使用**：`ssrf::validate_target_url` 返回的 pinned 地址必须与 `ssrf_guarded` 配套（校验与请求解析到同一地址）；pinning 与禁代理、禁重定向缺一即被绕过
6. **高频调用点持有共享 Client**：`reqwest::Client` 内部是连接池，clone 廉价；每请求/每调用新建 client 会重复 TCP/TLS 握手。DAO/工具实例应在构造或首次调用时惰性构建并持有（`OnceLock` 或结构体字段）
7. **安全组件单点**：SSRF 校验、响应大小限制（`read_limited_response_body`）、敏感头脱敏（`sanitize_response_headers`）一律从 `pkg::http::ssrf` 引用，禁止复刻

### 14.4 子进程执行基建（pkg/process）

**核心原则：子进程是基建层职责。短命 CLI 调用一律经 `pkg::process::exec` 原语执行，禁止手写 spawn/timeout/kill 流水账。**

1. **两类职责分清**：`exec`（生产端，怎么跑）面向短命 CLI 调用，**不进注册中心**；`registry`（管理端，跑起来之后）面向 Agent 可管理的长生命周期进程（如 shell_exec 后台模式），注册是显式行为而非自动
2. **禁止手写 spawn 流水线**：`Command::new` + `kill_on_drop` + `tokio::time::timeout` + `wait_with_output` 的组合是 exec 原语的内部实现，业务层不得复刻（历史上有 6 处复制粘贴）
3. **输出捕获恒并发读**：先 `wait()` 后读 stdout 的写法在子进程输出超过管道缓冲区（~64KB）时双向阻塞直到超时、输出全丢（真实 bug：codex runtime）。exec 原语内部恒用 `wait_with_output()`，此类死锁结构性不可能
4. **超时必终止**：超时返回 `timed_out = true` 而非 Err，由调用方决定语义；子进程由 `kill_on_drop` 终止、tokio 后台回收，无僵尸
5. **spawn 失败保留错误分类**：NotFound / PermissionDenied 映射对应 ErrorCode，供调用方给出安装引导/权限提示
6. **stdin 注入 best-effort**：Broken pipe（命令未读 stdin 即退出）是合法行为，不视为失败
7. **长交互进程例外**：需要边跑边读流/轮询状态的进程（如 lark-cli `config init --new` 绑定流程）不走 exec，直接持 `tokio::process::Child` 管理

**反模式（禁止）**：
- ❌ 前端注释写「与后端同源，规则变动需同步」——同源靠代码共享，不靠人肉同步
- ❌ pkg 定义校验规则后前端复制等价实现（pkg 不能被 wasm 引用正是下沉 common 的理由）
- ❌ 同一规则双端各写一份 match / 矩阵（三处以上重复必然漂移，凭据规则落地时 api-key 连字符已被复制错过一次）

详见：[api_protocol_convention.md](./design/api_protocol_convention.md)

---

*本文档是编码规范的唯一事实源。AGENTS.md 中的规范速查指向本文档对应章节。*
