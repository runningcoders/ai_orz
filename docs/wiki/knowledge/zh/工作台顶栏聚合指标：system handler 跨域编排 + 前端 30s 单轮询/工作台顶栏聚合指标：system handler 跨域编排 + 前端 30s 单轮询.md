---
kind: knowledge_card
name: 工作台顶栏聚合指标：system handler 跨域编排 + 前端 30s 单轮询
category: 前端应用 / 工作台
scope:
  - "src/handlers/system/workspace_metrics.rs"
  - "src/handlers/system/mod.rs"
  - "common/src/api/system.rs"
  - "src/router.rs"
  - "frontend/src/pages/workspace.rs"
  - "frontend/src/api/system.rs"
source_files:
  - "src/handlers/system/workspace_metrics.rs#L33-L143"
  - "common/src/api/system.rs#L481-L547"
  - "src/router.rs#L1049-L1053"
  - "frontend/src/pages/workspace.rs#L263-L290"
  - "frontend/src/pages/workspace.rs#L943-L1030"
  - "frontend/src/api/system.rs#L211-L214"
  - "docs/design/workspace_topbar_metrics_design.md"
  - "docs/wiki/zh/content/核心模块/处理器层/System模块处理器/System模块处理器.md"
  - "docs/wiki/zh/content/API 参考/RESTful API/系统管理模块 API/系统管理模块 API.md"
  - "docs/wiki/zh/content/项目概述/核心功能特性/多维统计系统/多维统计系统.md"
---

## §1 概述

工作台顶栏聚合端点 `GET /api/v1/system/workspace/metrics` — system handler 域的跨域编排 handler，一次响应覆盖顶栏全部数字。项目/Agent 概览（SQLite 实时）+ 运行态三色计数（runtime 内存态，未注册运行不计）+ 模型/工具窗口读数（DuckDB 批次刷盘，最近 1~2 分钟未落库属预期）+ AOP 队列积压（system 内存态）。前端 30 秒单轮询消费；首帧 None 时顶栏整体不渲染避免 0 值闪现。

## §2 关键文件表

| 文件 | 说明 |
|------|------|
| src/handlers/system/workspace_metrics.rs#L33-L143 | 双宏 handler，五 domain 编排 + 降级；单维度失败 log_error! 后按 0 呈现 |
| common/src/api/system.rs#L481-L547 | 聚合 DTO 五件套（GetWorkspaceMetricsRequest / WorkspaceMetricsResponse / AgentRuntimeCounts / ModelUsageMetrics / ToolUsageMetrics） |
| src/router.rs#L1049-L1053 | /workspace/metrics 路由，system_routes() 手动注册 |
| frontend/src/pages/workspace.rs#L263-L290, L943-L1030 | Option 信号 + 30s 单轮询 + 顶栏从单一快照渲染 |
| frontend/src/api/system.rs#L211-L214 | get_workspace_metrics 封装（query 传 minutes） |

对应 Wiki 长文：[System模块处理器](docs/wiki/zh/content/核心模块/处理器层/System模块处理器/System模块处理器.md)、[系统管理模块 API](docs/wiki/zh/content/API%20参考/RESTful%20API/系统管理模块%20API/系统管理模块%20API.md)、[多维统计系统](docs/wiki/zh/content/项目概述/核心功能特性/多维统计系统/多维统计系统.md)

## §3 架构约定

- 数据源分工：SQLite（概览两数）+ runtime 内存态（三色计数）+ DuckDB（模型/工具窗口读数，批次刷盘语义）+ system 内存态（队列积压），一个响应四种存储介质
- 降级策略：模型/工具 DuckDB 查询失败 → log_error! + default()；概览 SQLite 查询失败 → unwrap_or(0)；整体快照照常返回
- 窗口参数：minutes 缺省 60，clamp 至 1..=1440；model_call_time_series 和 tool_call_stats 共用同一窗口保证口径一致
- 前端信号：Option<WorkspaceMetricsResponse>，None 时顶栏整体不渲染（避免 sidebar 未到时的 0 值闪现），与 sidebar 数据到达解耦
- 旧细粒度端点保留：后端 token-stats / tools/runtime-stats / runtime-list 路由不动（其他消费方与调试仍可用）；前端失去唯一消费点的 get_token_stats / get_tool_runtime_stats / list_runtime_agents 封装删除

## §4 硬约束

- handler 只做编排：所有计数/统计/遍历均调用 domain 现成方法，禁止出现 SQL、业务规则或跨域事务，避免反向依赖观感
- DTO 单一事实源：请求/响应结构体定义在 common/src/api/system.rs，双端各自 import，禁止前端本地重复定义
- `runtime_domain().list_runtime_agents(None, None, None)` 返回未注册运行的 Agent 不计入三色计数，是权威实时；不要遍历 HR 表或缓存做兜底
- 批次刷盘语义：model/tool 读数偏低属预期（最近 1~2 分钟未落库），前端与后端注释均需明确标注为统计事件批次落盘延迟而非故障
- 双宏强制：register_handler_tool + generate_http_handler（CODE_STANDARDS §7.2），路由在 system_routes() 手动注册
- 顶栏单轮询节奏固定 30s，未来需要亚 30s 实时性时方向是 SSE 推送而非提高轮询频率
