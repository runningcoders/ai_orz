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
| `status` | INTEGER | 状态：0=Expired（已过期/软删除，1=Published（已发布），2=Draft（草稿）|
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

- `SkillDao`：DAO 接口定义（基础数据，不含向量）
- `SkillVectorDao`：向量索引接口定义
- `sqlite::SqliteSkillDao`：SQLite 实现
- `sqlite_test.rs`：单元测试
- `vector::SqliteVssSkillVectorDao`：向量索引 SQLite VSS 实现
- `vector_test.rs`：向量索引测试

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

### `SkillDao` 基础数据接口（不含向量）

```rust
#[async_trait]
pub trait SkillDao: Send + Sync {
    // ========== 基础 CRUD ==========

    /// Insert a new skill
    async fn insert(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError>;

    /// Update an existing skill
    async fn update(&self, ctx: RequestContext, skill: &SkillPo) -> Result<(), AppError>;

    /// Soft delete (mark as expired)
    async fn delete_by_id(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;

    /// Find skill by id (excludes Expired status)
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<SkillPo>, AppError>;

    /// 通用组合查询
    async fn query(&self, ctx: RequestContext, query: SkillQuery) -> Result<Vec<SkillPo>, AppError>;

    /// List skills by status
    async fn list_by_status(&self, ctx: RequestContext, status: SkillStatus) -> Result<Vec<SkillPo>, AppError>;

    /// List skills by category
    async fn list_by_category(&self, ctx: RequestContext, category: &str) -> Result<Vec<SkillPo>, AppError>;

    /// List skills by author
    async fn list_by_author(&self, ctx: RequestContext, author_id: &str) -> Result<Vec<SkillPo>, AppError>;

    // ========== 业务操作 ==========

    /// Install a published shared skill to an agent as a private draft copy
    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill: &SkillPo,
        target_agent_id: &str,
    ) -> Result<SkillPo, AppError>;

    /// 统一搜索入口（关键词 + 业务过滤，向量搜索由 SkillVectorDao 单独处理）
    async fn search(&self, ctx: RequestContext, search: SkillSearch) -> Result<Vec<SkillPo>, AppError>;

    // ========== 文件操作 ==========

    /// 读取 skill.md 主文件内容
    fn read_main_content(&self, skill: &SkillPo) -> Result<String, AppError>;

    /// 写入 skill.md 主文件内容
    fn write_main_content(&self, skill: &SkillPo, content: &str) -> Result<(), AppError>;

    /// 列出技能目录下的所有文件（小文件自动预读内容）
    fn list_files(&self, skill: &SkillPo) -> Result<Vec<SkillFile>, AppError>;

    /// 读取指定文件名的内容
    fn read_file(&self, skill: &SkillPo, filename: &str) -> Result<String, AppError>;

    /// 写入指定文件名的内容
    fn write_file(&self, skill: &SkillPo, filename: &str, content: &str) -> Result<(), AppError>;

    /// 删除整个技能目录（卸载/删除时调用）
    fn delete_skill_dir(&self, skill: &SkillPo) -> Result<(), AppError>;
}
```

### `SkillQuery` 通用查询参数

```rust
#[derive(Debug, Clone, Default)]
pub struct SkillQuery {
    pub ids: Option<Vec<String>>,           // 按 ID 批量查询
    pub status: Option<SkillStatus>,
    pub exclude_status: Option<SkillStatus>,
    pub category: Option<String>,
    pub author_id: Option<String>,
    pub keyword: Option<String>,
    pub limit: Option<usize>,
}
```

### `SkillSearch` 统一搜索入参

```rust
#[derive(Debug, Clone, Default)]
pub struct SkillSearch {
    /// 关键词搜索查询（用于传统 LIKE 匹配）
    pub keyword: Option<String>,
    /// 查询向量（用于向量语义搜索，DAL 层填充）
    pub query_vector: Option<Vec<f32>>,
    /// 返回 Top K 结果（向量搜索专用）
    pub top_k: Option<i32>,
    /// 业务过滤条件（直接复用 SkillQuery）
    pub filters: SkillQuery,
}
```

### `SkillVectorDao` 向量索引接口

```rust
#[async_trait]
pub trait SkillVectorDao: Send + Sync {
    /// 插入或更新技能的向量索引
    async fn upsert_vector(
        &self,
        ctx: RequestContext,
        skill_id: &str,
        vector_params: &VectorIndexParams,
    ) -> Result<(), AppError>;

    /// 纯向量语义搜索，返回完整的向量行数据 + 相似度距离
    async fn search_vector(
        &self,
        ctx: RequestContext,
        query_vector: &[f32],
        top_k: i32,
    ) -> Result<Vec<VectorSearchHit>, AppError>;

    /// 获取指定技能的完整向量行数据（包含元信息）
    async fn get_vector_row(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<VectorRow>, AppError>;
}
```

## DAL 接口边界

DAL 层对上层优先暴露 `Skill` 业务实体，`SkillPo` 主要限制在 DAO 持久化边界内使用。对于安装这类会创建新技能副本的操作，DAO 负责原子复制文件并写入数据库，DAL 负责把 DAO 返回的 `SkillPo` 组装为完整 `Skill`：

```rust
#[async_trait]
pub trait SkillDal: Send + Sync {
    async fn get_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<Skill>, AppError>;
    async fn query(&self, ctx: RequestContext, query: SkillQuery) -> Result<Vec<Skill>, AppError>;
    async fn list_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>, AppError>;
    async fn update(&self, ctx: RequestContext, skill: &Skill) -> Result<(), AppError>;

    /// 将已发布技能安装到 Agent，返回安装后新创建的完整 Skill 业务实体
    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<Skill, AppError>;
}
```

> `get_po_by_id` 仅作为 DAL 内部/底层优化能力保留，不作为 Domain/Handler 的公共依赖方向。

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

## `Skill` 业务实体

```rust
impl Skill {
    /// 从 SkillPo 构造 Skill 业务实体
    pub fn from_po(po: SkillPo) -> Self;

    /// 获取 ID
    pub fn id(&self) -> &str;

    /// 获取名称
    pub fn name(&self) -> &str;

    /// 获取描述
    pub fn description(&self) -> &str;

    /// 获取分类
    pub fn category(&self) -> &str;

    /// 获取状态
    pub fn status(&self) -> SkillStatus;

    /// 获取作者
    pub fn author(&self) -> &str;

    /// 获取内容路径
    pub fn content_path(&self) -> &str;

    /// 获取主内容（如果存在）
    pub fn main_content(&self) -> Option<&str>;

    /// 获取指定文件的内容（如果存在）
    pub fn file_content(&self, filename: &str) -> Option<&str>;

    /// 获取所有文件名列表
    pub fn file_names(&self) -> Vec<&str>;
}
```

## DAO 接口行为说明

### `find_by_id` 查询语义

`find_by_id` 方法默认**只返回非过期（非 Expired）状态的技能记录**：

```rust
async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<SkillPo>, AppError>;
```

- 查询条件：`id = ? AND status != 0` (0 = Expired)
- 如果技能已被软删除（status = Expired），返回 `None`
- 这与 Agent 模块的查询行为保持一致

### 软删除机制

`delete_by_id` 执行**软删除**：将技能状态设置为 `SkillStatus::Expired`，而不是物理删除：

```rust
async fn delete_by_id(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;
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

### DAO 层测试 (`src/service/dao/skill/sqlite_test.rs`)

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

### DAL 层测试 (`src/service/dal/skill_test.rs`)

| 测试用例 | 说明 |
|----------|------|
| `test_create_and_get_by_id` | 测试创建和获取完整技能 |
| `test_get_skill_po_only` | 测试只获取 PO（不加载文件） |
| `test_update_skill_basic_info` | 测试更新技能基本信息（名称、描述、分类、标签） |
| `test_update_skill_status` | 测试更新技能状态（Draft → Published） |
| `test_update_nonexistent_skill` | 测试更新不存在的技能（INSERT OR REPLACE 行为） |
| `test_delete_skill` | 测试软删除技能 |
| `test_list_by_status` | 测试按状态查询 |
| `test_list_by_category` | 测试按分类查询 |
| `test_list_by_author` | 测试按作者查询 |
| `test_query_skills` | 测试通用查询 |
| `test_search_skill` | 测试技能搜索（关键词匹配） |
| `test_install_to_agent` | 测试安装技能到 Agent |
| `test_file_operations` | 测试技能文件读写操作 |

当前测试结果：**13/13 全部通过，零失败**

### Domain 层测试 (`src/service/domain/hr/skill_test.rs`)

| 测试用例 | 说明 |
|----------|------|
| `test_create_and_get_by_id` | 测试创建和获取技能 |
| `test_update_skill` | 测试更新技能 |
| `test_delete_skill` | 测试删除技能 |
| `test_list_by_status` | 测试按状态查询 |
| `test_list_by_category` | 测试按分类查询 |
| `test_list_by_author` | 测试按作者查询 |
| `test_query_skills` | 测试通用查询 |

当前测试结果：**11/11 全部通过，零失败**（包含 agent 和 skill 测试）

## 分层架构（完整）

### 公共层 (`common`)

- `common::enums::skill::SkillStatus`：技能状态枚举，支持 sqlx 类型
- `common::config::AppConfig`：技能路径计算方法

### 模型层 (`src/models`)

- `src/models::skill::SkillPo`：技能持久化对象
- `src/models::skill::Skill`：技能完整业务实体（PO + 文件 + 搜索元信息）
- `src/models::skill::SkillFile`：技能附属文件信息

### DAO 层 (`src/service/dao/skill`)

- `SkillDao`：DAO 接口定义（基础数据，不含向量）
- `SkillVectorDao`：向量索引接口定义
- `sqlite::SqliteSkillDao`：SQLite 实现
- `sqlite_test.rs`：单元测试
- `vector::SqliteVssSkillVectorDao`：向量索引 SQLite VSS 实现
- `vector_test.rs`：向量索引测试

### DAL 层 (`src/service/dal/skill`)

- `SkillDal`：DAL 接口定义
- `SkillDalImpl`：DAL 实现
- `skill_test.rs`：DAL 层测试

### Domain 层 (`src/service/domain/hr`)

- `HrDomain`：HR Domain 总入口，聚合 AgentManage + SkillManage
- `SkillManage`：技能管理 trait 定义
- `skill.rs`：SkillManage 实现
- `skill_test.rs`：Domain 层测试

## Domain 层设计

### 整体架构

将 Skill 管理作为 HR Domain 的一个子模块，与 Agent 管理平级：

```
HR Domain
├── AgentManage - Agent 管理
└── SkillManage - 技能管理
```

### 目录结构

```
src/service/domain/hr/
├── mod.rs          # HR Domain 总入口
├── agent.rs        # Agent 管理
├── skill.rs        # Skill 管理
├── agent_test.rs   # Agent 测试
└── skill_test.rs   # Skill 测试
```

### 核心 Trait 设计

#### 1. 总 HR Domain Trait

```rust
pub trait HrDomain: Send + Sync {
    fn agent_manage(&self) -> &dyn AgentManage;
    fn skill_manage(&self) -> &dyn SkillManage;
}
```

#### 2. SkillManage Trait

**设计原则：Domain 层是抽象业务层，少即是多**

```rust
/// 技能更新复合参数
#[derive(Debug, Clone)]
pub struct UpdateSkillParams<'a> {
    /// 技能实体（包含要更新的元数据）
    pub skill: &'a Skill,
    /// 文件写入操作列表（文件名 -> 内容）
    pub file_writes: Vec<(&'a str, &'a str)>,
    /// 文件删除操作列表（文件名）
    pub file_deletes: Vec<&'a str>,
}

#[async_trait::async_trait]
pub trait SkillManage: Send + Sync {
    // A. 技能基础管理（CRUD）
    async fn create_skill(&self, ctx: RequestContext, skill: &Skill) -> Result<(), AppError>;
    async fn get_skill(&self, ctx: RequestContext, id: &str) -> Result<Option<Skill>, AppError>;
    async fn update_skill(&self, ctx: RequestContext, params: UpdateSkillParams<'_>) -> Result<(), AppError>;
    async fn delete_skill(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;

    // B. 技能查询与搜索
    async fn query_skills(&self, ctx: RequestContext, query: SkillQuery) -> Result<Vec<Skill>, AppError>;
    async fn list_by_status(&self, ctx: RequestContext, status: SkillStatus) -> Result<Vec<Skill>, AppError>;
    async fn list_by_category(&self, ctx: RequestContext, category: &str) -> Result<Vec<Skill>, AppError>;
    async fn list_by_author(&self, ctx: RequestContext, author_id: &str) -> Result<Vec<Skill>, AppError>;
    async fn list_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>, AppError>;
    async fn search_skills(&self, ctx: RequestContext, search: SkillSearch) -> Result<Vec<Skill>, AppError>;

    // C. Agent 技能安装
    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<Skill, AppError>;
}
```

> Domain 和 DAL 层公共接口优先暴露 `Skill` 业务实体，不暴露 `SkillPo` / `get_skill_po` 这类持久化对象接口；DAL 内部仅保留轻量 PO 查询作为存储优化，默认不向上层传播 PO。

### 实现结构

```
HrDomainImpl
├── agent_dal: Arc<dyn AgentDal>
├── tool_dal: Arc<dyn ToolDal>
└── skill_dal: Arc<dyn SkillDal>
```

`HrDomainImpl` 同时实现：
- `AgentManage` trait
- `SkillManage` trait

## 后续扩展

数据层已完成，待后续开发：

1. ✅ **DAL 层**：业务数据访问层封装（已完成）
2. ✅ **Domain 层**：技能管理领域逻辑（已完成）
3. ⏳ **Handler 层**：HTTP API 接口
4. ⏳ **Agent 集成**：Agent 自动沉淀技能流程

## 变更记录

| 日期 | 变更 |
|------|------|
| 2026-04-16 | 完成数据层开发，包括表结构、枚举、PO、DAO、单元测试 |
| 2026-05-13 | 更新文档，添加 DAL 层和 Domain 层设计说明 |
| 2026-05-14 | 添加完整的 hr skill 测试，修复 find_by_id 查询语义，排除过期技能 |
| 2026-05-14 | 文档更新：修正状态枚举过期名称（Available/Pending → Published/Draft），更新 DAO 接口定义（SkillDaoTrait → SkillDao + SkillVectorDao），添加 SkillQuery/SkillSearch/SkillVectorDao 接口说明，更新分层架构 |
