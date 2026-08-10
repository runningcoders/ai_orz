---
kind: design
name: 进程列表通过 shell_list LLM 工具双露（HTTP + Tool Call）
source: session
category: adr
---

# 进程列表通过 shell_list LLM 工具双露（HTTP + Tool Call）

_来源：eb51721 → 8be1663 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
Agent 需要能查询当前上下文下的运行进程，且应与已有的 shell_status/shell_kill 构成三件套，并复用 Agent scope 过滤。

## 决策驱动
- 统一暴露面
- Agent 能力闭环
- 复用现有 scope 机制

## 备选方案
- **单独注册 HTTP 接口 /api/v1/system/processes** _（已否决）_ — 优点：前端可直接调用；缺点：Agent 无法直接发现；需额外封装
- **通过 LLM tool `shell_list` 暴露，并在 router 上挂同名 HTTP 端点** — 优点：Agent 与前端共享同一 handler；自动继承 Agent scope 过滤；与 shell_status/shell_kill 风格一致；缺点：HTTP 端点本质是 tool 的包装

## 决策
在 `common/src/api/system.rs` 定义 `ListProcessesRequest/Response`，以 `#[register_handler_tool(id = "shell_list", tags = "shell")]` 注册 LLM 工具，同时在 `src/router.rs` 的 system nest 下挂 `GET /processes` 指向同一 handler，实现前后端双暴露。

## 影响
进程列表查询天然受 Agent scope 约束；Agent 与前端使用同一数据源与权限模型；后续新增进程相关能力应沿用此模式。