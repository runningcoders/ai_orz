# 工作台顶栏聚合指标设计

> 🎯 **本文档定位**：工作台顶栏统计指标聚合端点（`GET /api/v1/system/workspace/metrics`）的设计决策快照——为什么聚合、聚合什么、刷新节奏怎么定；接口细节以实际代码为准
> 状态：v1.0（2026-09-16 落地）
> 查阅场景：需要理解顶栏指标为何单端点单轮询、新增顶栏指标往哪加、各维度数据源与降级策略时打开
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 项目整体分层架构
> - [health_metrics.rs](src/handlers/system/health_metrics.rs) — 同域前置范例：跨域聚合编排先例（本设计沿用其模式）
> - 暂无对应 plan 文档（一次性小特性，落地过程见 git 历史）

---

## 一、目标与决策表

### 1.1 背景

改造前顶栏指标由前端分散维护：

| 指标 | 旧数据源 | 旧轮询 | 问题 |
|------|---------|--------|------|
| 项目/Agent/运行中/忙碌 4 卡 | sidebar 资源（全量列表前端计数） | 随图数据加载 | 忙碌卡与思考点口径不一致 |
| 空闲/思考/休息三色 | `runtime-list` 接口 | 5s | 与其余指标节奏割裂，高频空转 |
| 模型/工具窗口读数 | `token-stats` + `tools/runtime-stats` | 30s | 两个接口各自请求 |
| 队列积压 | aop backlog 接口 | 30s | 同上 |

核心诉求（用户原话驱动）：「顶部这些数据，如果刷新时间基本一致，并且都是一些单个数字的值，且过滤条件一致，我们是不是考虑增加一个接口，专门来聚合这些信息呢？作为统一监控指标？」——即：一次请求拿全顶栏，一个节奏刷新，数字同源。

### 1.2 决策表

| # | 问题 | 决策 | 理由 |
|---|------|------|------|
| 1 | 聚合端点放哪个模块？ | system handler 域（与 health_metrics 同位） | 各域 handler 只管本域明细；「跨域聚合读数」属系统监控职责，放任何业务域都会造成反向依赖观感 |
| 2 | 数据怎么来？ | handler 直接编排调用 project / hr / runtime / finance / system 五个 domain 的现成方法 | 不新增 domain 能力；handler→domain 单向调用合法；聚合逻辑只发生在编排层 |
| 3 | 双分层（5s 实时通道 + 30s 统计通道）还是单接口？ | **单接口 + 30s 统一轮询**（用户拍板「5S 和 30S 也差不了太多，干脆合并成一个吧」） | 三色计数降频到 30s 观感可接受；单接口把请求数从 5s 一发 + 30s 三发收敛为 30s 一发，DuckDB 统计查询不再被高频触发 |
| 4 | 首帧如何避免 0 值闪现？ | 前端信号设计为 `Option<WorkspaceMetricsResponse>`，None 时顶栏整体不渲染 | 快照语义天然支持；顶栏不再依赖 sidebar 数据到达，独立成帧 |
| 5 | 单维度查询失败怎么办？ | 降级为 0 值 + `log_error!`，整体快照照常返回 | 监控读数可用性优先；一个 DuckDB 查询失败不应拖垮整个顶栏 |

### 1.3 数据源口径（改造后）

| 指标 | 数据源 | 存储介质 | 实时性 |
|------|--------|---------|--------|
| project_count | `count_projects(ProjectQuery::default())` | SQLite | 实时 |
| active_project_count | `count_projects(status_in=[InProgress, Completed, Archived])` | SQLite | 实时（与前端 `is_active_project` 同口径） |
| agent_count | `count_agents(exclude_status=Deleted)` | SQLite | 实时 |
| runtime 三色 | `list_runtime_agents` 遍历计数 | 内存态（DashMap） | 实时权威（未注册运行的 Agent 不计入） |
| model 读数 | `model_call_time_series` 求和 | DuckDB | 批次刷盘，最近 1~2 分钟未落库属预期 |
| tool 读数 | `tool_call_stats` | DuckDB | 同上 |
| queue_backlog | `all_queue_stats` 累加 pending_count | 内存态 | 实时 |

## 二、架构

```
前端 workspace.rs
  │  30s 单轮询 use_future（首帧立即执行）
  │  GET /api/v1/system/workspace/metrics?minutes=60
  ▼
Adapter: system/workspace_metrics（双宏 handler，只编排）
  ├─ project_domain().project_manage().count_projects   → 概览两数
  ├─ hr_domain().agent_manage().count_agents            → Agent 总数
  ├─ runtime_domain().list_runtime_agents               → 三色计数（内存）
  ├─ finance_domain()  model_call_time_series / tool_call_stats → 窗口读数（DuckDB）
  └─ system_domain().aop_monitor().all_queue_stats      → 队列积压（内存）
  ▼
单一快照 WorkspaceMetricsResponse
  ▼
前端 Option 信号 → Some(m) 时顶栏整体渲染（概览卡 + 三色 + 8 项数字指标）
```

响应契约（DTO 单一事实源在 common）：

```rust
pub struct WorkspaceMetricsResponse {
    pub project_count: u64,
    pub agent_count: u64,
    /// 运行中项目数（status 1..=3，与前端 is_active_project 同口径）
    pub active_project_count: u64,
    pub runtime: AgentRuntimeCounts,   // idle / busy / resting
    pub window_minutes: u32,
    pub model: ModelUsageMetrics,      // total_calls / tokens_input / tokens_output
    pub tool: ToolUsageMetrics,        // total_calls / failed_calls / avg_duration_ms
    pub queue_backlog: u64,
}
```

> 当前实现：[system.rs::WorkspaceMetricsResponse](common/src/api/system.rs#L491-L516)

## 三、涉及文件清单

| 层 | 文件 | 说明 |
|----|------|------|
| ① DTO | [system.rs](common/src/api/system.rs#L481-L547) | 聚合 DTO 五件套（请求/响应/三色/模型读数/工具读数），经 `common::api` 全量转发 |
| ② Adapter | [workspace_metrics.rs](src/handlers/system/workspace_metrics.rs#L33-L143) | 双宏 handler，五 domain 编排 + 降级 |
| ② Router | [router.rs](src/router.rs#L1049-L1053) | `/workspace/metrics` 手动注册 |
| ③ 前端 API | [system.rs](frontend/src/api/system.rs#L211-L214) | `get_workspace_metrics` 封装（query 传 minutes） |
| ③ 前端信号 | [workspace.rs](frontend/src/pages/workspace.rs#L263-L266) | `Option<WorkspaceMetricsResponse>` 信号 |
| ③ 前端轮询 | [workspace.rs](frontend/src/pages/workspace.rs#L275-L290) | 30s 单循环（旧 5s/30s 双循环删除） |
| ③ 前端顶栏 | [workspace.rs](frontend/src/pages/workspace.rs#L943-L1030) | 顶栏整体改从单一快照渲染，None 不渲染 |

同步删除的前端死代码（失去唯一消费点）：`frontend/src/api/finance.rs` 的 `get_token_stats` / `get_tool_runtime_stats`、`frontend/src/api/hr.rs` 的 `list_runtime_agents`，及其对应 DTO re-export。

## 四、边界与行为红线

- **handler 只做编排**：所有计数/统计/遍历均调用 domain 现成方法，聚合 handler 内禁止出现 SQL、业务规则或跨域事务。
- **DTO 单一事实源**：请求/响应结构体定义在 `common/src/api/system.rs`，双端各自 import，禁止前端本地重复定义。
- **旧细粒度端点保留**：后端 `token-stats` / `tools/runtime-stats` / `runtime-list` 路由与 handler 不动（其他消费方与调试仍可用）；仅前端失去唯一消费点的三个封装函数被清理。
- **批次刷盘语义**：model/tool 读数偏低（最近 1~2 分钟）不是故障，是统计事件批次落盘的固有延迟，前端注释与本文档均明确标注。
- **窗口参数防滥用**：`minutes` 缺省 60，clamp 至 1..=1440。
- **双宏强制**：`register_handler_tool` + `generate_http_handler`（CODE_STANDARDS §7.2），路由在 `system_routes()` 手动注册。

## 五、扩展模式

新增顶栏指标的三步：

1. DTO：在 `WorkspaceMetricsResponse`（或其子结构体）加字段；
2. 后端：`workspace_metrics.rs` 对应维度赋值（优先复用 domain 现成方法，无现成能力则先在 domain 层补）；
3. 前端：顶栏 RSX 直接从 `m` 快照消费，无需新端点、新轮询、新信号。

若未来某指标需要亚 30s 的实时性，方向是 SSE 推送而非提高轮询频率——单接口快照的「同源性」价值高于个别指标的刷新延迟。
