# SQLx 0.8 + SQLite 开发规范与最佳实践

> 🎯 **本文档定位**：SQLx 0.8 + SQLite 开发规范与最佳实践手册（STRICT 模式 / 枚举映射 / FTS5 全文搜索 / 测试隔离 / 软删除约定 / 离线查询缓存）；项目内 SQL 编写与 sqlx::query 宏使用的唯一规范权威参考。
> 状态：v1.2（2026-04-15 初始发布，2026-08-15 整理）
> 查阅场景：写 SQL / 新增 DAO 方法 / 迁移表结构 / 编写 sqlx 测试 / 排查 sqlx 编译报错时打开；具体 SQL 语句与宏参数直接看 DAO 实现代码。
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目整体分层架构与开发规范 §SQLite + SQLx 规范
> - [pagination_and_count_convention.md](./pagination_and_count_convention.md) — 查询分页与通用 count 规范（query 过滤条件复用的上游约束）

本文档记录 ai_orz 项目从 rusqlite 迁移到 sqlx 0.8 过程中总结出的开发规范和避坑指南。

## 目录

- [数据库 Schema 设计](#数据库-schema-设计)
- [枚举类型映射](#枚举类型映射)
- [可空性处理规则](#可空性处理规则)
- [查询宏使用规范](#查询宏使用规范)
- [SQL 关键字转义](#sql-关键字转义)
- [测试隔离最佳实践](#测试隔离最佳实践)
- [软删除查询约定](#软删除查询约定)
- [FTS5 全文搜索规范](#fts5-全文搜索规范)
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
> 对应迁移文件参考：[migrations/ 目录](file:///Users/aman/Technology/rust/ai_orz/migrations/)

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

> 相关实现细节见：[dao/*/sqlite.rs 实现](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/)

**常见错误：**
- ❌ 不要添加 `#[sqlx(rename_all = "lowercase")]`：这是给字符串枚举用的，会导致 sqlx 期望解析字符串而不是整数
- ❌ 忘记 `From<i64>`：sqlx 从 SQLite 获取的整数默认是 i64，缺少转换会编译失败

### 条件编译（给 common 包前后端共享枚举）

如果枚举在 common 包被前后端共享，需要给 sqlx 相关代码添加条件编译：

> 相关实现细节见：[dao/*/sqlite.rs 实现](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/)

Cargo.toml 中配置：
> 相关实现细节见：[DAO 层 sqlx 用法](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/)

这样前端 WASM 编译时可以禁用 sqlx 特性，避免编译 libsqlite3-sys 失败。

## 可空性处理规则

1. **严格按照 schema 定义**：只在真正允许 NULL 的字段使用 `Option<T>`，NOT NULL 字段直接用非 Option 类型
2. **SQLite TEXT 列特例**：即使定义为 `NOT NULL DEFAULT ''`，SQLx 默认也会推断为 `Option<String>`，实际开发中推荐：
   - 如果字段确实允许 NULL：保留 `Option<String>`
   - 如果字段是 `NOT NULL DEFAULT ''`：保持 `Option<String>` 但在构造函数中设置 `Some("")`，满足数据库约束
   - 尝试改为非 Option 容易遇到编译错误，不推荐强行修改

示例：
> 相关实现细节见：[DAO 层 sqlx 用法](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/)

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
> 对应迁移文件参考：[migrations/ 目录](file:///Users/aman/Technology/rust/ai_orz/migrations/)

**注意：** sqlx 查询缓存会缓存原始 SQL，修改转义后必须重新生成缓存。

## 测试隔离最佳实践

使用 `#[sqlx::test]` 宏，每个测试自动创建独立的内存数据库 `sqlite::memory:`，自动运行迁移，测试结束自动销毁。

> 相关实现细节见：[dao/*/sqlite.rs 实现](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/)

优势：
- 完全隔离，并行测试完全不会相互污染
- 彻底解决原 rusqlite 全局单例带来的并行测试数据污染问题

## 软删除查询约定

- 软删除设置 `status = 0`
- 所有 `find_by_id`、`list_by_*`、`count_*` 查询都需要添加 `AND "status" != 0` 过滤条件
- 例外：专门按状态查询的函数（如 `list_by_status`）不需要添加，由调用方处理

**错误案例：** 漏掉过滤会导致删除后仍然能查到数据，测试断言失败。

## FTS5 全文搜索规范

### 1. 概述

项目使用 SQLite 内置 FTS5 扩展实现全文搜索，搭配 `trigram` 分词器支持中文搜索（3 字符以上）。所有实体的关键词搜索统一走 FTS5 MATCH + BM25 相关性排序，**禁止使用 LIKE 进行关键词搜索**。

### 2. 表结构设计

每个需要全文搜索的实体对应一张 FTS5 虚拟表，命名约定：`{实体名}_fts`。

```sql
-- ✅ 正确：使用 trigram 分词器，content 指定源表
CREATE VIRTUAL TABLE IF NOT EXISTS skills_fts USING fts5(
    name,
    description,
    tags,
    content='skills',
    content_rowid='rowid',
    tokenize='trigram'
);
```
> 对应迁移文件参考：[migrations/ 目录](file:///Users/aman/Technology/rust/ai_orz/migrations/)

**设计要点：**
- 使用 `tokenize='trigram'` 支持中文搜索（需 SQLite 3.34+）
- `content='源表名'` + `content_rowid='rowid'` 建立外部内容模式，数据不重复存储
- 索引字段选择：name、description、tags 等用户可见的文本字段

### 3. 触发器自动同步

使用 INSERT/UPDATE/DELETE 触发器自动维护 FTS 索引，应用层无感知。

```sql
-- INSERT 触发器
CREATE TRIGGER IF NOT EXISTS skills_fts_insert AFTER INSERT ON skills
BEGIN
    INSERT INTO skills_fts(rowid, name, description, tags)
    VALUES (new.rowid, new.name, new.description, new.tags);
END;

-- UPDATE 触发器（先 DELETE 再 INSERT）
CREATE TRIGGER IF NOT EXISTS skills_fts_update AFTER UPDATE ON skills
BEGIN
    INSERT INTO skills_fts(skills_fts, rowid, name, description, tags)
    VALUES ('delete', old.rowid, old.name, old.description, old.tags);
    INSERT INTO skills_fts(rowid, name, description, tags)
    VALUES (new.rowid, new.name, new.description, new.tags);
END;

-- DELETE 触发器
CREATE TRIGGER IF NOT EXISTS skills_fts_delete AFTER DELETE ON skills
BEGIN
    INSERT INTO skills_fts(skills_fts, rowid, name, description, tags)
    VALUES ('delete', old.rowid, old.name, old.description, old.tags);
END;
```
> 对应迁移文件参考：[migrations/ 目录](file:///Users/aman/Technology/rust/ai_orz/migrations/)

### 4. 存量数据回填

迁移中必须包含存量数据回填：

```sql
-- 回填存量数据到 FTS 索引
INSERT INTO skills_fts(rowid, name, description, tags)
SELECT rowid, name, description, tags FROM skills;
```
> 对应迁移文件参考：[migrations/ 目录](file:///Users/aman/Technology/rust/ai_orz/migrations/)

### 5. 关键词转义（重要！）

**禁止直接拼接用户输入到 MATCH 查询中**，必须使用 `escape_fts5_keyword` 函数转义。

该函数定义在 `src/pkg/storage/fts5.rs`，属于存储层公共工具，所有 DAO 统一从这里导入复用。

> 相关实现细节见：[DAO 层 sqlx 用法](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/)

**转义策略：** 将用户输入用双引号包裹作为短语匹配（phrase query），内部双引号双写转义。这样 FTS5 不会把空格解释为 AND 操作符，搜索精度更高。

```
"hello world"  →  短语匹配，按顺序匹配 "hello world"
hello AND world  →  同时包含两个词，顺序无关（不使用）
```

**错误案例：**
- ❌ 直接拼接：`format!("{keyword}")` —— 特殊字符会引发 FTS5 语法错误
- ❌ 从其他 DAO 导入：`use crate::service::dao::memory::sqlite::escape_fts5_keyword` —— 造成 DAO 间依赖

### 6. MATCH 查询写法

> 相关实现细节见：[DAO 层 sqlx 用法](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/)

**注意事项：**
- MATCH 左侧必须使用**完整表名**（非别名），否则 SQLite 会将别名解释为列名
- BM25 rank 越小越相关，默认升序排列
- 使用 `rank as fts_rank` 返回相关性评分，供上层混合排序使用

### 7. 中文搜索限制

trigram 分词器基于 3 字符滑动窗口，**少于 3 个字符的关键词无法命中**。这是已知限制，不需要特殊处理。

测试中使用 4+ 字符关键词验证 FTS5 搜索功能。

### 8. 与向量搜索的关系

FTS5 关键词搜索和向量语义搜索是互补关系，在 DAL 层组合为**混合搜索**，具体架构见 [vector_search_architecture.md](./vector_search_architecture.md)。

## 离线查询缓存

修改查询后需要重新生成缓存，步骤：

> 相关实现参考：[migrations/ 目录 + AGENTS.md 测试规范](file:///Users/aman/Technology/rust/ai_orz/AGENTS.md)

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
| sqlx 编译报错 "no such table" | 查询修改后缓存过期 | 重新生成 sqlx 离线查询缓存 |
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

---

## 五、扩展模式

### 5.1 新增数据库表或迁移文件
所有新表须遵循 STRICT 模式 + .sqlx 缓存同步。新增迁移时：
1. 在 `migrations/` 目录下新增 `*_up.sql` / `*_down.sql` 对，表级约束保持 STRICT + 关键字段索引，参考现有：[migrations 目录](file:///Users/aman/Technology/rust/ai_orz/migrations)
2. DAO 层同步新增对应 `dao/xxx/mod.rs` trait 定义与 `sqlite.rs` 实现，遵循「COUNT 与 LIST 复用 push_query_filters」约定，参考：[dao 目录](file:///Users/aman/Technology/rust/ai_orz/src/service/dao)
3. 更新 `.sqlx/` 离线查询缓存保持 CI 编译通过，保持查询宏参数三要素（列数/占位符/参数顺序）一致

### 5.2 新增枚举类型持久化
新增枚举需要落库时遵循「枚举类型安全」约定：
1. 枚举统一定义在 `common/src/enums/`，配置 `#[repr(i32)]` + `#[derive(sqlx::Type)]` + `impl From<i64>`，参考：[task_status.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/enums/task_status.rs)
2. SQL 查询中枚举字段统一 `as "field: EnumType"` 标注；软删除 `status = 0` 语义保持一致，参考现有 DAO 查询：[dao/agent/sqlite.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/agent/sqlite.rs)
3. 前端 wasm32 侧通过条件编译 `#[cfg(feature = "sqlx")]` 隔离 sqlx 派生宏，参考：[common/src/enums/mod.rs](file:///Users/aman/Technology/rust/ai_orz/common/src/enums/mod.rs)
