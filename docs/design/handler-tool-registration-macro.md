# 工具注册宏设计文档

## 设计目标

将现有的 HTTP Handler 直接注册为内置工具，复用 Handler 已有的：
- 完整权限校验
- 参数解析逻辑
- 错误处理
- 业务逻辑

保证 HTTP API 和工具调用行为一致，降低重复开发量，便于扩展。

## 架构设计

### 整体结构

```
ai-orz-macros/ (独立 proc-macro crate)
└── src/lib.rs
    └── register_handler_tool - 属性宏实现

src/pkg/tool_registry/handler_adapter/
├── mod.rs
│   ├── HandlerFn trait - 处理函数 trait
│   ├── GenericHandlerFn - 泛型实现
│   ├── HandlerToolAdapter - 适配器实现 CoreTool
│   └── 重导出宏
└── macros.rs
    └── 重导出 ai-orz-macros::register_handler_tool
```

### 核心 Trait

```rust
/// Trait for handler functions that can be adapted to CoreTool
///
/// This trait is implemented for handler functions that:
/// - Take `RequestContext` + parsed parameters
/// - Return `Result<Value, AppError>`
/// - `T` is `Serialize` for JSON response
#[async_trait]
pub trait HandlerFn<Params>: Send + Sync + DynClone
where
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    async fn call(&self, ctx: RequestContext, params: Params) -> Result<Value, AppError>;
}

dyn_clone::clone_trait_object!(<Params> HandlerFn<Params> where Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static);
```

### 适配器结构

```rust
/// Adapter that converts a Handler to CoreTool
#[derive(Clone)]
pub struct HandlerToolAdapter<Params>
where
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    po: ToolPo,
    parameters_schema: Value,
    inner: Box<dyn HandlerFn<Params>>,
    _phantom: PhantomData<Params>,
}
```

`CoreTool` 实现：

```rust
#[async_trait]
impl<Params> CoreTool for HandlerToolAdapter<Params>
where
    Params: for<'de> Deserialize<'de> + Serialize + Send + Sync + Clone + 'static,
{
    async fn call(&self, mut ctx: RequestContext, args: Value) -> Result<Value, ToolError> {
        // Parse JSON args to Params type
        let params: Params = match serde_json::from_value(args) {
            Ok(p) => p,
            Err(e) => {
                return Err(ToolError::JsonError(e));
            }
        };

        match self.inner.call(ctx, params).await {
            Ok(result) => Ok(result),
            Err(app_error) => Err(ToolError::ToolCallError(app_error.to_string().into())),
        }
    }

    fn po(&self) -> &ToolPo {
        &self.po
    }
}
```

## 属性宏语法

### 使用方式

```rust
#[register_handler_tool(
    id = "list_skill_files",
    name = "list_skill_files",
    description = "List all files in a skill",
    params = "common::api::ListSkillFilesParams",
)]
async fn list_skill_files_handler(ctx: RequestContext, params: ListSkillFilesParams) -> Result<Value, AppError> {
    // implementation...
}
```

### 参数说明

| 参数 | 必须 | 说明 | 示例 |
|------|------|------|------|
| `id` | 是 | 工具唯一 ID | `id = "list_skill_files"` |
| `name` | 是 | 工具显示名称 | `name = "list_skill_files"` |
| `description` | 是 | 工具描述（给 LLM 看） | `description = "List all files in a skill"` |
| `params` | 是 | 参数类型完整路径 | `params = "common::api::ListSkillFilesParams"` |
| `neural` | 否 | 标记为神经工具（无值，出现即为 true）。唤醒 Agent 时自动注入 Prompt | `neural` |
| `tags` | 否 | 工具标签（单字符串，逗号分隔多个） | `tags = "project_management"` |

### 宏生成代码

宏会自动生成：

1. **工厂结构体**：`{ID_TO_UPPER}_FACTORY` 例如 `LIST_SKILL_FILES_FACTORY`
2. **实现 `BuiltinToolFactory`**：
   - `create_po()` - 创建 ToolPo，自动从参数生成 JSON Schema
   - `create()` - 创建 `HandlerToolAdapter` 实例
3. **自动注册**：使用 `ctor` 宏在程序启动时自动注册到全局注册表

## 使用流程

### 新 Handler 注册为工具

1. 按项目约定实现 Handler 核心逻辑，签名为：
   ```rust
   async fn handler_name(ctx: RequestContext, params: ParamsType) -> Result<Value, AppError>
   ```
   > 注意：原 HTTP handler 的提取 path/query 参数部分已经在 handler 入口处理了，核心逻辑就是接收 `RequestContext` + 解析好的参数结构体

2. 添加属性宏：
   ```rust
   #[register_handler_tool(
       id = "handler_id",
       name = "handler_name",
       description = "description for LLM",
       params = "path::to::ParamsType",
   )]
   ```

3. 编译完成！宏自动生成工厂和注册代码，无需其他操作。

### 现有 Handler 改造为工具

现有 HTTP Handler 需要做一点小重构：

1. **拆分核心逻辑**：将原来 axum extractor 后的核心逻辑抽出来，变成：
   ```rust
   // 新：核心逻辑，供工具调用
   pub async fn list_skill_files(
       ctx: RequestContext,
       params: ListSkillFilesParams,
   ) -> Result<Value, AppError> {
       // ... 原有逻辑 ...
   }

   // 保留：HTTP 入口，调用核心逻辑
   pub async fn list_skill_files_handler(
       State(state): State<AppState>,
       Path(params): Path<ListSkillFilesParams>,
       Extractor(ctx): Extractor<RequestContext>,
   ) -> Response {
       let params = ListSkillFilesParams::from(params);
       list_skill_files(ctx, params).await.into()
   }
   ```

2. 在抽出来的核心逻辑上添加 `#[register_handler_tool]` 属性宏即可。

这样 HTTP 和工具共享同一份核心逻辑，保证行为一致性。

## 错误处理

| 场景 | 错误来源 | 转换方式 |
|------|----------|----------|
| JSON 参数解析失败 | `serde_json::from_value` | `ToolError::JsonError(e)` |
| Handler 业务错误 | `AppError` | `ToolError::ToolCallError(app_error.to_string().into())` |

## JSON Schema 生成

利用 `schemars::schema_for!` 自动从 `Params` 类型生成 JSON Schema，不需要手写。

## 优势

1. **零重复代码**：核心逻辑一份，HTTP 和工具共用
2. **一致性**：权限校验、错误处理行为一致
3. **自动注册**：加个属性宏就完事，不需要手动改注册表
4. **类型安全**：参数解析由 serde 完成，编译时检查
5. **参数文档自动生成**：JSON Schema 自动从类型生成

## 待验证

- [ ] 实际测试一个现有 Handler 改造，验证可用性
- [ ] 确认 `ctor` 全局注册是否正常工作
- [ ] 确认 Rig 调用流程是否能正常调用

## 更新记录

| 日期 | 更新内容 | 作者 |
|------|----------|------|
| 2026-06-21 | 初始设计文档完成 | AI Orz |
| 2026-07-25 | 补充 neural/tags 参数说明；新增「工具注册规范」与「工具注册统计」章节 | AI Orz |

---

## 工具注册规范（2026-07-25 治理后）

### 哪些 handler 应该注册为工具

工具是给 Agent 使用的，判定原则：

| 类别 | 是否注册 | 说明 |
|------|---------|------|
| **业务 CRUD / 查询 / 协作类** | ✅ 注册 | Agent 完成任务的基础能力（项目管理、消息发送、技能管理、工具调用等） |
| **资源发现类** | ✅ 注册 + neural | Agent 唤醒时需感知可用资源（query_tools、query_memory、search_skill 等） |
| **认证 / 会话类**（login、logout、get_user_by_username） | ❌ 不注册 | 非工具语义，且存在用户枚举风险 |
| **敏感字段管理类**（create_user、update_user 涉及 password_hash） | ❌ 不注册 | 安全风险，仅管理员手动调用 |
| **高危删除类**（delete_user、delete_organization） | ❌ 不注册 | 破坏性操作，仅管理员手动调用 |
| **调试类**（debug_call_tool 绕过 Agent 授权） | ❌ 不注册 | admin only，不应暴露给 Agent |
| **系统运维类**（backup、cron_trigger、health、aop、logs） | ❌ 不注册 | 运维操作非业务工具 |
| **SSE / 事件流类**（subscribe_sse） | ❌ 不注册 | 长连接不适合工具请求-响应模型 |
| **A2A 协议回调类**（agent_card、callback、send_task） | ❌ 不注册 | 对外协议接口非 Agent 内部工具 |

### neural 标记使用原则

neural 意味着「Agent 唤醒时自动注入 Prompt」，**仅用于资源发现与感知类工具**：

| 类型 | 是否 neural | 示例 |
|------|------------|------|
| 资源发现 / 检索类 | ✅ neural | query_tools、list_tools、query_model_providers、query_message_channels、query_users、query_artifacts、query_memory、search_memory、search_skill、search_skills、list_messages |
| 记忆写入类 | ✅ neural（保留） | save_short_term_memory、save_long_term_memory、settle_memory、update_memory、delete_memory |
| 消息发送类 | ✅ neural（保留） | send_message、send_task_assignment_message、send_tool_call_message |
| 工具调用类 | ✅ neural（保留） | request_tool_call |
| CRUD / 配置管理类 | ❌ 非 neural | create_project、update_task、bind_tool_to_agent 等 |

### tags 标签体系

| tag | 覆盖域 | 说明 |
|-----|--------|------|
| `project_management` | project/project、project/task | 项目与任务管理 |
| `collaboration` | hr/agent（协作相关）、send_message_to_agent | Agent 间协作 |
| `tool_management` | finance/tool、hr/agent（tool pack 系列） | 工具管理与调用 |
| `skill_management` | hr/skill、hr/agent（skill pack 系列） | 技能管理 |
| `messaging` | finance/message | 消息收发 |
| `file_management` | finance/attachment | 文件资产管理 |

---

## 工具注册统计（2026-07-25 治理后）

### 总体数据

| 指标 | 治理前 | 第一轮治理后 | 第二轮扩展后 |
|------|--------|-------------|-------------|
| 总 handler 函数数 | 161 | 161 | 161 |
| 注册为工具的 handler 数 | 110 | 106（-6 移除 +2 补齐） | **118**（+12 扩展） |
| 未注册为工具的 handler 数 | 51 | 55 | **43** |
| neural 工具数 | 18 | 20（+2 新增） | **20** |
| 有 tags 的工具数 | 20 | 52（+32 新增） | **64**（+12 新增） |

### 本次治理变更明细

#### 1. 移除工具注册（6 个，安全敏感 / 高危 / 调试类）

| 文件 | 移除理由 |
|------|----------|
| finance/tool/debug_call_tool.rs | admin only，绕过 Agent 授权 |
| organization/user/get_user_by_username.rs | 用于认证，用户枚举风险 |
| organization/user/create_user.rs | 涉及 password_hash 敏感字段 |
| organization/user/update_user.rs | 涉及 password_hash 敏感字段 |
| organization/user/delete_user.rs | 高危删除操作 |
| organization/organization/delete_organization.rs | 高危删除操作 |

#### 2. 新增 neural 标记（2 个，资源发现类）

| 文件 | 理由 |
|------|------|
| hr/skill/search_skills.rs | 与单数 search_skill(neural) 语义相近，保持一致 |
| finance/message/list_messages.rs | Agent 对话时需检索历史消息，与 query_memory(neural) 语义类似 |

#### 3. 补齐工具注册（2 个，遗漏的一致性补齐）

| 文件 | tags | 理由 |
|------|------|------|
| project/task/list_tasks.rs | project_management | 与同域 list_project_tasks、list_agent_tasks 一致 |
| project/task/query_tasks.rs | project_management | 与同域 query_projects、query_artifacts 一致 |

#### 4. description 规范化（4 个）

| 文件 | 变更 |
|------|------|
| hr/agent/get_reception_agent.rs | 中文 description 改为英文，与全项目一致 |
| finance/message/send_message.rs | 补充参数、场景、副作用说明 |
| finance/tool/send_tool_call_message.rs | 补充异步语义、返回值、结果送达方式说明 |
| project/task/mark_done.rs | 补充状态转换、失败条件、使用场景说明 |

#### 5. tags 批量补充（32 个文件）

| 域 | tag | 文件数 |
|----|-----|--------|
| finance/tool | tool_management | 12（含 send_tool_call_message） |
| hr/skill | skill_management | 13 |
| finance/message | messaging | 3（含 send_message） |
| 单独补充 | — | 4（send_message→messaging、send_tool_call_message→tool_management、list_tasks→project_management、query_tasks→project_management） |

### 最终工具分布（118 个）

#### 按业务域

| 业务域 | 工具数 | neural | tags |
|--------|--------|--------|------|
| finance/attachment | 6 | 0 | file_management×6 |
| finance/mcp_server | 6 | 0 | - |
| finance/mcp_tool | 2 | 0 | - |
| finance/message | 5 | 3（list_messages、send_message、send_task_assignment_message） | messaging×4、collaboration×1 |
| finance/message_channel | 8 | 1（query_message_channels） | - |
| finance/model_provider | 8 | 1（query_model_providers） | - |
| finance/tool | 13 | 4（list_tools、query_tools、request_tool_call、send_tool_call_message） | tool_management×13 |
| hr/agent | 24 | 7（query_memory、search_memory、save_short_term_memory、save_long_term_memory、settle_memory、update_memory、delete_memory） | collaboration×4、skill_management×3、tool_management×3 |
| hr/skill | 13 | 2（search_skill、search_skills） | skill_management×13 |
| organization/organization | 3 | 0 | - |
| organization/organization_me | 2 | 0 | - |
| organization/user | 2 | 1（query_users） | - |
| project/artifact | 7 | 1（query_artifacts） | - |
| project/project | 6 | 0 | project_management×6 |
| project/task | 10 | 0 | project_management×10 |
| user/profile | 2 | 0 | - |

#### neural 工具清单（20 个）

**资源发现 / 检索类（11 个）**：query_tools、list_tools、query_model_providers、query_message_channels、query_users、query_artifacts、query_memory、search_memory、search_skill、search_skills、list_messages

**记忆写入类（5 个）**：save_short_term_memory、save_long_term_memory、settle_memory、update_memory、delete_memory

**消息发送类（3 个）**：send_message、send_task_assignment_message、send_tool_call_message

**工具调用类（1 个）**：request_tool_call

#### 未注册为工具的 handler（43 个，按类型）

| 类型 | 数量 | 代表 |
|------|------|------|
| A2A 协议回调 | 6 | agent_card、callback、send_task |
| finance/attachment（upload_attachment，Multipart 不支持宏） | 1 | upload_attachment |
| SSE / 事件流 | 1 | subscribe_sse |
| 认证 / 会话 | 2 | login、logout |
| 系统初始化 | 2 | check_initialized、initialize_system |
| 备份恢复 | 4 | create_backup、restore_backup |
| 定时触发器 | 7 | create_cron_trigger、pause_cron_trigger |
| 健康检查 / 指标 / AOP 监控 | 9 | health、health_metrics、aop、aop_stats |
| 日志查询 | 3 | query_logs、log_stats |
| Embedding 运维 | 2 | switch_embedding、rebuild_progress |
| 已移除的安全敏感 handler | 6 | debug_call_tool、create_user、delete_user 等 |

### 第二轮扩展变更明细（2026-07-25）

#### 6. Agent Pack 系列注册（6 个）

| 文件 | tags | 理由 |
|------|------|------|
| hr/agent/install_skill_pack.rs | skill_management | Agent 批量安装技能包 |
| hr/agent/install_tool_pack.rs | tool_management | Agent 批量安装工具包 |
| hr/agent/list_installed_skill_packs.rs | skill_management | 查询已安装技能包 |
| hr/agent/list_installed_tool_packs.rs | tool_management | 查询已安装工具包 |
| hr/agent/uninstall_skill_pack.rs | skill_management | 卸载技能包 |
| hr/agent/uninstall_tool_pack.rs | tool_management | 卸载工具包 |

#### 7. Attachment 系列重构注册（6 个，排除 upload_attachment）

| 文件 | tags | 改造内容 |
|------|------|---------|
| finance/attachment/get_attachment.rs | file_management | 老式 axum handler → generate_http_handler |
| finance/attachment/get_attachment_content.rs | file_management | 同上 |
| finance/attachment/delete_attachment.rs | file_management | 同上 |
| finance/attachment/update_attachment_content.rs | file_management | 同上（path + body 组合） |
| finance/attachment/list_attachments.rs | file_management | 同上（query + flatten pagination） |
| finance/attachment/create_text_attachment.rs | file_management | 同上 |

**改造涉及**：
- `common/src/api/attachment.rs`：新增 4 个 Request 类型（GetAttachmentRequest、GetAttachmentContentRequest、DeleteAttachmentRequest、UpdateAttachmentContentRequest），给现有类型加 JsonSchema/Params derive
- `common/src/api/text_content.rs`：给 TextContentResponse 加 JsonSchema derive
- `src/router.rs`：6 条路由从直接调用函数改为 `{fn_name}_handler`
- **upload_attachment 因使用 Multipart 提取器，generate_http_handler 宏不支持，无法注册为工具**

---

## 第三轮改造：宏能力增强 + 老写法统一化（2026-07-26）

### 改造目标

将剩余的老式 axum handler（手动 `Extension` / `Path` / `Query` / `Json` 提取器）统一改造为 `generate_http_handler` 宏模式，为后续工具注册打好基础。Agent 后续可探查系统健康度、日志、AOP 队列等运维指标。

### 宏能力增强

`generate_http_handler` 宏新增空 struct 分支：当 params 类型无命名字段时，不生成 `Json` 提取器，只从 `Extension` 提取 ctx 并用 `Default::default()` 构造空 params。

- 修复潜在 bug：`get_reception_agent`、`list_organizations`、`get_current_organization`、`list_users_by_current_organization`、`get_current_user` 等 5 个空 struct GET handler 之前会生成 `Json` 提取器，对无 body 的 GET 请求返回 400
- 文件：`ai-orz-macros/src/lib.rs`、`common/src/api/{organization,user,agent}.rs`

### 改造明细（16 个 handler）

#### 8. Backup 模块（3 个）

| 文件 | 改造内容 |
|------|---------|
| system/backup/create_backup.rs | POST 无 body → 空 struct `CreateBackupRequest` |
| system/backup/list_backups.rs | GET 无 body → 空 struct `ListBackupsRequest` |
| system/backup/delete_backup.rs | DELETE + Path<u64> → `DeleteBackupRequest{version:u64}` |

**未改造**：`restore_backup.rs`（返回 text/plain 脚本，不兼容宏）

#### 9. Logs 模块（3 个）

| 文件 | 改造内容 |
|------|---------|
| system/logs/query_logs.rs | GET + Query → `LogQueryRequest`（7 个 query 字段） |
| system/logs/log_stats.rs `get_log_level_distribution` | GET + Query → 复用 `LogStatsQueryParams`（加 Params derive） |
| system/logs/log_stats.rs `get_log_time_series` | 同上 |

#### 10. Health Metrics（1 个）

| 文件 | 改造内容 |
|------|---------|
| system/health_metrics.rs | GET 无 body → 空 struct `GetHealthMetricsRequest`；ctx 从 middleware 注入（带用户身份） |

#### 11. Initialize System（2 个）

| 文件 | 改造内容 |
|------|---------|
| organization/initialize_system.rs `check_initialized` | GET 无 body → 空 struct `CheckInitializedRequest` |
| organization/initialize_system.rs `initialize_system` | POST + Body → 复用 `InitializeSystemRequest`（加 Params derive） |

#### 12. AOP 模块（7 个）

| 文件 | 改造内容 |
|------|---------|
| system/aop.rs `get_all_queue_stats` | GET 无 body → 空 struct `GetAllQueueStatsRequest` |
| system/aop.rs `get_queue_stats` | GET + Path<String> → `GetQueueStatsRequest{consumer}` |
| system/aop.rs `list_events` | GET + Path + Query → `ListEventsRequest`（path + query 组合） |
| system/aop.rs `get_event` | GET + Path<(String,String)> → `GetEventRequest{consumer,event_id}` |
| system/aop_stats.rs `get_stats_overview` | GET 无 body → 空 struct `GetStatsOverviewRequest` |
| system/aop_stats.rs `get_stats_time_series` | GET + Query → `GetStatsTimeSeriesRequest` |
| system/aop_stats.rs `get_stats_distribution` | GET + Query → `GetStatsDistributionRequest` |

### 改造涉及

- `common/src/api/system.rs`：新增 16 个 Request/Response 类型（Backup、Health、AOP）
- `common/src/api/log_stats.rs`：扩展 `LogStatsQueryParams` 加 Params derive，新增 `LogQueryRequest`
- `common/src/api/organization.rs`：新增 `CheckInitializedRequest`
- `src/handlers/system/backup/{create,list,delete}_backup.rs`：3 个 handler 改造
- `src/handlers/system/logs/{query_logs,log_stats}.rs`：3 个 handler 改造
- `src/handlers/system/health_metrics.rs`：1 个 handler 改造
- `src/handlers/organization/initialize_system.rs`：2 个 handler 改造
- `src/handlers/system/{aop,aop_stats}.rs`：7 个 handler 改造
- `src/router.rs`：13 条路由更新函数引用（加 `_handler` 后缀）

### 仍保持老写法的 handler（11 个，不兼容宏）

| 类型 | 数量 | 代表 | 原因 |
|------|------|------|------|
| A2A 协议回调 | 4 | callback、send_task、cancel_task、get_task | 自定义协议分发 |
| A2A JSON-RPC / SSE | 2 | jsonrpc、send_subscribe | 流式响应 / RPC 分发 |
| A2A Agent Card | 1 | agent_card | 公开路由，无 ctx |
| Multipart 上传 | 1 | upload_attachment | Multipart 提取器 |
| SSE 事件流 | 1 | subscribe_sse | SSE 流式响应 |
| 认证 / 会话 | 2 | login、logout | 设置 Cookie + 自定义响应 |
| 备份恢复脚本 | 1 | restore_backup | 返回 text/plain |
| 健康检查 | 1 | health | 裸路由无 middleware，无 ctx |

### 后续可注册为工具的 handler（待用户确认）

本轮改造后，以下 handler 已具备工具注册条件（宏模式 + 业务语义清晰）：

- **运维监控类**（适合 Agent 探查系统状态）：get_health_metrics、get_all_queue_stats、get_stats_overview、get_stats_time_series、get_stats_distribution、query_logs、get_log_level_distribution、get_log_time_series
- **系统管理类**（需 Admin 权限）：list_backups、check_initialized
- **高危操作**（不建议注册）：create_backup、delete_backup、initialize_system
