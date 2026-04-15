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

- **总测试数**: 104 个
- **通过率**: 100% (104/104) ✅

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

**沉淀机制：**
- 短期记忆聚合多条原始细节为一条摘要
- 每日"睡眠"阶段：短期记忆沉淀到长期知识图谱
- 知识关系独立存表，支持灵活查询扩展

---

## 九、事件总线设计 ✅

详见 [docs/event_design.md](./docs/event_design.md)

**核心设计要点：**
- ✅ 所有事件先持久化到 SQLite `messages` 表
- ✅ 总线只存元数据 `message_id`，不存完整内容
- ✅ 服务启动自动恢复所有 `pending` 事件
- ✅ 优先级排序：`priority DESC, created_at ASC`
- ✅ 顺序保证：相同 `order_key` 顺序消费，不同 `order_key` 并行消费

---

## 十、已完成架构演进 ✅

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

---

## 十一、开发工作流规范

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
- **结构体**：PascalCase 命名（`AgentPo`、`ModelProviderDal`）
- **变量/函数**：snake_case 命名（`create_agent`、`find_by_id`）
- **trait**：PascalCase + 后缀 `Trait`（`AgentDaoTrait`、`ModelProviderDalTrait`）

---

## 十四、可见性规范

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
