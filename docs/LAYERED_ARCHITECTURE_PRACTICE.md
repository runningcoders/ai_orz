# 分层架构重构实践记录

> 记录日期：2024-04-28
> 背景：工具绑定架构从 DAO 层组装重构为严格分层调用

---

## 📋 问题背景

最初的设计存在**职责混淆**问题：
- `CortexDao` 直接依赖 `ToolCallDao` 进行工具实体组装
- DAO 层承担了超出"数据持久化"的业务逻辑
- 跨 DAO 依赖导致分层边界模糊，测试隔离困难

**反模式示例：**
```rust
// ❌ 错误：DAO 层跨领域依赖其他 DAO
impl CortexDao for CortexDaoSqliteImpl {
    async fn wake(&self, ctx: &RequestContext, brain_id: Uuid) -> Result<Brain> {
        // ToolCallDao 被直接在 DAO 层调用
        let tool_dao = ToolCallDaoSqliteImpl::new();
        let tools = tool_dao.get_enabled_tools(ctx).await?;
        // ...
    }
}
```

---

## ✅ 最终分层架构方案

### 核心原则

**单向依赖 + 逐层调用 + 单一职责**

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

### 各层职责边界

| 层级 | 职责 | 禁止做 |
|------|------|--------|
| **DAO** | 单一数据源的 CRUD 操作<br>SQL 拼接<br>数据库实体 PO 转换 | ❌ 跨 DAO 依赖<br>❌ 业务逻辑<br>❌ 实体组装/装饰 |
| **DAL** | 组合多个 DAO<br>提供业务级数据接口<br>业务实体组装/过滤 | ❌ 跨 DAL 依赖<br>❌ 复杂业务编排 |
| **Domain** | 组合多个 DAL<br>核心业务逻辑编排<br>跨领域事务 | ❌ 直接操作数据库 |
| **Handler** | HTTP 路由<br>参数校验<br>调用 Domain 服务 | ❌ 业务逻辑 |

---

## 🔧 具体重构实践

### 1. Tool 实体组装重构

**问题**：`Tool` 实体需要 `ToolPo` + `Box<dyn CoreTool>` 两部分组合而成。最初想在 DAO 层完成组装。

**解决方案**：分层组装
- **DAO 层 (`ToolCallDao`)**：只负责从数据库读取 `ToolPo`，不做任何组装
- **DAL 层 (`ToolDal`)**：调用 `ToolCallDao` 获取 PO，然后通过 `ToolRegistry` 查找注册的工厂，组装完整 `Tool` 实体
- **Domain 层**：调用 `ToolDal` 获取完整工具列表，注入到 Agent/Brain

**代码示例：**
```rust
// ✅ DAO 层：只做数据读取
impl ToolCallDao for ToolCallDaoSqliteImpl {
    async fn get_tools_by_agent(&self, ctx: &RequestContext, agent_id: Uuid) -> Result<Vec<ToolPo>> {
        // 纯 SQL 查询
    }
}

// ✅ DAL 层：负责业务组装
impl ToolDalImpl {
    async fn list_tools_by_agent(&self, ctx: &RequestContext, agent_id: Uuid) -> Result<Vec<Tool>> {
        let pos = self.tool_call_dao.get_tools_by_agent(ctx, agent_id).await?;
        
        // 通过注册表过滤并组装完整实体
        let registry = get_registry();
        let mut tools = Vec::new();
        for po in pos {
            if let Some(factory) = registry.get_factory(&po.id) {
                let our_tool = factory.create(po.clone());
                tools.push(Tool { po, our_tool });
            }
        }
        Ok(tools)
    }
}
```

### 2. 工具注册表模式

引入 `ToolRegistry` 单例模式，解决工具实例化问题：

```rust
// 工具工厂 trait
pub trait BuiltinToolFactory: Send + Sync {
    fn id(&self) -> &str;
    fn create(&self, po: ToolPo) -> Box<dyn CoreTool>;
}

// 注册表（可扩展支持动态注册）
pub struct ToolRegistry {
    factories: RwLock<HashMap<String, Box<dyn BuiltinToolFactory>>>,
}

impl ToolRegistry {
    pub fn register_builtin_factory(&self, factory: Box<dyn BuiltinToolFactory>) {
        self.factories.write().insert(factory.id().to_string(), factory);
    }
    
    pub fn get_factory(&self, id: &str) -> Option<Box<dyn BuiltinToolFactory>> {
        self.factories.read().get(id).cloned()
    }
}
```

**设计优点：**
- ✅ 支持编译期静态注册 + 运行期动态注册
- ✅ 解耦工具定义和实例化
- ✅ 支持热插拔和扩展

---

## 🧪 测试最佳实践

### 1. 测试隔离原则

**每个测试都是独立的，不依赖其他测试的副作用**

```rust
#[sqlx::test]
async fn test_add_tool_to_agent(pool: SqlitePool) {
    // ✅ 每个测试独立初始化各层
    tool::init();
    crate::service::dao::tool_call::init();
    crate::service::dal::tool::init();
    register_test_factory(); // 测试专用工厂
    
    let tool_dal = tool::get_dal();
    // ... 测试逻辑
}
```

### 2. 测试专用 Mock 工厂

在测试环境中，使用简单的测试工厂验证组装逻辑，不依赖真实工具实现：

```rust
// 测试工具工厂
#[derive(Clone)]
struct TestToolFactory;

impl BuiltinToolFactory for TestToolFactory {
    fn id(&self) -> &str {
        "test_tool"
    }
    
    fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
        Box::new(TestTool { po })
    }
}

// 测试工具实现
#[derive(Clone)]
struct TestTool {
    po: ToolPo,
}

#[async_trait]
impl CoreTool for TestTool {
    fn po(&self) -> &ToolPo {
        &self.po
    }
    
    async fn call(&self, _ctx: &RequestContext, _args: Value) -> Result<Value, ToolError> {
        Ok(Value::Null) // 测试用空实现
    }
}
```

### 3. 分层测试策略

| 层级 | 测试重点 |
|------|----------|
| DAO | SQL 正确性、数据转换、事务边界 |
| DAL | 组合逻辑、实体组装、业务规则过滤 |
| Domain | 业务流程、跨领域协调、事务一致性 |
| Handler | API 契约、参数校验、错误码映射 |

---

## ❌ 常见陷阱

### 陷阱 1：DAO 层跨 DAO 依赖

**错误**：
```rust
// ❌ CortexDao 直接调用 ToolCallDao
async fn wake(&self, ctx: &RequestContext, brain_id: Uuid) -> Result<Brain> {
    let tool_dao = ToolCallDaoSqliteImpl::new();
    let tools = tool_dao.get_enabled_tools(ctx).await?;
    // ...
}
```

**危害**：
- 分层边界模糊
- 测试时需要 mock 多层依赖
- 难以单独优化和替换

**正确做法**：上层注入/调用，DAO 只做自己的事。

---

### 陷阱 2：组装逻辑位置错误

**错误**：在 DAO 层完成实体组装
```rust
// ❌ DAO 层承担组装职责
async fn get_tool(&self, ctx: &RequestContext, id: Uuid) -> Result<Option<Tool>> {
    let po = ...; // 从数据库读取
    // 这里做组装... ❌
}
```

**为什么错**：
- DAO 层不应该依赖业务逻辑（注册表、工厂）
- 不同业务场景可能需要不同的组装策略
- 难以测试不同组装逻辑

**正确做法**：组装逻辑放在 DAL 或 Domain 层。

---

### 陷阱 3：循环依赖

**错误**：
```
DaoA → DaoB → DaoA
```

**预防**：
- 严格禁止同层互调
- 依赖方向只能是向上（上层调用下层）
- 跨领域组合放在 Domain 层

---

## 📦 Rig 包名问题记录

### 问题描述

`rig-core` crate 在 `0.34` 版本中，`ToolDefinition` 的位置发生变化，导致编译错误：

```
error[E0670]: `async fn` is not permitted in Rust 2015
error[E0432]: unresolved import `rig::completion::ToolDefinition`
```

### 根本原因

1. **Edition 配置问题**：Rust 2015 edition 不支持 `async fn` 在 trait 中，即使使用了 `#[async_trait]`。

2. **Rig 内部模块重构**：
   - `rig-core 0.34` 内部模块结构调整
   - `ToolDefinition` 的导出路径变化
   - 部分类型从 `rig::completion::*` 移动到 `rig::tool::*`

### 解决方案

**方案 1：确认 Edition 配置**

确保 `Cargo.toml` 使用正确的 edition：
```toml
[package]
edition = "2024"  # 推荐使用最新版
```

**方案 2：正确的导入路径**

```rust
// ✅ 正确：从 rig::tool 导入
use rig::tool::{ToolDyn, ToolError, ToolDefinition};

// ❌ 错误：旧路径
// use rig::completion::ToolDefinition;
```

**方案 3：版本锁定**

如果依赖特定版本的 rig，在 `Cargo.toml` 中明确锁定：
```toml
[dependencies]
rig-core = "=0.34"  # 锁定精确版本
```

### 经验教训

1. **外部 crate 升级需要谨慎**：每次升级前检查 CHANGELOG
2. **类型重导出不稳定**：避免过度依赖 crate 的 re-export 路径
3. **Edition 全局影响**：项目级别的 edition 配置会影响所有依赖
4. **最小化依赖暴露**：内部 trait 尽量封装，不要直接暴露外部 crate 类型

---

## ✅ 重构验证标准

完成分层重构后，通过以下标准验证正确性：

### 1. 编译检查
```bash
cargo check --tests
# 期望：0 错误
```

### 2. 测试覆盖率
```bash
cargo test
# 期望：所有测试通过
```

**当前结果**：✅ 165 passed; 0 failed

### 3. 架构守护检查

- [ ] 没有 `use super::super::` 跨层直接访问
- [ ] DAO 层不依赖其他 DAO
- [ ] DAL 层不依赖其他 DAL
- [ ] 所有业务逻辑在 Domain/DAL/DAO 正确分层

---

## 📝 总结

### 这次重构的核心收获

1. **单一职责是根本**：每个层、每个对象只做一件事
2. **依赖方向要正确**：永远是上层调用下层，不要反向
3. **测试隔离很重要**：可测试的设计才是好设计
4. **注册表模式通用**：遇到"需要动态创建不同类型实例"的场景，注册表/工厂模式几乎总是正确答案

### 后续演进方向

1. 引入 `tower::Layer` 模式实现横切关注点（日志、指标、缓存）
2. 考虑引入 `Repository` 模式进一步抽象数据访问
3. 实现编译期分层检查（通过 cargo 工具或自定义 linter）

---

**文档维护者**：架构组  
**下次更新**：Domain 层完成后
