# Agent Artifact 创建能力方案（定稿）

> 本文档是已评审定稿的方案，基于此产出详细实施 plan。

## 一、背景与目标

### 1.1 现状问题

- 20 个 neural 工具中只有 `query_artifacts` 一个与 artifact 相关，且只读不写
- `create_artifact` handler 的 `GeneratedContent` 分支显式 `bail_err!(UnsupportedOperation)`
- Agent 无法自行生产文件并注册为 artifact
- `fs_write` 工具没有 Agent 维度路径隔离

### 1.2 目标

1. **fs_write Agent 路径隔离**：每个 Agent 只能写自己的 `agents/{agent_id}/` 目录
2. **文本类产物创建**：Agent 通过工具传 `content` 创建 GeneratedContent artifact
3. **文件类产物注册**：Agent 用 fs_write 写文件到自己目录，再通过工具**复制**文件到 artifact 目录注册元信息
4. **产物更新能力**：Agent 能更新已存在的 artifact 内容（直接覆盖，不做版本）
5. **打通 create_artifact handler 的 GeneratedContent 分支**：HTTP 接口也能创建文本类 artifact
6. **artifact 工具统一归口 project_management 工具包**：去掉 neural flag，Agent 入职自动安装

### 1.3 设计原则

- 文件归属清晰：artifact 创建后文件在 `artifacts/projects/{pid}/{aid}/` 下，artifact 自治
- Agent 目录是"工作台"，产物提交后工作副本保留（复制不移动）
- artifact 目录的写入入口单一：只有 artifact 工具/Domain 层能写入
- 产物更新直接覆盖，不做版本管理（第一版）
- 产物状态保持现有 2 个（正常/已删除）

---

## 二、决策汇总

| # | 决策点 | 结论 |
|---|--------|------|
| 1 | 版本管理 | 第一版不做，直接覆盖 |
| 2 | 产物状态 | 保持现有 2 个状态（正常/已删除） |
| 3 | artifact 工具标签 | `tags = "project_management"`，不加 `neural` |
| 4 | 现有 query_artifacts | 去掉 `neural` flag，改为 `tags = "project_management"` |
| 5 | 创建工具 | 保留两种：`create_text_artifact`（文本）+ `register_artifact_from_path`（文件） |
| 6 | 文件流转方式 | **复制**（不移动），Agent 保留工作副本 |
| 7 | fs_write 路径隔离 | base_path 改为 `agents/{agent_id}/` |
| 8 | fs_read 路径隔离 | **不隔离**，保持全局可读 |
| 9 | 打通 create_artifact handler GeneratedContent 分支 | 做 |
| 10 | update_artifact_content 补工具注册 | 做，`tags = "project_management"`，不加 `neural` |

---

## 三、改造方案

### 3.1 改造 1：fs_write Agent 路径隔离

修改 [fs_write.rs](src/pkg/tool_registry/fs_write.rs) 的 `FsWriteCoreTool::call`：

1. 不再丢弃 ctx，从 `ctx.agent_id()` 获取 agent_id
2. base_path 从全局 `.ai_orz` 改为 `config::get().agent_data_dir(agent_id)`
3. 路径校验逻辑保持现有 `resolve_and_validate_path`，base_path 替换
4. `additional_allowed_paths` 保留，用于特殊场景扩展
5. **fs_read 不改**，保持全局可读

### 3.2 改造 2：新增工具 `create_text_artifact`

**工具定义**：
```rust
#[register_handler_tool(
    id = "create_text_artifact",
    name = "create_text_artifact",
    description = "Create a text-based artifact with content. The content will be saved to artifact storage.",
    params = "common::api::CreateTextArtifactParams",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn create_text_artifact(ctx: RequestContext, params: CreateTextArtifactParams) -> Result<ArtifactDetail>
```

**参数**：
```rust
pub struct CreateTextArtifactParams {
    pub project_id: String,
    pub task_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub file_name: Option<String>,    // 默认从 name 派生（加 .md）
    pub mime_type: Option<String>,    // 默认 text/plain
    pub file_type: Option<FileType>,  // 默认 Document
    pub tags: Option<Vec<String>>,
}
```

**流程**：校验 ctx.agent_id() → 调 Domain `create_generated_artifact` → 返回 ArtifactDetail

### 3.3 改造 3：新增工具 `register_artifact_from_path`

**工具定义**：
```rust
#[register_handler_tool(
    id = "register_artifact_from_path",
    name = "register_artifact_from_path",
    description = "Register an existing file (in agent's directory) as an artifact. The file will be copied to artifact storage.",
    params = "common::api::RegisterArtifactFromPathParams",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn register_artifact_from_path(ctx: RequestContext, params: RegisterArtifactFromPathParams) -> Result<ArtifactDetail>
```

**参数**：
```rust
pub struct RegisterArtifactFromPathParams {
    pub project_id: String,
    pub task_id: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub source_path: String,           // agent 目录下的相对路径
    pub file_name: Option<String>,     // 默认从 source_path 派生 basename
    pub mime_type: Option<String>,     // 默认从扩展名推断
    pub file_type: Option<FileType>,   // 默认从 mime_type 推断
    pub tags: Option<Vec<String>>,
}
```

**流程**：
1. 校验 ctx.agent_id() 存在
2. 计算源文件绝对路径：`agent_data_dir(agent_id).join(source_path)`
3. 路径安全校验：源路径必须在 `agents/{agent_id}/` 之下（防穿越）
4. 校验源文件存在、是文件、可读
5. 推断 mime_type（若未传）
6. 调 Domain `create_generated_artifact_from_file`（**复制**文件到 artifact 目录）
7. 返回 ArtifactDetail

### 3.4 改造 4：新增工具 `update_artifact_content`（neural→project_management）

当前 `update_artifact_content` 是 HTTP only handler，**没有注册为工具**。本次补工具注册：

**方式**：在现有 [update_artifact_content.rs](src/handlers/project/artifact/update_artifact_content.rs) 的函数上加 `#[register_handler_tool(...)]` 宏：

```rust
#[register_handler_tool(
    id = "update_artifact_content",
    name = "update_artifact_content",
    description = "Update the content of a generated_content artifact. Overwrites existing content.",
    params = "common::api::UpdateArtifactContentRequest",
    tags = "project_management"
)]
#[generate_http_handler]
pub async fn update_artifact_content(...) -> Result<...>
```

**注意**：现有函数签名和逻辑保持不变，只加宏属性。

### 3.5 改造 5：调整 `query_artifacts` 的 tag

修改 [query_artifacts.rs](src/handlers/project/artifact/query_artifacts.rs) 的宏属性：

```rust
// 改前
#[register_handler_tool(
    id = "query_artifacts",
    ...
    neural
)]

// 改后
#[register_handler_tool(
    id = "query_artifacts",
    ...
    tags = "project_management"
)]
```

**影响**：query_artifacts 不再无条件常驻，改为通过 `project_management` 工具包注入。Agent 入职自动安装 `project_management`，实际可用性不受影响。

### 3.6 改造 6：Domain 层新增方法

在 [ArtifactManage trait](src/service/domain/project/mod.rs#L329-L426) 新增两个方法：

#### `create_generated_artifact`（文本类）

```rust
async fn create_generated_artifact(
    &self, ctx: RequestContext,
    project_id: String, task_id: Option<String>,
    name: String, description: String,
    content: Vec<u8>, file_name: String,
    mime_type: String, file_type: FileType,
    tags: Vec<String>, created_by: String,
) -> Result<Artifact>;
```

**流程**：
1. `validate_project_and_task` 校验
2. 构造 `Artifact::new_*_with_source_type(..., GeneratedContent, ...)`
   - `FileMeta { file_path: file_name, mime_type, file_size: content.len() }`
3. `set_tags` + `enrich_ctx!`
4. `artifact_dal.create(ctx, &artifact)` 建 DB 记录
5. `artifact_dal.write_content(ctx, &artifact, &content)` 落盘
6. **错误回滚**：若 write_content 失败，调 `delete` 回滚 DB 记录

#### `create_generated_artifact_from_file`（文件类，复制）

```rust
async fn create_generated_artifact_from_file(
    &self, ctx: RequestContext,
    project_id: String, task_id: Option<String>,
    name: String, description: String,
    source_path: PathBuf, file_name: String,
    mime_type: String, file_type: FileType,
    tags: Vec<String>, created_by: String,
) -> Result<Artifact>;
```

**流程**：
1. `validate_project_and_task` 校验
2. 读取源文件大小：`std::fs::metadata(&source_path)?.len()`
3. 构造 `Artifact::new_*_with_source_type(..., GeneratedContent, ...)`
   - `FileMeta { file_path: file_name, mime_type, file_size }`
4. `set_tags` + `enrich_ctx!`
5. `artifact_dal.create(ctx, &artifact)` 建 DB 记录
6. **复制文件**：
   - 计算目标路径：通过 `artifact_dal` 暴露的路径计算方法（或 Domain 层直接用 config 计算）
   - `std::fs::create_dir_all(parent)` 确保目录
   - `std::fs::copy(source_path, target_path)` 复制（agent 原文件保留）
7. **错误回滚**：若 copy 失败，调 `delete` 回滚 DB 记录
8. 返回 artifact

**注意**：Domain 层需要能计算 artifact 的目标存储路径。当前 `resolve_generated_content_path` 在 DAO 层（[sqlite.rs:41-59](src/service/dao/artifact/sqlite.rs#L41-L59)）。两种方案：
- **方案 A**：在 Domain 层用 `config::get().artifact_project_dir(project_id).join(artifact_id).join(file_name)` 直接计算（config 已是公共方法）
- **方案 B**：在 DAL/DAO 层暴露 `resolve_content_path` 方法
- **推荐 A**：避免新增 DAL 方法，config 方法已足够

### 3.7 改造 7：打通 create_artifact handler 的 GeneratedContent 分支

修改 [create_artifact.rs:40-44](src/handlers/project/artifact/create_artifact.rs#L40-L44)：

```rust
ArtifactSourceType::GeneratedContent => {
    let content = params.content
        .ok_or_else(|| err!(InvalidRequest, "content is required for generated_content artifact"))?;
    let file_name = params.file_name
        .ok_or_else(|| err!(InvalidRequest, "file_name is required for generated_content artifact"))?;
    let mime_type = params.mime_type.unwrap_or_else(|| "text/plain".to_string());
    let file_type = params.file_type.unwrap_or(FileType::Document);

    let artifact = domain.artifact_manage()
        .create_generated_artifact(
            ctx, params.project_id, params.task_id,
            params.name, params.description.unwrap_or_default(),
            content.into_bytes(), file_name, mime_type, file_type,
            params.tags.unwrap_or_default(), current_user_id,
        ).await?;

    Ok(to_detail(&artifact))
}
```

HTTP 接口和 neural 工具走同一 Domain 方法。

### 3.8 改造 8：现有 artifact 工具补 `tags = "project_management"`

除 query_artifacts 外，检查现有 artifact handler 是否有遗漏的 tag。根据研究，现有 7 个 artifact handler 都没打 `project_management` tag。

**策略**：
- **query_artifacts**：去掉 `neural`，加 `tags = "project_management"`（改造 5）
- **create_artifact / update_artifact_content / get_artifact / list_artifacts / get_artifact_content / delete_artifact**：这些是 HTTP only handler，**当前没有注册为工具**（没有 `#[register_handler_tool]`）。本次只对 `update_artifact_content` 补工具注册（改造 4），其他保持 HTTP only 不变
- **create_text_artifact / register_artifact_from_path**：新工具，加 `tags = "project_management"`

---

## 四、文件清单

### 新建文件
- `src/handlers/project/artifact/create_text_artifact.rs` — 文本类 artifact 创建工具
- `src/handlers/project/artifact/register_artifact_from_path.rs` — 文件类 artifact 注册工具
- `src/handlers/project/artifact/mime_util.rs` — mime_type 推断工具（基于扩展名）

### 修改文件
- `src/pkg/tool_registry/fs_write.rs` — CoreTool::call 使用 ctx 计算 agent 目录作为 base_path
- `src/handlers/project/artifact/mod.rs` — 注册新 handler 模块
- `src/handlers/project/artifact/create_artifact.rs` — 打通 GeneratedContent 分支
- `src/handlers/project/artifact/update_artifact_content.rs` — 补 `#[register_handler_tool]` 宏
- `src/handlers/project/artifact/query_artifacts.rs` — 去掉 `neural`，加 `tags = "project_management"`
- `src/service/domain/project/mod.rs` — ArtifactManage trait 新增两个方法
- `src/service/domain/project/artifact.rs` — 实现两个新方法
- `common/src/api/artifact.rs` — 新增 `CreateTextArtifactParams` / `RegisterArtifactFromPathParams`

---

## 五、风险与影响

### 5.1 fs_write 路径隔离的破坏性

**高风险**：改造后所有现有 fs_write 调用如果写到非 agent 目录都会失败。
- **缓解**：排查现有调用，必要时在 `additional_allowed_paths` 配置兼容路径
- **测试**：需要更新所有 fs_write 相关测试

### 5.2 query_artifacts 去 neural 的影响

**中风险**：现有依赖 query_artifacts 常驻的 Agent，如果没安装 `project_management` 工具包会失去此工具。
- **缓解**：Agent 入职自动安装 `project_management`，实际影响小
- **测试**：需要验证安装 project_management 后 query_artifacts 仍可用

### 5.3 复制操作的磁盘占用

**低风险**：文件类产物创建后有两份副本（agent 目录 + artifact 目录）。
- **缓解**：产物通常不大；Agent 可主动清理工作副本

### 5.4 错误回滚的最终一致性

**低风险**：建 DB 记录后落盘/复制失败，回滚 DB 记录。回滚本身也可能失败。
- **缓解**：记录错误日志，人工介入排查

---

## 六、下一步

本方案定稿后，用 writing-plans skill 产出详细实施 plan（含 TDD 步骤、具体代码、测试用例）。
