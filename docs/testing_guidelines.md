# 项目测试规范

## 概述

本文档定义了 ai_orz 项目的测试编写规范，旨在统一测试代码风格，减少重复代码，提高测试可维护性。

## 核心原则

### ✅ 原则 1：初始化代码必须抽取公共函数

**所有测试共用的初始化逻辑，必须抽取到独立的公共函数中，禁止每个测试重复编写。**

#### 反模式（禁止）

```rust
#[sqlx::test]
async fn test_a(pool: SqlitePool) {
    // ❌ 重复初始化代码
    crate::service::dao::agent::init();
    crate::service::dal::agent::init();
    super::init();
    let domain = domain();
    // ...
}

#[sqlx::test]
async fn test_b(pool: SqlitePool) {
    // ❌ 同样的初始化代码重复写一遍
    crate::service::dao::agent::init();
    crate::service::dal::agent::init();
    super::init();
    let domain = domain();
    // ...
}
```

#### 正确模式（必须）

```rust
/// ✅ 抽取公共初始化函数
fn init_test_env() {
    // 所有 DAO 初始化
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
    crate::service::dao::skill::init();
    
    // 所有 DAL 初始化
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
    crate::service::dal::skill::init();
    
    // Domain 初始化
    super::init();
}

#[sqlx::test]
async fn test_a(pool: SqlitePool) {
    init_test_env();  // ✅ 一行调用
    let domain = domain();
    // ...
}

#[sqlx::test]
async fn test_b(pool: SqlitePool) {
    init_test_env();  // ✅ 一行调用
    let domain = domain();
    // ...
}
```

### ✅ 原则 2：初始化函数命名规范

初始化函数命名统一使用 `init_xxx_test_env` 格式：

| 范围 | 命名示例 | 说明 |
|------|----------|------|
| 单个模块 | `init_hr_test_env()` | HR Domain 测试初始化 |
| 多个模块 | `init_tool_domain_test_env()` | Tool Domain 测试初始化 |
| 全局集成测试 | `init_integration_test_env()` | 集成测试全局初始化 |

### ✅ 原则 3：初始化函数放置位置

- **单元测试**：放在对应 `*_test.rs` 文件顶部，所有测试函数之前
- **集成测试**：放在 `tests/common/mod.rs` 等公共模块中，所有测试可导入
- **跨模块复用**：考虑提升到更上层的公共模块，避免跨测试文件重复定义

### ✅ 原则 4：初始化函数内容边界

初始化函数**只包含**：
- DAO 单例初始化
- DAL 单例初始化
- Domain 单例初始化
- 全局配置加载
- 其他所有测试共用的前置操作

初始化函数**不包含**：
- 特定测试用例的测试数据构造
- 特定测试用例的业务操作调用
- 任何与测试用例逻辑相关的代码

## 进阶模式

### 模式 A：带返回值的初始化函数

如果多个测试需要共同的对象，可在初始化函数中返回：

```rust
fn init_hr_test_env() -> (Arc<dyn HrDomain>, RequestContext) {
    // 初始化所有依赖
    crate::service::dao::agent::init();
    crate::service::dal::agent::init();
    super::init();
    
    // 返回公共对象
    let ctx = RequestContext::new_simple("admin", pool);
    (domain(), ctx)
}

// 测试中使用：
#[sqlx::test]
async fn test_create(pool: SqlitePool) {
    let (domain, ctx) = init_hr_test_env(pool);  // ✅ 一行搞定
    // ...
}
```

### 模式 B：分层初始化（按依赖层级）

对于复杂依赖场景，可分层初始化：

```rust
/// 底层：仅初始化 DAO
fn init_daos() {
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
}

/// 中层：初始化 DAL（依赖 DAO）
fn init_dals() {
    init_daos();
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
}

/// 上层：初始化 Domain（依赖 DAL）
fn init_hr_test_env() {
    init_dals();
    super::init();
}
```

### 模式 C：测试数据工厂函数

除了初始化环境，测试数据构造也应抽取工厂函数：

```rust
/// ✅ Agent 测试数据工厂
fn create_test_agent(name: &str) -> Agent {
    let po = AgentPo::new(
        name.to_string(),
        vec!["worker".to_string()],
        "Test agent".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    Agent::from_po(po)
}

// 测试中使用：
#[sqlx::test]
async fn test_create(pool: SqlitePool) {
    init_hr_test_env();
    let agent = create_test_agent("TestAgent");  // ✅ 一行创建测试数据
    // ...
}
```

## 现有最佳实践参考

### 示例 1：HR Domain 测试（已实现）

**文件**：`src/service/domain/hr/agent_test.rs`

```rust
/// ✅ 优秀示例：HR Domain 测试初始化
fn init_hr_test_env() {
    // 初始化所有 DAO
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
    crate::service::dao::skill::init();
    crate::service::dao::tool_call::init();
    
    // 初始化所有 DAL
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
    crate::service::dal::skill::init();
    
    // 初始化 HR Domain
    super::init();
}

// 4个测试全部复用：
// test_create_and_find_by_id → init_hr_test_env()
// test_list_agents → init_hr_test_env()
// test_update_agent → init_hr_test_env()
// test_delete_agent → init_hr_test_env()
```

**收益**：
- 消除了 4 份重复的初始化代码（每份 3 行 × 4 = 12 行）
- 后续新增依赖只需修改 1 处，无需修改所有测试
- 初始化逻辑统一维护，不易遗漏

### 示例 2：Message Channel 测试（已实现）

**文件**：`src/service/domain/message_channel_test.rs`

```rust
/// ✅ 优秀示例：Message Channel 测试初始化
fn init_message_channel_test_env() {
    crate::service::dao::message_channel::init();
    crate::service::dao::lark::init();
    crate::service::dao::wechat::init();
    crate::service::dao::slack::init();
    crate::service::dao::email::init();
    crate::service::dao::webhook::init();
    
    crate::service::dal::message_channel::init();
    super::init();
}

// 14 个测试全部复用此初始化函数
```

**收益**：14 个测试 × 7 行初始化 = 消除 98 行重复代码

## 代码审查检查清单

提交测试代码时，必须检查以下事项：

- [ ] **没有重复的初始化代码**：所有测试共用的初始化逻辑已抽取到公共函数
- [ ] **初始化函数命名规范**：使用 `init_xxx_test_env` 格式
- [ ] **初始化函数职责单一**：仅包含环境初始化，不包含测试业务逻辑
- [ ] **测试数据工厂化**：重复的测试数据构造已抽取工厂函数
- [ ] **注释清晰**：公共函数有清晰的文档注释说明用途

## 重构指南

如果你遇到以下情况，请考虑重构：

1. **同一初始化代码重复出现 2 次以上** → 立即抽取
2. **初始化代码超过 3 行** → 考虑抽取
3. **新增依赖需要修改 N 个测试的初始化** → 说明应该抽取公共函数了

### 重构步骤

1. 找到所有测试中重复的初始化代码
2. 在测试文件顶部创建 `init_xxx_test_env()` 函数
3. 将重复代码移动到该函数中
4. 将所有测试中的初始化代码替换为函数调用
5. 运行测试验证重构正确

## 收益总结

| 收益 | 说明 |
|------|------|
| **减少重复代码** | 消除 70%+ 的测试样板代码 |
| **降低维护成本** | 新增依赖只需修改 1 处，无需修改所有测试 |
| **统一初始化逻辑** | 避免部分测试遗漏初始化步骤导致的偶发失败 |
| **提高可读性** | 测试用例聚焦业务逻辑，而非环境准备 |
| **降低出错概率** | 初始化逻辑只需编写和验证 1 次 |

## 相关文档

- [分层架构实践指南](./LAYERED_ARCHITECTURE_PRACTICE.md)
- [代码风格规范]（待补充）

---

**最后更新**：2026-05-08
**维护者**：AI Orz 开发团队
