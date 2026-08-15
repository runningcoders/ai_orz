---
kind: knowledge_card
name: SQLite + SQLx 0.8 存储工程规范：STRICT 表模式 + FTS5 全文检索 + 枚举 i32 类型安全 + .sqlx 离线构建 + sqlx::test 隔离
category: 工程规范
scope:
  - migrations/**/*.sql
  - src/models/**/*.rs
  - src/service/dao/**/sqlite.rs
  - tests/**/*.rs
  - .sqlx/**/*
  - src/pkg/storage/**/*.rs
source_files:
  - migrations/20260420000000_initial.sql#L10-L115
  - common/src/enums/task.rs#L8-L59
  - src/service/dao/project/sqlite.rs#L75-L137
  - src/service/dao/task/sqlite.rs#L82-L120
  - src/pkg/storage/fts5.rs
  - .sqlx/query-*.json
  - docs/design/sqlx_guide.md
  - docs/plan/Query接口分页与List接口简化重构.md
  - docs/wiki/zh/content/核心模块/存储系统/存储架构设计.md
  - docs/wiki/zh/content/基础设施/存储系统/存储系统.md
  - docs/wiki/zh/content/开发指南.md
---

## §1 概述与定位

本知识卡是 ai_orz 项目 SQLite + SQLx 0.8 开发规范的唯一权威参考，覆盖 STRICT 表模式、FTS5 全文检索、枚举 i32 类型安全映射、.sqlx 离线查询缓存、sqlx::test 测试隔离五大核心工程规范。触发读取场景：编写新增迁移文件、新增 DAO SQL 查询、新增枚举持久化、排查 sqlx 编译错误或类型推断失败、编写集成测试时。所有 DAO 查询宏参数、可空性处理、软删除约定、FTS5 关键词转义均以本文档为唯一标准。

## §2 关键文件表

| 文件 | 角色 | 核心入口/约束 |
|------|------|---------------|
| [migrations/*.sql](migrations) | 数据库迁移脚本 | 所有 CREATE TABLE 必须加 STRICT；TEXT NOT NULL DEFAULT ''；INTEGER NOT NULL DEFAULT 0；FTS5 虚拟表 + INSERT/UPDATE/DELETE 三触发器 + 存量回填 |
| [common/src/enums/task.rs](common/src/enums/task.rs) | 枚举类型安全样例 | `#[repr(i32)]` + `#[derive(sqlx::Type)]` + `#[sqlx(type_name = "INTEGER")]` + `From<i32>` + `From<i64>` 五件套；禁止 `rename_all = "lowercase"`；条件编译 `#[cfg(feature = "sqlx")]` |
| [dao/project/sqlite.rs](src/service/dao/project/sqlite.rs) | Project DAO SQLite 实现 | INSERT 末尾禁多余逗号；SELECT `"status" as "status: ProjectStatus"` 枚举显式标注；find_by_id 默认 `AND "status" != 0` 软删除过滤；QueryBuilder 动态构建 COUNT + LIST 复用 push_query_filters |
| [dao/task/sqlite.rs](src/service/dao/task/sqlite.rs) | Task DAO SQLite 实现 | INSERT 用双引号包裹关键字 `"status"` `"assignee_type"`；枚举字段 INSERT 时转 `as i32`；sqlx::query! 占位符 `?` 与参数严格对齐 |
| [sqlx_guide.md](docs/design/sqlx_guide.md) | SQLx 开发规范手册 | STRICT 开启原因 + 坑点；枚举常见错误排查表；FTS5 trigram 中文 3 字符限制；.sqlx 缓存必须纳入 git 版本控制 |
| [pkg/storage/fts5.rs](src/pkg/storage/fts5.rs) | FTS5 关键词转义工具 | `escape_fts5_keyword` 函数（短语匹配 + 内部双引号双写转义）；DAO 层统一从 storage 导入，禁止跨 DAO 互相引用 |
| [Query接口分页与List接口简化重构.md](docs/plan/Query接口分页与List接口简化重构.md) | 查询重构 Plan 快照 | COUNT 与 LIST 复用 push_query_filters 约定；QueryBuilder 动态 WHERE 条件构建模式 |

## §3 架构与约定

```
migrations/*.sql (建表 + FTS 触发器 + 存量回填)
          ↓ STRICT 模式 + sqlx 类型推断正确
    sqlx query 宏 (编译期类型检查)
          ↓ 枚举 as "field: EnumType" 标注
DAO 层 (sqlite.rs) → insert/find_by_id/list_by_*/query/count
          ↓ 软删除 "status" != 0 默认过滤
DAL/Domain 层 (业务逻辑)
          ↓ sqlx::test 独立内存库隔离
集成测试 (tests/**/*.rs)
```

**核心机制要点：**

1. **STRICT 表模式强制开启**：所有 `CREATE TABLE` 末尾必须加 `STRICT;`，SQLite 强制校验列类型；未开启时即使 NOT NULL 列 sqlx 也推断为 `Option<T>`，导致大面积类型不匹配。TEXT NOT NULL 默认 `''`，INTEGER NOT NULL 默认 `0`。
2. **枚举 i32 类型安全映射五件套**：`#[repr(i32)]`（整数表示）+ `#[derive(sqlx::Type)]`（sqlx 识别）+ `#[sqlx(type_name = "INTEGER")]`（数据库类型匹配）+ `From<i32>` + `From<i64>`（SQLite 默认返回 i64）。严禁加 `#[sqlx(rename_all = "lowercase")]`——这是字符串枚举专用，会导致解析失败。前端 WASM 侧用 `#[cfg(feature = "sqlx")]` 条件编译隔离。
3. **查询宏三要素一致**：列数、`?` 占位符数、绑定参数数三者必须完全一致。INSERT 末尾字段禁止多余逗号（会生成多余占位符报 16 values for 15 columns）。SQLite 关键字（status/role/message_type/assignee_type）作为列名必须用**双引号**转义：`"status"`，否则报 no such column。
4. **FTS5 全文检索工程链路**：每个实体对应 `{实体}_fts` 虚拟表（`USING fts5(... tokenize='trigram')`），外部内容模式 `content='源表名' content_rowid='rowid'`。三触发器自动同步：INSERT 直接插入；UPDATE 先 DELETE 旧再 INSERT 新；DELETE 用 `skills_fts(skills_fts, rowid, ...)` 语法。用户关键词必须走 `escape_fts5_keyword` 转义（双引号包裹短语匹配），禁止直接拼接。trigram 分词器 3 字符以下无法命中，属于已知限制。
5. **.sqlx 离线查询缓存与 CI 构建**：修改 SQL 查询后必须重新生成 `.sqlx/` 目录下的 query-*.json 缓存文件；缓存纳入 git 版本控制，CI 走离线编译不需要连接数据库。初始化顺序严格 DAO→DAL→Domain，否则 Option::unwrap() on None panic。sqlx::test 宏每个测试自动创建独立 sqlite::memory: 内存库、自动运行迁移、测试结束自动销毁，完全隔离无污染。

## §4 硬约束与红线

1. **所有建表必须 STRICT**：新增迁移文件 `CREATE TABLE` 遗漏 STRICT 为一级红线，会导致 sqlx 可空性全盘推断错误。
2. **枚举五件套缺一不可**：新增持久化枚举缺任何一件（repr/Type/type_name/From<i32>/From<i64>）直接编译失败或运行时枚举解码失败。
3. **禁止 rename_all = "lowercase" 给整数枚举**：给 i32 枚举加字符串 rename_all 会导致 sqlx 期望解析字符串而非整数，触发 "invalid value 0 for enum" 错误。
4. **SQL 关键字必须双引号转义**：status/role/assignee_type/message_type 作为列名遗漏双引号会报 "no such column"，改完必须重新生成 .sqlx 缓存。
5. **软删除默认 AND "status" != 0**：所有 find_by_id/list_by_*/count_* 查询遗漏软删除过滤属于数据正确性红线，测试会断言删除后查不到。
6. **FTS5 禁止 LIKE 关键词搜索**：全文搜索必须走 FTS5 MATCH + BM25，用 LIKE 属于性能红线。
7. **FTS5 关键词必须转义**：用户输入未经 `escape_fts5_keyword` 转义直接拼接 MATCH 属于 SQL 语法安全红线，特殊字符触发 FTS5 syntax error。
8. **.sqlx 必须版本控制**：未纳入 git 或 CI 时别人拉代码后离线编译报错，属于工程发布红线。
9. **插入参数三要素一致**：列数/占位符/绑定参数三者不一致会编译失败，属于 SQL 编写基本功红线。
10. **consumer::init 禁写 DB**：service::init() 只做同步单例注册不做 DB IO，DB 幂等默认数据统一走 init_base_data().await 第二阶段。
