---
kind: wiki_knowledge_card
name: 思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件
category: 前端HTTP观测链路
scope:
  - "src/handlers/hr/agent/runtime_*.rs"
  - "src/handlers/hr/agent/cancel_thinking.rs"
  - "common/src/api/runtime.rs"
  - "frontend/src/components/runtime_panel.rs"
  - "src/pkg/agent_runtime_state.rs"
source_files:
  - src/handlers/hr/agent/runtime_status.rs:Ln-Lm
  - src/handlers/hr/agent/runtime_list.rs:Ln-Lm
  - src/handlers/hr/agent/cancel_thinking.rs:Ln-Lm
  - common/src/api/runtime.rs:Ln-Lm
  - frontend/src/components/runtime_panel.rs:Ln-Lm
  - src/pkg/agent_runtime_state.rs:Ln-Lm
  - docs/design/thinking_task_policy_engine_design.md
  - （2026-09-04 清理：superpowers 目录已归档，待 doc-maintainer 跟进）
  - docs/wiki/zh/content/前端应用/组件系统/业务组件/思考运行时面板观测接口.md
---

# 思考运行时前端观测接口与面板

## §1 整体方案

思考运行时观测对外暴露 3 个 HTTP Handler + 1 个前端组件：(a) **GET /agents/{id}/runtime-status** 查询单 Agent 状态 + 思考快照（如果在思考）；(b) **GET /agents/runtime-list** 列出全部 Busy/Resting 状态的 Agent + 每 Agent 精简思考快照 + 统计摘要；(c) **POST /agents/{id}/cancel-thinking** 取消指定 Agent 的思考（原子信号 + 幂等返回 was_thinking）；(d) **frontend/src/components/runtime_panel.rs** 前端组件，轮询 (a)/(b)，展示思考进度条 + 轮次/token 表格 + 取消按钮，取消后 toast 提示。DTO 统一在 common/src/api/runtime.rs（前后端共用单一事实源）。

链路：Handler (鉴权+参数校验) → RuntimeDomain.runtime_status() / runtime_list() / cancel_think() → StateManager（agent_runtime_state.rs 内存态 DashMap 读/写 cancel_token）→ 返回 ApiResponse<RuntimeStatusResponse / RuntimeListResponse / CancelThinkingResponse>。Frontend panel 用 use_resource 每 500ms 轮询 runtime-status（单 Agent 对话页）或 runtime-list（组织/概览页），cancel 按钮 POST cancel-thinking，按钮 disable 直到 was_thinking=true 时的下一次轮询返回状态 idle。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [src/handlers/hr/agent/runtime_status.rs](src/handlers/hr/agent/runtime_status.rs) | GET /agents/{id}/runtime-status | Path(agent_id) + Query(optionally with_snapshot_detail=true) → RuntimeStatusResponse { status: AgentStatus, think_snapshot: Option<ThinkRuntimeSnapshot> }。鉴权：组织下成员可见；SuperAdmin 不受限 |
| [src/handlers/hr/agent/runtime_list.rs](src/handlers/hr/agent/runtime_list.rs) | GET /agents/runtime-list | PagedResult<RuntimeListItem>（organization_id 过滤当前组织），每个 item：agent_id/name/avatar/status/scene/rounds/elapsed/tokens/cancelable；Query 支持过滤 status: Busy/Resting/All。默认按 updated_at desc |
| [src/handlers/hr/agent/cancel_thinking.rs](src/handlers/hr/agent/cancel_thinking.rs) | POST /agents/{id}/cancel-thinking | CancelThinkingResponse { success: bool, was_thinking: bool }：success=true 表示命令已提交（cancel_token=true）；was_thinking=true 表示命中了正在思考；前端用 was_thinking 决定 toast 文案（取消成功 vs 无需取消）。Handler 必须返回 200 + 结构体，禁止裸 bool |
| [common/src/api/runtime.rs](common/src/api/runtime.rs) | DTO 单一事实源（前后端共用）| `RuntimeStatusRequest` / `RuntimeStatusResponse` / `RuntimeListRequest` / `RuntimeListResponse` / `RuntimeListItem` / `CancelThinkingResponse` / `ThinkRuntimeSnapshotDTO` 全部定义在此；禁止前端/handler 本地镜像 |
| [frontend/src/components/runtime_panel.rs](frontend/src/components/runtime_panel.rs) | 前端组件（RuntimePanel）| props: agent_id (single) 或 list_mode=true；use_resource 轮询：500ms interval（思考中）或 2s interval（空闲，降低成本）；取消按钮：POST cancel-thinking → was_thinking=true 时加 1s loading；UI：进度条（rounds / max_rounds 百分比）+ 2 列表格：Key/Value（场景 Scene / 当前轮次 / 已用时间 / 输入输出 tokens / 当前工具调用 / 最近退出原因）|
| [src/pkg/agent_runtime_state.rs](src/pkg/agent_runtime_state.rs) | 内存态实现（StateManager）| 三个 Domain 方法在 `impl AgentRuntimeStateManager`：fn runtime_status_snapshot / fn runtime_list(organization_id, pagination) / fn cancel_think(agent_id)。DashMap 读不加锁（DashMap 自身并发安全） |
| 【Wiki 长文】思考运行时面板观测接口.md（新建）| 系统化上下文 §1-§10 | [思考运行时面板观测接口](docs/wiki/zh/content/前端应用/组件系统/业务组件/思考运行时面板观测接口.md) |
| 【① Design】thinking_task_policy_engine_design.md §三 L60-L80 接口层架构图 | 接口层 vs 状态管理层数据流图 | [docs/design/thinking_task_policy_engine_design.md](docs/design/thinking_task_policy_engine_design.md) |
| 【② Plan】执行蓝图 §File Structure 表格 | 改动文件清单 + Handler/前端组件变更摘要 | （2026-09-04 清理：superpowers 目录已归档，待 doc-maintainer 跟进）|

## §3 架构约定

1. **Handler 只鉴权+参数校验，禁止直接访问 AgentRuntimeStateManager**：Handler → RuntimeDomain.awakening().runtime_status(...) 三个能力必须走 Domain 层（符合 §3.1 分层架构：Handler 不能直接调 pkg 层）。RuntimeDomain 内部再持有 &StateManager 引用。
2. **runtime-list 分页与 count 复用通用 count 规范（§4.9）**：RuntimeListRequest 含 Pagination（page_size/page_no），DAO/Domain 层统一 count(query) 透传（本场景虽然是内存态 DashMap，但必须遵守 PagedResult<T> 形状，以便未来落盘时 Handler/前端零改动）。
3. **DTO 只定义在 common/src/api/runtime.rs**：handler 与 frontend 均用 `use common::api::runtime::*`；禁止 frontend/src/api/ 本地镜像；禁止裸 bool/()/String 响应（CancelThinkingResponse 即使只有 2 字段也结构体化）。
4. **轮询退避策略**：前端 panel 轮询间隔思考中 500ms / 空闲 2s；连续 10 次 idle → 退到 5s；收到消息 SSE 推送「思考开始」事件 → 立即重置为 500ms。
5. **取消接口幂等性**：连续 3 次点击 POST cancel-thinking，返回 success=true / was_thinking 只有第一次 true（其余 false）——前端组件必须容忍 was_thinking=false 这种"重复点击"，不展示错误 toast。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 runtime-status/runtime-list 返回敏感信息**：ThinkRuntimeSnapshot 禁止暴露完整 prompt/历史消息（只允许暴露统计值、工具调用名、场景）。任何含用户隐私的字段一律不进入 RuntimeListResponse（列表页更严格：只有 agent 基本信息+思考统计，完全无工具名）。
2. ❌ **禁止 Handler 本地造 RuntimeListItem**：列表页响应的每个 item 字段（rounds/tokens/elapsed）必须从 AgentRuntimeInfo.think_runtime.as_ref() 读；禁止写死 0/false/"" 占位。
3. ✅ **DTO 与状态流转保持一致**：ThinkRuntimeSnapshot 所有字段与 src/pkg/agent_runtime_state.rs 中定义的内存结构体**字段一一对应，名称完全相同**；前端不需要二次映射（减少漂移）。
4. ✅ **前端取消按钮 loading + disable**：POST 请求期间按钮 disable + 显示 loading；请求返回后 disable 保留 500ms（避免下一轮轮询前的双击重复提交），直到下一次轮询拿到新状态。
5. ✅ **测试强制覆盖**：Handler 三个接口每个至少 2 条单元测试（思考中 vs 未思考状态）；CancelThinkingResponse.was_thinking 真假两种情况分别断言。
6. ✅ **四类互引闭环**：本卡含 wiki 长文绝对路径 1 条 + Design + Plan；对应新建的 wiki 长文 cite 段必须回链本卡 + Design + Plan。
