# 技能系统设计文档

## 设计目标

ai_orz 技能系统用于沉淀 Agent 沉淀出的可复用技能，支持：

1. **技能沉淀**：将 Agent 成功完成任务的经验沉淀为可复用技能
2. **分类管理**：按分类组织技能，支持关键词搜索
3. **状态管理**：支持待沉淀、可用、过期三种状态，支持软删除
4. **文件存储**：技能源码文件存储在本地数据目录，支持相对路径管理

## 数据库设计

### `skills` 表结构

```sql
CREATE TABLE IF NOT EXISTS skills (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    tags JSON NOT NULL DEFAULT '[]',
    category TEXT NOT NULL DEFAULT 'uncategorized',
    author TEXT NOT NULL,
    root_user_id TEXT NOT NULL,
    content_path TEXT NOT NULL,
    status INTEGER NOT NULL DEFAULT 2,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS idx_skills_status ON skills(status);
CREATE INDEX IF NOT EXISTS idx_skills_category ON skills(category);
CREATE INDEX IF NOT EXISTS idx_skills_author ON skills(author);
CREATE INDEX IF NOT EXISTS idx_skills_root_user_id ON skills(root_user_id);
```

**字段说明：**

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | TEXT | 技能唯一 ID（UUID v7）|
| `name` | TEXT | 技能名称 |
| `description` | TEXT | 技能描述 |
| `tags` | JSON | 标签数组 JSON |
| `category` | TEXT | 分类，默认 `uncategorized` |
| `author` | TEXT | 作者用户 ID |
| `root_user_id` | TEXT | 归属用户 ID |
| `content_path` | TEXT | 技能内容文件相对路径 |
| `status` | INTEGER | 状态：0=Expired（已过期/软删除，1=Available（可用），2=Pending（待沉淀）|
| `created_by` | TEXT | 创建人 |
| `modified_by` | TEXT | 修改人 |
| `created_at` | INTEGER | 创建时间戳（毫秒）|
| `updated_at` | INTEGER | 更新时间戳（毫秒）|

### 状态定义 (`SkillStatus`)

| 值 | 枚举名 | 含义 |
|----|--------|------|
| 0 | `Expired` | 已过期/软删除，默认不查询 |
| 1 | `Published` | 已发布，正式沉淀完成，已经发布到共享库，可以被检索和使用 |
| 2 | `Draft` | 草稿，Agent 自有技能，还在私有迭代中，未发布到共享库 |

## 分层架构

### 公共层 (`common`)

- `common::enums::skill::SkillStatus`：技能状态枚举，支持 sqlx 类型
- `common::config::AppConfig`：技能路径计算方法

### 模型层 (`src/models`)

- `src/models::skill::SkillPo`：技能持久化对象

### DAO 层 (`src/service/dao/skill`)

- `SkillDaoTrait`：DAO 接口定义
- `sqlite::SqliteSkillDao`：SQLite 实现
- `sqlite_test.rs`：单元测试（7 个独立测试）

## 路径存储设计

技能内容文件存储在数据目录下，按技能类型分目录存储：

- `{data_root}/agents/{agent_id}/skills/{skill_id}`：Agent 自有草稿技能（`Draft` 状态）
- `{data_root}/skills/{skill_id}`：已发布共享技能（`Published` 状态）

`content_path` 存储相对路径，例如：
- `agents/{agent_id}/skills/{skill_id}`（Draft）
- `skills/{skill_id}`（Published）

### 路径计算方法（在 `AppConfig`）

```rust
// 获取共享技能根目录
pub fn skills_root_dir(&self) -> PathBuf;

// 获取 Agent 自有技能根目录
pub fn agent_skills_root_dir(&self, agent_id: &str) -> PathBuf;

// 获取 Agent 自有技能目录
pub fn agent_skill_dir(&self, agent_id: &str, skill_id: &str) -> PathBuf;

// 获取 Agent 自有技能内容文件路径
pub fn agent_skill_content_path(&self, agent_id: &str, skill_id: &str) -> PathBuf;

// 获取 Agent 自有技能相对路径
pub fn agent_skill_relative_path(&self, agent_id: &str, skill_id: &str) -> String;

// 获取共享技能目录
pub fn shared_skill_dir(&self, skill_id: &str) -> PathBuf;

// 获取共享技能内容文件路径
pub fn shared_skill_content_path(&self, skill_id: &str) -> PathBuf;

// 获取共享技能相对路径
pub fn shared_skill_relative_path(&self, skill_id: &str) -> String;

// 根据技能状态获取正确的内容文件绝对路径
pub fn skill_content_path(&self, agent_id: &str, skill_id: &str, status: SkillStatus) -> PathBuf;

// 根据技能状态获取正确的相对路径（存储到数据库）
pub fn skill_relative_path(&self, agent_id: &str, skill_id: &str, status: SkillStatus) -> String;
```

默认技能内容文件命名为 `skill.md`，存储技能 markdown 内容。

## DAO 接口定义

```rust
#[async_trait::async_trait]
pub trait SkillDaoTrait: Send + Sync + std::fmt::Debug {
    /// 插入新技能
    async fn insert(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError>;

    /// 根据 ID 查询技能
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<SkillPo>, AppError>;

    /// 更新技能信息
    async fn update(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError>;

    /// 按状态查询列表（默认只查询非过期）
    async fn list_by_status(&self, ctx: RequestContext, status: SkillStatus) -> Result<Vec<SkillPo>, AppError>;

    /// 按分类查询列表
    async fn list_by_category(&self, ctx: RequestContext, category: &str) -> Result<Vec<SkillPo>, AppError>;

    /// 关键词搜索（名称和描述中包含关键词）
    async fn search(&self, ctx: RequestContext, keyword: &str) -> Result<Vec<SkillPo>, AppError>;

    /// 软删除技能（标记为 Expired）
    async fn delete_by_id(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;

    /// 按作者查询所有非过期技能
    async fn list_by_author(&self, ctx: RequestContext, author: &str) -> Result<Vec<SkillPo>, AppError>;
}
```

## `SkillPo` 构造

```rust
impl SkillPo {
    /// 创建新技能
    pub fn new(
        id: String,
        name: String,
        description: String,
        tags: Vec<String>,
        category: String,
        author: String,
        root_user_id: String,
        content_path: String,
    ) -> Self;

    /// 解析 tags JSON 为 Vec<String>
    pub fn parse_tags(&self) -> Vec<String>;
}
```

## SQLx 开发规范遵循

本模块开发遵循项目 `docs/sqlx_dev_guide.md` 中的规范：

1. ✅ 所有表开启 `STRICT` 模式
2. ✅ SQL 关键字（`status`）用双引号转义
3. ✅ 自定义枚举使用显式类型标注：`status AS "status: SkillStatus"`
4. ✅ TEXT 字段保持 `Option<String>` 适配 SQLx 默认推断
5. ✅ 所有查询静态编写，避免动态 SQL 拼接
6. ✅ `.sqlx` 目录纳入版本控制，支持离线编译

## 单元测试

| 测试用例 | 说明 |
|----------|------|
| `test_insert_and_find_by_id` | 测试插入和按 ID 查询 |
| `test_update` | 测试更新技能信息 |
| `test_list_by_status` | 测试按状态过滤 |
| `test_list_by_category` | 测试按分类过滤 |
| `test_search` | 测试关键词搜索 |
| `test_delete_by_id` | 测试软删除（标记过期）|
| `test_list_by_author` | 测试按作者查询 |

当前测试结果：**7/7 全部通过，零失败**

## 后续扩展

数据层已完成，待后续开发：

1. **DAL 层**：业务数据访问层封装
2. **Domain 层**：技能管理领域逻辑
3. **Handler 层**：HTTP API 接口
4. **Agent 集成**：Agent 自动沉淀技能流程

## 变更记录

| 日期 | 变更 |
|------|------|
| 2026-04-16 | 完成数据层开发，包括表结构、枚举、PO、DAO、单元测试 |
