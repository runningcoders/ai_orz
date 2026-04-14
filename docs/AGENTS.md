# AI Orz 开发规范

## 代码结构规范

### 1. 分层职责规范（严格遵守）

| 层级 | 职责 | 不做什么 |
|------|------|------|
| **models** | 定义 PO 持久化实体、业务实体 | 不要放业务逻辑 |
| **service/dao** | 单一数据源操作，增删改查，不包含业务逻辑 | 不要放业务逻辑，只做单一数据源操作 |
| **service/dal** | 组合多个 DAO，提供业务级数据操作能力 | 不要放核心业务规则 |
| **service/domain** | 核心业务逻辑实现 | 只放业务规则，编排 DAL |
| **handlers** | HTTP 接口接收请求，调用 domain，返回响应 | 不要放业务逻辑 |

> **约定**：所有 service 层公共方法都必须带 `ctx: RequestContext` 参数，方便日志追踪和权限控制。现在已经架构升级为自动从请求提取注入 RequestContext，无需手动提取。

### 2. Handler 层规范

**拆分规则：**
- 按业务域分组（`organization/`、`user/`、`agent/`、`model_provider/`）
- 每个 handler 方法拆分到独立文件
- `mod.rs` 只导出模块，不写实现
- 每个文件包含：DTO 定义 + handler 实现

> 示例：
> ```
> src/handlers/organization/
> ├── organization/
> │   ├── create_organization.rs
> │   ├── get_organization.rs
> │   ├── update_organization.rs
> │   └── mod.rs
> ├── user/
> │   ├── create_user.rs
> │   ├── list_users.rs
> │   └── mod.rs
> └── auth/
>     ├── login.rs
>     ├── logout.rs
>     └── mod.rs
> ```

### 3. DAO 层规范

**结构规范：**
- trait 定义放在 `mod.rs`
- 具体实现放在 `sqlite.rs`（当前只实现 SQLite）
- 单例 `OnceLock` 放在实现文件 `sqlite.rs`，不是 `mod.rs`
- `mod.rs` 只导出 `pub use sqlite::{dao, init};`
- 单元测试文件名：`sqlite_test.rs`，和实现文件同名

> **DAO trait 示例：**
> ```rust
> // src/service/dao/agent/mod.rs
> pub trait AgentDaoTrait: Send + Sync {
>     fn insert(&self, ctx: RequestContext, agent: &AgentPo) -> Result<(), AppError>;
>     fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<AgentPo>, AppError>;
>     // ...
> }
> ```

### 4. 枚举规范

**所有枚举都分组存放在 `common/src/enums/`：**

```
common/src/enums/
├── agent.rs          # AgentStatus 等 Agent 相关枚举
├── organization.rs   # OrganizationStatus, OrganizationScope
├── user.rs          # UserRole, UserStatus
├── message.rs        # MessageRole, MessageType, MessageStatus
├── provider.rs       # ProviderType, ModelProviderStatus
```

- 所有枚举都添加 `#[derive(Serialize, Deserialize, Clone, Debug)]`
- 所有枚举都需要实现 `fn from_i32(v: i32) -> Self` 和 `fn to_i32(&self) -> i32` 方法方便数据库转换
- 前后端共享同一个枚举定义，保证 API 契约一致

### 5. 共享类型规范

**所有前后端共享的类型：**
- DTO → `common/src/api/` 按业务分组存放
- 枚举 → `common/src/enums/` 按领域分组存放
- 配置结构体 → `common/src/config.rs`
- 常量 → `common/src/constants/`

> PO（持久化对象）不用移动到 common，保留在后端 `src/models/`

### 6. 记忆系统设计规范 ✅

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

### 7. 单元测试规范 ✅

**测试设计原则：**
- 每个 DAO/DAL/Domain 模块对应一个独立测试文件
- **每个测试函数独立**：使用随机临时 SQLite 文件，每次测试重新初始化全局 storage OnceLock
- 测试之间互不干扰，一个失败不影响其他
- 测试直接使用具体实现类，不依赖全局单例
- 所有测试都使用 DAO/DAL 接口方法测试，不直接拼接 SQL

**当前状态：**
- ✅ 所有测试都已正确添加模块声明
- ✅ 每个测试独立，互不干扰
- ✅ 66 个测试全部通过

### 8. 事件总线设计 ✅

详见 [event_design.md](./event_design.md)

**核心设计要点：**
- ✅ 所有事件先持久化到 SQLite `messages` 表
- ✅ 总线只存元数据 `message_id`，不存完整内容
- ✅ 服务启动自动恢复所有 `pending` 事件
- ✅ 优先级排序：`priority DESC, created_at ASC`
- ✅ 顺序保证：相同 `order_key` 顺序消费，不同 `order_key` 并行消费

### 9. 认证规范 ✅

**认证方案：**
- JWT 签名 + HttpOnly Cookie 存储
- 路由分组：公共路由 / 保护路由
- 未认证自动 302 重定向到首页
- RequestContext 中间件自动从 JWT 解析出 `user_id` 和 `organization_id` 注入，无需 handler 手动提取

### 10. 配置规范 ✅

**配置设计：**
- 默认配置嵌入二进制，存放在 `common/config/ai_orz.toml`
- 后端加载配置文件，默认配置 + 用户自定义覆盖
- 前端编译时嵌入默认配置
- 前端运行时允许用户修改配置保存到 `localStorage`
- 优先级：**用户 localStorage 修改 > 编译默认配置**

### 11. 命名规范

- **文件**：snake_case 命名（`create_user.rs`、`model_provider.rs`）
- **结构体**：PascalCase 命名（`AgentPo`、`ModelProviderDal`）
- **变量/函数**：snake_case 命名（`create_agent`、`find_by_id`）
- **trait**：PascalCase + 后缀 `Trait`（`AgentDaoTrait`、`ModelProviderDalTrait`）

### 12. SQLx + SQLite 开发规范

完整规范详见 [sqlx_guide.md](./sqlx_guide.md)，核心要点：

- **所有表必须启用 `STRICT` 模式**，保证 sqlx 可空性推断正确
- **枚举使用 i32 映射**：添加 `#[repr(i32)]` + `#[derive(sqlx::Type)]` + `From<i64>` 实现
- **仅枚举需要显式类型标注**：`status as "status: TaskStatus"`，普通字段不需要
- **SQL 关键字必须转义**：`status`、`role` 等关键字用作列名时用 `"status"` 双引号转义
- **软删除约定**：已删除 `status = 0`，所有查询默认添加 `AND "status" != 0` 过滤
- **`.sqlx` 目录必须纳入版本控制**，保证离线编译正常
- **测试使用 `#[sqlx::test]`**，每个测试独立内存数据库，彻底解决测试污染

### 13. 可见性规范

- 遵循最小可见性原则：不需要公开的就是 private
- 只有 trait 定义和 `dao()`/`init()` 需要公开
- 具体实现结构体本身保持 crate 可见性即可

### 14. 依赖添加规范

- 尽量使用 Rust 官方标准库，不需要就不加
- 第三方依赖选择活跃维护的知名 crate
- 所有依赖添加到 workspace 根 `Cargo.toml`
- common crate 只添加公共需要的依赖

### 15. 前端规范

- 每个页面对应一个组件文件
- 所有 API DTO 从 common 导入，不重复定义
- 配置管理：全局保存，优先从 localStorage 读取，默认 fallback 到编译嵌入默认配置

### 12. 可见性规范

- 遵循最小可见性原则：不需要公开的就是 private
- 只有 trait 定义和 `dao()`/`init()` 需要公开
- 具体实现结构体本身保持 crate 可见性即可

### 13. 依赖添加规范

- 尽量使用 Rust 官方标准库，不需要就不加
- 第三方依赖选择活跃维护的知名 crate
- 所有依赖添加到 workspace 根 `Cargo.toml`
- common crate 只添加公共需要的依赖

### 14. 前端规范

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
