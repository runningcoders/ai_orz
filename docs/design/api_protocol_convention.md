# 前后端 API 协议规范（common DTO 单一事实源）

> 🎯 **本文档定位**：前后端共享 API 契约规范——common 作为单一事实源、枚举/DTO 对齐、禁止裸类型返回、DAL 结构体不泄漏
> 状态：定稿（2026-08-09 决策定稿，协议漂移修复已落地）
> 查阅场景：新增 HTTP 接口 DTO、排查前后端枚举不一致、检查接口响应是否符合裸类型禁用红线时打开；具体 DTO 定义直接看 common/src/api/
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构（Handler 层约束）
> - [pagination_and_count_convention.md](./pagination_and_count_convention.md) — 分页与通用 count 查询接口规范（协议的子集）
> - 【② Plan 落地】[前端API协议结构重构.md](../plan/前端API协议结构重构.md) — 前端 DTO 镜像 → common::api 直接 re-export
> - 【③ Wiki 长文】[API协议规范.md](docs/wiki/zh/content/架构设计/API协议规范/API协议规范.md) — common DTO 单一事实源 + ApiResponse 信封标准
> - 【④ RAG 卡】[附件存储与DTO协议统一](docs/wiki/knowledge/zh/附件存储与DTO协议统一：AttachmentFinance域资产%20+%20PagedResult%20T%20map全链路%20+%20common%3A%3Aapi单一事实源%20+%20count与query复用WHERE/附件存储与DTO协议统一：AttachmentFinance域资产%20+%20PagedResult%20T%20map全链路%20+%20common%3A%3Aapi单一事实源%20+%20count与query复用WHERE.md) — §1 common crate DTO 单一事实源 §红线 5/6 本地 DTO 禁用 + 裸类型禁用

> 决策时间：2026-08-09
> 背景：E2E 调试与代码盘点发现前后端协议漂移风险——部分接口 DTO 在 `frontend/src/api/` 与 `common/src/api/` 双份定义、部分接口返回裸原始类型（bool/()）、部分 DAL 内部结构体直接泄漏到 HTTP API。任一侧单独演进都会造成线上协议不一致（UserRole 枚举歧义已造成真实缺陷）。

## 核心原则

**`common` crate 是前后端 API 协议的单一事实源（Single Source of Truth）。**

所有 HTTP 接口的请求/响应结构体、共享枚举，必须且只能定义在 `common/src/api/` 与 `common/src/enums/`，由后端 handler 与前端 api 客户端共同引用，编译期对齐，禁止双份定义。

## 强制规则

### 规则 1：禁止裸原始类型响应

后端 handler 即便只返回一个字段，也必须使用标准 Response 结构体。`#[generate_http_handler]` 宏会将 `Result<T>` 包装为 `ApiResponse<T>` 信封（code/message/data），`data` 内不允许出现裸值。

```rust
// ❌ 禁止：data 是裸 bool / 裸 () / 裸 String
pub async fn check_initialized(...) -> Result<bool> { ... }
pub async fn delete_backup(...) -> Result<()> { ... }

// ✅ 正确：标准 Response 结构体，字段语义自解释
pub async fn check_initialized(...) -> Result<CheckInitializedResponse> { ... }
// pub struct CheckInitializedResponse { pub initialized: bool }
pub async fn delete_backup(...) -> Result<DeleteBackupResponse> { ... }
// pub struct DeleteBackupResponse { pub success: bool }
```

例外：`/health` 探活纯文本、备份恢复脚本 `text/plain` 下载等文件/探活类端点不走 ApiResponse 信封，属显式例外。

### 规则 2：DTO 只定义在 common

- 新增接口的 Request/Response 结构体一律先定义在 `common/src/api/<域>.rs`，再被 handler 与前端引用。
- 禁止在 `frontend/src/api/*.rs` 本地镜像后端结构体（历史上 log_stats/system 域曾各镜像 17 个结构体，已收敛）。
- 禁止 handler 直接返回 DAL/Domain 内部结构体（历史上 BackupInfo/LogEntry/LogPageResult 曾从 DAL 泄漏到 API）。若 DAL 需要该类型，DAL re-export common 定义保持单一来源。

### 规则 3：共享枚举禁止数字比较

权限等语义判断必须使用枚举提供的方法（如 `UserRole::has_permission` / `find_root`），禁止 `role == 0` / `role >= 2` 之类的数字大小比较——SuperAdmin=0 的取值曾与「未恢复哨兵 role==0」语义正面相撞，引发超管 `/user/me` 请求风暴与角色解析错误（E2E-2/E2E-4）。

### 规则 4：前端兼容导入路径

前端既有 `crate::api::system::*` 等导入路径较多时，api 模块内用 `pub use common::api::{...}` re-export 保持路径可用，但类型本体必须在 common。注意 frontend 是 bin crate：无人按名引用的 re-export 会触发 unused import（-D warnings），只 re-export 实际被引用的类型。

## 命名约定

| 类型 | 命名 | 示例 |
|------|------|------|
| 请求参数 | `<Action><Entity>Request` | `QueryLogsRequest` |
| 响应体 | `<Action><Entity>Response` | `QueryLogsResponse` |
| 列表项 | `<Entity>ListItem` / `<Entity>Item` | `TaskListItem` |
| 分页包装 | `PagedResult<T>`（offset 分页统一用 common 的 `PaginationParams` + `PagedResult`） | `PagedResult<TaskListItem>` |
| 无业务字段的响应 | `<Action><Entity>Response { success: bool }`，不用 `()` | `DeleteBackupResponse` |

## 本次收敛记录（2026-08-09）

| 改造项 | 前 | 后 |
|--------|----|----|
| `initialize/check` | `Result<bool>` 裸返回 | `CheckInitializedResponse { initialized }` |
| `delete_backup` / `delete_skill` | `Result<()>` | `DeleteBackupResponse` / `DeleteSkillResponse { success }` |
| `BackupInfo` | DAL 私有结构泄漏 API | 定义移入 `common::api::system`，DAL re-export |
| `LogEntry` / `LogPageResult` | DAL 私有结构泄漏 API | 定义移入 common（`LogEntry` + `QueryLogsResponse`），`raw` 统一为 `Option<Value>` 消除双端漂移 |
| 前端 `api/log_stats.rs` / `api/system.rs` | 本地镜像 17 个结构体 | 全部删除，re-export common 定义 |
| `common::api::user.rs` 死定义 `EmptyResponse{code,message}` | 与 mod.rs 同名结构体并存 | 删除（无引用） |

## 关联文档

- [docs/LAYERED_ARCHITECTURE_PRACTICE.md](../LAYERED_ARCHITECTURE_PRACTICE.md) — 分层架构实践（Handler 属适配层，DTO↔Command 转换职责）
- [docs/design/NAMING_CONVENTION.md](./NAMING_CONVENTION.md) — 全项目命名约定
