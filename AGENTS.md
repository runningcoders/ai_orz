# AI Orz 开发规范与最佳实践

本文总结了 AI_orz 项目中 Agent 开发过程中总结出的最佳实践和规范。

---

## 一、项目整体结构规范

### 1.1 三级 Workspace 结构

```
ai_orz/
├── **common** 公共共享 crate（必须）
│   ├── src/api/              # API 请求响应 DTO 按功能分组
│   ├── src/constants/        # 公共常量、基础类型
│   └── src/enums/           # 公共枚举
│
├── **src** 后端服务
│   ├── models/               # 持久化 PO 实体
│   ├── handlers/             # HTTP 接口层
│   │   └── {domain}/         # 按业务域分组
│   │       └── {feature}/    # 按功能分组，每个方法一个独立文件
│   ├── service/
│   │   ├── dao/              # 数据访问层 DAO（单一数据源操作）
│   │   ├── dal/              # 业务数据访问层 DAL（组合 DAO）
│   │   └── domain/           # 领域层（核心业务逻辑）
│   ├── middleware/           # Axum 中间件
│   └── pkg/                  # 公共工具包（只放 common 没有的后端工具）
│
└── **frontend** 前端 Dioxus 应用
    ├── src/api/              # API 客户端（所有 DTO 从 common 导入）
    └── src/components/       # UI 组件（每个页面对应一个组件）
```

### 1.2 common crate 规范（必须遵守）

**必须放到 common 的内容：**
- 所有前后端共用的 request/response DTO
- 所有前后端共用的枚举类型（`UserRole`、`ProviderType`、各种状态枚举）
- 公共基础类型（`ApiResponse<T>`、`EmptyResponse`）
- 公共常量定义

**禁止放到 common 的内容：**
- 后端 PO 实体（保持在后端 `models/`）
- 后端业务逻辑
- 前端 UI 组件

**优点：**
- ✅ 消除前后端重复定义
- ✅ 保证 API 契约编译期一致，不会出现前后端字段不匹配
- ✅ 类型安全，修改一处自动同步到两端

---

## 二、代码分层规范

### 2.1 分层职责规范（严格遵守）

| 层级 | 职责 | 不做什么 |
|------|------|------|
| **models** | 定义 PO 持久化实体、业务实体 | 不要放业务逻辑 |
| **service/dao** | 单一数据源操作，增删改查，不包含业务逻辑 | 不要放业务逻辑，只做单一数据源操作 |
| **service/dal** | 组合多个 DAO，提供业务级数据操作能力 | 不要放核心业务规则 |
| **service/domain** | 核心业务逻辑实现 | 只放业务规则，编排 DAL |
| **handlers** | HTTP 接口接收请求，调用 domain，返回响应 | 不要放业务逻辑 |

> **约定**：所有 service 层公共方法都必须带 `ctx: RequestContext` 参数，方便日志追踪和权限控制。现在已经架构升级为自动从请求提取注入 RequestContext，无需手动提取。

### 2.2 实体定义

所有持久化 PO 实体统一放在 `src/models/`：

- `agent.rs` - Agent 实体
- `brain.rs` - Brain 实体
- `cortex.rs` - Cortex 实体
- `model_provider.rs` - ModelProvider 实体
- `organization.rs` - Organization 组织实体
- `user.rs` - User 用户实体
- `memory.rs` - 记忆系统所有实体
- `task.rs` - 任务实体
- `project.rs` - 项目实体
- `message.rs` - 消息实体
- `artifact.rs` - 产物实体
- `file.rs` - 文件元数据公共结构体

### 2.3 数据访问层（DAO）

每个 DAO 模块统一放在 `service/dao/{domain}/`：

```
service/dao/memory/
├── mod.rs          # 定义 trait + 单例导出
├── sqlite.rs       # SQLite 特定实现
└── sqlite_test.rs  # 单元测试
```

**规范：**
- trait 定义放在 `mod.rs`
- 具体实现放在 `sqlite.rs`（当前只实现 SQLite）
- 单例 `OnceLock` 放在实现文件 `sqlite.rs`，不是 `mod.rs`
- `mod.rs` 只导出 `pub use sqlite::{dao, init};`
- 单元测试文件名：`sqlite_test.rs`，和实现文件同名

### 2.4 SQL 建表语句规范

- 所有建表语句统一放在 `migrations/` 目录，使用 sqlx migrate 管理
- 不再使用 `src/pkg/storage/sql.rs` 常量方式，迁移文件更加清晰可追溯

---

## 三、枚举类型安全规范

### 3.1 基本原则

所有存储在数据库中的枚举状态/角色字段，**必须使用 Rust 枚举类型**，禁止直接使用 `i32` 存储：

✅ **正确做法：**
```rust
// common 中定义枚举，实现 sqlx Type
pub enum UserRole {
    SuperAdmin = 0,
    Admin = 1,
    Member = 2,
}

// PO 中直接使用枚举类型
pub struct UserPo {
    pub id: String,
    pub organization_id: String,
    pub username: String,
    pub role: UserRole, // ✅ 直接用枚举
    pub status: UserStatus, // ✅ 直接用枚举
    // ...
}
```

❌ **错误做法：**
```rust
pub struct UserPo {
    pub role: i32, // ❌ 不使用魔法数字
    pub status: i32, // ❌
}
```

### 3.2 实现要求（sqlx 0.8 版本）

- 在 common 中定义枚举
- 为枚举实现 `serde::Serialize/Deserialize`（用于 API 序列化）
- 为枚举实现 `#[derive(sqlx::Type)]`，添加 `#[repr(i32)]`
- 实现 `From<i64>` 适配 sqlx 类型推断：`impl From<i64> for MyEnum { fn from(v: i64) -> Self { (v as i32).into() } }`
- 前后端共享同一个枚举定义，保证 API 契约一致
- 如果 common 枚举需要给前端 WASM 编译使用，所有 sqlx 相关代码需要添加 `#[cfg(feature = "sqlx")]` 条件编译

---

## 四、Handler 分层规范

### 4.1 拆分原则

- 按业务域分组（hr、finance、organization、user 等）
- 每个业务方法拆分到独立文件，每个文件只放一个 handler 方法
- `mod.rs` 只保留模块导出，不存放实现
- 所有 request/response DTO 定义在 `common/src/api/`，handler 直接导入使用

**目录示例：**
```
src/handlers/organization/
├── initialize_system.rs        # 单个 handler 方法
├── organization/
│   ├── get_organization.rs     # 单个 handler 方法
│   ├── list_organizations.rs
│   ├── update_organization.rs
│   └── mod.rs                  # 模块导出
├── organization_me/
│   ├── get_current_organization.rs
│   ├── update_current_organization.rs
│   └── mod.rs
├── user/
│   ├── list_users_by_current_organization.rs
│   ├── create_user.rs
│   └── mod.rs
└── mod.rs
```

---

## 五、单元测试规范

### 5.1 测试位置

- 每个 DAO 模块下直接放置测试文件：`service/dao/xxx/sqlite_test.rs`
- 测试使用 `#[sqlx::test]`，每个测试自动创建独立内存数据库，不依赖全局连接池，独立可运行

### 5.2 测试设计原则

- 每个测试函数独立，互不干扰，一个失败不影响其他
- 使用随机临时 SQLite 文件或内存数据库，每次测试重新初始化
- 测试直接使用具体实现类，不依赖全局单例（但可以初始化全局单例）
- 所有测试都使用 DAO/DAL 接口方法测试，不直接拼接 SQL

### 5.3 当前测试统计

- **总测试数**: 165 个
- **通过率**: 100% (165/165) ✅
- **测试覆盖**: 数据层 100% 覆盖

---

## 六、API 设计规范

### 6.1 所有 service 层方法必须传递 `RequestContext`

**强制约定**：所有 service 层（DAO/DAL/Domain）公共方法都必须传递 `ctx: RequestContext` 作为第一个参数 ✅

```rust
// ✅ 正确
fn wake_cortex(&self, ctx: RequestContext, provider: &ModelProvider, prompt: &str) -> Result<String, AppError>;

// ❌ 错误 - 缺少 ctx
fn wake_cortex(&self, provider: &ModelProvider, prompt: &str) -> Result<String, AppError>;
```

**原因**：方便后续日志串联、追踪、权限扩展，即使当前不用也必须传递

### 6.2 RequestContext 自动提取

后端使用 Axum 中间件自动从 JWT 和请求头提取 `RequestContext` 并注入到 Extension，**所有新 handler 不再需要手动提取**：

✅ **正确做法（新规范）：**
```rust
pub async fn handler(
    ctx: RequestContext, // 直接从 Extension 提取，由中间件注入
    Json(req): Json<CreateRequest>,
) -> Result<impl IntoResponse, AppError> {
    // ...
}
```

❌ **旧做法（已弃用）：**
```rust
// 不再需要手动从 extensions 提取
let ctx = RequestContext::from_parts(parts);
```

---

## 七、SQLx 0.8 + SQLite 开发规范

完整规范详见 [docs/sqlx_guide.md](./docs/sqlx_guide.md)，核心要点：

- **所有表必须启用 `STRICT` 模式**，保证 sqlx 可空性推断正确
- **枚举使用 i32 映射**：添加 `#[repr(i32)]` + `#[derive(sqlx::Type)]` + `From<i64>` 实现
- **仅枚举需要显式类型标注**：`status as "status: TaskStatus"`，普通字段不需要
- **SQL 关键字必须转义**：`status`、`role` 等关键字用作列名时用 `"status"` 双引号转义
- **软删除约定**：已删除 `status = 0`，所有查询默认添加 `AND "status" != 0` 过滤
- **`.sqlx` 目录必须纳入版本控制**，保证离线编译正常
- **测试使用 `#[sqlx::test]`**，每个测试独立内存数据库，彻底解决测试污染

---

## 八、记忆系统设计规范 ✅

四层记忆架构，对齐人类认知：

| 层级 | 职责 | 存储位置 | 是否随请求带入 prompt |
|------|------|------|----------|
| **Core Memory** | 核心认知：角色设定、能力清单 | AgentPo + 内存 | ✅ 每次都带 |
| **Working Memory** | 当前会话工作记忆：正在进行的对话 | Brain 内存 | ✅ 每次都带 |
| **Short-Term Memory** | 最近会话摘要索引 | SQLite 数据库 | 需要时检索相关摘要带入 |
| **Long-Term Knowledge** | 长期沉淀知识图谱 | SQLite 知识节点 + 文件原始细节 | 需要时检索相关知识带入 |

### 5.3 当前测试统计

- **总测试数**: 158 个
- **通过率**: 100% (158/158) ✅

---

## 十、架构演进最新 ✅

### 第一轮重构（2026-04-08）

- [x] 基础架构重构：Brain → Cortex 概念澄清，最终实体关系 `Agent → Brain → Cortex → ModelProvider`
- [x] 四层记忆系统设计：核心记忆 → 工作记忆 → 短期记忆索引 → 长期知识图谱
- [x] 四层记忆系统 DAO 开发完成，数据库表创建完成
- [x] Agent 管理和 ModelProvider 管理全栈 CRUD 开发完成
- [x] 所有测试通过：31/31 ✅

### 第二轮重构（2026-04-08 ~ 2026-04-09）

- [x] **组织用户权限体系全栈开发**：组织初始化、JWT 登录、用户角色权限控制
- [x] **common crate 提取重构**：所有前后端共用 DTO、枚举、常量提取到独立 common crate，消除重复定义
- [x] **枚举类型安全改造**：所有数据库存储的枚举状态/角色字段改为原生枚举类型，编译期类型检查
- [x] **RequestContext 自动提取**：Axum 中间件自动注入，handler 无需手动提取
- [x] 新增前端页面：登录、个人信息、组织信息、用户管理
- [x] 所有测试通过：35/35 ✅

### 第三轮重构（2026-04-10 ~ 2026-04-11）

- [x] **rusqlite → sqlx 0.8 迁移**：彻底删除 rusqlite，全链路切换到 sqlx 异步
- [x] 解决测试污染问题：每个测试独立内存数据库，并行测试完全隔离
- [x] 所有表开启 STRICT 模式，类型安全提升
- [x] 修复依赖倒置问题，建立清晰分层初始化流程：`DAO → DAL → Domain`
- [x] 解决前端 WASM 编译问题，条件编译分离 sqlx 依赖
- [x] 所有测试通过：79/79 ✅

### 第四轮开发（2026-04-12 ~ 2026-04-15）

- [x] **任务系统数据层开发**：任务表、任务状态枚举、完整 DAO CRUD
- [x] **项目系统数据层开发**：项目表、项目状态枚举、完整 DAO CRUD
- [x] **产物与消息附件统一存储**：新增 `artifacts` 表，统一 `FileMeta` 设计，日期分层路径存储
- [x] 更新 `messages` 表结构：新增 `file_type` + `modified_by`，统一文件元数据
- [x] 所有测试通过：**104/104** ✅

### 第五轮开发（2026-04-17 ~ 2026-04-18）

- [x] **工具模块基础架构**：支持多种协议（builtin/http/mcp），符合项目分层规范
- [x] **Agent 工具绑定架构设计**：ToolDao 拼装完整工具复合实体，严格遵循分层规范
- [x] **Rig 0.35 适配**：适配 breaking changes，一次性传入工具向量给 Rig Agent
- [x] **完整单元测试覆盖**：新增测试覆盖全链路
- [x] 所有测试通过：**119/119** ✅

### 第六轮开发（2026-04-20 ~ 2026-04-21）

- [x] **工具调用自动日志追踪**：装饰器模式非侵入实现，daily JSONL 存储完整调用轨迹
- [x] **目录重构**：`tool_call_logging` → `tool_tracing` 统一目录结构
- [x] **单例设计**：应用启动初始化一次，测试可创建本地实例
- [x] **集成到现有架构**：ToolDao 拼装工具时自动包装装饰器，上层无感知
- [x] 所有测试通过：**128/128** ✅

### 第七轮开发（2026-04-23 ~ 2026-04-24）

- [x] **统一命名规范**：接口 trait 不带 `Trait` 后缀，具体实现类带 `Impl` 后缀
- [x] **DAO 命名对齐**：所有 DAO 接口改名为 `XxxDao`，实现改名为 `XxxDaoSqliteImpl`
- [x] **DAL 命名对齐**：所有 DAL 接口改名为 `XxxDal`，实现改名为 `XxxDalImpl`
- [x] **严格依赖倒置**：上层只依赖 trait 接口，完全不依赖具体实现类，实现彻底隐藏
- [x] **ArtifactDao 重构**：去掉全局 storage 依赖，改为从 RequestContext 获取连接
- [x] **测试隔离经验总结**：写入文档，有状态内存组件必须每次新建实例
- [x] 所有测试通过：**149/149** ✅

### 第八轮开发（2026-04-27 ~ 2026-04-28）

- [x] **消息领域层完整开发**：消息投递（send_to_agent/send_to_user/dequeue/ack/nack）+ 消息管理
- [x] **消息交互架构设计**：Agent 作为可投递目标，支持用户 ↔ Agent 双向对话
- [x] **混合模式工具调用设计**：简单工具走 rig 原生 auto，关键工具走自建 manual 链路可收敛控制
- [x] **工具调用复用消息存储**：工具调用本身就是特殊消息，复用现有消息表不需要新建表
- [x] **事件队列重构**：泛型 topic 分离设计，每个 topic 独立单例保证类型安全，彻底解决消息错乱问题
- [x] **SQLite schema 迁移经验**：开发阶段直接重建表比迂回修改更干净
- [x] 所有测试通过：**159/159** ✅

### 第九轮重构（2026-04-28）

- [x] **分层架构大重构**：工具绑定架构从 DAO 层组装重构为严格分层调用
- [x] **DAO 单一职责澄清**：DAO 只做持久化，实体组装/装饰逻辑上移到 DAL 层
- [x] **跨 DAO 依赖消除**：CortexDao 不再直接调用 ToolCallDao，所有组装由上层 DAL/Domain 完成
- [x] **工具注册表模式落地**：实现 `ToolRegistry` + `BuiltinToolFactory`，支持静态注册和动态扩展
- [x] **Tool 实体组装逻辑清晰**：DAO 层返回 `ToolPo` → DAL 层通过工厂组装完整 `Tool` 实体
- [x] **测试最佳实践沉淀**：测试隔离原则、Mock 工厂模式、分层测试策略
- [x] **Rig 包名问题完整记录**：Edition 配置、导入路径、版本锁定等最佳实践
- [x] **完整文档沉淀**：新增 `LAYERED_ARCHITECTURE_PRACTICE.md` 详细记录重构过程和避坑指南
- [x] 所有测试通过：**165/165** ✅

---

## 十九、分层架构重构最佳实践（最新）

> 📖 **完整文档**：详见 [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md)

### 核心分层原则重申

```
Handler (API 层)
    │
    ▼
Domain (领域层) → 组合多个 DAL，实现业务逻辑
    │
    ▼
DAL (业务数据层) → 组合多个 DAO，提供业务级数据操作
    │
    ▼
DAO (数据访问层) → 单一数据源操作，只做 CRUD
```

### 绝对禁止的反模式

| 反模式 | 危害 |
|--------|------|
| ❌ DAO 层调用其他 DAO | 分层边界模糊，测试隔离困难 |
| ❌ DAL 层调用其他 DAL | 循环依赖风险，复杂度失控 |
| ❌ 跨层直接访问 | 业务逻辑散落，难以维护 |
| ❌ DAO 层做实体组装/装饰 | 业务逻辑泄露到数据层 |

### 注册表模式最佳实践

遇到"需要动态创建不同类型实例"的场景，统一使用注册表 + 工厂模式：

```rust
pub trait BuiltinToolFactory: Send + Sync {
    fn id(&self) -> &str;
    fn create(&self, po: ToolPo) -> Box<dyn CoreTool>;
}

pub struct ToolRegistry {
    factories: RwLock<HashMap<String, Box<dyn BuiltinToolFactory>>>,
}
```

### Known Issue: Rig 包名问题

**问题现象**：
```
error[E0670]: `async fn` is not permitted in Rust 2015
error[E0432]: unresolved import `rig::completion::ToolDefinition`
```

**解决方案**：
1. 确保 `Cargo.toml` 使用 `edition = "2024"`
2. 从正确路径导入：`use rig::tool::{ToolDyn, ToolError};`
3. 避免从 `rig::completion::*` 导入工具相关类型
4. 必要时锁定精确版本：`rig-core = "=0.34"`

---

## 二十、消息交互系统设计规范

### 核心设计思想

工具调用本身就是一种特殊消息，复用现有消息表存储，不需要单独新建表：

| 消息类型 | 用途 | 存储位置 |
|---------|------|---------|
| `Text` | 普通文本消息 | messages 表 |
| `ToolCallRequest` | 工具调用请求 | messages 表（content 存储 JSON）|
| `ToolCallResult` | 工具调用结果 | messages 表（content 存储 JSON）|

### 混合模式工具调用

混合模式兼顾简洁性和可控性：

| 模式 | 适用场景 | 调用方式 |
|------|---------|---------|
| **auto 模式** | 简单无状态工具 | 交给 rig 原生自动调用 |
| **manual 模式** | 关键业务工具 | 自建链路控制，可收敛、可重试、可审核 |

### 代码组织规范

- **枚举分组存放**：所有消息相关枚举 `MessageRole`/`MessageStatus`/`MessageType` 统一放在 `common/src/enums/message.rs`，不拆小文件
- **工具调用结构体**：`ToolCallRequest`/`ToolCallResult` 统一放在 `src/models/message.rs`，不单独新建文件
- **ContextTool 定义**：带上下文的工具 trait 放在 `pkg/tool_registry`，提供转换方法适配 rig 接口
- **日志装饰器**：`ContextToolCallLogger` 放在 `pkg/tool_tracing`，非侵入实现自动日志追踪

### 投递队列设计

消息使用**拉取模式**消费：

1. Agent 启动后 `dequeue` 获取下一个待处理消息
2. 处理完成 `ack` 确认，失败 `nack` 放回队列
3. 支持并发处理，不会重复消费

---

## 二十、泛型 Topic 分离事件队列设计规范

### 问题背景

之前使用全局动态 `Box<dyn EventQueue>` 方案，不同业务事件会互相干扰，导致消息错乱。

### 解决方案：泛型 + 泛型特化单例

每个 topic（事件类型）独立一个单例队列，类型安全隔离：

```rust
// 每个事件类型 E 自动对应一个独立队列
pub struct EventQueueDaoInMemoryImpl<E: Event + Clone> {
    events: UnsafeCell<HashMap<String, Box<E>>>,
    queues: UnsafeCell<HashMap<String, BinaryHeap<EventRef>>>,
    // ...
}

// 单例通过泛型参数自动分离
static INSTANCES: OnceLock<HashMap<String, Arc<dyn EventQueueDyn>>> = OnceLock::new();
```

### 优点

- ✅ **类型安全**：不同 topic 编译期类型分离
- ✅ **彻底隔离**：不会出现消息错乱问题
- ✅ **零运行时开销**：泛型特化，单例访问无 Box 开销
- ✅ **符合用户偏好**：按 topic 逻辑分离，命名符合业界通用概念

---

## 二十一、SQLx 离线编译问题解决规范

### 常见问题

```
error: `SQLX_OFFLINE=true` but there is no cached data for this query
```

### 标准解决流程

1. **本地有可用数据库** 运行：
```bash
cargo sqlx prepare --database-url sqlite://./test.db
```

2. **检查更新**：确认 `.sqlx/` 目录下新增了缓存文件
3. **提交缓存**：`.sqlx` 目录必须纳入版本控制
4. **重新推送**：让 CI 使用更新后的缓存编译

### 经验总结

| 要点 | 说明 |
|------|------|
| `.sqlx` 必须提交 | 否则 CI 离线编译一定会失败 |
| schema 变更后必须重新 prepare | SQL 查询变了缓存也要更 |
| 开发阶段 schema 变更推荐重建表 | SQLite 不支持 `ALTER COLUMN DROP NOT NULL`，重建比迂回修改更干净 |

---

## 最终架构流程图

### 核心设计思想

工具调用追踪是横切关注点，使用**装饰器模式**实现非侵入式自动日志记录：
- 不修改 Rig 原生 `ToolDyn` 接口
- 不修改上层调用链
- 在 `ToolDao` 拼装工具实体时自动完成包装

### 目录结构规范

```
src/pkg/tool_tracing/
├── entry.rs        # ToolCallEntry 数据结构定义 + ToolCallStatus 枚举
├── logger.rs       # ToolCallLogger 单例工厂，提供日志写入方法
├── decorator.rs    # LoggingToolDecorator 实现 Rig ToolDyn trait
├── mod.rs          # 模块导出
└── logger_test.rs  # 单元测试
```

### 存储路径规范

日志按工具 + 日期分文件存储，格式：
```
{base_data_path}/tools/{tool_id}/call_trace/{YYYYMMDD}.jsonl
```

### 设计决策总结

| 设计点 | 方案 |
|--------|------|
| 包装时机 | ToolDao 拼装工具实体时自动包装 |
| 配置获取 | ToolCallLogger 从全局 config singleton 获取 base path |
| 实例管理 | 全局单例工厂，应用启动 `init()` 一次 |
| 测试支持 | 保留 `new()` 构造方法，测试可创建本地实例 |
| 写入时机 | 工具调用完成后写入一次，不拆分多次写入 |
| 侵入性 | 装饰器模式，完全不修改原有代码 |

更多详细设计详见 [docs/tool_design.md](../docs/tool_design.md)

---

## 十八、Rig 0.35 升级适配规范

rig-core 0.35 有重大不兼容变更，适配要点：

### 核心变化

| 0.34 | 0.35 |
|------|------|
| 支持增量 `agent.tool(...)` 添加工具 | 必须一次性 `agent.tools(tool_set)` 传入所有工具 |
| `ToolSet::new()` 创建 | `ToolSet::from_tools_boxed(Vec<Box<dyn ToolDyn>>)` 创建 |

### 适配方案

```rust
// 从 Agent.tools: Vec<Tool> 提取 Box<dyn ToolDyn>
let tool_dyns: Vec<Box<dyn ToolDyn>> = agent
    .tools()
    .iter()
    .map(|tool| {
        // SAFETY: Tool 中 tool 已经是 Box<dyn ToolDyn + Send + Sync>
        // Rig 要求 Box<dyn ToolDyn>，而我们的 trait 对象额外约束了 Send + Sync
        // 这是一个安全的协变转换，因为额外的约束只会让它更严格
        unsafe { std::mem::transmute(tool.tool.clone()) }
    })
    .collect();

let tool_set = ToolSet::from_tools_boxed(tool_dyns);
let agent = AgentBuilder::new(model)
    .tools(tool_set)
    .build();
```

### 安全性说明

所有注册工具都保证实现 `Send + Sync`，Cortex 本身需要 `Send + Sync`，因此 transmute 转换是安全的。转换后的 `Box<dyn ToolDyn>` 仍然满足 Rig 的所有要求。

---

## 开发工作流规范

### 11.1 提交前标准流程

**强制流程**：代码开发完成 → 编译通过 → 运行测试 → 运行 `cargo fix` → 再提交推送 ✅

```bash
# 标准工作流
cargo check      # 先检查编译错误
# ... 修复错误 ...
cargo test       # 运行所有单元测试
# ... 修复测试失败 ...
cargo fix --allow-dirty  # 自动移除无用导入，修复可自动修复的警告
git add .
git commit
git push
```

**为什么这么做**：
- 自动移除无用导入，减少 `unused import` 警告
- 自动修复一些简单的编译错误
- 保持代码干净，提交历史整洁

### 11.2 经验总结

1. **设计对齐人类认知** → 架构更容易理解，也更容易演进
2. **渐进式实现** → 先做基础架构，再慢慢完善功能
3. **分层清晰** → 每个层职责单一，方便测试和替换
4. **常量集中管理** → SQL 建表语句统一放在迁移文件，可追溯
5. **预留扩展** → 数据库层分离，方便以后支持其他数据库
6. **原始细节人类可读** → 按天 markdown 存储，不需要工具直接就能看
7. **API 契约统一** → common crate 共享 DTO，前后端类型一致
8. **类型安全优先** → 使用枚举代替魔法数字，编译期抓错
9. **cargo fix 养成习惯** → 提交前自动清理，代码更干净

---

## 十二、domain 层开发规范

### 12.1 严格分层调用规则

**严格单向分层调用，禁止跨层调用：**

```
Handler → Domain → DAL → DAO → DB
```

| 层级 | 只能调用 | 禁止跨层直接调用 |
|------|----------|------------------|
| Handler | Domain | ❌ 禁止直接调用 DAL 或 DAO |
| Domain | DAL | ❌ 禁止直接调用 DAO 或 DB |
| DAL | DAO | ❌ 禁止直接调用 DB |

### 12.2 复用原则

**每一层只复用直接下一层提供的方法：**

- **Domain 层应该尽量复用 DAL 层提供的方法**，不需要重复组合 DAO
- **Handler 层应该尽量复用 Domain 层提供的方法**，禁止跳域调用 DAL/DAO
- 如果现有 Domain 方法已经能满足需求，handler 直接复用组合即可，不需要新增 Domain 方法
- 只有当需要新增核心业务逻辑编排时，才在 Domain 新增方法，该方法复用已有的 DAL 方法

**核心思想：** domain 是业务逻辑唯一出口，handler 必须走 domain，不能跳级。

---

## 十三、命名规范

- **文件**：snake_case 命名（`create_user.rs`、`model_provider.rs`）
- **结构体**：PascalCase 命名（`AgentPo`、`ModelProviderDalImpl`）
- **变量/函数**：snake_case 命名（`create_agent`、`find_by_id`）
- **接口 trait**：PascalCase **不带 Trait 后缀**（`AgentDao`、`ModelProviderDal`）
- **具体实现类**：接口名 + `Impl` 后缀（`AgentDalImpl`）；DAO 实现带介质标识 `XxxDaoSqliteImpl`

### 完整对齐示例

| 层级 | 接口 trait | 具体实现结构体 | 示例 |
|------|------------|----------------|------|
| DAO | `XxxDao` | `XxxDaoSqliteImpl` | `OrganizationDao` + `OrganizationDaoSqliteImpl` |
| DAL | `XxxDal` | `XxxDalImpl` | `AgentDal` + `AgentDalImpl` |
| Domain | `XxxDomain` | `XxxDomainImpl` | `HrDomain` + `HrDomainImpl` |

**设计原因：**
- 接口就是抽象，名字简洁不需要后缀
- 具体实现才需要后缀标识，因为实现类不对外暴露，只需要区分接口
- DAO 因为可能有多种存储实现，所以加上介质标识后缀（`SqliteImpl`）
- 严格依赖倒置：上层只依赖接口 trait，不依赖具体实现类

---

## 十四、测试隔离规范

### 14.1 基本原则

**测试隔离保证每个测试独立运行，不会互相污染，并发执行也不会出问题：**

| 组件类型 | 测试做法 | 生产做法 |
|---------|---------|---------|
| **无状态组件** | 可以复用单例 | 使用单例 |
| **有状态组件**（内存队列、缓存） | 测试每次调用 `new()` 新建实例 | 使用单例 |

### 14.2 实现模式

**对外暴露两种方式，兼顾隔离和性能：**
```rust
// 模块级导出，符合依赖倒置
pub fn new() -> Arc<dyn EventQueueDao> {
    Arc::new(EventQueueDaoInMemoryImpl::new())
}

// 生产单例
static INSTANCE: OnceLock<Arc<dyn EventQueueDao>> = OnceLock::new();

pub fn dao() -> Arc<dyn EventQueueDao> {
    INSTANCE.get_or_init(|| new()).clone()
}
```

### 14.3 经验总结

- 内存有状态组件如果复用全局单例，多个并发测试会互相干扰，导致随机失败
- 测试每次新建实例可以彻底解决竞争问题
- 生产仍然使用单例，不损失性能
- 上层只依赖 trait 接口，不知道具体实现，符合依赖倒置

---

## 十五、可见性规范

- 遵循最小可见性原则：不需要公开的就是 private
- 只有 trait 定义和 `dao()`/`init()` 需要公开
- 具体实现结构体本身保持 crate 可见性即可

---

## 十五、依赖添加规范

- 尽量使用 Rust 官方标准库，不需要就不加
- 第三方依赖选择活跃维护的知名 crate
- 所有依赖添加到 workspace 根 `Cargo.toml`
- common crate 只添加公共需要的依赖

---

## 十六、前端规范

- 每个页面对应一个组件文件
- 所有 API DTO 从 common 导入，不重复定义
- 配置管理：全局保存，优先从 localStorage 读取，默认 fallback 到编译嵌入默认配置

---

## 最终架构流程图

```
HTTP Request
    ↓
Axum 路由
    ↓
RequestContext 中间件 ← JWT Cookie 解析
    ↓
Handler (handlers 层，每个方法一个文件)
    ↓
Domain (domain 层，核心业务逻辑)
    ↓
DAL (dal 层，业务数据访问组合)
    ↓
DAO (dao 层，单一数据源操作)
    ↓
SQLite 数据库
```

所有层级严格遵守依赖方向，不允许反向依赖 ✅
