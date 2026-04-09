# Agent 开发规范与最佳实践

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

### 2.1 实体定义

所有持久化 PO 实体统一放在 `src/models/`：

- `agent.rs` - Agent 实体
- `brain.rs` - Brain 实体
- `cortex.rs` - Cortex 实体
- `model_provider.rs` - ModelProvider 实体
- `organization.rs` - Organization 组织实体
- `user.rs` - User 用户实体
- `memory.rs` - 记忆系统所有实体

### 2.2 数据访问层（DAO）

每个 DAO 模块统一放在 `service/dao/{domain}/`：

```
service/dao/memory/
├── mod.rs          # 定义 trait + 单例 + 导出
├── sqlite.rs       # SQLite 特定实现
└── sqlite_test.rs  # 单元测试
```

### 2.3 SQL 建表语句规范

- 所有建表语句统一放在 `src/pkg/storage/sql.rs` 作为常量
- 每个常量对应一个实体，注释标明对应实体
- 单元测试也使用同一个常量，不重复写建表语句
- 启动时自动创建所有表，在 `pkg/storage/sqlite.rs::init_db` 中统一初始化

```rust
// pkg/storage/sql.rs
/// SQLite: 短期记忆索引表建表语句
///
/// 对应实体: [crate::models::memory::ShortTermMemoryIndexPo]
pub const SQLITE_CREATE_TABLE_SHORT_TERM_MEMORY_INDEX: &str = r#"..."#;
```

### 2.4 数据库架构分层（可扩展设计）

```
pkg/storage/
├── mod.rs      # 公共接口：全局 Storage 单例 + init
├── sql.rs       # 所有建表 SQL 常量集中定义
└── sqlite.rs    # SQLite 特定初始化，调用 sql 常量建表
```

**优点：**
- 预留扩展其他数据库，以后添加 PostgreSQL 只需要添加 `pkg/storage/postgres.rs`
- SQL 常量集中管理，不会重复
- 公共接口保持不变，上层不需要改动

---

## 三、枚举类型安全规范

### 3.1 基本原则

所有存储在数据库中的枚举状态/角色字段，**必须使用 Rust 枚举类型**，禁止直接使用 `i32` 存储：

✅ **正确做法：**
```rust
// common 中定义枚举，实现 ToSql/FromSql
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

### 3.2 实现要求

- 在 common 中定义枚举
- 为枚举实现 `serde::Serialize/Deserialize`（用于 API 序列化）
- 为枚举实现 `rusqlite::ToSql` 和 `rusqlite::FromSql`（启用 common 的 `rusqlite` 可选特性）
- 为枚举实现 `Default`（默认值为正常/启用状态）
- 添加 `from_i32() -> Self` 和 `to_i32() -> i32` 方法方便转换

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

### 2.1 测试位置

- 每个 DAO 模块下直接放置测试文件：`service/dao/xxx/sqlite_test.rs`
- 测试使用内存数据库，不依赖全局连接池，独立可运行

### 2.2 测试规范

```rust
// 错误：硬编码建表语句 ❌
conn.execute("CREATE TABLE ...", ()).unwrap();

// 正确：使用定义好的常量 ✅
conn.execute(crate::pkg::storage::sql::SQLITE_CREATE_TABLE_XXX, ()).unwrap();
```

### 5.1 当前测试统计

- **总测试数**: 35 个
- **通过率**: 100% (35/35)
- **记忆系统新增测试**: 8 个

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

## 七、已完成架构演进 ✅

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

---

## 八、开发工作流规范

### 8.1 提交前标准流程

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

### 8.2 经验总结

1. **设计对齐人类认知** → 架构更容易理解，也更容易演进
2. **渐进式实现** → 先做基础架构，再慢慢完善功能
3. **分层清晰** → 每个层职责单一，方便测试和替换
4. **常量集中管理** → SQL 建表语句统一放在 `pkg/storage/sql.rs`，避免重复
5. **预留扩展** → 数据库层分离，方便以后支持其他数据库
6. **原始细节人类可读** → 按天 markdown 存储，不需要工具直接就能看
7. **API 契约统一** → common crate 共享 DTO，前后端类型一致
8. **类型安全优先** → 使用枚举代替魔法数字，编译期抓错
9. **cargo fix 养成习惯** → 提交前自动清理，代码更干净

---

## 九、domain 层开发规范

### 9.1 尽量复用已有方法

尽量复用已有的 DAO/DAL 方法，不轻易在 domain 新增方法。简单的查询可以在 handler 直接调用 DAL 方法，不需要新增 domain 方法包装：

✅ **正确：**
```rust
// handler 直接复用已有 DAL 方法
let user = user_dal().get_user_by_id(ctx, user_id)?;
```

❌ **避免：**
```rust
// 无意义包装，增加不必要层
// domain 中定义 get_user_by_id，里面只调用 DAL 的同名方法
let user = domain().user_manage().get_user_by_id(ctx, user_id)?;
```

**原则：** 只有当需要核心业务逻辑编排时才在 domain 新增方法，简单数据查询直接在 handler 调用 DAL。
