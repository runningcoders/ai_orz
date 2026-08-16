# 统一后台进程管理 + shell_exec 超时移交修复

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：进程管理与shell_exec修复 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。

> **文档状态**：草稿（5 个子模块设计冻结：call_id 单一事实源 / pkg/process 注册中心 / SystemDomain ProcessManager / shell_exec 重构 / shell_status shell_kill 双露工具）
> 查阅场景：
> - 排查「shell_exec 执行日志找不到 PID 反查链路断」按 §二 第 1 条 call_id 四关联检查
> - 排查 Agent 越权管理别人启动的进程，直接定位 §二 第 3 条权限边界红线
> - 新增任何 spawn 进程/产日志的工具，参考 §四 4.1 速查表接入 ctx.tool_call_id() 消费模式
> 关联文档：
> - 对应 design 文档：[tool_design.md](../design/tool_design.md) — 工具系统设计（统一进程管理架构 + shell_exec 超时 detach 语义）
> - 姊妹 Plan 与规范：
>   - [ARCHITECTURE.md](../ARCHITECTURE.md) — 唯一权威架构总纲（分层边界 + pkg 基础设施约定）
>   - [前端工具与进程管理.md](前端工具与进程管理.md) — 前端进程管理页面 + shell_list API 配套前端消费端
> - Wiki 长文真实路径：[docs/wiki/zh/content/功能模块/工具生态系统/运行时诊断工具组.md](docs/wiki/zh/content/功能模块/工具生态系统/运行时诊断工具组.md) — shell_exec/shell_status/shell_kill 进程管理双露工具
> - Wiki 长文真实路径：[docs/wiki/zh/content/功能模块/工具生态系统/工具生态系统.md](docs/wiki/zh/content/功能模块/工具生态系统/工具生态系统.md) — 工具系统全景，含进程管理器位置
> - RAG 卡真实路径 1：[docs/wiki/knowledge/zh/进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤/进程列表通过 shell_list LLM 工具双暴露，复用 Agent scope 过滤.md](docs/wiki/knowledge/zh/进程列表通过%20shell_list%20LLM%20工具双暴露，复用%20Agent%20scope%20过滤/进程列表通过%20shell_list%20LLM%20工具双暴露，复用%20Agent%20scope%20过滤.md)
> - RAG 卡真实路径 2：[docs/wiki/knowledge/zh/进程详情采用共享组件 + Modal 弹窗，不新建独立路由页/进程详情采用共享组件 + Modal 弹窗，不新建独立路由页.md](docs/wiki/knowledge/zh/进程详情采用共享组件%20+%20Modal%20弹窗，不新建独立路由页/进程详情采用共享组件%20+%20Modal%20弹窗，不新建独立路由页.md)

---

## 一、目标（为什么做）

三大已确认问题：shell_exec 超时直接 kill 进程丢失长任务产出；进程无统一管理面（PID/日志/状态散落在 shell_exec 内部，跨工具反查断链）；call_id 在 ToolCallDao::execute 内部生成导致 CoreTool::call 签名拿不到它（日志命名与进程追踪回退 log_id，同请求多次调用互相覆盖）。

| 问题维度 | 解决方式 |
|---------|---------|
| call_id 生成时机过晚（ToolCallDao 内部），下游工具拿不到做追踪 | call_id 升级为业务可指定的幂等键，单点收口：RequestContext 新增 `tool_call_id` 字段 + builder；ToolCallDao::execute 优先复用 ctx 已有，否则单点生成并注入后再调 CoreTool |
| 同请求多次 shell_exec 日志混在同一 log_id 命名文件 | shell_exec 日志文件名改 `{call_id}.log`；优先 ctx.tool_call_id()，缺失回退 ctx.log_id |
| shell_exec 超时 kill 进程，长异步任务被误杀 | 超时语义默认 detach：不 kill，写 registry 后返回 timeout 让调用者用 shell_status 查或 shell_kill 终止；新增可选 timeout_action="detach"\|"kill"（默认 detach） |
| 进程状态/探活/终止/kill 散落在工具内部，无统一入口 | 新增 pkg/process 纯基础设施（ProcessRegistry 全局单例 + Unix 原语 is_alive/terminate）；SystemDomain 提供带权限边界的 ProcessManager trait |
| shell_exec 与未来任何 spawn 进程工具无统一幂等防重机制 | ToolCallDao::execute 入口：call_id 为业务指定（非自动生成）时，先查 ToolCallLogger 历史；命中 Completed 直接返回（metadata 标 deduplicated=true），Failed 允许重试 |

**收敛后效果**：call_id 四端关联（ToolCallEntry.call_id ↔ 日志 {call_id}.log ↔ ProcessEntry.call_id+pid ↔ 工具返回 JSON call_id）任一可反查其余；shell_exec 同步与后台模式统一 spawn + 日志文件模型；进程经 SystemDomain 管理时有 Agent scope 校验（只能管自己启动的）；业务指定 call_id 时天然幂等。

---

## 二、架构思路（怎么做的）

五层从上到下：请求上下文携带 call_id → 执行层注入/复用 → 进程注册中心记录 → 工具消费写日志 → Handler/工具双露查询管理：

```
RequestContext（pkg 层）
  └─ 新增可选字段 tool_call_id: Option<String> + builder + getter

    ↓ 构造链路（业务指定优先）

ToolCallDao::execute（dao/tool_call 层）
  ├─ 取 id：ctx.tool_call_id() 有值 → 复用（触发防重检查）
  │                  无值 → 生成新 UUID v7 注入 ctx 后调 CoreTool::call
  ├─ 幂等防重（仅业务指定 call_id）：
  │    查 ToolCallLogger.read_call_by_id(tool_id, call_id)
  │      → Completed 命中 → 直接返回历史 output（entry.metadata.deduplicated=true）
  │      → Failed 命中    → 允许重试，正常执行
  └─ 构造 ToolCallEntry 用同一个 call_id

    ↓ 消费端（任何 spawn 进程/产日志的工具）

shell_exec 重构（pkg/tool_registry/shell_exec.rs）
  ├─ sync/background 统一：spawn 起就把 stdout/stderr 重定向到 {call_id}.log
  ├─ sync：等待结束后从日志文件读摘要（max_output_size_bytes 截断），全量留盘
  ├─ 超时语义：默认 detach → 返回 { timeout, call_id, pid, log_path }
  │            timeout_action=kill 时保留旧 kill 行为
  └─ spawn 成功即写 ProcessRegistry（同步/后台都注册）；退出 mark_exited

    ↓ 注册中心（pkg 纯基础设施，无业务感知）

pkg/process/mod.rs（新增，src/pkg/mod.rs 注册）
  ├─ ProcessEntry { pid, tool_id, call_id, agent_id/project_id/task_id,
  │                  command, working_dir, log_path, background, started_at,
  │                  status: ProcessStatus, exit_code, finished_at }
  ├─ ProcessRegistry（once_cell + Mutex<HashMap<pid, ProcessEntry>>）
  │     register / get / list / mark_exited / refresh(pid) / tail_log(path, n)
  └─ Unix 原语（libc）：is_alive(pid)=kill(pid,0)；terminate(pid)=SIGKILL
       Windows 桩：返回 unsupported

    ↓ 领域层权限边界（scope 校验）

SystemDomain：ProcessManager trait（system/mod.rs 或拆 process.rs 子文件）
  ├─ get_process / list_processes / kill_process / process_status(ctx, pid, tail_lines)
  └─ scope 规则：ctx.agent_id=Some → 必须与 entry.agent_id 匹配 → 不匹配 PermissionDenied
                  ctx 无 agent_id（人类/管理面）→ 放行

    ↓ 适配层：Handler + LLM 工具双露

handlers/system/process/
  ├─ shell_status.rs  ──#[register_handler_tool]── 双露：HTTP GET /processes/{pid}/status + LLM 工具 shell_status
  ├─ shell_kill.rs    ──#[register_handler_tool]── 双露：HTTP POST /processes/{pid}/kill  + LLM 工具 shell_kill
  └─ shell_list.rs    ──#[register_handler_tool]── 双露：HTTP GET /processes              + LLM 工具 shell_list
       shell_exec create_po() tags 追加 "shell" → 三件套 + shell_exec 可按 tag 分组绑定
```

**关键边界（行为红线，回归必保）**：
1. **call_id 四关联不破裂**：经 `ToolCallDao::execute` 调用的任何进程工具，必须同时满足 `entry.call_id == 日志文件名主干 == ProcessEntry.call_id == 返回 JSON.call_id`；pid 也须通过 entry.metadata/ProcessEntry/返回 JSON 三处互通
2. **幂等防重只对「业务指定 call_id」生效**：自动生成 UUID v7 的路径（=当前 awakening 全链路）绝不触发防重查询（避免每次调用多一次 JSONL 扫描）
3. **ProcessManager scope 校验**：ctx.agent_id 有值时必须与 entry.agent_id 严格匹配（Agent 只能管理自己启动的进程）；不匹配返回 PermissionDenied，绝不泄露别的 Agent 的进程状态或日志
4. **shell_exec 超时默认 detach**：未显式传 timeout_action=kill 时，超时绝不 SIGKILL 子进程（避免长任务被误杀）；返回体明确 message 提示用 shell_status/shell_kill 接管
5. **Windows 下仅提供桩实现**：is_alive/terminate 返回 unsupported 错误，不阻塞 CI（CI + 开发环境为 macOS/Linux）；ProcessEntry 元信息仍可读

---

## 三、涉及文件（改动清单 → 查代码直接跳）

按分层索引：

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **pkg 基础设施（新增/改造）** | | |
| [src/pkg/request_context.rs](src/pkg/request_context.rs) | RequestContext | 新增 `tool_call_id: Option<String>` 字段 + builder 方法 + getter；默认 None 不影响现有构造点 |
| [src/pkg/process/mod.rs](src/pkg/process/mod.rs) | 进程注册中心（新增） | ProcessEntry / ProcessStatus 结构体；ProcessRegistry 全局单例（register/get/list/mark_exited/refresh/tail_log）；Unix 原语（libc）is_alive/terminate；Windows 桩 unsupported |
| [src/pkg/mod.rs](src/pkg/mod.rs) | pkg 模块注册 | 新增 `pub mod process;` |
| Cargo.toml（workspace 根） | 依赖声明 | 新增 `libc` 依赖 |
| **DAO 层（call_id 注入 + 幂等）** | | |
| [src/service/dao/tool_call/impl.rs](src/service/dao/tool_call/impl.rs#L78) | ToolCallDao::execute | 取 id 顺序：ctx.tool_call_id() → 复用；否则 UUID v7 生成并 ctx.to_builder().tool_call_id(id).build()；幂等防重逻辑（仅业务指定路径） |
| **工具层（shell_exec 重构）** | | |
| [src/pkg/tool_registry/shell_exec.rs](src/pkg/tool_registry/shell_exec.rs) | shell_exec 核心 | sync/background 统一 spawn + stdout/stderr 重定向 `{call_id}.log`；超时语义默认 detach + timeout_action 参数；ProcessRegistry 注册/注销；返回 JSON 补 call_id 字段；parameters_schema 同步；tags 追加 "shell" |
| [src/pkg/tool_tracing/logger.rs](src/pkg/tool_tracing/logger.rs) | ToolCallLogger | 复用既有 `read_call_by_id` 做幂等防重反查（限定 tool 目录扫描） |
| **Domain 层（权限边界）** | | |
| [src/service/domain/system/mod.rs](src/service/domain/system/mod.rs) | SystemDomain | 新增 `ProcessManager` trait 定义；由 SystemDomainImpl 实现（可拆 process.rs 子文件）；scope 校验 + 调 pkg/process 原语；get/list/kill/status(tail_lines) 4 方法 |
| **Adapter 层（Handler + LLM 工具双露）** | | |
| common/src/api/system.rs（或对应分组） | 参数/响应 DTO | 新增 ShellStatusParams/Response、ShellKillParams/Response、ShellListResponse（对齐 shell_exec 返回结构） |
| [src/handlers/system/process/shell_status.rs](src/handlers/system/process/shell_status.rs) | shell_status（新增） | `#[register_handler_tool(id="shell_status", tags="shell")]` + `#[generate_http_handler]`；调 system domain process_status |
| [src/handlers/system/process/shell_kill.rs](src/handlers/system/process/shell_kill.rs) | shell_kill（新增） | 同上：id="shell_kill" tags="shell"；调 kill_process |
| src/handlers/system/process/shell_list.rs | shell_list（新增，前端配套） | 同上：id="shell_list" tags="shell"；list_processes（见 [前端工具与进程管理.md](前端工具与进程管理.md) 配套） |
| src/handlers/system/mod.rs + router.rs | Handler 路由注册 | system nest 下注册 /processes GET、/processes/{pid}/status GET、/processes/{pid}/kill POST |
| **文档修订** | | |
| [docs/archive/design-archive/tool_design.md](docs/archive/design-archive/tool_design.md) | 工具设计（只追加不改旧文） | 追加「2026-08-09 更新」章节：同步默认 + Agent background 调用级决策 + dispatch_mode=async 仅显式配置；shell_exec 超时 detach；统一进程管理架构 |
| [AGENTS.md](AGENTS.md) | 功能总览表 | 新增「统一后台进程管理」功能行；测试统计口径同步 |

---

## 四、分发点速查表（新增同类功能第一站）

### 4.1 新增任何 spawn 进程/产日志的工具（接入 call_id 追踪 + 进程注册）

| 步骤 | 动作 | 参考入口 |
|------|------|---------|
| 1 | 优先取 `ctx.tool_call_id()`，缺失回退 `ctx.log_id` 用于日志文件命名 | [shell_exec.rs](src/pkg/tool_registry/shell_exec.rs) 日志命名段 |
| 2 | spawn 成功后 `ProcessRegistry::global().register(ProcessEntry{ ... call_id: ctx.tool_call_id(), agent_id: ctx.agent_id(), project_id: ctx.project_id(), task_id: ctx.task_id() })` | shell_exec.rs 注册段 |
| 3 | 返回 JSON 中携带 `{ call_id, pid, log_path }` 字段（确保 JSONL call_trace.output 反查链路） | shell_exec.rs 返回体构建段 |
| 4 | 进程退出后 `ProcessRegistry::global().mark_exited(pid, exit_code)` | shell_exec.rs sync/background 退出收尾段 |

> 核心调用：`ctx.tool_call_id()`（`RequestContext` getter）——**任何工具都不自己生成追踪 call_id**，一律消费 ctx 中已注入的。

### 4.2 新增双露管理类工具（HTTP + LLM 工具共用 Handler）

| 分发点 | 处理逻辑 | 新增时参考 |
|-------|---------|-----------|
| Handler 骨架模式 | `#[register_handler_tool(id="xxx", tags="shell")]` + `#[generate_http_handler]` 宏双露；内部调对应 system domain manager 方法 | [src/handlers/system/process/shell_kill.rs](src/handlers/system/process/shell_kill.rs) + shell_status.rs 三件套 |
| DTO 契约放置 | `common/src/api/system.rs`（或对应域分组） | 现有 system API DTO 同目录 |

---

## 五、验收清单

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要

| 模块 | 验证结果 |
|------|---------|
| pkg/process 单测（register/get/list/tail_log + sleep 进程探活终止） | 待执行 |
| shell_exec 新增进程用例（timeout detach + kill 分支；现有 7 个解析类回归） | 待执行 |
| call_id 四关联 + ctx.tool_call_id 缺失回退兼容 | 待执行 |
| 幂等防重 3 分支（Completed 直接返回 / Failed 重试 / 自动生成 ID 不触发） | 待执行 |
| ProcessManager scope 三分支（不匹配拒 / 匹配放行 / 无 agent 放行）+ kill 生效 | 待执行 |
| shell三件套 Handler 双露路由注册 | 待执行 |
| Clippy + fmt | 执行状态：已按项目规范执行 clippy 校验与 fmt 格式化流程 |
| AGENTS.md 功能条目/测试数更新 | 待同步 |

### 与计划的偏离
暂无。本轮不含 HTTP 工具前端创建表单 + PUT/DELETE 方法放开（按原假设与边界，另行安排独立计划）。

---

## 七、后续扩展路径（4 步模板）

> **核心不变量**：call_id 四关联（红线 #1）；幂等防重仅业务指定路径触发（红线 #2）；ProcessManager scope 校验（红线 #3）。

1. **进程注册中心 DB 持久化**：v1 内存版（重启丢失）→ 落 DB 新表 processes；pkg/process 抽象 ProcessStore trait，现有内存版改 InMemoryProcessStore，新增 SQLiteProcessStore 实现
2. **前端进程管理页内嵌日志 tail WebSocket**：当前 shell_status 返回一次性 tail_lines；后续升级 SSE/WS 推送日志增量（复用现有 AOP 事件中心），入口 [前端工具与进程管理.md](前端工具与进程管理.md) ProcessDetailContent 组件
3. **Windows 进程原语补齐**：`#[cfg(windows)]` 下用 winapi 或者 windows crate 实现 is_alive/terminate；CI 加 windows runner 跑对应测试
4. **除 shell_exec 外的进程类工具接入**：如新增 `python_exec` / `docker_run` 等进程类工具，按 §四 4.1 四步接入（call_id 消费 → registry 注册 → 返回体携带 → 退出标记）；进程元数据字段（agent_id/project_id/task_id）已在 ProcessEntry 预留