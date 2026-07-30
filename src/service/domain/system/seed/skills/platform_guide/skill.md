# 平台使用指南

本指南帮助你理解平台能力，高效使用神经工具完成任务。所有神经工具在你唤醒时自动可用，无需安装。

## 神经工具总览

平台提供 22 个神经工具，分为五类。调用方式见各工具的参数说明。

### 工具管理（5 个）
- `list_tools` — 列出所有可用工具
- `query_tools` — 按条件查询工具
- `list_tool_tags` — 列出工具的分类标签，用于发现可安装的工具包
- `request_tool_call` — 同步调用 Manual 工具，结果当前轮返回
- `send_tool_call_message` — 异步派发工具调用，结果下一轮通过 ToolCallResult 送达

### 技能管理（4 个）
- `search_skill` — 按关键词或标签搜索技能库，按需加载未常驻的技能
- `search_skills` — 批量搜索技能
- `list_skill_tags` — 列出所有已发布技能的标签，用于发现技能包
- `uninstall_skill_from_agent` — 卸载不再需要的技能副本

### 消息通信（3 个）
- `send_message` — 向用户发送消息
- `list_messages` — 查看历史消息
- `send_task_assignment_message` — 向其他 Agent 分配任务

### 资源发现（3 个）
- `query_model_providers` — 查询可用的模型 Provider
- `query_message_channels` — 查询消息通道
- `query_users` — 查询用户列表

## 技能发现与加载

技能分两类加载方式：

1. **常驻技能**（tags 含 `neural`）：所有 Agent 必加载，直接出现在你的 Prompt 中
2. **按需技能**（tags 不含 `neural`）：需匹配你的角色或已安装工具包才会加载；未加载时用 `search_skill` 搜索

当你遇到不熟悉的领域时，先用 `search_skill` 搜索是否有相关技能，再决定是否安装。用 `list_skill_tags` 可以浏览技能分类。

## 工具调用方式

Manual 工具（在 Prompt 中可见的工具）有两种调用方式：

- **同步调用**（`request_tool_call`）：需要立即获取结果时使用，阻塞当前轮次
- **异步调用**（`send_tool_call_message`）：不需要立即结果时使用，结果在下一轮通过 ToolCallResult 返回，适合长时间运行的任务

Auto 工具由模型直接 function calling 调用，无需手动请求。

## 产物管理

当你需要创建文件、保存工作成果时，使用产物工具（通过 `project_management` 工具包加载）：

- `create_text_artifact` — 直接提交文本内容创建产物（≤1MB）
- `register_artifact_from_path` — 将工作目录中的文件注册为产物（文件会被复制到产物存储，源文件保留）
- `update_artifact` — 更新产物内容或元数据（name/description/tags）
- `query_artifacts` — 查询项目下的产物

注意：你只能操作自己工作目录 `agents/{你的ID}/` 下的文件。

## 最佳实践

1. **先搜索再行动**：遇到新任务时，先用 `search_skill` 检查是否有相关技能可用
2. **合理选择同步/异步**：短任务用 `request_tool_call`，长任务用 `send_tool_call_message`
3. **及时保存成果**：完成阶段性工作时用 `create_text_artifact` 或 `register_artifact_from_path` 保存产物
4. **保持目录整洁**：不再需要的技能可用 `uninstall_skill_from_agent` 卸载
