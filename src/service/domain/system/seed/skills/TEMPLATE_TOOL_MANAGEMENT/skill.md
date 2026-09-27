# 工具管理

工具是你对外部世界的「手和眼睛」——像人类用双手操作、用眼睛观察一样，你通过工具查询数据、读写文件、调用外部服务、执行动作。系统的同步/异步分发机制类比日常协作场景：**同步工具像你当面问同事、对方马上答复**，当前回合就能拿到结果；**异步工具像把材料发给同事明早给你**，不阻塞当下，下一轮再收结果。`tool_call_id` 则像一张「办事回执单」，拿着它可以追溯整件事的入参、出参、耗时、错误记录。

你通过 function calling 调用平台工具，系统按工具的 `dispatch_mode` 配置**自动决定同步/异步分发**，你无需关心内部路由细节，只需处理结果：**同步立即用、异步等下一轮 ToolCallResult 消息**。

## 你可能调用的工具查询（neural 常驻）

不熟悉当前可用工具时，先用以下 4 个查清楚再调用（tags 均为 `tool_management`；参数与默认值见工具 Schema，此处只记 Schema 看不出来的语义）：

| 工具 | 用途与语义 |
|------|------|
| `list_tools` | 全部已注册工具的分页列表（名称/描述/参数定义） |
| `query_tools` | 按关键词（匹配名/描述）、工具包 tag、Agent、协议等条件过滤 |
| `list_tool_tags` | 列出所有启用工具的 tag（了解工具包分类） |
| `list_installed_tool_packs` | 查看指定 Agent 已安装的工具包 tags（`installed_tags`） |

**常用工具包标签示例**：`messaging` / `project_management` / `file_management` / `collaboration` / `skill_management` / `tool_management`。标签系统会扩展，以 `list_tool_tags` 为准。

## 调用与结果处理

### 同步工具（`dispatch_mode = sync`，默认）

结果在当前回合立即返回，拿到就用在后续推理里。典型：查询类、短计算类。

### 异步工具（`dispatch_mode = async`）

当前回合立即返回 `request_id` / `message_id`，执行结果在**下一轮 awaken** 以 `ToolCallResult` 消息送达（含 `request_id` 对应你发起的调用）。典型：外部 API 调用、文件/大数据处理。多个独立异步调用可一次全发起，下一轮集中收结果。

> 分发模式是工具配置决定的，不是你调用时选的。若你认为某工具的同步/异步与场景期望不符，可以向系统设计者反馈调整 `dispatch_mode`。

### 追溯与失败

- **追溯**：重要调用保存返回的 `tool_call_id`，需要时用 `get_tool_call_entry(call_id=...)` 查完整入参/出参/耗时/错误。`query_tool_call_entries` 可多条件批量查历史——**默认 limit=1 只返回最新一条，按需调大**。调用时（如工具支持）带 project_id / task_id，便于按上下文查历史。
- **何时可追溯**：同步成功 / 异步执行后成功或失败 → 有 trace_ref 可追溯；参数校验失败或工具未找到 → 无（调用没真正执行）。
- **失败处理**：先读错误判断类型——参数错 → 修正重试；工具内部错 → 考虑替换或上报；同步若频繁超时 → 该工具可能应切 async（反馈给系统）。**不要无脑同参数重试**。
