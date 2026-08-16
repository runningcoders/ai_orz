# 产物与消息附件统一存储设计

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：attachment_storage 设计文档归档冻结，设计决策已沉淀至 wiki 长文。生效方案：见源码和 wiki 长文。

> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构（Attachment 归 Finance 域）
> - [seed-config-migration.md](./seed-config-migration.md) — Seed 配置迁移（导出包中会携带 Attachment 引用）
> - 【② Plan 落地】[项目任务增强.md](../plan/项目任务增强.md) — Project Artifact 按需注入 with_artifacts option
> - 【③ Wiki 长文】[制品和附件.md](docs/wiki/zh/content/数据模型/项目和任务模型/制品和附件.md) — Attachment 通用资产 vs Artifact 项目产物
> - 【③ Wiki 长文】[文件存储模型.md](docs/wiki/zh/content/数据模型/系统模型/文件存储模型.md) — data_dir/attachments 分层目录 + 路径安全
> - 【③ Wiki 长文】[财务领域.md](docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/财务领域.md) — Finance 域三能力 attachment + credential + mcp_server
> - 【④ RAG 卡 1】[附件存储与DTO协议统一：AttachmentFinance域资产 + PagedResult T map全链路 + common::api单一事实源 + count与query复用WHERE](docs/wiki/knowledge/zh/附件存储与DTO协议统一：AttachmentFinance域资产%20+%20PagedResult%20T%20map全链路%20+%20common%3A%3Aapi单一事实源%20+%20count与query复用WHERE/附件存储与DTO协议统一：AttachmentFinance域资产%20+%20PagedResult%20T%20map全链路%20+%20common%3A%3Aapi单一事实源%20+%20count与query复用WHERE.md) — §1 Attachment 归 Finance 域 §红线 1 禁止直接写文件 §红线 2 路径穿越双校验
> - 【④ RAG 卡 2】[任务状态机与项目聚合](docs/wiki/knowledge/zh/任务状态机与项目聚合：TaskStatus%204%20态%20+%20progress%200-100%20自动联动%20+%20execution_plan_result%20JSON%20Patch%20+%20TaskGraph%20依赖%20DAG/任务状态机与项目聚合：TaskStatus%204%20态%20+%20progress%200-100%20自动联动%20+%20execution_plan_result%20JSON%20Patch%20+%20TaskGraph%20依赖%20DAG.md) — Project Artifact 产物消费方

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
- Batch 4.2 在 Finance Attachment 自身补充 `POST /api/v1/finance/attachments/text`，让 Attachment 除 multipart 上传外也支持 Agent/系统通过 JSON 创建小型 UTF-8 文本文件资产；该扩展不属于 Project Artifact Batch 3.1。

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

### Batch 4.2：Attachment 简单文本内容创建与编辑

通用上传能力稳定后，Finance Attachment 需要补充面向“小文本文件”的直接创建、内容读取与全量替换能力，使用户或 Agent 可以直接创建和维护 `txt/md/json/yaml/toml/csv` 等轻量文本资产。该能力仍归属 Finance Domain，不拆独立文件编辑 Domain；大文件和二进制文件继续走 multipart upload。

路由规划：

```http
POST /api/v1/finance/attachments/text
GET  /api/v1/finance/attachments/{id}/content
PUT  /api/v1/finance/attachments/{id}/content
```

请求与响应 DTO 建议复用通用文本结构，并在 Attachment API 中组合资源上下文：

```rust
pub struct CreateTextAttachmentRequest {
    pub file_name: String,
    pub content: String,
    pub mime_type: Option<String>,
    pub purpose: Option<String>,
}

pub type CreateTextAttachmentResponse = AttachmentDetail;

pub struct TextContentResponse {
    pub content: String,
    pub encoding: String, // 固定 "utf-8"
    pub size: u64,
    pub updated_at: i64,
}

pub struct UpdateTextContentRequest {
    pub content: String,
    pub expected_updated_at: Option<i64>,
}

pub struct AttachmentContentResponse {
    pub attachment: AttachmentDetail,
    pub text: TextContentResponse,
}
```

创建边界：

- `POST /attachments/text` 仅用于 JSON 传入的小型 UTF-8 文本内容，默认最大 `64KB`；
- `file_name` 只能是安全文件名，不能包含 `/`、`\`、`..`、绝对路径、空路径或目录目标；
- `mime_type` 可选，不传则按扩展名推断；即使传入，也只能作为提示，不能绕过文本类型和 UTF-8 校验；
- `purpose` 可选，如 `skill` / `message` / `artifact` / `tool_result`；
- 成功后由 AttachmentDal 生成 `id`、`stored_name`、`relative_path`、`FileType`、`size`，写入文件并持久化 metadata，返回 `AttachmentDetail`；
- Handler 不写文件、不生成物理路径，只把请求转换为 Finance Domain 命令。

编辑边界：

- 仅支持 UTF-8 简单文本，第一版不支持二进制、富文本、分片上传、patch/diff、版本历史；
- 默认最大内容建议沿用 Skill 小文件预读阈值 `64KB`，超过阈值返回 `Unsupported` 或 `PayloadTooLarge`，避免绕过通用上传的大文件边界；
- 可编辑类型通过 MIME 与扩展名双重兜底判断：`text/*`、`.txt`、`.md`、`.json`、`.yaml`、`.yml`、`.toml`、`.csv`；
- `PUT` 为全量替换语义，不做增量 patch；
- `expected_updated_at` 可选：传入时执行乐观锁校验，不匹配返回 `409 Conflict`；不传时按显式覆盖处理；
- 更新成功后必须刷新 `size`、`modified_by`、`updated_at`，`mime_type/file_type` 可保持原值或按文件名重新推断，但不得信任用户直接提交路径。

分层职责：

```text
POST /api/v1/finance/attachments/text
GET/PUT /api/v1/finance/attachments/{id}/content
    ↓
Finance Attachment Handler
    ↓
Finance Domain AttachmentManage
    ├─ 创建：校验 file_name/content/mime_type/purpose 与文本大小
    ├─ 读取/编辑：校验 Attachment 存在、未删除、属于当前 root_user_id
    ├─ 校验 MIME/扩展名/大小/UTF-8/乐观锁
    ↓
AttachmentDal
    ├─ 创建：生成 id/stored_name/relative_path/FileType/size，写入文件与 metadata
    ├─ 读取/编辑：read_file / write_file_bytes
    └─ 更新 Attachment 元数据实体
    ↓
AttachmentDao(SQLite metadata + 文件系统 primitive IO)
```

当前代码能力确认与需补齐缺口：

| 层级 | 当前能力 | Batch 4.2 需补齐 |
|------|----------|------------------|
| DAO | 已有 `insert/find_by_id/query/delete` 元数据持久化；已有 `write_file/read_file/file_exists` 基础文件 primitive，且 `write_file` 可覆盖同一路径内容 | 需新增“更新 Attachment 元数据”的持久化 primitive，例如更新 `size/mime_type/file_type/modified_by/updated_at`；DAO 不做 UTF-8、MIME、归属、乐观锁等业务判断 |
| DAL | 已有 `create_from_upload`：生成 `id/stored_name/relative_path/FileType/size`，写文件并插入 metadata；已有 `read_file` | 需新增 `create_from_text` 与 `update_text_content`：复用路径/文件名生成与类型推断，完成写文件 + 插入/更新 metadata 的组合编排 |
| Domain | 已有 `create_attachment` 与 `get_attachment(include_file_content=true)`，可读 bytes 并校验 root_user_id | 需新增文本语义接口：校验安全文件名、文本类型、UTF-8、64KB、归属、乐观锁，并把结果映射为 `AttachmentTextContent` |
| Handler | 已有 multipart upload/get/list/delete | 需新增 JSON 文本创建、文本内容读取、文本内容全量替换三个用户 action；Handler 不直接碰文件系统 |

结论：现有 DAO/DAL 已具备“文件创建/读取”的底层基础，但还不具备完整文本内容链路所需的“文本创建命令、文本更新编排、元数据更新、乐观锁与 UTF-8 校验”。Batch 4.2 必须把这些作为同一闭环实现，而不是只补 Handler/DTO。

建议接口形态：

```rust
pub struct TextAttachmentCreate {
    pub file_name: String,
    pub content: String,
    pub mime_type: Option<String>,
    pub purpose: Option<String>,
}

pub struct TextContentUpdate {
    pub content: String,
    pub expected_updated_at: Option<i64>,
}

#[async_trait::async_trait]
pub trait AttachmentManage: Send + Sync {
    async fn create_text_attachment(
        &self,
        ctx: RequestContext,
        create: TextAttachmentCreate,
    ) -> Result<Attachment, AppError>;

    async fn get_text_content(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<AttachmentTextContent>, AppError>;

    async fn update_text_content(
        &self,
        ctx: RequestContext,
        id: &str,
        update: TextContentUpdate,
    ) -> Result<AttachmentTextContent, AppError>;
}
```

`AttachmentTextContent` 属于 Finance Domain 业务返回对象，可包含 `attachment: Attachment` 与 `content/encoding/size/updated_at`；Handler 再转换为 `AttachmentContentResponse`。创建文本 Attachment 时，Handler 也只把 `CreateTextAttachmentRequest` 转成 Domain 命令，不直接读写文件、不直接改 `AttachmentPo`，也不直接拼接文件路径。

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
| 2026-06-17 | 规划 Attachment 简单文本内容创建/读取/全量替换 API，明确 UTF-8、小文件、乐观锁、Finance Domain 归属与分层边界 | Hermes |