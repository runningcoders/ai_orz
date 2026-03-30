# 命名规范

## 1. 通用原则

- **清晰明确**：命名应自解释，顾名思义
- **简洁**：在清晰的前提下尽量简短
- **一致性**：同一项目内保持风格统一

## 2. 命名风格

### 2.1 蛇底式（snake_case）⭐ 推荐

用于：**变量名、函数名、方法名、文件（模块）名**

```
// ✅ 正确
user_id, agent_name, create_agent, find_by_id, new_ctx_with_user

// ❌ 错误（直接拼接两个单词）
userid, agentname, createAgent, newCtxWithUser
```

### 2.2 帕斯卡命名（PascalCase）

用于：**类型名、结构体名、枚举名、Trait 名**

```
// ✅ 正确
AgentPo, RequestContext, AgentDaoTrait, AgentPoStatus

// ❌ 错误（小驼峰）
agentPo, requestContext, agentDaoTrait
```

### 2.3 全大写下划线（SNAKE_CASE）

用于：**常量、全局常量**

```rust
// ✅ 正确
const MAX_SIZE: i32 = 100;
const LOG_ID: &str = "X-Log-Id";

// ❌ 错误
const maxSize: i32 = 100;
const logId: &str = "X-Log-Id";
```

## 3. 函数/方法命名

### 3.1 获取数据（只读操作）

```rust
// ✅ get_ 前缀（如果有参数）
fn get_agent_by_id(id: &str) -> Option<Agent>
fn get_user_name(user_id: &str) -> String

// ✅ 直接命名（无参数或简单获取）
fn agent_dao() -> Arc<dyn AgentDaoTrait>  // 单例获取
fn new() -> Self                             // 构造新实例
fn uid(&self) -> String                       // 只读属性获取
```

### 3.2 创建/新增

```rust
// ✅ new 前缀或直接命名
fn new(user_id: String) -> Self
fn create_agent(req: CreateReq) -> Result<Agent>
```

### 3.3 修改/更新

```rust
// ✅ update 前缀
fn update_agent(id: &str, req: UpdateReq) -> Result<()>
```

### 3.4 删除（软删除）

```rust
// ✅ delete 前缀
fn delete(id: &str, deleted_by: &str) -> Result<()>
```

### 3.5 列表/批量

```rust
// ✅ find_all 或 find_by_xxx
fn find_all() -> Vec<Agent>
fn find_by_org(org_id: &str) -> Vec<Agent>
```

### 3.6 判断/布尔

```rust
// ✅ is_ / has_ / can_ 前缀
fn is_deleted(&self) -> bool
fn has_permission(&self, action: &str) -> bool
```

## 4. 变量命名

### 4.1 普通变量

```rust
// ✅ snake_case
let user_id = "123";
let agent_name = "my_agent";
let ctx = RequestContext::new(None, None);

// ❌ 错误
let userId = "123";
let agentName = "my_agent";
```

### 4.2 布尔变量

```rust
// ✅ is_ / has_ / can_ 前缀
let is_active = true;
let has_permission = false;
let can_edit = true;
```

### 4.3 集合变量

```rust
// ✅ 复数形式
let agents = Vec::new();
let user_ids = vec!["1", "2", "3"];
```

### 4.4 临时变量

```rust
// ✅ 简洁但有意义
let ctx = RequestContext::new(None, None);
let dao = agent_dao();
let conn = &db.conn;
```

## 5. 文件/目录命名

### 5.1 Rust 源文件

```rust
// ✅ snake_case.rs
// 文件名与内部类型对应（文件名全小写 + 下划线）
agent.rs              // 对应 mod agent
agent_sqlite.rs      // 对应 AgentDao 等
request_context.rs   // 对应 RequestContext

// ❌ 错误（直接拼接）
agentdao.rs
userid.rs
createagent.rs
```

### 5.2 模块目录

```
// ✅ snake_case
service/
├── dao/
│   ├── agent/
│   │   ├── mod.rs
│   │   └── sqlite.rs
│   └── org/
│       ├── mod.rs
│       └── sqlite.rs
```

## 6. 类型命名

### 6.1 结构体/枚举

```rust
// ✅ PascalCase
struct AgentPo { ... }
struct RequestContext { ... }
enum AgentPoStatus { ... }
```

### 6.2 Trait

```rust
// ✅ PascalCase + Trait 后缀（可选）
trait AgentDaoTrait { ... }
trait Service { ... }
```

## 7. 常量命名

```rust
// ✅ 全大写下划线
const MAX_RETRY: u32 = 3;
const DEFAULT_TIMEOUT: u64 = 5000;

// HTTP Header Keys
pub mod http_header {
    pub const LOG_ID: &str = "X-Log-Id";
    pub const USER_ID: &str = "X-User-Id";
}
```

## 8. 常见错误对照

| 错误写法 | 正确写法 |
|---------|---------|
| `userId`, `agentName` | `user_id`, `agent_name` |
| `newAgent()`, `createAgent()` | `new_agent()`, `create_agent()` |
| `getUserById()` | `get_user_by_id()` |
| `agentDAO`, `OrgDAO` | `agent_dao`, `org_dao` |
| `agentPo`, `requestContext` | `AgentPo`, `request_context`（文件） |
| `maxSize`, `logId` | `max_size`, `log_id` |
| `ctxWithUser` | `ctx_with_user` |

## 9. 特殊约定

### 9.1 DAO 单例获取

```rust
// ✅ 函数名直接用实体名
fn agent_dao() -> Arc<dyn AgentDaoTrait>
fn org_dao() -> Arc<dyn OrganizationDaoTrait>

// ❌ 避免
fn get_agent_dao()
fn new_agent_dao()
```

### 9.2 DAO 初始化

```rust
// ✅ init 后缀
pub(super) fn init()
```

### 9.3 ID 生成

```rust
// ✅ generate_id / generate_log_id
fn generate_id() -> String
fn generate_log_id() -> String
```

### 9.4 Context 方法

```rust
// ✅ 只读方法用名词形式
fn uid(&self) -> String       // 获取用户 ID
fn uname(&self) -> String     // 获取用户名
fn log_id(&self) -> &str      // 获取日志 ID
```

## 10. Context 传递规范 ⭐

### 10.1 核心原则

**service 层所有对外暴露的方法，都必须传递 RequestContext 参数，保证链路完整性。**

### 10.2 规则

1. **所有接口方法第一个参数必须是 `ctx: RequestContext`**
2. **删除 `deleted_by` 等用户相关参数**，从 `ctx.uid()` 获取
3. **只在必要时传递 Context**，内部私有方法可省略
4. **只读操作也传递 Context**，便于日志记录

### 10.3 示例

```rust
// ✅ 正确：接口包含 ctx
pub trait AgentDaoTrait: Send + Sync {
    fn insert(&self, ctx: RequestContext, conn: &Connection, agent: &AgentPo) -> Result<(), AppError>;
    fn find_by_id(&self, ctx: RequestContext, conn: &Connection, id: &str) -> Result<Option<AgentPo>, AppError>;
    fn update(&self, ctx: RequestContext, conn: &Connection, agent: &AgentPo) -> Result<(), AppError>;
    fn delete(&self, ctx: RequestContext, conn: &Connection, id: &str) -> Result<(), AppError>;
}

// ✅ 正确：从 context 获取用户信息
fn delete(&self, ctx: RequestContext, conn: &Connection, id: &str) -> Result<(), AppError> {
    // deleted_by 从 ctx 获取，不再作为参数
    conn.execute("UPDATE ... SET modified_by = ?1 ...", rusqlite::params![ctx.uid()])
}

// ❌ 错误：用户信息作为单独参数
fn delete(&self, conn: &Connection, id: &str, deleted_by: &str) -> Result<(), AppError>
```

### 10.4 Context 获取用户信息

```rust
ctx.uid()      // 获取用户 ID，未登录返回 ""
ctx.uname()    // 获取用户名，未登录返回 ""
ctx.log_id     // 获取日志 ID
```

### 10.5 调用链路

```
HTTP Request
    ↓
Handler（从 header 提取 ctx）
    ↓
Domain（传递 ctx）
    ↓
DAL（传递 ctx）
    ↓
DAO（传递 ctx，可记录日志）
    ↓
Database
```

## 11. 本项目检查清单

- [ ] 变量名使用 snake_case
- [ ] 函数名使用 snake_case
- [ ] 类型名使用 PascalCase
- [ ] 常量使用 SNAKE_CASE
- [ ] 文件名使用 snake_case.rs
- [ ] 避免直接拼接两个单词（用下划线）
- [ ] 布尔变量用 is_/has_/can_ 前缀
- [ ] 只读方法返回值而非 &T（除非需要引用）
- [ ] service 层方法必须传递 RequestContext
