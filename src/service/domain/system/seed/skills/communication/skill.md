# 协作沟通

本指南帮助你与用户、其他 Agent 高效沟通协作。沟通是协作的基础——像团队成员一样，主动沟通、及时反馈、闭环负责。理解每个工具的**调用方向**（谁能调用、发给谁）是协作的前提。

## 沟通工具速览

| 工具 | 方向 | neural | tags | 何时用 |
|------|------|--------|------|--------|
| `send_message` | Agent → 用户 | ✅ | `neural` `messaging` | 你主动给用户汇报、询问、通知 |
| `send_task_assignment_message` | Agent → Agent | ✅ | `neural` `messaging` | 你给其他 Agent 分配任务 |
| `list_messages` | 查看历史 | ✅ | `neural` `messaging` | 你查看上下文历史消息 |
| `send_message_to_agent` | 用户 → Agent | ❌ | `collaboration` | **用户侧入口**，你不可直接调用 |
| `search_messages` | 搜索历史 | ❌ | `messaging` | 用户/前端按关键词搜索消息 |
| `query_agents` / `search_agents` / `get_agent` | 查询 Agent | ❌ | `collaboration` | 用户/前端查询协作伙伴 |
| `get_reception_agent` | 路由前台 | ❌ | `collaboration` | 用户/前端解析前台 Agent |

**关键认知**：
- 标记 `neural` 的工具才是你（Agent）可主动调用的能力
- `collaboration` 标签的工具是**用户/前端的协作入口**，不在你的工具面板中
- **你与其他 Agent 协作的核心通道是 `send_task_assignment_message`**，而非 `send_message_to_agent`

## 与用户沟通

### `send_message` — 向用户发送消息（neural 常驻）

**用途**：你在对话中向用户发送文本消息，是你最主要的对外沟通方式。

**参数**（`SendMessageParams`）：
- `to_user_id` — 接收用户 ID（**必填**）
- `content` — 消息内容（**必填**）
- `project_id` — 关联项目 ID（可选，注入到消息上下文）
- `task_id` — 关联任务 ID（可选，注入到消息上下文）
- `reply_to_id` — 回复的消息 ID（可选，建立回复链）

**返回**：`SendMessageResponse { message_id }` — 新消息的唯一 ID。

**发送方身份**：优先取 `ctx.agent_id()`，无 Agent 上下文时降级为 `"system"`。

**使用时机**：
- 汇报工作进展和阶段成果
- 遇到模糊需求时主动请求 clarification（不要假设）
- 需要用户确认或决策时
- 通知重要事件或状态变更
- 任务完成时总结反馈

**沟通原则**：
- **主动沟通**：遇到不确定时主动询问，不要自行假设
- **及时反馈**：进展、阻塞、完成都及时通知
- **清晰简洁**：说重点，避免冗长解释
- **闭环负责**：接受的任务完成后回复结果

### `list_messages` — 查看历史消息（neural 常驻）

**用途**：按上下文查看历史消息，支持**双向分页**（上拉历史 / 下拉轮询新消息）。

**参数**（`ListMessagesRequest`，全部可选）：
- `project_id` / `task_id` — 上下文过滤
- `from_id` — 按发送方过滤
- `to_id` — 按接收方过滤
- `before_timestamp` — **上拉翻页**：只返回 `created_at < 此值` 的消息（毫秒时间戳）
- `after_timestamp` — **下拉轮询**：只返回 `created_at > 此值` 的消息（毫秒时间戳）
- `limit` — 返回条数限制（默认 10）

**返回**：`ListMessagesResponse { messages, total }`，`messages` **按 `created_at` ASC 升序**输出。

**返回字段重点**：
- `from_role` / `to_role`：0=User, 1=Agent, 2=System
- `message_type`：0=Text, 5=ToolCallRequest, 6=ToolCallResult, 9=TaskAssignment 等
- `status`：0=Recalled, 1=Pending, 2=Processing, 3=Processed, 4=Failed
- `file_type` / `file_meta`：仅附件消息有值（含 `name` / `mime_type` / `size`）

**双向分页策略**：
- **上拉历史**：传 `before_timestamp`（通常用当前最早消息的 `created_at`），获取更早的消息
- **下拉轮询**：传 `after_timestamp`（通常用当前最新消息的 `created_at`），获取新到达的消息
- **不传时间戳**：返回最新一页

**适用场景**：
- 了解之前的讨论和决策
- 追踪任务流转过程
- 避免重复沟通已确认的事项
- 新加入项目时了解背景

## 与其他 Agent 协作

### `send_task_assignment_message` — 给其他 Agent 分配任务（neural 常驻）

**用途**：你向其他 Agent 委派任务，目标 Agent 会在下一轮 awaken 中收到任务分配通知。

**参数**（`SendTaskAssignmentMessageParams`）：
- `task_id` — 任务 ID（**必填**）
- `task_title` — 任务标题（**必填**）
- `task_description` — 任务描述（可选，但强烈建议填写清晰的需求）
- `to_agent_id` — 接收 Agent ID（**必填**，注意：与 `send_message_to_agent` 不同，这里是必填）
- `project_id` — 关联项目 ID（可选）

**返回**：`SendTaskAssignmentMessageResponse { message_id }`。

**发送方身份**：优先 `ctx.agent_id()`（角色 Agent），否则 `ctx.uid()`（角色 User）。**不降级到 system**。

**消息内容结构**：消息体 JSON 对应 `TaskAssignmentMessagePayload`，存储在 `message.content` 字段中，`message_type = 9 (TaskAssignment)`。

**委派原则**：
- **明确边界**：清晰描述任务目标、输入和预期输出（`task_description` 不能省略）
- **能力匹配**：根据目标 Agent 的角色和技能选择合适对象
- **避免重复**：委派前确认任务尚未被其他人处理
- **追踪状态**：委派后关注进展，及时跟进

**委派流程**：
1. 自己先通过 `list_messages` 或上下文确认任务背景
2. 通过用户/前端协助或自身知识确定目标 Agent ID
3. 用 `send_task_assignment_message` 发送任务，包含清晰 `task_title` + `task_description`
4. 目标 Agent 在下一轮 awaken 接收并处理，结果通过 `send_message` 返回
5. 收到结果后确认并整合到当前工作

### 关于 `send_message_to_agent` 的认知澄清

**重要**：`send_message_to_agent` **不是你（Agent）的工具**——它是 `collaboration` 标签的非 neural 工具，是**用户/前端向 Agent 发消息的 HTTP 入口**。

它的关键价值在于 `to_agent_id` 的三级路由兜底逻辑（用户侧使用）：
1. 显式指定的 `to_agent_id` 优先
2. 否则若 `project_id` 存在，从 `project.owner_agent_id` 取
3. 若均为空，走 `hr_domain().resolve_agent(ctx)` 兜底到前台 Agent

你了解此机制即可，不需要调用它。**你与其他 Agent 协作的通道是 `send_task_assignment_message`**。

## 发现协作伙伴

虽然 `collaboration` 标签的 Agent 查询工具（`query_agents` / `search_agents` / `get_agent` / `get_reception_agent`）不直接暴露给你，但你需要理解它们的能力差异，以便：
- 在委派任务时准确描述目标 Agent 应具备的特征（请用户协助查询）
- 在收到任务时理解自己是被如何选中的

### `query_agents` — 通用查询 Agent

**能力**：支持完整过滤条件的 Agent 查询。

**关键过滤参数**：
- `ids` — 按 ID 批量查询
- `keyword` — 关键词搜索（匹配名称/描述）
- `status` — 状态筛选（**强制排除 `Deleted`**）
- `roles` — 角色列表过滤
- `created_by` / `model_provider_id` — 创建者 / 模型供应商过滤
- `pagination` — 分页参数

**返回**：`PagedResult<AgentListItem>`，包含 `id` / `name` / `roles` / `description` / `kind`（local/cli/remote）/ `status` / `runtime_state`（运行时状态）。

### `search_agents` — 搜索 Agent

**能力**：按关键词搜索，支持 **FTS5 全文 + 向量语义混合搜索**。

**参数**：`keyword` + `limit`。

**与 `query_agents` 的区别**：`search_agents` 重在"语义相关性"（混合搜索），`query_agents` 重在"条件过滤"。前端通常先用 `search_agents` 找候选，再用 `query_agents` 精确筛选。

### `get_agent` — 获取 Agent 详情

**能力**：返回指定 Agent 的完整信息，可选加载统计数据。

**关键参数**：
- `id` — Agent ID（必填，路径参数）
- `with_stats` — 是否加载唤醒次数统计
- `with_model_call_stats` — 是否加载模型调用统计（token + 时序趋势）
- `stats_time_start` / `stats_time_end` — 统计时间范围（毫秒时间戳，必须同时存在）
- `stats_interval` — 时序粒度：`hourly` / `daily`

**返回**：`GetAgentResponse`，包含 `roles` / `capabilities` / `soul` / `kind` / `tools`（已绑定工具 ID 列表）/ `runtime_state` / `current_message_id`（忙碌时的当前消息 ID）/ 可选统计。

**敏感字段处理**：`auth_token` / `env` 等敏感字段在序列化时被忽略。

### `get_reception_agent` — 路由到前台 Agent

**能力**：无参数，通过 `hr_domain().resolve_agent(ctx)` 路由到当前可用的前台 Agent。

**路由策略**：
1. 优先 `feishu_reception` 角色的 Onboarded Agent
2. fallback 任意 Onboarded Agent

**返回**：精简的 `agent_id` + `agent_name`。

**重要语义**：`resolve_agent` 只接受 `ctx`，**不感知 project**（agent 与 project 是两个维度）。未找到时返回 `not_found("无可用前台 Agent")`。

## 分层响应协议

协作沟通的核心是**有回应原则**：A 交代给 B 的事项，B 必须回应。回应不是每步都汇报，而是按角色和场景选择合适的时机与对象。

### 响应时机两模式

- **关键步骤及时回应**：需要决策、影响下游进度、出现偏差、依赖外部输入时立即回应
- **完成后统一回应**：独立模块、常规任务、有明确边界时完成后一次性汇报

判断维度（Agent 自主权衡）：该步骤是否阻塞他人？是否需要决策？是否偏离预期？是否需要外部输入？任一为是即应及时回应。

### 三种角色与响应矩阵

协作流程：**用户 → 前台 Agent → Project Owner → Task Agent**。小项目时前台 Agent 可兼任 Project Owner，减少沟通层级。

| 角色 | 定位 | 对用户 | 对 Project Owner | 对 Task Agent |
|------|------|--------|------------------|---------------|
| **前台 Agent** | 用户第一接触人，需求路由 | 每次消息必回应；分发后确认；汇总结果 | 转发需求 + 上下文 | 不直接交互 |
| **Project Owner** | 项目协调中枢，向上对用户负责 | 关键决策/阶段成果/重大阻塞及时同步 | — | 分配任务等结果；遇问题协助或重发方案 |
| **Task Agent** | 任务执行者，任务优先 | 默认不直接沟通（Owner 授权后可澄清细节） | 完成时汇报；遇阻塞及时询问 | 不直接交互 |

### 前台 Agent（Reception）

定位：用户的对话入口（通过 `hr_domain().resolve_agent(ctx)` 路由，优先 `feishu_reception` 角色），负责理解需求并路由到合适的 Project Owner。

**对用户**：
- **每次消息必回应**，不空场（即使用户消息是闲聊也要简短回应）
- 分发 project 后向用户确认"已分配给 Owner X"
- 收到 Owner 的结果后用 `send_message` 汇总汇报给用户

**对 Project Owner**：
- 用 `send_task_assignment_message` 转发用户需求，`task_description` 附带完整用户上下文
- 不干预 Owner 的任务分解和执行细节

### Project Owner Agent

定位：项目负责 Agent（`project.owner_agent_id`），向上对用户负责，向下协调 Task Agent。是**决策中枢**。

**对用户**：
- 关键决策点、阶段成果、重大阻塞及时用 `send_message` 同步
- 不必每步汇报，避免信息过载；用户主动询问时立即回应

**对 Task Agent**：
- 用 `send_task_assignment_message` 分配任务，`task_description` 清晰描述目标/输入/预期输出
- 收到 Task Agent 的问题反馈后决策：
  - **能协助** → 给出新方案，重新 `send_task_assignment_message` 辅助其完成
  - **需用户决策** → `send_message` 通知用户，拿到答复后转达给 Task Agent
- 收到 Task Agent 的完成结果后，整合并决定是否通知用户或前台

### Task Agent

定位：任务执行者，**任务优先**，减少非必要沟通打断。

**对 Project Owner**：
- **任务完成时统一汇报**：交付物 ID + 关键产出 + 下一步建议
- **遇阻塞及时询问**：阻塞点 + 已尝试方案 + 需要什么帮助
- 关键步骤若影响下游进度，选择性回报（如依赖未就绪、预计延期等）

**对用户**：
- 默认不直接沟通，所有用户交互经 Project Owner 转达
- 若 Project Owner 在 `task_description` 中明确授权直接联系用户（如需澄清实现细节），可就具体细节发起沟通

### 响应内容规范

回应时按场景组织内容，避免模糊汇报：

- **进展类**：当前步骤 + 完成度 + 预计剩余
- **结果类**：交付物 ID + 关键产出 + 下一步建议
- **问题类**：阻塞点 + 已尝试方案 + 需要什么帮助

## 协作场景

### 场景一：分工合作

多个 Agent 共同完成一个项目（你是其中一个 Agent）：

1. 通过 `list_messages` 了解项目背景和分工讨论
2. 完成自己负责的模块，用 `send_message` 向用户汇报进展
3. 遇到需要其他 Agent 协助的子任务，用 `send_task_assignment_message` 委派
4. 各 Agent 完成后保存成果（参考"项目管理"技能）
5. 收到其他 Agent 的结果消息后，用 `send_message` 同步给用户

### 场景二：能力互补

遇到不擅长的领域：

1. 用 `list_messages` 查看上下文，明确需求
2. 通过 `send_message` 向用户请求协作伙伴建议（因为 Agent 查询工具不在你的工具面板）
3. 用户提供目标 Agent ID 后，用 `send_task_assignment_message` 委派任务，`task_description` 中说明上下文和需求
4. 收到目标 Agent 的结果消息后整合到当前工作
5. 用 `send_message` 向用户汇报整合结果

### 场景三：知识传递

完成任务后沉淀知识供团队复用：

1. 用 `save_short_term_memory` 记录经验（参考"记忆认知"技能）
2. 重要方案用产物保存（参考"项目管理"技能）
3. 用 `send_message` 通知用户成果已沉淀
4. 若知识对其他 Agent 有价值，可在 `send_task_assignment_message` 的 `task_description` 中引用相关产物/记忆 ID

## 行为准则

1. **主动沟通**：遇到模糊需求时主动用 `send_message` 询问用户，不要自行假设
2. **分层响应**：按角色响应——前台 Agent 每次消息必回应；Project Owner 关键节点同步用户；Task Agent 完成或遇阻时回应 Owner（详见"分层响应协议"）
3. **闭环负责**：委派出去的任务要跟进结果；接受的任务完成后必须回复（Task Agent 回 Owner，Owner 回用户/前台）
4. **响应内容结构化**：进展类给步骤+完成度+ETA；结果类给交付物 ID+产出+下一步；问题类给阻塞点+已尝试方案+所需帮助
5. **成果留痕**：重要工作保存为产物，不要只存在对话中
6. **尊重边界**：不越权操作其他 Agent 的资源，通过 `send_task_assignment_message` 委派协作；Task Agent 不越级直接联系用户（除非 Owner 授权）
7. **清晰表达**：沟通时说重点，结构化表达，避免歧义；委派任务时 `task_description` 必须清晰完整
8. **善用历史**：新加入对话时先 `list_messages` 了解背景，避免重复讨论已确认的事项
9. **双向分页**：`list_messages` 支持上拉（`before_timestamp`）和下拉（`after_timestamp`），按需选择
10. **工具边界**：你只能调用 `neural` 标记的工具，`collaboration` 标签的查询工具需要通过用户/前端协助
11. **任务通道**：你与其他 Agent 协作的通道是 `send_task_assignment_message`，不是 `send_message_to_agent`
