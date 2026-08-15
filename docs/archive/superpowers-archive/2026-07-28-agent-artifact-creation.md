# Agent Artifact 创建能力实施 Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 Agent 能自行生产文件并注册为 artifact，支持文本类（传 content）和文件类（从 agent 目录复制）两种创建方式，同时打通 HTTP 接口的 GeneratedContent 分支。

**Architecture:** Domain 层新增两个方法（`create_generated_artifact` / `create_generated_artifact_from_file`），Handler 层新增两个工具（`create_text_artifact` / `register_artifact_from_path`）并补全 `create_artifact` 的 GeneratedContent 分支。所有 artifact 工具统一归口 `project_management` 工具包，去掉 `neural` flag。`fs_write` 改为 Agent 目录隔离。

**Tech Stack:** Rust, axum, sqlx, ai-orz-macros (register_handler_tool / generate_http_handler)

---

## File Structure

### 新建文件
- `src/handlers/project/artifact/create_text_artifact.rs` — 文本类 artifact 创建工具
- `src/handlers/project/artifact/register_artifact_from_path.rs` — 文件类 artifact 注册工具
- `src/handlers/project/artifact/mime_util.rs` — mime_type 推断工具（基于扩展名）

### 修改文件
- `src/pkg/tool_registry/fs_write.rs` — CoreTool::call 使用 ctx 计算 agent 目录作为 base_path
- `src/handlers/project/artifact/mod.rs` — 注册新 handler 模块
- `src/handlers/project/artifact/create_artifact.rs` — 打通 GeneratedContent 分支
- `src/handlers/project/artifact/update_artifact_content.rs` — 加 `tags = "project_management"`
- `src/handlers/project/artifact/query_artifacts.rs` — 去 `neural`，加 `tags = "project_management"`
- `src/service/domain/project/mod.rs` — ArtifactManage trait 新增两个方法签名
- `src/service/domain/project/artifact.rs` — 实现两个新方法
- `common/src/api/artifact.rs` — 新增 `CreateTextArtifactParams` / `RegisterArtifactFromPathParams`

---

## Task 1: fs_write Agent 路径隔离

**Files:**
- Modify: `src/pkg/tool_registry/fs_write.rs:122-133`

- [ ] **Step 1: 修改 CoreTool::call 签名和 base_path 计算**

将 `src/pkg/tool_registry/fs_write.rs` 第 123 行的 `_ctx` 改为 `ctx`，并在第 132-133 行替换 base_path 计算：

```rust
#[async_trait::async_trait]
impl CoreTool for FsWriteCoreTool {
    async fn call(&self, ctx: RequestContext, args: Value) -> Result<Value> {
        // Parse arguments
        let args: WriteFileArgs = serde_json::from_value(args)
            .map_err(|e| anyhow!("Invalid arguments: {}", e))
            .map_err(common::error::Error::from)?;

        // Validate required parameters for mode
        validate_args(&args)?;

        // Get agent-scoped data path from context
        let agent_id = ctx
            .agent_id()
            .ok_or_else(|| anyhow!("agent_id is required for fs_write"))?;
        let base_path = crate::config::get().agent_data_dir(agent_id);
        let additional_allowed = self
            .config
            .additional_allowed_paths
            .as_deref()
            .unwrap_or(&[]);
        match resolve_and_validate_path(&base_path, &args.path, additional_allowed)? {
            // ... 后续逻辑不变
```

**注意**：
- `_ctx: RequestContext` 改为 `ctx: RequestContext`
- `crate::config::get().base_data_path()` 改为 `crate::config::get().agent_data_dir(agent_id)`
- 需要在文件顶部确保 `use common::error::Error;` 或用 `anyhow!` 生成错误（已有 `use anyhow::anyhow;`）

- [ ] **Step 2: 验证编译通过**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: 编译通过（可能有 warning，但无 error）

- [ ] **Step 3: 更新现有 fs_write 测试**

fs_write 现有测试（`src/pkg/tool_registry/fs_write.rs:320-421`）不涉及 `call` 方法，只测试 `validate_args`、`is_sensitive_filename`、`split_lines`，不需要修改。

但如果项目中有集成测试调用了 `FsWriteCoreTool::call`，需要传带 agent_id 的 RequestContext。搜索是否有此类测试：

Run: `grep -r "FsWriteCoreTool" --include="*.rs" src/ tests/`

如果有集成测试引用 `FsWriteCoreTool::call`，需要更新测试构造带 `agent_id` 的 RequestContext。

- [ ] **Step 4: 提交**

```bash
git add src/pkg/tool_registry/fs_write.rs
git commit -m "feat(fs_write): isolate write path to agent directory

Change base_path from global base_data_path to agent_data_dir(agent_id),
ensuring each agent can only write to its own agents/{agent_id}/ directory."
```

---

## Task 2: Domain 层新增 create_generated_artifact 方法（文本类）

**Files:**
- Modify: `src/service/domain/project/mod.rs:425` (ArtifactManage trait 末尾，在 `}` 前插入)
- Modify: `src/service/domain/project/artifact.rs` (在 `impl ProjectDomainImpl` 块之前插入新方法实现)
- Test: `src/service/domain/project/artifact.rs` (#[cfg(test)] 模块)

- [ ] **Step 1: 在 ArtifactManage trait 新增方法签名**

在 `src/service/domain/project/mod.rs` 的 `ArtifactManage` trait 中，`update_artifact_content` 方法之后（约第 425 行 `}` 之前）新增：

```rust
    /// 创建 GeneratedContent 类型产物（文本类，直接传 content）。
    ///
    /// 建 DB 记录后落盘，若落盘失败回滚 DB 记录。
    #[allow(clippy::too_many_arguments)]
    async fn create_generated_artifact(
        &self,
        ctx: RequestContext,
        project_id: String,
        task_id: Option<String>,
        name: String,
        description: String,
        content: Vec<u8>,
        file_name: String,
        mime_type: String,
        file_type: FileType,
        tags: Vec<String>,
        created_by: String,
    ) -> Result<Artifact>;
```

- [ ] **Step 2: 在 ProjectDomainImpl 实现 create_generated_artifact**

在 `src/service/domain/project/artifact.rs` 的 `impl super::ArtifactManage for ProjectDomainImpl` 块中，`create_attachment_artifact` 方法之后（约第 71 行之后）新增：

```rust
    /// 创建 GeneratedContent 类型产物（文本类，直接传 content）
    #[allow(clippy::too_many_arguments)]
    async fn create_generated_artifact(
        &self,
        ctx: RequestContext,
        project_id: String,
        task_id: Option<String>,
        name: String,
        description: String,
        content: Vec<u8>,
        file_name: String,
        mime_type: String,
        file_type: FileType,
        tags: Vec<String>,
        created_by: String,
    ) -> Result<Artifact> {
        self.validate_project_and_task(ctx.clone(), &project_id, task_id.as_deref())
            .await?;

        let file_size = content.len() as u64;
        let file_meta = FileMeta::new(file_name, mime_type, file_size);

        let mut artifact = if let Some(task_id) = task_id {
            Artifact::new_task_with_source_type(
                project_id,
                task_id,
                name,
                description,
                file_type,
                file_meta,
                ArtifactSourceType::GeneratedContent,
                created_by.clone(),
            )
        } else {
            Artifact::new_project_with_source_type(
                project_id,
                name,
                description,
                file_type,
                file_meta,
                ArtifactSourceType::GeneratedContent,
                created_by.clone(),
            )
        };
        artifact.po.set_tags(tags, &created_by);
        let ctx = enrich_ctx!(&ctx, &artifact);

        // 建 DB 记录
        self.artifact_dal.create(ctx.clone(), &artifact).await?;

        // 落盘，失败则回滚 DB 记录
        if let Err(e) = self.artifact_dal.write_content(ctx.clone(), &artifact, &content).await {
            let _ = self.artifact_dal.delete(ctx, &artifact.po.id).await;
            return Err(e);
        }

        Ok(artifact)
    }
```

- [ ] **Step 3: 验证编译通过**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: 编译通过

- [ ] **Step 4: 提交**

```bash
git add src/service/domain/project/mod.rs src/service/domain/project/artifact.rs
git commit -m "feat(domain): add create_generated_artifact for text content

Domain layer method to create GeneratedContent artifacts with text content.
Creates DB record first, then writes content to disk. Rolls back DB record
on write failure."
```

---

## Task 3: Domain 层新增 create_generated_artifact_from_file 方法（文件类，复制）

**Files:**
- Modify: `src/service/domain/project/mod.rs` (ArtifactManage trait)
- Modify: `src/service/domain/project/artifact.rs` (impl)

- [ ] **Step 1: 在 ArtifactManage trait 新增方法签名**

在 `src/service/domain/project/mod.rs` 的 `ArtifactManage` trait 中，`create_generated_artifact` 之后新增：

```rust
    /// 创建 GeneratedContent 类型产物（文件类，从源路径复制文件）。
    ///
    /// 建 DB 记录后复制文件到 artifact 目录，若复制失败回滚 DB 记录。
    /// 源文件保留（不移动），Agent 保留工作副本。
    #[allow(clippy::too_many_arguments)]
    async fn create_generated_artifact_from_file(
        &self,
        ctx: RequestContext,
        project_id: String,
        task_id: Option<String>,
        name: String,
        description: String,
        source_path: std::path::PathBuf,
        file_name: String,
        mime_type: String,
        file_type: FileType,
        tags: Vec<String>,
        created_by: String,
    ) -> Result<Artifact>;
```

- [ ] **Step 2: 在 ProjectDomainImpl 实现 create_generated_artifact_from_file**

在 `src/service/domain/project/artifact.rs` 的 `create_generated_artifact` 实现之后新增：

```rust
    /// 创建 GeneratedContent 类型产物（文件类，从源路径复制文件）
    #[allow(clippy::too_many_arguments)]
    async fn create_generated_artifact_from_file(
        &self,
        ctx: RequestContext,
        project_id: String,
        task_id: Option<String>,
        name: String,
        description: String,
        source_path: std::path::PathBuf,
        file_name: String,
        mime_type: String,
        file_type: FileType,
        tags: Vec<String>,
        created_by: String,
    ) -> Result<Artifact> {
        self.validate_project_and_task(ctx.clone(), &project_id, task_id.as_deref())
            .await?;

        // 读取源文件大小
        let file_metadata = std::fs::metadata(&source_path).map_err(|e| {
            common::error::Error::from(anyhow::anyhow!(
                "Failed to read source file metadata: {}",
                e
            ))
        })?;
        if !file_metadata.is_file() {
            bail_err!(InvalidRequest, "source_path is not a file: {:?}", source_path);
        }
        let file_size = file_metadata.len();

        let file_meta = FileMeta::new(file_name.clone(), mime_type, file_size);

        let mut artifact = if let Some(task_id) = task_id {
            Artifact::new_task_with_source_type(
                project_id.clone(),
                task_id,
                name,
                description,
                file_type,
                file_meta,
                ArtifactSourceType::GeneratedContent,
                created_by.clone(),
            )
        } else {
            Artifact::new_project_with_source_type(
                project_id.clone(),
                name,
                description,
                file_type,
                file_meta,
                ArtifactSourceType::GeneratedContent,
                created_by.clone(),
            )
        };
        artifact.po.set_tags(tags, &created_by);
        let ctx = enrich_ctx!(&ctx, &artifact);

        // 建 DB 记录
        self.artifact_dal.create(ctx.clone(), &artifact).await?;

        // 复制文件到 artifact 目录
        let config = crate::config::get();
        let target_dir = config.artifact_path(&project_id, &artifact.po.id);
        let target_path = target_dir.join(&file_name);

        // 安全校验：目标路径必须在 artifacts_dir 之下
        if !target_path.starts_with(config.artifacts_dir()) {
            let _ = self.artifact_dal.delete(ctx.clone(), &artifact.po.id).await;
            bail_err!(InvalidRequest, "Invalid artifact file path: path traversal detected");
        }

        if let Err(e) = std::fs::create_dir_all(&target_dir) {
            let _ = self.artifact_dal.delete(ctx.clone(), &artifact.po.id).await;
            return Err(common::error::Error::from(anyhow::anyhow!(
                "Failed to create artifact directory: {}",
                e
            )));
        }

        if let Err(e) = std::fs::copy(&source_path, &target_path) {
            let _ = self.artifact_dal.delete(ctx, &artifact.po.id).await;
            return Err(common::error::Error::from(anyhow::anyhow!(
                "Failed to copy file to artifact storage: {}",
                e
            )));
        }

        Ok(artifact)
    }
```

**注意**：
- `use std::path::PathBuf` 不需要在 trait 文件中 import，用全路径 `std::path::PathBuf`
- `bail_err!` 已在 artifact.rs 顶部 import（`use common::error::{Result, bail_err, err};`）
- `anyhow::anyhow!` 需要确认 artifact.rs 顶部有 `use anyhow::anyhow;`，如果没有需要加

- [ ] **Step 3: 确认 artifact.rs 顶部 import**

检查 `src/service/domain/project/artifact.rs` 顶部是否有 `use anyhow::anyhow;`。如果没有，在现有 import 之后添加：

```rust
use anyhow::anyhow;
```

- [ ] **Step 4: 验证编译通过**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: 编译通过

- [ ] **Step 5: 提交**

```bash
git add src/service/domain/project/mod.rs src/service/domain/project/artifact.rs
git commit -m "feat(domain): add create_generated_artifact_from_file

Domain layer method to create GeneratedContent artifacts by copying a file
from agent's directory. Source file is preserved (not moved). Rolls back DB
record on copy failure."
```

---

## Task 4: 新增 API DTO

**Files:**
- Modify: `common/src/api/artifact.rs` (文件末尾追加)

- [ ] **Step 1: 新增 CreateTextArtifactParams 和 RegisterArtifactFromPathParams**

在 `common/src/api/artifact.rs` 文件末尾追加：

```rust
/// Create text artifact params (neural tool: create_text_artifact).
///
/// Agent provides text content directly; the tool handles file creation
/// and artifact metadata registration in one step.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateTextArtifactParams {
    /// Project ID. Required.
    pub project_id: String,
    /// Optional task ID. `None` means project-level artifact.
    pub task_id: Option<String>,
    /// Artifact display name.
    pub name: String,
    /// Optional artifact description.
    pub description: Option<String>,
    /// Text content of the artifact.
    pub content: String,
    /// File name for storage. Defaults to derived from name (with .md extension).
    pub file_name: Option<String>,
    /// MIME type. Defaults to "text/plain".
    pub mime_type: Option<String>,
    /// File type category. Defaults to Document.
    pub file_type: Option<FileType>,
    /// Optional tags.
    pub tags: Option<Vec<String>>,
}

/// Register artifact from file path params (neural tool: register_artifact_from_path).
///
/// Agent provides a file path in its own directory; the tool copies the file
/// to artifact storage and registers metadata. Source file is preserved.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct RegisterArtifactFromPathParams {
    /// Project ID. Required.
    pub project_id: String,
    /// Optional task ID. `None` means project-level artifact.
    pub task_id: Option<String>,
    /// Artifact display name.
    pub name: String,
    /// Optional artifact description.
    pub description: Option<String>,
    /// Source file path, relative to agent's directory.
    pub source_path: String,
    /// File name for artifact storage. Defaults to basename of source_path.
    pub file_name: Option<String>,
    /// MIME type. Defaults to inferred from file extension.
    pub mime_type: Option<String>,
    /// File type category. Defaults to inferred from mime_type.
    pub file_type: Option<FileType>,
    /// Optional tags.
    pub tags: Option<Vec<String>>,
}
```

- [ ] **Step 2: 验证编译通过**

Run: `cargo build -p common 2>&1 | head -20`
Expected: 编译通过

- [ ] **Step 3: 提交**

```bash
git add common/src/api/artifact.rs
git commit -m "feat(api): add CreateTextArtifactParams and RegisterArtifactFromPathParams"
```

---

## Task 5: 新增 mime_util 工具模块

**Files:**
- Create: `src/handlers/project/artifact/mime_util.rs`

- [ ] **Step 1: 创建 mime_util.rs**

创建 `src/handlers/project/artifact/mime_util.rs`：

```rust
//! MIME type inference utility for artifact file registration.

use common::enums::FileType;

/// Infer MIME type from file extension.
///
/// Returns "application/octet-stream" for unknown extensions.
pub fn infer_mime_type(file_name: &str) -> String {
    let ext = file_name
        .rsplit('.')
        .next()
        .filter(|ext| ext.len() < file_name.len()) // Avoid treating "file" as extension
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "txt" => "text/plain".to_string(),
        "md" => "text/markdown".to_string(),
        "html" | "htm" => "text/html".to_string(),
        "css" => "text/css".to_string(),
        "js" => "application/javascript".to_string(),
        "json" => "application/json".to_string(),
        "xml" => "application/xml".to_string(),
        "csv" => "text/csv".to_string(),
        "tsv" => "text/tab-separated-values".to_string(),
        "yaml" | "yml" => "application/x-yaml".to_string(),
        "toml" => "application/toml".to_string(),
        "pdf" => "application/pdf".to_string(),
        "zip" => "application/zip".to_string(),
        "gz" | "gzip" => "application/gzip".to_string(),
        "tar" => "application/x-tar".to_string(),
        "png" => "image/png".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "gif" => "image/gif".to_string(),
        "svg" => "image/svg+xml".to_string(),
        "webp" => "image/webp".to_string(),
        "mp3" => "audio/mpeg".to_string(),
        "mp4" => "video/mp4".to_string(),
        "wav" => "audio/wav".to_string(),
        "py" => "text/x-python".to_string(),
        "rs" => "text/x-rust".to_string(),
        "go" => "text/x-go".to_string(),
        "java" => "text/x-java".to_string(),
        "c" | "h" => "text/x-c".to_string(),
        "cpp" | "hpp" | "cc" => "text/x-c++".to_string(),
        "sh" | "bash" => "application/x-sh".to_string(),
        "sql" => "application/sql".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

/// Infer FileType from MIME type.
///
/// Returns `FileType::Document` for unknown MIME types.
pub fn infer_file_type(mime_type: &str) -> FileType {
    if mime_type.starts_with("image/") {
        FileType::Image
    } else if mime_type.starts_with("video/") {
        FileType::Video
    } else if mime_type.starts_with("audio/") {
        FileType::Audio
    } else if mime_type == "application/zip"
        || mime_type == "application/gzip"
        || mime_type == "application/x-tar"
    {
        FileType::Archive
    } else {
        FileType::Document
    }
}

/// Extract the basename (file name without directory) from a path.
pub fn basename(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_mime_type() {
        assert_eq!(infer_mime_type("report.md"), "text/markdown");
        assert_eq!(infer_mime_type("data.csv"), "text/csv");
        assert_eq!(infer_mime_type("image.png"), "image/png");
        assert_eq!(infer_mime_type("unknown.xyz"), "application/octet-stream");
        assert_eq!(infer_mime_type("noext"), "application/octet-stream");
    }

    #[test]
    fn test_infer_file_type() {
        assert_eq!(infer_file_type("image/png"), FileType::Image);
        assert_eq!(infer_file_type("video/mp4"), FileType::Video);
        assert_eq!(infer_file_type("audio/mpeg"), FileType::Audio);
        assert_eq!(infer_file_type("application/zip"), FileType::Archive);
        assert_eq!(infer_file_type("text/plain"), FileType::Document);
        assert_eq!(infer_file_type("application/octet-stream"), FileType::Document);
    }

    #[test]
    fn test_basename() {
        assert_eq!(basename("output/data.csv"), "data.csv");
        assert_eq!(basename("data.csv"), "data.csv");
        assert_eq!(basename("a/b/c/report.md"), "report.md");
        assert_eq!(basename("dir\\file.txt"), "file.txt");
    }
}
```

- [ ] **Step 2: 在 mod.rs 注册模块**

修改 `src/handlers/project/artifact/mod.rs`，在现有 `mod` 声明中添加：

```rust
mod mime_util;
```

放在 `mod response;` 之后。

- [ ] **Step 3: 运行测试验证**

Run: `cargo test -p ai_orz mime_util 2>&1 | tail -20`
Expected: 3 个测试全部 PASS

- [ ] **Step 4: 提交**

```bash
git add src/handlers/project/artifact/mime_util.rs src/handlers/project/artifact/mod.rs
git commit -m "feat(artifact): add mime_util for MIME type inference

Provides infer_mime_type (from file extension), infer_file_type (from MIME),
and basename extraction. Used by register_artifact_from_path tool."
```

---

## Task 6: 新增工具 create_text_artifact

**Files:**
- Create: `src/handlers/project/artifact/create_text_artifact.rs`
- Modify: `src/handlers/project/artifact/mod.rs`

- [ ] **Step 1: 创建 create_text_artifact.rs**

创建 `src/handlers/project/artifact/create_text_artifact.rs`：

```rust
//! Handler: create_text_artifact - Create a text-based artifact with content

use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::artifact::{ArtifactDetail, CreateTextArtifactParams};
use common::enums::FileType;
use common::error::{Result, bail_err, err};

/// Create a text-based artifact with content.
///
/// Agent provides text content directly; the tool handles file creation
/// and artifact metadata registration in one step.
#[register_handler_tool(
    id = "create_text_artifact",
    name = "create_text_artifact",
    description = "Create a text-based artifact with content. The content will be saved to artifact storage.",
    params = "common::api::CreateTextArtifactParams",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn create_text_artifact(
    ctx: RequestContext,
    params: CreateTextArtifactParams,
) -> Result<ArtifactDetail> {
    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if params.project_id.trim().is_empty() {
        bail_err!(InvalidRequest, "project_id不能为空");
    }
    if params.name.trim().is_empty() {
        bail_err!(InvalidRequest, "name不能为空");
    }

    // Validate content size (max 1MB for text)
    let content_bytes = params.content.into_bytes();
    if content_bytes.len() > 1024 * 1024 {
        bail_err!(InvalidRequest, "Text content exceeds maximum size of 1MB");
    }

    let file_name = params.file_name.unwrap_or_else(|| {
        format!("{}.md", params.name)
    });
    let mime_type = params.mime_type.unwrap_or_else(|| "text/plain".to_string());
    let file_type = params.file_type.unwrap_or(FileType::Document);

    let artifact = project::domain()
        .artifact_manage()
        .create_generated_artifact(
            ctx,
            params.project_id,
            params.task_id,
            params.name,
            params.description.unwrap_or_default(),
            content_bytes,
            file_name,
            mime_type,
            file_type,
            params.tags.unwrap_or_default(),
            current_user_id,
        )
        .await?;

    Ok(response::to_detail(&artifact))
}
```

- [ ] **Step 2: 在 mod.rs 注册模块**

修改 `src/handlers/project/artifact/mod.rs`，添加：

```rust
mod create_text_artifact;
```

和对应的 pub use：

```rust
pub use create_text_artifact::create_text_artifact_handler;
```

- [ ] **Step 3: 验证编译通过**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: 编译通过

- [ ] **Step 4: 提交**

```bash
git add src/handlers/project/artifact/create_text_artifact.rs src/handlers/project/artifact/mod.rs
git commit -m "feat(artifact): add create_text_artifact tool

Neural tool for agents to create text-based artifacts by providing content
directly. Registered under project_management tool pack."
```

---

## Task 7: 新增工具 register_artifact_from_path

**Files:**
- Create: `src/handlers/project/artifact/register_artifact_from_path.rs`
- Modify: `src/handlers/project/artifact/mod.rs`

- [ ] **Step 1: 创建 register_artifact_from_path.rs**

创建 `src/handlers/project/artifact/register_artifact_from_path.rs`：

```rust
//! Handler: register_artifact_from_path - Register a file as artifact

use super::mime_util;
use super::response;
use crate::pkg::RequestContext;
use crate::service::domain::project;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::artifact::{ArtifactDetail, RegisterArtifactFromPathParams};
use common::error::{Result, bail_err, err};

/// Register an existing file (in agent's directory) as an artifact.
///
/// The file will be **copied** to artifact storage. Source file is preserved
/// so the agent can continue working on it.
#[register_handler_tool(
    id = "register_artifact_from_path",
    name = "register_artifact_from_path",
    description = "Register an existing file (in agent's directory) as an artifact. The file will be copied to artifact storage.",
    params = "common::api::RegisterArtifactFromPathParams",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn register_artifact_from_path(
    ctx: RequestContext,
    params: RegisterArtifactFromPathParams,
) -> Result<ArtifactDetail> {
    let agent_id = ctx
        .agent_id()
        .ok_or_else(|| err!(InvalidRequest, "agent_id is required for register_artifact_from_path"))?;

    let current_user_id = ctx.uid();
    if current_user_id.is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }
    if params.project_id.trim().is_empty() {
        bail_err!(InvalidRequest, "project_id不能为空");
    }
    if params.name.trim().is_empty() {
        bail_err!(InvalidRequest, "name不能为空");
    }
    if params.source_path.trim().is_empty() {
        bail_err!(InvalidRequest, "source_path不能为空");
    }

    // Compute source file absolute path
    let agent_dir = crate::config::get().agent_data_dir(agent_id);
    let source_path = agent_dir.join(&params.source_path);

    // Security: source path must be under agent's directory (prevent traversal)
    let source_canonical = source_path
        .canonicalize()
        .map_err(|_| err!(InvalidRequest, "源文件不存在或无法访问: {}", params.source_path))?;
    let agent_dir_canonical = agent_dir
        .canonicalize()
        .map_err(|_| err!(Internal, "Agent directory not accessible"))?;
    if !source_canonical.starts_with(&agent_dir_canonical) {
        bail_err!(InvalidRequest, "source_path 越界：必须在 agent 目录之下");
    }

    // Validate source file exists and is a file
    let file_metadata = std::fs::metadata(&source_canonical)
        .map_err(|_| err!(InvalidRequest, "源文件不存在: {}", params.source_path))?;
    if !file_metadata.is_file() {
        bail_err!(InvalidRequest, "source_path 不是文件: {}", params.source_path);
    }

    // Derive file_name and mime_type
    let file_name = params
        .file_name
        .unwrap_or_else(|| mime_util::basename(&params.source_path));
    let mime_type = params
        .mime_type
        .unwrap_or_else(|| mime_util::infer_mime_type(&file_name));
    let file_type = params
        .file_type
        .unwrap_or_else(|| mime_util::infer_file_type(&mime_type));

    let artifact = project::domain()
        .artifact_manage()
        .create_generated_artifact_from_file(
            ctx,
            params.project_id,
            params.task_id,
            params.name,
            params.description.unwrap_or_default(),
            source_canonical,
            file_name,
            mime_type,
            file_type,
            params.tags.unwrap_or_default(),
            current_user_id,
        )
        .await?;

    Ok(response::to_detail(&artifact))
}
```

- [ ] **Step 2: 在 mod.rs 注册模块**

修改 `src/handlers/project/artifact/mod.rs`，添加：

```rust
mod register_artifact_from_path;
```

和对应的 pub use：

```rust
pub use register_artifact_from_path::register_artifact_from_path_handler;
```

- [ ] **Step 3: 验证编译通过**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: 编译通过

- [ ] **Step 4: 提交**

```bash
git add src/handlers/project/artifact/register_artifact_from_path.rs src/handlers/project/artifact/mod.rs
git commit -m "feat(artifact): add register_artifact_from_path tool

Neural tool for agents to register a file from their directory as an artifact.
File is copied (not moved) to artifact storage, preserving agent's working copy."
```

---

## Task 8: 打通 create_artifact handler 的 GeneratedContent 分支

**Files:**
- Modify: `src/handlers/project/artifact/create_artifact.rs:40-44`

- [ ] **Step 1: 替换 GeneratedContent 分支**

将 `src/handlers/project/artifact/create_artifact.rs` 第 40-44 行：

```rust
        ArtifactSourceType::GeneratedContent => {
            bail_err!(
                UnsupportedOperation,
                "generated_content artifact create is not implemented yet"
            );
        }
```

替换为：

```rust
        ArtifactSourceType::GeneratedContent => {
            create_from_generated_content(ctx, params, current_user_id).await?
        }
```

- [ ] **Step 2: 新增 create_from_generated_content 函数**

在 `src/handlers/project/artifact/create_artifact.rs` 文件末尾（`create_from_attachment` 函数之后）新增：

```rust
async fn create_from_generated_content(
    ctx: RequestContext,
    params: CreateArtifactRequest,
    current_user_id: String,
) -> Result<crate::models::artifact::Artifact> {
    let content = params.content
        .ok_or_else(|| err!(InvalidRequest, "content 不能为空（generated_content 类型）"))?;
    let file_name = params.file_name
        .ok_or_else(|| err!(InvalidRequest, "file_name 不能为空（generated_content 类型）"))?;

    // Validate content size (max 1MB for text)
    let content_bytes = content.into_bytes();
    if content_bytes.len() > 1024 * 1024 {
        bail_err!(InvalidRequest, "Text content exceeds maximum size of 1MB");
    }

    let mime_type = params.mime_type.unwrap_or_else(|| "text/plain".to_string());
    let file_type = params.file_type.unwrap_or(common::enums::FileType::Document);

    project::domain()
        .artifact_manage()
        .create_generated_artifact(
            ctx,
            params.project_id,
            params.task_id,
            params.name,
            params.description.unwrap_or_default(),
            content_bytes,
            file_name,
            mime_type,
            file_type,
            params.tags.unwrap_or_default(),
            current_user_id,
        )
        .await
}
```

- [ ] **Step 3: 同时给 create_artifact 加 tags**

修改 `src/handlers/project/artifact/create_artifact.rs` 第 14-19 行的宏属性，加 `tags = "project_management"`：

```rust
#[register_handler_tool(
    id = "create_artifact",
    name = "create_artifact",
    description = "Create a new artifact in a project, supports creating from existing attachment or generated content",
    params = "common::api::CreateArtifactRequest",
    tags = "project_management"
)]
```

- [ ] **Step 4: 验证编译通过**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: 编译通过

- [ ] **Step 5: 提交**

```bash
git add src/handlers/project/artifact/create_artifact.rs
git commit -m "feat(artifact): implement GeneratedContent branch in create_artifact

Replace bail_err stub with actual implementation that creates GeneratedContent
artifacts via Domain layer. Also add project_management tag."
```

---

## Task 9: update_artifact_content 加 tags

**Files:**
- Modify: `src/handlers/project/artifact/update_artifact_content.rs:10-15`

- [ ] **Step 1: 加 tags 属性**

修改 `src/handlers/project/artifact/update_artifact_content.rs` 第 10-15 行的宏属性，加 `tags = "project_management"`：

```rust
#[register_handler_tool(
    id = "update_artifact_content",
    name = "update_artifact_content",
    description = "Fully replace the text content of a generated-content artifact, supports optimistic locking with expected_updated_at",
    params = "common::api::UpdateArtifactContentRequest",
    tags = "project_management"
)]
```

- [ ] **Step 2: 提交**

```bash
git add src/handlers/project/artifact/update_artifact_content.rs
git commit -m "feat(artifact): add project_management tag to update_artifact_content"
```

---

## Task 10: query_artifacts 去 neural 改 tags

**Files:**
- Modify: `src/handlers/project/artifact/query_artifacts.rs:15-21`

- [ ] **Step 1: 替换 neural 为 tags**

修改 `src/handlers/project/artifact/query_artifacts.rs` 第 15-21 行：

```rust
#[register_handler_tool(
    id = "query_artifacts",
    name = "query_artifacts",
    description = "Query artifacts with full filtering support (project_id, task_id, file_type, source_type, etc.)",
    params = "common::api::ArtifactQueryRequest",
    neural
)]
```

替换为：

```rust
#[register_handler_tool(
    id = "query_artifacts",
    name = "query_artifacts",
    description = "Query artifacts with full filtering support (project_id, task_id, file_type, source_type, etc.)",
    params = "common::api::ArtifactQueryRequest",
    tags = "project_management"
)]
```

- [ ] **Step 2: 提交**

```bash
git add src/handlers/project/artifact/query_artifacts.rs
git commit -m "refactor(artifact): change query_artifacts from neural to project_management tag

query_artifacts is no longer always-on neural tool. It is now part of the
project_management tool pack, auto-installed on agent onboarding."
```

---

## Task 11: 路由注册新 handler

**Files:**
- Modify: `src/router.rs`

- [ ] **Step 1: 搜索现有 artifact 路由**

Run: `grep -n "artifact" src/router.rs | head -20`

查看现有 artifact 路由的注册方式，确认新 handler 的路由路径。

- [ ] **Step 2: 注册 create_text_artifact 和 register_artifact_from_path 路由**

根据现有 artifact 路由模式（如 `POST /api/v1/project/artifacts`），添加：

- `POST /api/v1/project/artifacts/text` → `create_text_artifact_handler`
- `POST /api/v1/project/artifacts/register-from-path` → `register_artifact_from_path_handler`

**注意**：具体路径和路由宏的使用方式需要参考 `src/router.rs` 中现有的 artifact 路由注册代码。如果项目使用 `generate_http_handler` 宏自动生成路由，则可能不需要手动注册。

- [ ] **Step 3: 验证编译通过**

Run: `cargo build -p ai_orz 2>&1 | head -30`
Expected: 编译通过

- [ ] **Step 4: 提交**

```bash
git add src/router.rs
git commit -m "feat(router): register create_text_artifact and register_artifact_from_path routes"
```

---

## Task 12: 集成验证

- [ ] **Step 1: 运行全部测试**

Run: `cargo test -p ai_orz --lib 2>&1 | tail -30`
Expected: 所有测试通过

- [ ] **Step 2: 运行 mime_util 测试**

Run: `cargo test -p ai_orz mime_util 2>&1 | tail -10`
Expected: 3 个测试通过

- [ ] **Step 3: 检查 seed 数据同步**

新工具需要在系统启动时同步到 DB。检查 seed 机制是否自动处理：

Run: `grep -rn "register_builtin_factory\|seed.*tool" src/pkg/tool_registry/mod.rs | head -10`

如果 seed 机制自动同步 builtin 工具到 DB，则新工具会被自动注册。否则需要手动更新 seed 配置。

- [ ] **Step 4: 最终编译验证**

Run: `cargo build -p ai_orz 2>&1 | tail -10`
Expected: 编译通过，无 error

- [ ] **Step 5: 提交（如有剩余改动）**

```bash
git add -A
git commit -m "chore: integration verification for artifact creation tools"
```

---

## Self-Review Checklist

### Spec coverage

| 方案改造点 | 对应 Task |
|-----------|----------|
| 改造 1: fs_write Agent 路径隔离 | Task 1 |
| 改造 2: 新增工具 create_text_artifact | Task 6 |
| 改造 3: 新增工具 register_artifact_from_path | Task 7 |
| 改造 4: update_artifact_content 加 tags | Task 9 |
| 改造 5: query_artifacts 去 neural 改 tags | Task 10 |
| 改造 6: Domain 层新增两个方法 | Task 2 + Task 3 |
| 改造 7: 打通 create_artifact GeneratedContent 分支 | Task 8 |
| 改造 8: 现有 artifact 工具 tag 归口 | Task 8 (create_artifact) + Task 9 + Task 10 |
| API DTO | Task 4 |
| mime_util | Task 5 |
| 路由注册 | Task 11 |

### Placeholder scan

- 无 "TBD"、"TODO"、"implement later"
- 每个步骤都有具体代码
- 路由注册（Task 11）因依赖现有路由模式，留有"参考现有代码"指引，这是合理的

### Type consistency

- `CreateTextArtifactParams` 在 Task 4 定义，Task 6 使用 ✓
- `RegisterArtifactFromPathParams` 在 Task 4 定义，Task 7 使用 ✓
- `create_generated_artifact` 在 Task 2 定义，Task 6 和 Task 8 使用 ✓
- `create_generated_artifact_from_file` 在 Task 3 定义，Task 7 使用 ✓
- `mime_util::infer_mime_type` / `infer_file_type` / `basename` 在 Task 5 定义，Task 7 使用 ✓
