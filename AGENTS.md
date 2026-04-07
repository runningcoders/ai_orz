# Agent 开发规范与最佳实践

本文总结了 AI_orz 项目中 Agent 开发过程中总结出的最佳实践和规范。

---

## 一、代码分层规范

### 1.1 实体定义

所有记忆相关实体统一放在 `models/memory.rs`：

```rust
// models/memory.rs
pub struct MemoryTrace;         // 一条原始记忆，既可以在内存也可以写入文件
pub struct ShortTermMemoryIndexPo;  // 短期索引，SQLite 持久化
pub struct LongTermKnowledgeNodePo; // 长期知识图谱节点，SQLite 持久化
pub struct KnowledgeReferencePo;    // 知识节点引用原始短期索引
pub enum MemoryRole;               // 记忆角色：User / Assistant / System / Summary
```

### 1.2 数据访问层（DAO）

所有记忆相关 DAO 统一放在 `service/dao/memory/`：

```
service/dao/memory/
├── mod.rs          # 定义 trait + 单例 + 导出
├── sqlite.rs       # SQLite 特定实现
└── sqlite_test.rs  # 单元测试
```

### 1.3 SQL 建表语句规范

- 所有建表语句统一放在 `pkg/storage/sql.rs` 作为常量
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

### 1.4 数据库架构分层（可扩展设计）

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

## 二、单元测试规范

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

### 2.3 当前测试统计

- **总测试数**: 30 个
- **通过率**: 100% (30/30)
- **记忆系统新增测试**: 4 个

---

## 三、API 设计规范

### 3.1 所有 service 层方法必须传递 `RequestContext`

**强制约定**：所有 service 层（DAO/DAL/Domain）公共方法都必须传递 `ctx: RequestContext` 作为第一个参数 ✅

```rust
// ✅ 正确
fn wake_cortex(&self, ctx: RequestContext, provider: &ModelProvider, prompt: &str) -> Result<String, AppError>;

// ❌ 错误 - 缺少 ctx
fn wake_cortex(&self, provider: &ModelProvider, prompt: &str) -> Result<String, AppError>;
```

**原因**：方便后续日志串联、追踪、权限扩展，即使当前不用也必须传递

---

## 四、已完成 TODO ✅

- [x] 定义所有记忆实体：`MemoryTrace` / `ShortTermMemoryIndexPo` / `LongTermKnowledgeNodePo` / `KnowledgeReferencePo`
- [x] `MemoryTrace` 一条记忆既可以在内存也可以写入文件，ID = 内容 hash
- [x] 实现 `MemoryDao` 完整 API：追加/查询/检索/知识节点批量 upsert
- [x] 原始细节按天存储为 markdown 文件，人类可读
- [x] `pkg/storage` 重构：sql 常量集中 + sqlite 特定初始化，预留扩展其他数据库
- [x] 所有单元测试使用 SQL 常量，不重复建表语句
- [x] 所有 service 层方法都传递 `RequestContext`
- [x] 所有测试全部通过：30/30 ✅

---

## 五、经验总结

1. **设计对齐人类认知** → 架构更容易理解，也更容易演进
2. **渐进式实现** → 先做基础架构，再慢慢完善功能
3. **分层清晰** → 每个层职责单一，方便测试和替换
4. **常量集中管理** → SQL 建表语句统一放在 `pkg/storage/sql.rs`，避免重复
5. **预留扩展** → 数据库层分离，方便以后支持其他数据库
6. **原始细节人类可读** → 按天 markdown 存储，不需要工具直接就能看
