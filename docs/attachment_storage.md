# 产物与消息附件统一存储设计

本文档描述 ai_orz 项目中产物（Artifact）与消息附件（Attachment）的统一存储设计规范。

## 设计背景

项目中原消息附件元数据和产物存储分别设计，存在重复逻辑，为了简化架构、统一管理，决定将产物和消息附件的元数据存储统一设计。

## 核心设计决策

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

#### artifacts 表（产物表）

```sql
CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT NOT NULL PRIMARY KEY,
    task_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    file_type INTEGER NOT NULL,
    file_meta JSON NOT NULL DEFAULT '{}',
    status INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL DEFAULT '',
    modified_by TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE INDEX IF NOT EXISTS idx_artifacts_task_id ON artifacts(task_id);
```

字段说明：
| 字段 | 类型 | 说明 |
|------|------|------|
| id | TEXT | 产物唯一ID（UUID） |
| task_id | TEXT | 所属任务ID |
| name | TEXT | 产物名称 |
| description | TEXT | 产物描述 |
| file_type | INTEGER | 文件类型（FileType 枚举）|
| file_meta | JSON | 文件元数据（FileMeta）|
| status | INTEGER | 状态：0=已删除，1=正常 |
| created_by | TEXT | 创建人ID |
| modified_by | TEXT | 最后修改人ID |
| created_at | INTEGER | 创建时间戳（毫秒）|
| updated_at | INTEGER | 更新时间戳（毫秒）|

权限设计：
- 产物权限通过任务 `task_id` 继承，不冗余存储 `root_user_id`
- 查询产物需要先校验任务权限，再查询产物

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
│   ├── enums/
│   │   └── file.rs          # FileType 枚举
│   └── config.rs            # 路径生成方法：generate_date_relative_path

src/
├── models/
│   ├── file.rs              # FileMeta 公共结构体
│   ├── artifact.rs          # ArtifactPo 持久化对象
│   └── message.rs           # 更新 MessagePo 适配新字段
└── service/
    └── dao/
        ├── artifact/
        │   ├── mod.rs       # ArtifactDaoTrait 定义
        │   ├── sqlite.rs    # Sqlite 实现
        │   └── sqlite_test.rs # 单元测试
        └── message/
            ├── mod.rs       # 更新导入
            └── sqlite.rs    # 更新查询适配新字段
```

## DAO 接口设计

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

## 版本历史

| 日期 | 变更 | 作者 |
|------|------|------|
| 2026-04-15 | 初始设计文档，完成数据层开发 | 王挺 |
