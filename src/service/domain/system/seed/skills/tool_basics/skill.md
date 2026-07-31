# 工具管理

本指南帮助你理解平台的工具体系，掌握工具查询、调用、结果处理的完整流程。工具是你执行任务的核心能力延伸——从发现工具、选择调用模式，到解读结果、追溯历史，构成完整的工具使用闭环。

## 工具分类：Auto vs Manual

平台工具按加载与调用方式分为两类，这决定了你如何使用它们：

| 类型 | 加载方式 | 调用方式 | 你需要做什么 |
|------|---------|---------|------------|
| **Auto 工具** | 自动注入模型上下文 | 模型直接 function calling | 无需手动请求，模型自主决定调用 |
| **Manual 工具** | 通过 Prompt 展示给你 | 手动调用 `request_tool_call` 或 `send_tool_call_message` | 需要你明确发起调用请求 |

**关键认知**：
- Auto 工具对你透明，模型在推理时自动选择并调用，你不需要关注
- Manual 工具会以工具列表形式展示在你的 Prompt 中，**只有你主动调用才会执行**
- 本指南后续内容主要针对 Manual 工具

## 工具包：按标签组织

工具按 `tags` 分组管理，每个标签代表一个"工具包"。常见标签：

| 标签 | 含义 | 包含的工具示例 |
|------|------|--------------|
| `tool_management` | 工具管理 | `list_tools`、`query_tools`、`request_tool_call` |
| `skill_management` | 技能管理 | `search_skill`、`list_skill_tags`、`uninstall_skill_from_agent` |
| `messaging` | 消息通信 | `send_message`、`send_message_to_agent` |
| `collaboration` | 协作 | `search_agents`、`get_agent`、`get_reception_agent` |
| `project_management` | 项目管理 | `create_text_artifact`、`update_artifact`、`query_artifacts` |
| `file_management` | 文件管理 | 附件上传下载相关工具 |

工具包机制：Agent 入职时按 tag 自动安装工具，已安装 tag 内的工具**无需绑定即可调用**。

## 工具查询与发现

遇到新任务时，先了解有哪些工具可用。4 个核心查询工具（都是 `neural` 常驻）：

### `list_tools` — 列出所有工具

**用途**：返回所有已注册工具的分页列表，包含名称、描述、参数定义。

**参数**：仅分页参数（`limit` / `offset`），无过滤条件。

**适用**：初次了解工具全集、浏览可用能力。配合 `query_tools` 做精准筛选。

### `query_tools` — 按条件查询工具

**用途**：支持完整过滤条件的工具查询。

**关键参数**：
- `ids` — 按 ID 批量查询
- `keyword` — 关键词搜索（匹配名称、描述）
- `agent_id` — 查询某 Agent 绑定的工具
- `tags` — 按标签过滤（OR 语义，命中任一 tag 即可）
- `protocol` — 按协议类型过滤（HTTP / MCP / Built-in）

**适用**：按工具包标签查找特定类别的工具，或按关键词定位具体工具。

### `list_tool_tags` — 列出工具标签

**用途**：返回所有**已启用**工具（status=Enabled）的不重复 tag 列表，按字母升序。

**参数**：无参数。

**适用**：发现系统中所有可用的工具包分类，了解工具分类全貌。仅聚合 Enabled 工具的 tags，禁用工具的 tag 不会出现。

### `list_installed_tool_packs` — 查看已安装工具包

**用途**：返回指定 Agent 已安装的工具包标签列表（`installed_tags`）。

**参数**：`agent_id`（必填）。

**适用**：确认某个 Agent 当前可用的工具集。新工具包安装后，`installed_tags` 会立即更新，无需重启。

## 工具调用：两种模式

Manual 工具有两种调用模式，**根据任务特征选择**：

### 同步调用 — `request_tool_call`

**行为**：阻塞当前轮次，结果在**当前回合**立即返回，可以马上用于后续推理。

**关键参数**：
- `tool_id` — 工具 ID（必填）
- `tool_name` — 工具名称（必填）
- `params` — 工具调用参数（JSON，必填）
- `project_id` — 关联项目 ID（可选，会注入到调用上下文）
- `task_id` — 关联任务 ID（可选，会注入到调用上下文）

**返回**：`RequestToolCallResponse`
- `tool_call_id` — 本次调用的唯一 ID（即 `trace_ref.call_id`，可用于后续追溯）
- `status` — 调用状态（`completed` 表示成功完成）
- `result` — 工具返回的具体内容（JSON）

**适用场景**：
- 需要立即获取结果继续推理（如查询数据库、读取配置）
- 短时间运行的工具（查询类、计算类）
- 结果是后续决策的依赖项

**示例**：
```json
{
  "tool_id": "tool_xxx",
  "tool_name": "get_current_time",
  "params": {"timezone": "Asia/Shanghai"},
  "project_id": "proj_xxx"
}
```

### 异步调用 — `send_tool_call_message`

**行为**：不阻塞当前轮次，立即返回 `request_id` 和 `message_id`。工具执行结果在**下一轮 awaken** 中以 `ToolCallResult` 消息形式送达。

**关键参数**：与 `request_tool_call` 相同。

**返回**：`SendToolCallMessageResponse`
- `request_id` — 请求 ID（用于关联异步结果）
- `message_id` — 消息 ID（系统内部追踪用）
- `status` — 派发状态

**适用场景**：
- 长时间运行的工具（外部 API 调用、文件处理、复杂计算）
- 不需要立即结果的任务
- 并行发起多个工具调用（多个异步调用可同时进行）
- 调用结果可以延后处理

**关键差异**：异步调用的结果不是立即返回的，你需要在下一轮 awaken 时处理新到达的 `ToolCallResult` 消息。系统会自动调度工具执行，并通过消息系统投递结果。

### 模式选择决策

| 场景 | 推荐模式 | 理由 |
|------|---------|------|
| 查询类（读取数据、获取状态） | 同步 | 结果立即可用，用于后续推理 |
| 计算类（轻量计算、格式转换） | 同步 | 快速完成，无阻塞风险 |
| 外部 API 调用（可能慢） | 异步 | 避免阻塞当前轮次 |
| 文件处理 / 大数据处理 | 异步 | 耗时较长 |
| 需要并行多个调用 | 异步 | 多个异步可同时进行 |
| 结果是下一步决策的依赖 | 同步 | 必须等待结果 |

## 结果处理与追溯

### 同步调用的结果

`request_tool_call` 直接返回 `RequestToolCallResponse`，其中：
- `status` 为 `"completed"` 表示成功
- `result` 是工具返回的 JSON 数据
- `tool_call_id` 是本次调用的唯一 ID，**保留它用于后续追溯**

### 异步调用的结果

异步调用的结果以 `ToolCallResult` 消息形式在下一轮 awaken 送达，包含：
- `request_id` — 关联你发起的异步请求
- `tool_id` / `tool_name` — 工具标识
- 执行结果（成功携带 `result` + `trace_ref`，失败携带 `error_message`）
- `trace_ref` — 追踪引用（见下文）

### `trace_ref`：调用追踪引用

每次工具调用（无论同步/异步）成功或执行后失败时，系统会生成 `ToolCallTraceRef`：

| 字段 | 说明 |
|------|------|
| `tool_id` | 工具 ID |
| `call_id` | 本次调用的唯一 ID（即 `tool_call_id`） |

`trace_ref` 是调用追溯的钥匙，用于：
- 在产物、消息中引用调用记录，便于溯源
- 通过 `get_tool_call_entry` 查询完整调用详情

**何时会有 `trace_ref`**：
- ✅ 同步调用成功 → 有
- ✅ 异步调用执行后成功 → 有
- ✅ 异步调用执行后失败 → 有（如果执行已开始）
- ❌ 调用前参数校验失败 → 无（未真正执行）
- ❌ 策略失败（如工具未找到） → 无

### 调用历史查询

两个工具用于查询调用历史（都是 `tool_management` tag，非 neural）：

**`get_tool_call_entry`** — 查询单次调用完整详情

**参数**：
- `call_id` — 调用 ID（必填，路径参数）
- `tool_id` / `agent_id` / `project_id` / `task_id` — 可选，用于访问范围校验

**返回**：`ToolCallEntryDetail`，包含完整调用信息：
- `call_id` / `tool_id` / `tool_name` — 调用标识
- `agent_id` / `task_id` / `project_id` — 关联上下文
- `started_at` / `finished_at` / `duration_ms` — 时间信息（unix 毫秒）
- `input` — 调用入参（已脱敏）
- `output` — 调用输出（已脱敏，成功时有值）
- `error` — 错误信息（失败时有值）
- `status` — 调用状态（`Started` / `Completed` / `Failed`）

**`query_tool_call_entries`** — 批量查询调用历史

**参数**：
- `call_id` — 精确调用 ID
- `agent_id` / `project_id` / `task_id` / `tool_id` — 上下文过滤
- `status` — 按状态过滤（`Started` / `Completed` / `Failed`）
- `started_after` / `started_before` — 时间范围（unix 毫秒，闭区间）
- `limit` — 最大返回数（默认 1，即只返回最新一条）

**适用**：查询某 Agent / Project / Task 的调用历史、统计调用频次、排查失败原因。

### 失败处理

工具调用失败时：

1. **读错误信息**：判断失败类型
   - 参数错误 → 错误信息会指出哪个字段有问题
   - 工具内部错误 → 错误信息描述具体失败原因
   - 执行前失败 → 无 `trace_ref`，无法追溯
   - 执行后失败 → 有 `trace_ref`，可通过 `get_tool_call_entry` 查详情

2. **分类处理**：
   - 参数错误 → 修正参数后重试
   - 工具内部错误 → 记录问题，考虑换用替代工具或人工介入
   - 超时类错误 → 考虑改用异步调用

3. **不要无脑重试**：相同参数重试大概率还是失败，先分析原因再决定策略

## 最佳实践

1. **先查后用**：不熟悉的工具先 `query_tools` 查看参数定义，避免盲目调用
2. **看场景选模式**：短任务用同步 `request_tool_call`，长任务用异步 `send_tool_call_message`
3. **保留 `tool_call_id`**：重要调用保留 ID，便于后续 `get_tool_call_entry` 追溯
4. **善用标签**：通过 `list_tool_tags` 发现工具包，通过 `list_installed_tool_packs` 确认可用工具集
5. **失败分析**：失败时先分析错误信息再决定重试策略，避免无意义重复
6. **并行优化**：多个独立调用用异步模式并行发起，提升效率
7. **关联上下文**：调用时传 `project_id` / `task_id`，便于后续按上下文查询调用历史
8. **追溯链路**：`tool_call_id` → `get_tool_call_entry` → 完整调用详情（入参、出参、耗时、状态）
