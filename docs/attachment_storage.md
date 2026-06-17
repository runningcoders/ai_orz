# 产物与消息附件统一存储设计

本文档描述 ai_orz 项目中产物（Artifact）与消息附件（Attachment）的统一存储设计规范。

## 设计背景

项目中原消息附件元数据和产物存储分别设计，存在重复逻辑，为了简化架构、统一管理，决定将产物和消息附件的元数据存储统一设计。

## 核心设计决策

### 0. Attachment 作为 Finance Domain 的用户资产

通用文件上传能力不归属于 Message / Skill / Project 中的任一业务域，而是作为**用户资产（Attachment Asset）**归入 `finance` Domain 管理：

- 上传文件是可被多个业务域复用的通用资源；
- 后续可服务于 Skill 附件、Message 附件、Project Artifact、Tool 大结果附件、头像/配置文件等场景；
- Handler 必须遵循统一分层：`handler → finance domain → attachment dal → attachment dao`；
- Handler 不直接写文件，不直接调用 DAL/DAO；
- 业务接口不直接接收 multipart 文件流，而是引用已上传的 `attachment_id`。

通用上传链路：

```text
POST /api/v1/finance/attachments/upload
    ↓ multipart/form-data
Finance Attachment Handler
    ↓ UploadAttachmentCommand
Finance Domain
    ↓
AttachmentDal
    ↓
AttachmentDao(SQLite 元数据 + 文件系统落盘)
    ↓
AttachmentDetail / attachment_id
```

### 1. 元数据复用统一结构

产物和消息附件共用：
- **`FileType` 枚举**：定义文件类型（Document/Image/Audio/Video/Binary）
- **`FileMeta` 结构体**：存储文件元信息（相对路径、MIME类型、文件大小）
- 统一 JSON 序列化存储到数据库

### 2. 物理存储路径设计

采用日期分层目录结构，避免单目录文件过多：

```
<base_data_path>/attachments/YYYYMMDD/{file_id}{extension}
```

示例：
```
.ai_orz/attachments/20260415/
├── 01HNQVJZABCD123456789ABCDE.md
├── 01HNQVKWXYZ89012345678ABC.png
└── 01HNQVX0GHI45678901234JK.mp3
```

### 3. 数据库表设计

#### attachments 表（通用上传文件资产表）

`attachments` 表记录用户上传文件的资产元数据。物理文件存放在统一 attachments 目录下，业务域通过 `attachment_id` 引用上传结果，再根据自身语义导入或绑定。

```sql
CREATE TABLE IF NOT EXISTS attachments (
    id TEXT NOT NULL PRIMARY KEY,
    original_name TEXT NOT NULL,
    stored_name TEXT NOT NULL,
    relative_path TEXT NOT NULL,
    mime_type TEXT NOT NULL DEFAULT '',
    file_type INTEGER NOT NULL,
    size INTEGER NOT NULL DEFAULT 0,
    purpose TEXT NOT NULL DEFAULT '',
    status INTEGER NOT NULL DEFAULT 1,
    root_user_id TEXT NOT NULL,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_attachments_root_user_id ON attachments(root_user_id);
CREATE INDEX IF NOT EXISTS idx_attachments_purpose ON attachments(purpose);
CREATE INDEX IF NOT EXISTS idx_attachments_status ON attachments(status);
```

字段说明：

| 字段 | 类型 | 说明 |
|------|------|------|
| id | TEXT | 上传文件资产 ID，后续业务接口通过该 ID 引用文件 |
| original_name | TEXT | 用户上传时的原始文件名，仅作为展示元数据，不参与物理路径拼接 |
| stored_name | TEXT | 系统生成的存储文件名，通常为 `{id}{extension}` |
| relative_path | TEXT | 相对 attachments 根目录的路径，如 `20260617/{id}.md` |
| mime_type | TEXT | MIME 类型，可来自 multipart header 与扩展名推断 |
| file_type | INTEGER | 文件类型（FileType 枚举）|
| size | INTEGER | 文件大小，单位 bytes |
| purpose | TEXT | 可选用途标记，如 `skill` / `message` / `artifact` / `tool_result` |
| status | INTEGER | 状态：0=已删除，1=正常 |
| root_user_id | TEXT | 文件资产所属根用户，用于权限隔离 |
| created_by | TEXT | 上传人 |
| modified_by | TEXT | 最后修改人 |
| created_at | INTEGER | 创建时间戳（毫秒）|
| updated_at | INTEGER | 更新时间戳（毫秒）|

权限设计：
- 通用 Attachment 是用户资产，必须保存 `root_user_id`；
- 查询、导入、删除时都必须校验当前 `RequestContext` 与资产归属；
- `relative_path` 只作为系统内部路径片段，不允许外部直接提交任意路径进行业务绑定。

#### artifacts 表（产物表）

```sql
CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT NOT NULL PRIMARY KEY,
    project_id TEXT NOT NULL,
    task_id TEXT,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    file_type INTEGER NOT NULL,
    file_meta JSON NOT NULL DEFAULT '{}',
    source_type INTEGER NOT NULL DEFAULT 1,
    tags TEXT NOT NULL DEFAULT '[]',
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_artifacts_project_id ON artifacts(project_id);
CREATE INDEX IF NOT EXISTS idx_artifacts_task_id ON artifacts(task_id);
```

字段说明：
| 字段 | 类型 | 说明 |
|------|------|------|
| id | TEXT | 产物唯一ID（UUID） |
| project_id | TEXT | 所属项目 ID，必填；项目级与任务级产物都必须记录 |
| task_id | TEXT | 可选所属任务 ID；NULL 表示项目级产物 |
| name | TEXT | 产物名称 |
| description | TEXT | 产物描述 |
| file_type | INTEGER | 文件类型（FileType 枚举）|
| file_meta | JSON | 文件元数据（FileMeta）|
| source_type | INTEGER | 产物来源：1=attachment，2=generated_content，3=remote_url |
| tags | TEXT | 标签 JSON 数组 |
| status | INTEGER | 状态：0=已删除，1=正常 |
| created_by | TEXT | 创建人ID |
| modified_by | TEXT | 最后修改人ID |
| created_at | INTEGER | 创建时间戳（毫秒）|
| updated_at | INTEGER | 更新时间戳（毫秒）|

权限与归属设计：
- 产物权限通过 `project_id` 继承 Project Domain 的项目权限，不在 artifacts 表冗余 `root_user_id`；
- `task_id` 仅作为可选细分归属，创建/查询时如传入 task，Domain 必须校验 `task.project_id == artifact.project_id`；
- Artifact 管理面采用独立资源 API：`/api/v1/project/artifacts`，通过 query/body 参数传递 `project_id` / `task_id`，避免路径强绑定导致项目级与任务级查询不通用；
- 来源枚举支持 `attachment`、`generated_content`、`remote_url`；Batch 3.1 创建闭环仅落地 `attachment` 引用 Finance Attachment；
- `attachment` 来源由 Handler 编排 Finance Domain 和 Project Domain，只读取 Attachment metadata 组装 `file_meta`，不做二次文件复制/搬运；
- `generated_content` 当前仅在 DTO 契约中预留，创建 Handler 暂返回 Unsupported；后续落地时由 Handler 接收 `content + file_name + mime_type` 并交给 Project Domain，最终由 Artifact DAL/文件存储辅助模块写入 `artifacts/projects/{project_id}/{artifact_id}/{file_name}`；Handler 不直接读写文件或调用 DAO/DAL；
- 大文件仍应走 Finance Attachment 上传，再创建 attachment 引用型 Artifact，避免绕过通用上传能力；
- 后续可在 Finance Attachment 自身补充直接写入文本/文件内容的能力，让 Attachment 除 multipart 上传外也支持 Agent/系统创建文件资产；该扩展不属于 Project Artifact Batch 3.1。

#### messages 表变更

原 `meta_json` 字段更名为 `file_meta`，新增 `file_type` 和 `modified_by`：

```sql
-- 修改后的 messages 表相关字段
file_type INTEGER,
file_meta JSON NOT NULL DEFAULT '{}',
modified_by TEXT NOT NULL DEFAULT '',
```

说明：
- `file_type`：可选，消息如果是附件类型，存储附件文件类型
- `file_meta`：存储附件元数据，结构和产物一致
- `modified_by`：记录最后修改人，支持撤回/修改审计

## 代码分层结构

```
common/
├── src/
│   ├── api/
│   │   └── attachment.rs    # Attachment 上传/查询 API DTO
│   ├── enums/
│   │   └── file.rs          # FileType 枚举
│   └── config.rs            # 路径生成方法：generate_date_relative_path

src/
├── models/
│   ├── attachment.rs        # AttachmentPo / Attachment / AttachmentQuery
│   ├── file.rs              # FileMeta 公共结构体
│   ├── artifact.rs          # ArtifactPo 持久化对象
│   └── message.rs           # 更新 MessagePo 适配新字段
├── handlers/
│   └── finance/
│       └── attachment/      # upload/get/list/delete 等用户资产管理面 Handler
└── service/
    ├── dal/
    │   └── attachment.rs    # AttachmentDal：上传编排、元数据+文件组合
    ├── domain/
    │   └── finance/
    │       └── attachment.rs # Finance Attachment 用户资产管理能力
    └── dao/
        ├── attachment/
        │   ├── mod.rs       # AttachmentDao trait 定义
        │   ├── sqlite.rs    # SQLite 元数据 + 文件落盘基础实现
        │   └── sqlite_test.rs
        ├── artifact/
        │   ├── mod.rs       # ArtifactDaoTrait 定义
        │   ├── sqlite.rs    # Sqlite 实现
        │   └── sqlite_test.rs # 单元测试
        └── message/
            ├── mod.rs       # 更新导入
            └── sqlite.rs    # 更新查询适配新字段
```

## DAO 接口设计

### AttachmentDao

`AttachmentDao` 保持单一职责：负责 `AttachmentPo` 持久化和给定路径的文件系统基础读写，不承载业务归属、用途解释、跨领域导入等规则。

```rust
#[async_trait::async_trait]
pub trait AttachmentDao: Send + Sync {
    async fn insert(&self, ctx: RequestContext, po: &AttachmentPo) -> Result<(), AppError>;
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<AttachmentPo>, AppError>;
    async fn query(&self, ctx: RequestContext, query: AttachmentQuery) -> Result<Vec<AttachmentPo>, AppError>;
    async fn update_status(&self, ctx: RequestContext, id: &str, status: i32) -> Result<(), AppError>;
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;

    fn write_file(&self, relative_path: &str, bytes: &[u8]) -> Result<(), AppError>;
    fn read_file(&self, relative_path: &str) -> Result<Vec<u8>, AppError>;
    fn file_exists(&self, relative_path: &str) -> bool;
}
```

### AttachmentDal

`AttachmentDal` 负责上传编排和实体组装：生成 ID/存储名/相对路径，推断 `FileType`，写入文件，插入元数据，并返回业务实体。

```rust
#[async_trait::async_trait]
pub trait AttachmentDal: Send + Sync {
    async fn create_from_upload(
        &self,
        ctx: RequestContext,
        upload: AttachmentUpload,
    ) -> Result<Attachment, AppError>;

    async fn get_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<Attachment>, AppError>;
    async fn query(&self, ctx: RequestContext, query: AttachmentQuery) -> Result<Vec<Attachment>, AppError>;
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;
    fn read_file(&self, attachment: &Attachment) -> Result<Vec<u8>, AppError>;
}
```

### Finance Domain

Finance Domain 对外暴露用户资产语义，不暴露 DAO/DAL 细节：

```rust
#[async_trait::async_trait]
pub trait AttachmentManage: Send + Sync {
    async fn create_attachment(&self, ctx: RequestContext, upload: AttachmentUpload) -> Result<Attachment, AppError>;
    async fn get_attachment(&self, ctx: RequestContext, id: &str) -> Result<Option<Attachment>, AppError>;
    async fn query_attachments(&self, ctx: RequestContext, query: AttachmentQuery) -> Result<Vec<Attachment>, AppError>;
    async fn delete_attachment(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;
}
```

### ArtifactDao

```rust
#[async_trait::async_trait]
pub trait ArtifactDaoTrait: Send + Sync + std::fmt::Debug {
    /// Insert a new artifact
    async fn insert(&self, ctx: RequestContext, artifact: &ArtifactPo) -> Result<()>;

    /// Find artifact by id
    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<ArtifactPo>>;

    /// List all artifacts for a task
    async fn list_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<Vec<ArtifactPo>>;

    /// Count artifacts for a task
    async fn count_by_task(&self, ctx: RequestContext, task_id: &str) -> Result<i64>;

    /// Update artifact status
    async fn update_status(&self, ctx: RequestContext, id: &str, status: i32) -> Result<()>;

    /// Delete artifact (soft delete, set status = 0)
    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;
}
```

## 路径生成API

在 `common/src/config.rs` 中提供：

```rust
/// 消息附件存储根目录
pub fn attachments_dir(&self) -> PathBuf;

/// 获取单个附件完整路径
pub fn attachment_path(&self, rel: &str) -> PathBuf;

/// 任务产物存储根目录（复用 attachments）
pub fn artifacts_dir(&self) -> PathBuf;

/// 获取单个产物完整路径（复用 attachment_path）
pub fn artifact_path(&self, rel: &str) -> PathBuf;

/// 生成日期相对路径：YYYYMMDD/{file_id}{extension}
pub fn generate_date_relative_path(&self, file_id: &str, extension: &str) -> String;
```

## 软删除约定

遵循项目统一约定：
- `status = 0` 表示已删除（软删除）
- 所有默认查询都添加 `AND "status" != 0` 过滤已删除记录
- 保留数据用于审计，不物理删除

## 通用上传 API 设计

### Batch 2.4：Finance Attachment 管理面 API

第一阶段先落地通用上传与基础查询能力：

```http
POST   /api/v1/finance/attachments/upload
GET    /api/v1/finance/attachments/{id}
GET    /api/v1/finance/attachments
DELETE /api/v1/finance/attachments/{id}
```

上传请求：`multipart/form-data`

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| file | binary | 是 | 上传文件内容 |
| purpose | string | 否 | 用途标记，如 `skill` / `message` / `artifact` |

响应 DTO：

```rust
pub struct AttachmentDetail {
    pub id: String,
    pub original_name: String,
    pub stored_name: String,
    pub relative_path: String,
    pub mime_type: String,
    pub file_type: FileType,
    pub size: u64,
    pub purpose: String,
    pub root_user_id: String,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}
```

安全约束：
- 物理文件名必须由系统生成，不能直接使用 `original_name`；
- `relative_path` 必须由系统生成，禁止接受用户提交路径；
- 所有路径拼接必须防止 `../`、绝对路径、空路径；
- 单文件大小需设置上限，初期可先使用固定常量，后续如有必要再配置化；
- MIME 类型不可完全信任客户端，第一阶段可结合 multipart header 与扩展名做基础推断；
- 删除采用软删除，是否物理清理由后续后台清理策略统一处理。

## 与 Skill 文件更新的衔接

### Batch 2.5：Skill 文件引用导入

通用 Attachment 能力稳定后，Skill 更新接口不直接接收 multipart，而是接收已上传附件引用。跨域编排采用 **方案 B：Handler 层编排 Finance + HR**：

- Skill Handler 负责把用户请求里的 `attachment_id + target_path` 转换为 HR Domain 可处理的导入文件；
- Finance Domain 负责 Attachment 归属校验、元数据读取、按需装配物理文件读取结果；
- HR Skill Domain 只负责 Skill 自身业务规则、`target_path` 安全校验、写入 Skill 内容目录；
- HR Skill Domain 不直接依赖 Finance Domain，也不直接访问 Attachment DAL/DAO；
- Handler 只调用 Domain，不直接访问 DAO/DAL，不直接拼接或写入文件系统路径。

Skill 更新请求扩展：

```rust
pub struct SkillFileInput {
    pub attachment_id: String,
    pub target_path: String,
}
```

Finance Attachment 查询扩展为可选装配文件内容：

```rust
pub struct AttachmentGetOptions {
    pub include_file_content: bool,
}

pub struct AttachmentReadResult {
    pub relative_path: String,
    pub bytes: Vec<u8>,
    pub size: usize,
}

pub struct Attachment {
    pub po: AttachmentPo,
    pub read_results: Vec<AttachmentReadResult>,
}
```

约定：
- `include_file_content = false`：只返回 Attachment metadata，供普通管理面 GET 使用；
- `include_file_content = true`：Finance Domain 通过 Attachment DAL/DAO 读取物理文件，并把读取结果装配到 `Attachment.read_results`；
- 当前单附件只产生一个 `AttachmentReadResult`，保留集合结构是为了后续兼容多文件资产或组合附件；
- HTTP `GET /api/v1/finance/attachments/{id}` 默认不返回 bytes，避免管理面误传大字段；
- 内部编排场景由 Skill Handler 调用 Finance Domain 并显式要求装配文件内容。

最终链路：

```text
1. POST /api/v1/finance/attachments/upload
   → 返回 attachment_id

2. PUT /api/v1/hr/skills/{id}
   body.files = [{ attachment_id, target_path }]
   → Skill Handler 调 Finance Domain get_attachment(include_file_content = true)
   → Skill Handler 将 AttachmentReadResult 转换为 SkillFileImport
   → HR Skill Domain 校验 target_path 并导入到 Skill 内容目录
```

HR Skill Domain 接收的导入对象不包含 `attachment_id`，避免把 Finance 领域概念泄漏到 HR Domain：

```rust
pub struct SkillFileImport {
    pub target_path: String,
    pub bytes: Vec<u8>,
}
```

Skill 导入规则：
- `attachment_id` 必须属于当前 root_user_id；
- `target_path` 只能是 Skill 目录内的相对路径，禁止 `../` 和绝对路径；
- `target_path` 不能为空，不能指向目录，不能包含反斜杠路径分隔符，不能直接覆盖主文件 `skill.md`（大小写变体也拒绝）；
- 主文件 `skill.md` 仍由 Skill 的 `content` 字段负责；
- 附加文件由 `files` 引用导入，导入时复制文件到 Skill 自己的 `content_path` 目录；
- Skill Handler 不直接访问 Attachment DAO/DAL，也不直接访问 Skill DAO/DAL；
- Skill Handler 允许做跨 Domain 编排，但不能承载路径安全规则，路径安全仍由 HR Skill Domain 统一校验。

## 版本历史

| 日期 | 变更 | 作者 |
|------|------|------|
| 2026-04-15 | 初始设计文档，完成数据层开发 | 王挺 |
| 2026-06-17 | 落地 Finance Attachment 通用上传 API：新增 attachments 表、Attachment DAO/DAL、Finance Domain 管理能力、common DTO、Axum Handler/Router，并规划 Skill 文件引用导入链路 | Hermes |
