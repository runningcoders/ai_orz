# 技能系统设计文档

> 🎯 **本文档定位**：技能沉淀、分类管理、状态流转与源码文件存储的基础分层设计
> 状态：v1.0（2026-08-15 整理）
> 查阅场景：新增技能 CRUD 接口、排查技能文件路径解析、理解技能状态机与软删除语义时打开；字段级 PO/DAO 定义直接看代码
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构
> - [skill_system_enhancement_design.md](./skill_system_enhancement_design.md) — 技能系统增强：tag 过滤、技能包、唤醒注入机制
> - 【② Plan 落地（真实定稿 1 张）】
>   - [预置基础技能导入重构.md](../plan/预置基础技能导入重构.md) — 5 套 TEMPLATE_* 目录结构 + default.json 编译期嵌入快照
> - 【③ Wiki 长文（人类百科 6 篇）】
>   - [技能系统.md](docs/wiki/zh/content/功能模块/技能系统.md) — 沉淀→分类→状态→安装四步用户故事总览
>   - [技能包管理.md](docs/wiki/zh/content/功能模块/AI%20Agent%20管理/技能包管理.md) — Tag 筛选 + 批量安装 + 历史记录面板
>   - [技能与工具绑定.md](docs/wiki/zh/content/项目概述/核心功能特性/Agent%20全生命周期管理/技能与工具绑定.md) — 入职流程：创建 Agent→装默认技能→配凭证→绑工具→上线
>   - [Agent 和技能模型.md](docs/wiki/zh/content/数据模型/Agent%20和技能模型/Agent%20和技能模型.md) — SkillPo 字段 + Vectorizable skill 向量化实现
>   - [系统初始化.md](docs/wiki/zh/content/功能模块/用户与组织管理/系统初始化.md) — init_base_data 内 import_default_skills_if_empty() 首次启动自动导入
>   - [技能管理系统.md](docs/wiki/zh/content/前端应用/页面模块/HR%20管理页面/技能管理系统.md) — 前端编辑器：左侧文件树 + 中间 Markdown + 右侧 Prompt 预览
> - 【④ RAG 原子知识卡（Batch6 新增 1 张 + 关联平行卡 1 张）】
>   - [技能系统 Seed 预置导入与 Agent 入职绑定：5 套 TEMPLATE_* 编译期嵌入 + install_skill_pack 幂等 Tag 分发 + Prompt Token 熔断](docs/wiki/knowledge/zh/技能系统%20Seed%20预置导入与%20Agent%20入职绑定：5%20套%20TEMPLATE_*%20编译期嵌入%20+%20install_skill_pack%20幂等%20Tag%20分发%20+%20Prompt%20Token%20熔断/技能系统%20Seed%20预置导入与%20Agent%20入职绑定：5%20套%20TEMPLATE_*%20编译期嵌入%20+%20install_skill_pack%20幂等%20Tag%20分发%20+%20Prompt%20Token%20熔断.md) — 5 套模板（工具/记忆/项目/沟通/技能自管理）+ Draft 副本 id 重命名 + 6 条回归红线（含 content_path 防路径穿越）

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

### 管理面 API

Batch 2.3 已补齐 HR Skill 管理面 API。共享 DTO 位于 `common/src/api/skill.rs`，Handler 位于 `src/handlers/hr/skill/`，每个用户 action 单独文件，且只调用 HR Domain，不直接调用 DAL/DAO。

```http
POST   /api/v1/hr/skills
GET    /api/v1/hr/skills
GET    /api/v1/hr/skills/search
GET    /api/v1/hr/skills/{id}
PUT    /api/v1/hr/skills/{id}
DELETE /api/v1/hr/skills/{id}
GET    /api/v1/hr/agents/{agent_id}/skills
POST   /api/v1/hr/agents/{agent_id}/skills/{skill_id}
GET    /api/v1/hr/skills/{skill_id}/files/{*filename}
PUT    /api/v1/hr/skills/{skill_id}/files/{*filename}
GET    /api/v1/hr/skills/{skill_id}/files
```

约束：
- 列表与搜索返回 `SkillListItem` 摘要，不返回主内容大字段；
- 详情、创建、更新、安装返回 `SkillDetail`，包含主内容 `content` 和文件摘要；
- 创建/更新仅支持元数据与主文件 `skill.md` 内容写入；附件级读写、文件删除等复杂副作用后续等 Domain/DAL 语义稳定后再补；
- 安装到 Agent 复用 `SkillManage::install_to_agent`，创建 Agent 私有 Skill 副本并返回完整详情。

### Batch 2.5：Skill 文件引用导入

Skill 附加文件导入基于 Finance Attachment 通用上传能力，不让 Skill API 直接接收 multipart 文件流。用户先上传 Attachment，再在 Skill 更新请求中引用 `attachment_id + target_path`。

```json
{
  "files": [
    {
      "attachment_id": "att_xxx",
      "target_path": "references/demo.md"
    }
  ]
}
```

#### 架构决策：Handler 层编排 Finance + HR

采用方案 B，避免 HR Skill Domain 直接依赖 Finance Domain：

```text
PUT /api/v1/hr/skills/{id}
    ↓
Skill Handler
    ├─ Finance Domain get_attachment(include_file_content = true)
    │   └─ 校验 Attachment 归属当前 root_user_id，并装配文件读取结果
    ↓
    HR Skill Domain update/import_files
        └─ 校验 target_path，并写入 Skill 内容目录
```

职责边界：
- Skill Handler：允许跨 Domain 编排，将 `attachment_id` 转换为 HR Domain 可接受的导入文件；不访问 DAO/DAL，不直接读写文件系统；
- Finance Domain：负责用户资产语义、Attachment 归属校验、文件读取结果装配；
- HR Skill Domain：不接收 `attachment_id`，只接收已经读取好的 `SkillFileImport`，负责路径安全校验和写入 Skill 内容目录；
- Skill DAO/DAL：只负责 Skill 元数据、主内容、附加文件的基础文件读写，不感知 Attachment。

#### DTO 设计

`common/src/api/skill.rs` 扩展：

```rust
pub struct SkillFileInput {
    pub attachment_id: String,
    pub target_path: String,
}

pub struct UpdateSkillRequest {
    // existing fields...
    pub files: Option<Vec<SkillFileInput>>,
}
```

兼容性约定：
- `files = None`：保持现有更新行为；
- `files = Some(vec![])`：不导入附加文件；
- `files = Some([...])`：由 Handler 编排 Attachment 读取并交给 HR Domain 导入。

#### Domain 入参模型

HR Domain 不暴露 Attachment 概念，接收：

```rust
pub struct SkillFileImport {
    pub target_path: String,
    pub bytes: Vec<u8>,
}
```

Finance Domain 的 Attachment get 能力支持按需装配文件内容：

```rust
pub struct AttachmentGetOptions {
    pub include_file_content: bool,
}
```

`include_file_content = false` 用于普通 Attachment 管理面查询；`true` 用于 Skill Handler 等内部编排场景。

#### 路径安全规则

HR Skill Domain 统一校验 `target_path`：

- 只能是相对路径，拒绝绝对路径；
- 拒绝空路径、`.` / `..` 路径片段；
- 拒绝尾随 `/` 的目录目标；
- 拒绝反斜杠路径分隔符，避免跨平台路径语义差异；
- 拒绝直接覆盖主内容文件 `skill.md`，大小写变体（如 `Skill.md` / `SKILL.md`）也会被拒绝。

#### 验收测试

- DTO：旧请求不带 `files` 仍可反序列化；带 `files` 可正常序列化/反序列化；
- Finance：`get_attachment(include_file_content=false)` 不读取 bytes；`true` 返回 `AttachmentReadResult`；跨 `root_user_id` 不可读取；
- HR Skill：正常导入 `references/demo.md`；拒绝 `../evil.md`、`/tmp/evil.md`、`./evil.md`、尾随 `/` 的目录目标、反斜杠路径、空路径、直接覆盖主内容路径及其大小写变体；
- Handler：Skill 更新时可把多个 `attachment_id + target_path` 编排为多个 `SkillFileImport`。

### Batch 4.4：Skill 简单文本文件内容编辑 (Completed)

Skill 已支持主内容 `skill.md` 与附加文件导入。已完成补充显式的"小文本文件内容编辑"接口，用于前端或 Agent 直接读取/替换 Skill 目录内的文本文件内容。该能力属于 HR Skill Domain，不依赖 Finance Attachment，也不接收 `attachment_id`。

路由实现：

```http
GET  /api/v1/hr/skills/{skill_id}/files          - 列出所有文件
GET  /api/v1/hr/skills/{skill_id}/files/{*filename} - 读取指定文件内容
PUT  /api/v1/hr/skills/{skill_id}/files/{*filename} - 更新指定文件内容
```

选择 Axum wildcard 路由 `{*filename}` 而不是 query 参数 `path`，原因是：
- 现有的 `/skills/{id}` 路由在更前面，wildcard 不会产生歧义；
- URL 路径语义更清晰，`/files/references/design.md` 一目了然；
- 前端不需要手动 URL 编码完整路径，Axum 自动处理。

DTO 设计：

```rust
// CreateSkillRequest 新增支持初始化多文件
pub struct CreateSkillRequest {
    // existing fields...
    pub initial_files: Option<HashMap<String, String>>,
}

// 列出文件响应
pub struct ListSkillFilesResponse {
    pub skill_id: String,
    pub files: Vec<SkillFileListItem>,
}

pub struct SkillFileListItem {
    pub filename: String,
    pub file_size: u64,
    pub content: Option<String>, // 小文件 (<64KB) 直接返回内容
}

// 获取文件内容响应
pub struct GetSkillFileContentResponse {
    pub skill_id: String,
    pub filename: String,
    pub content: String,
}

// 更新文件内容请求
pub struct UpdateSkillFileContentRequest {
    pub content: String,
    pub expected_updated_at: Option<i64>, // 乐观锁
}
```

编辑规则：

- 仅支持 UTF-8 简单文本，默认最大内容 `64KB`；
- `PUT` 是全量替换，不存在文件会自动创建（包括父目录递归创建）；
- `expected_updated_at` 可选，用于乐观锁并发控制；
- 允许编辑主文件 `skill.md`；
- 附加文件路径必须是 Skill 内容目录内的相对路径：拒绝空路径、绝对路径、`.`/`..` 片段、尾随 `/`、反斜杠路径分隔符；
- 第一版不提供文件删除、重命名、批量编辑、二进制写入；如需新增附加文本文件，可通过 `PUT` 指定安全相对路径并写入内容。

分层职责：

```text
GET/PUT /api/v1/hr/skills/{id}/files/content?path=...
    ↓
Skill Handler
    ↓
HR Domain SkillManage
    ├─ 校验 Skill 存在、未过期、可被当前用户/Agent 编辑
    ├─ 校验 path 安全与文本内容边界
    ├─ path == skill.md 时读写主内容
    └─ 其他 path 读写附加文件
    ↓
SkillDal
    ↓
SkillDao(技能目录文件读写 + metadata 更新时间)
```

建议 Domain 接口形态：

```rust
pub struct SkillTextFilePath {
    pub path: String,
}

pub struct UpdateSkillTextFileParams {
    pub skill_id: String,
    pub path: String,
    pub content: String,
    pub expected_updated_at: Option<i64>,
}

#[async_trait]
pub trait SkillManage: Send + Sync {
    async fn get_text_file_content(
        &self,
        ctx: RequestContext,
        skill_id: &str,
        path: &str,
    ) -> Result<Option<SkillTextFileContent>, AppError>;

    async fn update_text_file_content(
        &self,
        ctx: RequestContext,
        params: UpdateSkillTextFileParams,
    ) -> Result<SkillTextFileContent, AppError>;
}
```

`SkillTextFileContent` 返回业务对象，不暴露 `SkillPo`。如实现中需要更新 `updated_at/modified_by`，由 Domain 修改业务实体后调用 DAL 更新；文件写入仍通过 SkillDal/SkillDao 完成。

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

### `SkillVectorDao` 向量索引接口 (Completed)

**已实现基于通用 `VectorStore` trait，通过 `ctx.vector_store()` 获取存储层实现，不绑定具体数据库。**

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
    async fn get_po_by_id(&self, ctx: RequestContext, id: String) -> Result<Option<SkillPo>, AppError>;
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

    // ========== 文件操作 ==========
    /// 列出技能目录下所有文件（小文件自动预读内容）
    fn list_files(&self, skill: &SkillPo) -> Result<Vec<SkillFile>, AppError>;

    /// 读取指定文件内容
    fn read_file(&self, skill: &SkillPo, filename: &str) -> Result<String, AppError>;

    /// 写入指定文件内容
    fn write_file(&self, skill: &SkillPo, filename: &str, content: &str) -> Result<(), AppError>;
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

本模块开发遵循项目 `docs/design/sqlx_guide.md` 中的规范：

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

## 完成状态

所有核心功能已开发完成：

| 模块 | 状态 |
|------|------|
| 数据库表结构 | ✅ 完成 |
| DAO 基础数据 + 文件操作 | ✅ 完成 |
| DAO 向量搜索 | ✅ 完成 |
| DAL 层业务数据封装 | ✅ 完成 |
| Domain 领域逻辑 | ✅ 完成 |
| Handler HTTP API 基础 CRUD | ✅ 完成 |
| Handler 多文件文本编辑 API | ✅ 完成 |
| Agent 集成：自动沉淀技能流程 | ⏳ 后续随着 Agent 运行时迭代逐步完善 |

## 变更记录

| 日期 | 变更 |
|------|------|
| 2026-04-16 | 完成数据层开发，包括表结构、枚举、PO、DAO、单元测试 |
| 2026-05-13 | 更新文档，添加 DAL 层和 Domain 层设计说明 |
| 2026-05-14 | 添加完整的 hr skill 测试，修复 find_by_id 查询语义，排除过期技能 |
| 2026-05-14 | 文档更新：修正状态枚举过期名称（Available/Pending → Published/Draft），更新 DAO 接口定义（SkillDaoTrait → SkillDao + SkillVectorDao），添加 SkillQuery/SkillSearch/SkillVectorDao 接口说明，更新分层架构 |
| 2026-06-19 | 完成多文件文本编辑功能：`initial_files` 创建支持 + 列出/读取/更新三个 API + 完整路径安全校验 |
| 2026-06-19 | 确认 Skill 向量搜索 DAO 已完整实现，基于通用 `VectorStore` trait |
