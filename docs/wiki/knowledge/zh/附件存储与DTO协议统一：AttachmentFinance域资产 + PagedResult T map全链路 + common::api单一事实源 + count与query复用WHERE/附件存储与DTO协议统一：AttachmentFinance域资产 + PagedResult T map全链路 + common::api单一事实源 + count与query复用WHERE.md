---
kind: RAG 原子知识卡
name: 附件存储与DTO协议统一：Attachment Finance域统一资产 + PagedResult T map 全链路 + common::api 单一事实源 + count与query 复用 WHERE
category: 基础设施 / DTO协议与资产存储
scope:
  - "src/service/domain/finance/attachment.rs"
  - "src/service/dal/attachment.rs"
  - "src/service/dao/attachment/**"
  - "src/service/domain/project/artifact.rs"
  - "src/service/dal/artifact.rs"
  - "common/src/api/** (所有模块)"
  - "common/src/models/pagination.rs (PagedResult, Pagination, PageQuery)"
source_files:
  - src/service/domain/finance/attachment.rs#L1-L120 (Finance Attachment Domain：通用上传 multipart → 写文件系统 + 写 attachments 表元数据 → 返回 attachment_id；下载按 id + ctx.uid() 校验归属；所有业务域不直接写文件，引用 attachment_id)
  - src/service/dal/attachment.rs (AttachmentDal trait：create_meta / get_by_id（校验 owner_id==ctx.uid）/ list_by_ids（批量查询，IN 分块 400）/ list_by_query（分页）/ count（复用 query WHERE）；文件写路径走相对 content_path 绝对落盘前防穿越)
  - src/service/dao/attachment/mod.rs (AttachmentDao：SQLite attachments 表 CRUD + file_path Blob 数据 + 分块存储；Vectorizable 未启用（附件通常大文件不直接向量化，用 artifact 摘要向量化）)
  - src/service/domain/project/artifact.rs#L1-L80 (Project Artifact Domain：create_artifact = 生成 artifact_id → attachment_id 引用 Finance 已上传的附件 → 写入 project_artifacts 关联表；按 project_id 查询所有 artifact 时用 JOIN attachments 拿文件大小+名称)
  - common/src/models/pagination.rs#L1-L80 (PagedResult<T> { items: Vec<T>, total: u64 } + Pagination(page, page_size) + PageQuery；PagedResult::map<U>(fn item→U) 把 DAO PO → DAL Entity → Domain DTO 全链路转换)
  - common/src/api/project_task.rs#L1-L50 (ProjectTask 模块 DTO：GetTasksRequest / QueryTaskRequest / UpdateTaskProgressRequest / PagedResult<TaskDto>；所有请求参数使用 struct + #[derive(Params) + serde(deny_unknown_fields)])
  - common/src/api/message_send.rs#L1-L40 (消息 DTO 示例：SendMessageRequest + SendMessageResponse；禁止裸 bool/String 响应，必须用结构体包裹)
  - docs/design/attachment_storage.md（§Attachment 归 Finance 域作为用户通用资产 §元数据与文件存储路径解耦 §统一 multipart 上传链路）
  - docs/design/pagination_and_count_convention.md（§query 核心 list 语法糖 §COUNT 与 LIST 复用 push_query_filters §PagedResult::map 全链路）
  - docs/design/api_protocol_convention.md（§common DTO 单一事实源 §请求参数 struct 化 §禁止裸原始类型响应）
  - docs/design/unified-idl-http-handler.md（§统一 Handler 模块命名约定 §一个业务方法一个独立文件 §DTO 仅在 common 定义）
  - docs/plan/项目任务增强.md（§Project/Task Artifact 按需注入 §with_artifacts option 字段）
  - docs/plan/Query接口分页与List接口简化重构.md（§list 接口只接分页 §query 接口接完整过滤 §通用 count 复用 WHERE）
  - docs/plan/批量查询与通用Query接口增强重构.md（§Query 结构体化 §PagedResult 统一返回格式 §IN 列表 400 分块）
  - docs/plan/前端API协议结构重构.md（§前端 DTO 镜像 → 直接从 common::api re-export §禁止 frontend/src/api 本地定义 DTO 重复项）
  - docs/wiki/zh/content/数据模型/项目和任务模型/制品和附件.md（Attachment 通用资产 vs Artifact 项目产物的关联关系 + SQL 结构）
  - docs/wiki/zh/content/架构设计/API协议规范/API协议规范.md（common DTO 单一事实源 + ApiResponse<T> 信封标准）
  - docs/wiki/zh/content/架构设计/API协议规范/分页与计数规范.md（query 核心 vs list 语法糖 + count 通用 WHERE 复用）
  - docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/财务领域.md（Finance 域组织图：attachment + identity_credential + mcp_server 三个子能力）
  - docs/wiki/zh/content/数据模型/系统模型/文件存储模型.md（统一文件存储：data_dir / attachments / agents / projects 三个目录 + 相对路径安全）
  - 【平行卡 1】docs/wiki/knowledge/zh/任务状态机与项目聚合：TaskStatus 4 态 + progress 0-100 自动联动 + execution_plan_result JSON Patch + TaskGraph 依赖 DAG/任务状态机与项目聚合：TaskStatus 4 态 + progress 0-100 自动联动 + execution_plan_result JSON Patch + TaskGraph 依赖 DAG.md（Project 详情聚合 inject_artifacts → 调 Project Artifact Domain list → 展示产物；Attachment 下游消费方）
  - 【平行卡 2】docs/wiki/knowledge/zh/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）/三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）.md（Attachment 不向量化，但 Artifact 摘要内容走 Task 域 ArtifactDao.search → 混合搜索）
---

## §1 概述

**本卡角色**：Attachment 附件资产、Artifact 项目产物、PagedResult 分页、common::api DTO 单一事实源的综合规范卡。**本卡是「写任何 Handler/DAL 必须先读」的规范卡，集中了分页/DTO/附件三大强制规范。** 覆盖 Finance Domain Attachment 作为通用资产（所有业务域引用 attachment_id，不直接写文件）、PagedResult<T> 泛型的三层 T map 转换（DAO PO → DAL Entity → Domain DTO/Response）、common crate 是 DTO 唯一事实源（前端/后端 Handler/DAL/Domain 全从 common::api 导入，禁止本地定义）、count 与 query 复用同一套 push_query_filters（防止 COUNT 与 LIST WHERE 条件漂移导致分页 total 与实际列表不一致）。

- **Attachment 归 Finance 域作为「用户通用资产」**（attachment_storage.md §核心设计决策 0）：上传文件是跨 Skill/Message/Project/Tool 的通用能力，不归任何业务域独有，Handler 必须严格按 `handler → finance domain → attachment dal → attachment dao` 分层；所有业务域只引用「已上传的 attachment_id」。上传链路：`POST /api/v1/finance/attachments/upload` multipart/form-data → Finance Attachment Domain.upload → AttachmentDal.create_meta + AttachmentDao.write_file（data_dir/attachments/{date}/{uuid}_{filename}，路径严格 canonicalize 前缀匹配 data_dir，防 `..` 穿越）→ 返回 {attachment_id, file_name, size, content_type}。业务域的创建接口（消息发送附件 / 项目 Artifact / Skill 大文件）body 里只带 `attachment_id: String`，不接受 multipart 流。
- **PagedResult<T> 全链路 map（三层统一）**（pagination_and_count_convention.md §规范）：统一签名是 `query(ctx, query: Query) -> Result<PagedResult<T>>`。DAO 层返回 `PagedResult<TaskPo>` → DAL 层 `.map(|po| TaskEntity { po })` → Domain 层再 `.map(|entity| TaskDto::from(entity))` → Handler 层直接作为 ApiResponse.data 返回（不用再手工改 items/total）。page_size 默认 20，上限 100（防止 page_size=10000 触发 OOM）。
- **common crate DTO 单一事实源（禁止 Handler/DAL/Frontend 本地重复定义 DTO）**（api_protocol_convention.md §3 铁律）：所有请求结构体（`CreateProjectRequest` / `UpdateTaskProgressRequest`）必须在 `common/src/api/{域}.rs` 中定义；所有 Handler 的请求签名统一 `Query<CreateProjectRequest>` / `Json<UpdateTaskProgressRequest>`；禁止 `Query<HashMap<String, String>>` / `Json<Value>`（类型丢失导致字段漂移）。前端 API 客户端通过 `pub use common::api::*;` 直接 re-export（frontend/src/api/mod.rs），禁止本地镜像 DTO 结构。响应结构体即便只有 1 个字段也要结构体包裹：`CreateProjectResponse { project_id: String }`，禁止 `ApiResponse<String>` / `ApiResponse<bool>`（裸原始类型）。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| domain/finance/attachment.rs Finance Attachment | 通用上传下载 | upload(ctx, multipart)：文件大小上限 100MB（可配置）→ 分块读 stream → AttachmentDal.write_file + create_meta → 返回 attachment_id；download(ctx, id)：先 get_by_id 校验 owner==ctx.uid → 打开文件流式响应 (StreamBody<Bytes>)；delete(ctx, id)：软删 status=0 + 文件系统异步清理（30 天后 GC 任务） | `:L1-L120` |
| dal/attachment.rs AttachmentDal trait | 业务级数据操作 | create_meta / get_by_id / list_by_ids（batch IN，每 400 个一批防 SQLite 999 溢出）/ query / count / write_file / delete_file；count 与 query 调用共享内部 push_query_filters(query, sql, args) | 见 AttachmentDal trait |
| dao/attachment/mod.rs AttachmentDao | SQLite CRUD + 路径安全 | attachments 表 12 字段（id/owner_id/file_name/content_type/size/content_path/status/md5_hash/created_at 等）；content_path 相对路径绝对拼接时 canonicalize + starts_with(data_dir)，绝对禁止直接写 `/root/passwd` 绕过 | 见 dao/attachment trait + sqlite impl |
| domain/project/artifact.rs Project Artifact | 产物聚合层 | create_artifact(ctx, project_id, attachment_id, description, kind)：先查附件存在且 owner == ctx.uid → 写 project_artifacts 关联表；get_project_detail 中 with_artifacts=true → JOIN attachments 返回含名称/大小 | `:L1-L80` |
| common/src/models/pagination.rs 分页统一类型 | PagedResult 泛型 | PagedResult { items, total } + impl<T> PagedResult { pub fn map<U>(self, f: impl FnMut(T) -> U) -> PagedResult<U> }（内部对 items 应用 f，total 不变）；Pagination { page: u32, page_size: u32 } + Default::default() = (1, 20) | `:L1-L80` |
| common/src/api/project_task.rs DTO 定义示例 | Task 域请求响应 | GetTasksRequest / QueryTaskRequest / UpdateTaskProgressRequest / GetTaskDetailResponse / PagedResult<TaskDto>；所有结构体 `#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Params)]`（Params 是 axum 提取器）+ `#[serde(deny_unknown_fields)]`（传未知字段直接 400，前端字段名漂移早发现） | `:L1-L50` |
| common/src/api/message_send.rs DTO 示例 | 消息域 DTO | SendMessageRequest { target_kind: TargetKind, target_id: String, content: String, attachments: Vec<String> }；响应 SendMessageResponse { message_id: String, delivery_warning: bool }；即便只有 1 字段也结构体不裸 String | `:L1-L40` |

**章节来源**
- [finance/attachment.rs:L1-L120](src/service/domain/finance/attachment.rs#L1-L120)
- [pagination.rs:L1-L80](common/src/models/pagination.rs#L1-L80)
- [attachment_storage.md:L15-L50](docs/design/attachment_storage.md#L15-L50)

---

## §3 query 与 count 复用 WHERE 的强制规范

以 Task 为例（分页和计数规范 §三层统一 count 透传）：

```
// DAO 层（dao/task/sqlite.rs）
fn push_query_filters(query: &TaskQuery, sql: &mut SqlBuilder, args: &mut Vec<Arg>) {
    if let Some(status) = query.status {
        sql.push(" AND status = ?");
        args.push(Arg::Int(status as i64));
    }
    if let Some(project_id) = &query.project_id {
        sql.push(" AND project_id = ?");
        args.push(Arg::Text(project_id.clone()));
    }
    if !query.tags_contains.is_empty() {
        sql.push(" AND json_array_length(tags) > 0"); // 标签过滤简化
    }
    // 软删除默认过滤
    sql.push(" AND status != 0");
}

async fn query(&self, ctx, query) -> Result<PagedResult<TaskPo>> {
    let mut sql = SqlBuilder::new("SELECT * FROM tasks WHERE 1=1");
    let mut args = vec![];
    push_query_filters(&query, &mut sql, &mut args);
    // ORDER + LIMIT/OFFSET
    sql.push(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
    args.push(query.pagination.page_size as i64);
    args.push(query.pagination.offset() as i64);
    let items = sqlx::query_as_with(&sql.build(), ...).fetch_all(...).await?;
    // count 用同一套
    let total = self.count(ctx, query).await?;
    Ok(PagedResult { items, total })
}

async fn count(&self, ctx, query) -> Result<u64> {
    let mut sql = SqlBuilder::new("SELECT COUNT(*) FROM tasks WHERE 1=1");
    let mut args = vec![];
    push_query_filters(&query, &mut sql, &mut args); // 完全同一函数！防止 WHERE 条件漂移
    Ok(sqlx::query_scalar_with(&sql.build(), args).fetch_one().await? as u64)
}
```

**禁止反模式 3 种（Grep 代码可以定位违规）**：① count 方法独立拼 WHERE，不调用 push_query_filters（会导致 count 返回 100 但 list 只有 50）；② Handler 层取 list 后用 `items.len()` 当 total（total 永远 ≤ page_size）；③ 新增过滤条件只改 query 方法忘改 count 方法。正确做法：所有过滤代码集中 push_query_filters 一个函数，count 和 query 都调它，单元测试对每个过滤字段必须同时 assert query + count。

---

## §4 硬约束与回归红线（8 条，本卡规范类卡数量多）

1. **任何业务域不得直接写文件系统**：Handler 直接 `std::fs::write(...)` → fail；Skill/Message/Project/Tool 想上传附件必须先走 Finance::Attachment Domain 的 upload 接口，拿到 attachment_id。（Grep 全局 `std::fs::write` / `tokio::fs::write`，只允许 dao/attachment/sqlite.rs 和 pkg/storage 里出现）
2. **Attachment 路径穿越双校验**：AttachmentDal.get_by_id 前 + AttachmentDao.write_file 前都 `canonicalize(path)` → `starts_with(CTX.data_dir)`，任一不通过 → 403 "非法路径"；单元测试用 `data_dir/../../etc/passwd` 作为 filename，必须写入时被拦截（403）。
3. **PagedResult 的 total 必须来自 COUNT 独立查询，不是 items.len()**：Handler/DAL 层对 query 结果返回 PagedResult，不能把 Vec 转成 PagedResult 填 len 当 total；Grep 代码 `total: items.len() as u64` = 违反（仅在测试 mock 场景允许，需 TODO 注释）。
4. **page_size 上限 100，默认 20**：common pagination.rs Pagination::validate() 中 page_size > 100 自动 clamp 到 100，不 panic 不报错；测试分页性能时设置 page_size=10000 自动变 100。
5. **common::api 是 DTO 单一事实源（单一真相源）**：
   - ① 禁止在 `src/handlers/**` 本地定义 Request/Response 结构体（Grep `struct XxxRequest` 在 src/handlers = 违反；Grep `struct XxxResponse` 在 src/handlers = 违反）
   - ② 禁止在 `frontend/src/api/` 本地镜像定义重复的 DTO（允许 re-export，不允许 struct 定义重复）
   - ③ Request 结构体必须 `#[derive(Params)]` + `#[serde(deny_unknown_fields)]`（前端传错字段名立即 400，防字段名漂移）
6. **API 响应禁止裸原始类型**：即便 Handler 只返回 1 个布尔值，也要用 `ApiResponse<ActionSuccessResponse>` 包裹，ActionSuccessResponse { success: bool, message: Option<String> }；ApiResponse<bool> / ApiResponse<String> / ApiResponse<()> 禁止出现在 Handler 签名中（统一 Error 与 data 结构）。
7. **count 与 query WHERE 条件 100% 共享 push_query_filters 代码**：DAO 层新增过滤字段（例如 TaskQuery 加 due_at_before），必须同时改 query 的 push_query_filters 和 count（但它们都调同一个函数，所以新增字段只改一处，测试用例要验证 count == real items total 对过滤字段也成立）。
8. **IN 列表查询 400 分块**（批量查询规范，与其他卡同）：`list_by_ids(&self, ctx, ids: Vec<String>)` 内部 chunk 400（SQLite 参数上限 999，取 400 留余量）→ 每块 SELECT ... WHERE id IN (?, ?, ...) → 结果 collect 合并；ids 长度 = 1500 → 分 4 次查询，不会因 SQLITE_MAX_VARIABLE_NUMBER 999 溢出 panic。
