# 管理页面补全实施计划（模块化方案）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 补全前端核心管理页面，从占位符和半成品状态升级为完整可用的管理界面

**Architecture:** 前端 Dioxus 0.7 + WASM，保持现有分层（API 客户端 → Signal 状态 → 组件渲染），所有页面复用 index.html 中已定义的 Mistral CSS 组件类，按业务域组织页面模块

**Tech Stack:** Rust + Dioxus 0.7 + reqwest (WASM) + serde_json + common crate 共享 DTO

---

## 调研结论总览

### 后端 Handler 复用情况

| 模块 | 后端 Handler | 可复用 | 需新增 |
|------|-------------|--------|--------|
| 消息对话 | list_messages, send_message_to_agent | ✅ 完全可复用 | 无 |
| 用户个人信息 | get_current_user, update_current_user | ✅ 完全可复用 | 无 |
| Agent 管理 | get_agent, update_agent_status, install/uninstall tool_pack/skill_pack, list_installed_* | ✅ 完全可复用 | 无 |
| 项目管理 | get_project, list_project_tasks, update_project_status, update_task_status | ✅ 完全可复用 | 无 |
| 技能库 | create_skill | ✅ 完全可复用 | 无 |
| 定时触发器 | create_cron_trigger | ✅ 可复用（Cron 类型不可用，仅 Once/Interval） | 无 |
| 消息渠道 | create_message_channel | ✅ 完全可复用 | 无 |

**结论：所有后端 Handler 均已完整实现，无需任何后端改动，全部工作集中在前端。**

### 前端 API 客户端覆盖情况

| 模块 | 前端 API 函数 | 状态 |
|------|-------------|------|
| 消息对话 | send_message_to_agent, load_latest/older/poll_messages | ❌ 路径错误（缺 `/finance` 前缀） |
| 用户个人信息 | get_current_user_info | ❌ 缺 update_current_user |
| Agent 管理 | 全部 17 个函数 | ✅ 已覆盖 |
| 项目管理 | 全部 11 个函数 | ✅ 已覆盖 |
| 技能库 | create_skill | ✅ 已有 |
| 定时触发器 | create_cron_trigger | ✅ 已有 |
| 消息渠道 | create_message_channel | ✅ 已有 |

---

## 模块一：消息对话模块（Bug 修复）

### 调研结论

**后端路由**（router.rs 第 344-350 行）：
- `GET /api/v1/finance/messages` → list_messages_handler
- `POST /api/v1/finance/messages/agents` → send_message_to_agent_handler

**前端现状**（frontend/src/api/message.rs）：
- 所有 4 个函数路径缺少 `/finance` 前缀
- 前端调用 `/api/v1/messages/...`，后端实际在 `/api/v1/finance/messages/...`
- 运行时会收到 404

**chat.rs DTO 字段问题**（frontend/src/pages/message/chat.rs）：
- 前端构造 `SendMessageToAgentParams` 时使用了不存在的字段 `agent_id` 和 `attachment_ids`
- 后端实际字段是 `to_agent_id: String`（必填）、`reply_to_id: Option<String>`
- 前端未提供必填的 `to_agent_id` 值

**后端 DTO 定义**（common/src/api/neural_tools.rs 第 145-157 行）：
```rust
pub struct SendMessageToAgentParams {
    pub to_agent_id: String,           // 必填
    pub content: String,               // 必填
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub reply_to_id: Option<String>,
}
```

### 改动方案

**Task 1: 修复消息 API 路径 + chat.rs DTO 字段**

Files:
- Modify: `frontend/src/api/message.rs` — 4 个函数路径添加 `/finance` 前缀
- Modify: `frontend/src/pages/message/chat.rs` — 修复 `SendMessageToAgentParams` 字段

message.rs 修复：
- `/api/v1/messages/agents` → `/api/v1/finance/messages/agents`
- `/api/v1/messages` → `/api/v1/finance/messages`（3 处）

chat.rs 修复：
- 将 `agent_id: None` 和 `attachment_ids: None` 删除
- 添加 `to_agent_id: agent_id.clone()`（从选中项目的 owner_agent_id 或其他方式获取）
- 添加 `reply_to_id: None`

**注意**：chat.rs 中需要确定 `to_agent_id` 的来源。根据之前的对话设计，用户与项目的 PMO Agent 对话，因此 `to_agent_id` 应来自项目的 `owner_agent_id` 字段。如果项目没有 `owner_agent_id`，需要先查询或让用户选择 Agent。

---

## 模块二：用户个人信息模块

### 调研结论

**后端 Handler**（src/handlers/user/profile/update_current_user.rs）：
- 路由：`PUT /api/v1/user/me`
- 已完整实现，支持修改 display_name、email、password_hash（均为 Optional）
- 返回 `UpdateCurrentUserResponse { data: UserInfoResponse }`
- 同时注册为神经工具

**后端 DTO**（common/src/api/user.rs）：
```rust
pub struct UpdateCurrentUserRequest {
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub password_hash: Option<String>,  // 注意：是哈希值
}
pub struct UpdateCurrentUserResponse {
    pub data: UserInfoResponse,
}
// UserInfoResponse 含：user_id, username, display_name, email, organization_id, role, role_name, status
```

**前端现状**：
- `organization.rs` 缺少 `update_current_user` 函数
- `profile.rs` 保存按钮是占位实现（`success.set("功能开发中")`）
- 角色映射可能错误：前端 `3 => "超级管理员"`，后端 `UserRole::Member = 3`

**UserRole 枚举**（需确认）：后端注释为 1=SuperAdmin, 2=Admin, 3=Member

### 改动方案

**Task 2: 新增 update_current_user API + 接通 profile 保存**

Files:
- Modify: `frontend/src/api/organization.rs` — 新增 `update_current_user` 函数
- Modify: `frontend/src/pages/user/profile.rs` — 接通保存按钮

organization.rs 新增：
```rust
pub async fn update_current_user(req: UpdateCurrentUserRequest) -> Result<UpdateCurrentUserResponse, String> {
    api_put("/api/v1/user/me", &req).await
}
```

profile.rs 保存按钮改为：
- 构造 `UpdateCurrentUserRequest { display_name: Some(...), email: Some(...), password_hash: None }`
- 调用 `update_current_user(req).await`
- 成功后更新 signal 显示

同时修复角色映射（需确认 UserRole 枚举实际值）。

---

## 模块三：Agent 管理模块

### 调研结论

**后端 Handler 全部可复用**：

| Handler | 路由 | 响应关键字段 |
|---------|------|-------------|
| get_agent | GET `/api/v1/hr/agents/{id}` | 含 runtime_state, current_message_id |
| update_agent_status | PUT `/api/v1/hr/agents/{id}/status` | 返回完整 GetAgentResponse |
| list_installed_tool_packs | GET `/api/v1/hr/agents/{agent_id}/tool-packs` | `{ installed_tags: Vec<String> }` |
| install_tool_pack | POST `/api/v1/hr/agents/{agent_id}/tool-packs/{tag}` | `{ installed_tags: Vec<String> }` |
| uninstall_tool_pack | DELETE `/api/v1/hr/agents/{agent_id}/tool-packs/{tag}` | `{ installed_tags: Vec<String> }` |
| list_installed_skill_packs | GET `/api/v1/hr/agents/{agent_id}/skill-packs` | `{ skill_packs: Vec<String> }`（注意字段名不同） |
| install_skill_pack | POST `/api/v1/hr/agents/{agent_id}/skill-packs/{tag}` | `{ installed_count: usize }` |
| uninstall_skill_pack | DELETE `/api/v1/hr/agents/{agent_id}/skill-packs/{tag}` | `{}`（空响应） |

**AgentStatus 枚举**（common/src/enums/agent.rs）：
```
0=Deleted, 1=Interviewing(默认), 2=PendingOnboard, 3=Onboarded, 4=Offboarded, 5=PendingOffboard
```

**AgentRuntimeState 枚举**：
```
0=Idle(默认), 1=Resting, 2=Busy
```

**前端 API 已全部覆盖**（hr.rs 17 个函数），但 install/uninstall 使用 empty 变体丢弃了响应。

**模型提供商**：
- `ListModelProvidersResponse` 字段名是 `providers`（不是 `list`）
- `ModelProviderListItem` 含 id, name, provider_type, model_name, description, created_at
- 前端 `list_model_providers()` 已有

**前端现状**：
- Agent 列表页（agents.rs）：已实现，但 model_provider_id 是手动输入文本框
- Agent 详情页（agent_detail.rs）：纯占位符

### 改动方案

**Task 3: Agent 列表页 — 模型提供商改为下拉选择**

Files:
- Modify: `frontend/src/pages/hr/agents.rs`

改动：
- import 新增 `use crate::api::finance::list_model_providers;` 和 `use common::api::ListModelProvidersResponseItem;`
- 新增 signal: `model_providers`
- use_effect 中同时加载模型提供商
- 创建 Modal 中的 model_provider_id 输入改为 select 下拉

**Task 4: Agent 详情页完整实现**

Files:
- Modify: `frontend/src/pages/hr/agent_detail.rs`

页面结构：
1. **基本信息卡片**：名称、角色、描述、能力（badge 标签）、灵魂提示词、模型提供商、状态 badge
2. **状态管理区域**：状态切换按钮（入职/离职等）
3. **工具包管理区域**：已安装工具包列表（badge + 卸载按钮）、安装输入框 + 按钮
4. **技能包管理区域**：已安装技能包列表（badge + 卸载按钮）、安装输入框 + 按钮

关键实现细节：
- 调用 `get_agent` 加载详情
- 调用 `list_installed_tool_packs` / `list_installed_skill_packs` 加载已安装列表
- 注意 skill_packs 字段名是 `skill_packs` 不是 `installed_tags`
- install/uninstall 后重新 list 刷新
- 状态切换使用正确的 AgentStatus 枚举值（1=面试中, 2=待入职, 3=已入职, 4=已离职, 5=待离职）
- runtime_state 展示：0=空闲, 1=休息中, 2=忙碌

---

## 模块四：项目管理模块

### 调研结论

**后端 Handler 全部可复用**：

| Handler | 路由 | 响应 |
|---------|------|------|
| get_project | GET `/api/v1/projects/{id}` | `GetProjectResponse`（15 字段） |
| list_project_tasks | GET `/api/v1/projects/{project_id}/tasks` | `Vec<TaskListItem>` |
| update_project_status | PUT `/api/v1/projects/{id}/status` | `GetProjectResponse` |
| update_task_status | PUT `/api/v1/tasks/{id}/status` | `GetTaskResponse` |

**ProjectStatus 枚举**（common/src/enums/project.rs）：
```
0=Deleted, 1=Active(默认), 2=PendingReview, 3=InProgress, 4=Completed, 5=Archived
```

**TaskStatus 枚举**（common/src/enums/task.rs）：
```
0=Cancelled(软删除), 1=PendingReview, 2=Pending(默认), 3=InProgress, 4=Completed, 5=Archived
```

**关键发现**：
1. `ListProjectsResponseItem`（= `ProjectListItem`）**没有 task_count 字段**
2. `GetProjectResponse` 比 `ProjectListItem` 多 5 个字段：workflow, guidance, start_at, due_at, end_at
3. `TaskListItem.progress` 是 `i32`（0-100）
4. 前端项目列表页状态映射**严重错误**：前端 0=已归档/1=进行中/2=已完成/3=已暂停，后端实际 0=Deleted/1=Active/2=PendingReview/3=InProgress/4=Completed/5=Archived
5. 后端 list_projects 返回 `Vec<ProjectListItem>`，前端期望 `ListProjectsResponse { projects: Vec }`（可能存在格式不匹配）
6. 后端 list_project_tasks 返回 `Vec<TaskListItem>`，前端期望 `ListTasksResponse { tasks: Vec }`（同上）

**前端现状**：
- 项目列表页（projects.rs）：已实现，但状态映射错误，任务数列显示 "-"
- 项目详情页（project_detail.rs）：纯占位符

### 改动方案

**Task 5: 项目列表页 — 修复状态映射 + 任务数显示**

Files:
- Modify: `frontend/src/pages/project/projects.rs`

改动：
1. 修复 `status_badge` / `status_text` 函数，对齐 ProjectStatus 枚举：
   - 0=已删除, 1=活跃, 2=待审核, 3=进行中, 4=已完成, 5=已归档
2. 任务数列：用 HashMap signal 存储每个项目的任务数
   - 加载项目列表后，遍历调用 `list_project_tasks` 获取任务数
   - 表格中从 HashMap 读取显示

**Task 6: 项目详情页完整实现**

Files:
- Modify: `frontend/src/pages/project/project_detail.rs`

页面结构：
1. **项目基本信息卡片**：名称、描述、状态 badge、优先级、标签、创建时间
2. **状态管理区域**：启动(→InProgress)、完成(→Completed)、归档(→Archived) 按钮
3. **任务列表表格**：标题、状态 badge、优先级、进度条、操作按钮（开始/完成）

关键实现细节：
- 调用 `get_project` 加载详情
- 调用 `list_project_tasks` 加载任务列表
- 状态切换使用正确的 ProjectStatus 枚举值
- 任务进度条：`div { style: "width: {progress}%; ..." }`
- 任务状态切换使用正确的 TaskStatus 枚举值
- 注意后端返回可能是 `Vec<TaskListItem>` 而非 `ListTasksResponse`，需确认前端 `list_project_tasks` 函数是否能正确解析

---

## 模块五：技能库模块

### 调研结论

**后端 Handler**：
- `POST /api/v1/hr/skills` → create_skill_handler
- 已注册为神经工具，同时支持 HTTP 调用
- 校验 name 和 user_id 非空
- 默认 category = "uncategorized"，默认 status = Draft

**CreateSkillRequest**（common/src/api/skill.rs 第 10-27 行）：
```rust
pub struct CreateSkillRequest {
    pub name: String,                                    // 必填
    pub description: String,                             // 必填
    pub tags: Vec<String>,                               // 必填（可为空 vec）
    pub category: Option<String>,                        // 可选
    pub status: Option<SkillStatus>,                     // 可选，默认 Draft
    pub content: Option<String>,                         // skill.md 主内容
    pub initial_files: Option<HashMap<String, String>>,  // filename → content 映射
}
```

**前端 API**：`create_skill` 函数已存在于 hr.rs

**前端页面**：skills.rs 只有列表 + 删除，无创建按钮

### 改动方案

**Task 7: 技能库页面新增创建功能**

Files:
- Modify: `frontend/src/pages/hr/skills.rs`

改动：
1. import 新增 `create_skill` 和 `CreateSkillRequest`
2. 新增 signal: `show_add_modal`, `new_name`, `new_description`, `new_tags`, `new_category`, `new_content`, `creating`
3. card-header 中添加"+ 创建技能"按钮
4. Modal 表单：
   - 技能名称（必填）
   - 技能描述（必填）
   - 标签（逗号分隔）
   - 分类（可选，placeholder "development"）
   - 技能内容（可选，textarea，写入 skill.md）
5. 创建时构造 `CreateSkillRequest`，tags 用逗号分割
6. 成功后关闭 Modal、清空表单、刷新列表

---

## 模块六：系统管理模块（定时触发器）

### 调研结论

**后端 Handler**：
- `POST /api/v1/system/cron-triggers` → create_cron_trigger_handler
- **重要**：handler 本地重新定义了 DTO（response.rs），但字段与 common::api 一致
- **Cron 类型当前不可用**：后端对 `TriggerType::Cron` 直接返回 `UnsupportedOperation` 错误

**TriggerType 枚举**（common/src/enums/cron_trigger.rs）：
```rust
pub enum TriggerType {
    Once = 0,       // 一次性触发
    Cron = 1,       // Cron 表达式（默认值，但 handler 尚未实现）
    Interval = 2,   // 固定间隔触发
}
```

**CreateCronTriggerRequest**（common/src/api/cron_trigger.rs）：
```rust
pub struct CreateCronTriggerRequest {
    pub name: String,                          // 必填
    pub trigger_type: TriggerType,             // 必填
    pub cron_expression: Option<String>,       // Cron 类型用（当前不可用）
    pub interval_seconds: Option<i64>,         // Interval 类型必填
    pub run_at: Option<i64>,                   // Once 类型必填
    pub payload: String,                       // 必填，JSON 字符串
}
```

**前端 API**：`create_cron_trigger` 函数已存在于 system.rs

**前端页面**：triggers.rs 只有列表 + 暂停/恢复 + 删除，无创建按钮

### 改动方案

**Task 8: 定时触发器页面新增创建功能**

Files:
- Modify: `frontend/src/pages/system/triggers.rs`

改动：
1. import 新增 `create_cron_trigger` 和 `CreateCronTriggerRequest`、`TriggerType`
2. 新增 signal: `show_add_modal`, `new_name`, `new_type`, `new_interval`, `new_payload`, `creating`
3. card-header 中添加"+ 创建触发器"按钮
4. Modal 表单：
   - 触发器名称（必填）
   - 类型选择（下拉：一次性触发 / 固定间隔触发，**不提供 Cron 选项**因为后端不支持）
   - 间隔秒数（Interval 类型时显示，如 300 = 5 分钟）
   - Payload JSON（必填，textarea，placeholder 示例）
5. 创建时根据类型构造请求：
   - Once: `trigger_type: TriggerType::Once, run_at: Some(now), interval_seconds: None, cron_expression: None`
   - Interval: `trigger_type: TriggerType::Interval, interval_seconds: Some(n), run_at: None, cron_expression: None`

---

## 模块七：消息渠道模块

### 调研结论

**后端 Handler**：
- `POST /api/v1/finance/message-channels` → create_message_channel_handler
- 已注册为神经工具

**ChannelType 枚举**（common/src/enums/message_channel.rs）：
```rust
pub enum ChannelType {
    Lark = 0,       // 飞书（默认值）← 注意：是 Lark 不是 Feishu
    Wechat = 1,     // 微信
    Slack = 2,      // Slack
    Email = 3,      // 邮件
    Webhook = 4,    // 通用 Webhook
}
// 没有 Feishu, DingTalk, Wecom
// 实现了 Display trait，输出小写字符串
```

**CreateMessageChannelRequest**（common/src/api/message_channel.rs，24 个字段）：
- 必填：`channel_type: ChannelType`, `channel_name: String`
- 可选通用：`user_id`, `agent_id`, `webhook_url`, `access_token`, `secret`
- Lark 专属：`lark_app_id`, `lark_app_secret`, `lark_encrypt_key`, `lark_verification_token`
- Wechat 专属：`wechat_app_id`, `wechat_app_secret`, `wechat_open_id`
- Email 专属：`email_smtp_host`, `email_smtp_port`, `email_username`, `email_password`, `email_from_address`, `email_to_address`
- Slack 专属：`slack_bot_token`, `slack_channel_id`
- Webhook 专属：`webhook_method`, `webhook_body_template`

**前端 API**：`create_message_channel` 函数已存在于 finance.rs

**前端页面**：message_channels.rs 只有列表 + 状态切换 + 删除，无创建按钮

### 改动方案

**Task 9: 消息渠道页面新增创建功能**

Files:
- Modify: `frontend/src/pages/finance/message_channels.rs`

改动：
1. import 新增 `create_message_channel` 和 `CreateMessageChannelRequest`、`ChannelType`
2. 新增 signal: `show_add_modal`, `new_name`, `new_type`, `new_webhook_url`, `creating`
3. card-header 中添加"+ 创建渠道"按钮
4. Modal 表单（简化版，MVP 只支持通用字段）：
   - 渠道名称（必填）
   - 渠道类型（下拉：飞书(Lark) / 微信 / Slack / 邮件 / Webhook）
   - Webhook URL（可选，Lark/Slack/Webhook 类型时显示）
5. 创建时构造 `CreateMessageChannelRequest`，仅填充必填 + 通用字段，其他字段留 None
6. 后续可按 channel_type 动态显示对应字段组（但 MVP 阶段先简化）

---

## 模块八：构建验证与修复

**Task 10: 全量编译验证 + 后端测试**

Files:
- All frontend files

- [ ] 前端 `cargo check` 通过
- [ ] 后端 `cargo check` 通过
- [ ] 后端 `cargo test --all` 全部通过
- [ ] 修复任何编译错误

---

## 枚举值速查表（前端开发参考）

### AgentStatus（common/src/enums/agent.rs）
| 值 | 枚举 | 中文 |
|----|------|------|
| 0 | Deleted | 已删除 |
| 1 | Interviewing | 面试中（默认） |
| 2 | PendingOnboard | 待入职 |
| 3 | Onboarded | 已入职 |
| 4 | Offboarded | 已离职 |
| 5 | PendingOffboard | 待离职 |

### AgentRuntimeState
| 值 | 枚举 | 中文 |
|----|------|------|
| 0 | Idle | 空闲（默认） |
| 1 | Resting | 休息中 |
| 2 | Busy | 忙碌 |

### ProjectStatus（common/src/enums/project.rs）
| 值 | 枚举 | 中文 |
|----|------|------|
| 0 | Deleted | 已删除 |
| 1 | Active | 活跃（默认） |
| 2 | PendingReview | 待审核 |
| 3 | InProgress | 进行中 |
| 4 | Completed | 已完成 |
| 5 | Archived | 已归档 |

### TaskStatus（common/src/enums/task.rs）
| 值 | 枚举 | 中文 |
|----|------|------|
| 0 | Cancelled | 已取消（软删除） |
| 1 | PendingReview | 待审核 |
| 2 | Pending | 待处理（默认） |
| 3 | InProgress | 进行中 |
| 4 | Completed | 已完成 |
| 5 | Archived | 已归档 |

### TriggerType（common/src/enums/cron_trigger.rs）
| 值 | 枚举 | 中文 | 后端支持 |
|----|------|------|---------|
| 0 | Once | 一次性 | ✅ |
| 1 | Cron | Cron 表达式（默认） | ❌ 不可用 |
| 2 | Interval | 固定间隔 | ✅ |

### ChannelType（common/src/enums/message_channel.rs）
| 值 | 枚举 | 中文 | Display 输出 |
|----|------|------|-------------|
| 0 | Lark | 飞书（默认） | "lark" |
| 1 | Wechat | 微信 | "wechat" |
| 2 | Slack | Slack | "slack" |
| 3 | Email | 邮件 | "email" |
| 4 | Webhook | 通用 Webhook | "webhook" |

### UserRole（需确认，后端注释 1=SuperAdmin, 2=Admin, 3=Member）

---

## 关键注意事项

1. **消息 API 路径**：后端消息路由嵌套在 `/finance` 下，完整路径是 `/api/v1/finance/messages`
2. **chat.rs DTO 字段**：`SendMessageToAgentParams` 的字段是 `to_agent_id`（不是 `agent_id`），且没有 `attachment_ids`
3. **skill_packs 字段名**：`ListInstalledSkillPacksResponse` 的字段名是 `skill_packs`（不是 `installed_tags`）
4. **Cron 类型不可用**：后端 create_cron_trigger 对 Cron 类型直接报错，前端不应提供该选项
5. **ChannelType 没有 Feishu**：飞书用 `Lark` 变体，不存在 DingTalk 和 Wecom
6. **ListProjectsResponseItem 无 task_count**：前端需用 HashMap 方案显示任务数
7. **项目状态映射错误**：前端 projects.rs 的 status_text/status_badge 与后端 ProjectStatus 枚举严重不匹配
8. **install/uninstall 响应**：tool_pack 返回 `installed_tags`，skill_pack 返回 `skill_packs` 或空
9. **模型提供商列表字段名**：`ListModelProvidersResponse` 的字段名是 `providers`
10. **后端 list_projects/list_project_tasks 返回 Vec**：前端期望包装类型，可能存在反序列化问题（需确认 `#[generate_http_handler]` 宏是否自动包装）
