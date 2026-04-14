# SQLx 0.8 + SQLite 开发规范与最佳实践

本文档记录 ai_orz 项目从 rusqlite 迁移到 sqlx 0.8 过程中总结出的开发规范和避坑指南。

## 目录

- [数据库 Schema 设计](#数据库-schema-设计)
- [枚举类型映射](#枚举类型映射)
- [可空性处理规则](#可空性处理规则)
- [查询宏使用规范](#查询宏使用规范)
- [SQL 关键字转义](#sql-关键字转义)
- [测试隔离最佳实践](#测试隔离最佳实践)
- [软删除查询约定](#软删除查询约定)
- [离线查询缓存](#离线查询缓存)
- [常见坑点排查](#常见坑点排查)

## 数据库 Schema 设计

### 1. 必须启用 STRICT 模式

**规则：** 所有 CREATE TABLE 必须添加 `STRICT;`

```sql
-- ✅ 正确
CREATE TABLE tasks (
    ...
) STRICT;

-- ❌ 错误：缺少 STRICT
CREATE TABLE tasks (...);
```

**原因：**
- SQLite 默认允许任意类型插入任意列，即使定义为 NOT NULL 也能插入 NULL
- 开启 STRICT 后 SQLite 强制校验列类型，sqlx 可以从数据库 schema 正确推断可空性
- 不开启 STRICT 时，即使列定义为 NOT NULL，sqlx 也会推断为 `Option<T>`，导致类型不匹配

### 2. 默认值约定

- TEXT 非空字段使用 `NOT NULL DEFAULT ''`
- INTEGER 非空字段使用 `NOT NULL DEFAULT 0`
- 允许使用可空字段，语义上真正允许 NULL 的才定义为可空

## 枚举类型映射

### i32 整数枚举映射（推荐）

```rust
// 1. 添加 repr(i32) 和 sqlx Type derive
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[repr(i32)]
pub enum TaskStatus {
    Canceled = 0,
    Pending = 1,
    InProgress = 2,
    Completed = 3,
    Archived = 4,
}

// 2. 添加 From<i64> 实现适配 sqlx 类型推断
impl From<i64> for TaskStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

// 3. 查询时需要显式类型标注
sqlx::query!(
    r#"SELECT id, title, status as "status: TaskStatus" FROM tasks WHERE id = ?"#,
    id
)
```

**常见错误：**
- ❌ 不要添加 `#[sqlx(rename_all = "lowercase")]`：这是给字符串枚举用的，会导致 sqlx 期望解析字符串而不是整数
- ❌ 忘记 `From<i64>`：sqlx 从 SQLite 获取的整数默认是 i64，缺少转换会编译失败

### 条件编译（给 common 包前后端共享枚举）

如果枚举在 common 包被前后端共享，需要给 sqlx 相关代码添加条件编译：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum TaskStatus {
    // ...
}

#[cfg(feature = "sqlx")]
impl From<i64> for TaskStatus {
    // ...
}

#[cfg(feature = "sqlx")]
impl<'a> sqlx::Type<sqlx::Sqlite> for TaskStatus {
    // ...
}
```

Cargo.toml 中配置：
```toml
[features]
default = ["sqlx"]
sqlx = ["sqlx-sqlite", "sqlx-core"]
```

这样前端 WASM 编译时可以禁用 sqlx 特性，避免编译 libsqlite3-sys 失败。

## 可空性处理规则

1. **严格按照 schema 定义**：只在真正允许 NULL 的字段使用 `Option<T>`，NOT NULL 字段直接用非 Option 类型
2. **SQLite TEXT 列特例**：即使定义为 `NOT NULL DEFAULT ''`，SQLx 默认也会推断为 `Option<String>`，实际开发中推荐：
   - 如果字段确实允许 NULL：保留 `Option<String>`
   - 如果字段是 `NOT NULL DEFAULT ''`：保持 `Option<String>` 但在构造函数中设置 `Some("")`，满足数据库约束
   - 尝试改为非 Option 容易遇到编译错误，不推荐强行修改

示例：
```rust
// 数据库定义：description TEXT NOT NULL DEFAULT ''
// SQLx 推断为 Option<String>，因此结构体保持 Option
pub struct ProjectPo {
    pub id: String,
    pub name: String,  // NOT NULL，sqlx 推断非 Option → 直接 String
    pub description: Option<String>,  // NOT NULL DEFAULT ''，但 sqlx 推断 Option → 保持 Option
    // ...
}

impl ProjectPo {
    pub fn new(..., description: String) -> Self {
        Self {
            ...,
            description: Some(description),  // 构造函数自动包装为 Some，保证不会为 None
        }
    }
}
```

## 查询宏使用规范

### 类型标注规则

- ✅ **仅自定义枚举需要显式类型标注**：`status as "status: TaskStatus"`
- ✅ **普通类型/Option 类型不需要标注**：sqlx 从 PO 结构体自动推断
- ❌ **不要给普通类型添加多余标注**：会引发双重 Option 错误

### 占位符规则

- ✅ SQLite 使用 `?` 占位符，不要用 PostgreSQL `$1` 风格
- 错误示例：`$1` → 正确：`?`

### 绑定参数规则

- 参数个数必须和占位符 `?` 个数完全一致
- 末尾多余逗号会产生额外占位符，仔细检查计数：
  ```rust
  // ❌ 错误：末尾多了逗号，生成 16 个占位符实际只需要 15 个
  INSERT INTO projects (
      id,
      name,
      ...
      updated_at,  // ← 这里逗号多了
  ) VALUES (
      ?,
      ?,
      ...
      ?,  // ← 这里也多了
  );

  // ✅ 正确：最后一个字段不加逗号
  ```

## SQL 关键字转义

SQLite 有很多保留关键字，如果用作列名必须用**双引号**转义：

常见需要转义的关键字：
- `status`
- `role`
- `message_type`
- `assignee_type`

示例：
```sql
-- ✅ 正确
SELECT * FROM tasks WHERE "status" != 0

-- ❌ 错误：会报 "no such column: status" 错误
SELECT * FROM tasks WHERE status != 0
```

**注意：** sqlx 查询缓存会缓存原始 SQL，修改转义后必须重新生成缓存。

## 测试隔离最佳实践

使用 `#[sqlx::test]` 宏，每个测试自动创建独立的内存数据库 `sqlite::memory:`，自动运行迁移，测试结束自动销毁。

```rust
#[sqlx::test]
async fn test_insert_project(pool: SqlitePool) -> Result<(), AppError> {
    // 每个测试都有全新干净的数据库
    // ...
}
```

优势：
- 完全隔离，并行测试完全不会相互污染
- 彻底解决原 rusqlite 全局单例带来的并行测试数据污染问题

## 软删除查询约定

- 软删除设置 `status = 0`
- 所有 `find_by_id`、`list_by_*`、`count_*` 查询都需要添加 `AND "status" != 0` 过滤条件
- 例外：专门按状态查询的函数（如 `list_by_status`）不需要添加，由调用方处理

**错误案例：** 漏掉过滤会导致删除后仍然能查到数据，测试断言失败。

## 离线查询缓存

修改查询后需要重新生成缓存，步骤：

```bash
# 删除临时数据库，创建干净数据库用于生成缓存
rm -f /tmp/migration.db && sqlite3 /tmp/migration.db "VACUUM;"
export DATABASE_URL="sqlite:///tmp/migration.db"
export SQLX_OFFLINE=false
cargo sqlx migrate run
cargo sqlx prepare
```

生成后，`.sqlx` 目录纳入 git 版本控制，CI 使用离线模式编译不需要在线连接数据库。

**.sqlx 必须版本控制**，否则其他人拉取代码后离线编译会报错。

## 初始化顺序

按依赖顺序初始化：
```
main
  ↓
service::init()
  ↓
dao::init_all()   ← 先初始化 DAO
  ↓
dal::init_all()   ← 再初始化 DAL
  ↓
domain::init_all() ← 最后初始化 Domain
```

否则会出现 `Option::unwrap() on None` panic。

## 常见坑点排查

| 问题 | 根因 | 解决方案 |
|------|------|----------|
| 枚举解码失败 `invalid value "0" for enum MyEnum` | 错误配置了 `rename_all = "lowercase"`，sqlx 期望字符串 | 去掉 `rename_all`，添加 `#[repr(i32)]` + `#[sqlx(type_name = "INTEGER")]` + `From<i64>` |
| sqlx 编译报错 "no such table" | 查询修改后缓存过期 | 重新 `cargo sqlx prepare` 生成新缓存 |
| 测试删除后仍然能查到数据 | `find_by_id` 漏掉 `AND "status" != 0` 过滤软删除 | 添加过滤条件 |
| 测试 panic "storage not initialized" | 初始化顺序错误，上层提前获取 DAO | 按 `DAO → DAL → Domain` 顺序初始化 |
| 编译报错 "16 values for 15 columns" | INSERT 语句末尾多余逗号，导致占位符比实际列数多 | 删除末尾多余逗号，核对列数、占位符、参数三者一致 |
| 所有 TEXT 字段都必须是 `Option<String>` | 没有开启 `STRICT` 模式，sqlx 无法推断可空性 | 给所有表添加 `STRICT`，按实际 schema 修正类型 |
| 前端 WASM 编译失败，找不到 libsqlite3 | common 包枚举未使用条件编译 | 添加 `#[cfg(feature = "sqlx")]` 给所有 sqlx 相关代码，前端禁用特性 |
| "cannot infer type of query" | 自定义枚举缺少类型标注 | 添加 `field as "field: EnumType"` |
| 双重 Option 编译错误 | 给普通 Option 字段多余添加类型标注 | 删除多余标注，仅枚举需要标注 |

## 版本历史

| 日期 | 变更 | 作者 |
|------|------|------|
| 2026-04-15 | 初始文档，整理 rusqlite → sqlx 迁移总结的规范 | 王挺 |
