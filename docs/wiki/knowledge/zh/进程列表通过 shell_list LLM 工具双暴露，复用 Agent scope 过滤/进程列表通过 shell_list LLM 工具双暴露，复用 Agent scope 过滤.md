---
kind: design
name: 进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤
source: session
category: adr
scope:
    - 'src/pkg/tool_registry/builtin.rs'
source_files:
    - docs/wiki/zh/content/功能模块/系统管理/后台进程管理系统.md
---

# 进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤

_来源：a756890 → eb51721 提交周期内记录的编码计划——内容为规划时意图，实现可能滞后或有出入。_

**状态：** accepted

## 背景
需要让 Agent 能查询当前上下文下的运行进程，同时保留 HTTP API 供前端系统管理页面使用；两种调用方对可见性要求不同（用户 ctx 全量、agent ctx 仅见自身）。

## 决策驱动
- 统一进程访问入口
- 复用现有 Agent scope 机制避免重复鉴权逻辑
- HTTP 与 tool_call 共享同一后端实现

## 备选方案
- **单独新增 HTTP-only 接口 + 独立 tool 实现** _（已否决）_ — 优点：职责清晰；缺点：重复实现 list/refresh 逻辑，维护两份代码
- **只暴露为 LLM tool，不开放 HTTP** _（已否决）_ — 优点：最小化 API 面；缺点：前端无法直接获取进程列表，需额外封装
- **通过 #[register_handler_tool] 注册 shell_list 并同时生成 HTTP handler** — 优点：单一实现、自动复用 scope 过滤、路由由 macro 生成；缺点：HTTP 路径与 tool id 绑定

## 决策
在 `src/handlers/system/process/shell_list.rs` 中用 `#[register_handler_tool(id = "shell_list", tags = "shell")]` + `#[generate_http_handler]` 将进程列表同时暴露为 LLM 工具和 `GET /processes`，内部委托 `system::domain().process_manager().list_processes(ctx)`，由 ctx 决定用户态全量或 agent 态仅自身。

## 影响
进程列表的权限模型与现有 tool 一致，新增 tool 无需再写鉴权；但 HTTP 路由顺序必须把 `/processes` 放在 `/processes/{pid}` 之前，否则会被路由遮蔽。