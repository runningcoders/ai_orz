# 统一后台进程管理 + shell_exec 超时移交修复

## 背景与决策

- 同步/异步定调：**同步是默认**（现状代码即如此），`dispatch_mode=async` 消息链路仅留给显式配置的重型工具；Agent 通过 `shell_exec` 的 `background` 参数做调用级决策（轮询式异步）。代码无需改，修订设计文档即可。
- 进程管理采用统一模块：pkg 层纯注册中心（无业务感知），SystemDomain 层提供带权限边界的管理能力，工具经 `#[register_handler_tool]` 宏双露（HTTP + LLM 工具，复用 `request_tool_call` 既有模式）。
- 进程注册中心第一版为内存版（服务重启条目丢失可接受，审计线索保留在 ToolCallEntry JSONL metadata）。

## call_id 单一事实源与全链路关联（新增）

现状断裂：`call_id` 在 `ToolCallDao::execute`（src/service/dao/tool_call/impl.rs:78）内部生成，`CoreTool::call(ctx, args)` 签名拿不到它；shell_exec 日志只能用请求级 `ctx.log_id` 命名（同一请求多次调用会混在一个日志文件），ProcessEntry 也无 call_id。

方案：call_id 升级为业务可指定的幂等键，单点收口在执行层：

1. `RequestContext`（src/pkg/request_context.rs）新增可选字段 `tool_call_id: Option<String>` + builder 方法 + getter，默认 None 不影响现有构造
2. `ToolCallDao::execute` 取 id 顺序：`ctx.tool_call_id()` 有值（业务指定）→ 直接复用；无值 → 生成新 UUID v7 并 `ctx.to_builder().tool_call_id(call_id).build()` 注入后再调 `CoreTool::call`；后续构造 ToolCallEntry 使用同一个 call_id
3. shell_exec 使用 `ctx.tool_call_id()`（缺失时回退 `ctx.log_id`）作为日志文件名 `{call_id}.log`，并写入 ProcessEntry.call_id
4. shell_exec 返回 JSON 已含 `pid` / `log_path`，补充 `call_id` 字段 —— JSONL call_trace 的 output 自然携带 pid，满足"执行日志带 PID"的反查需求

### 幂等防重（一并实现，成本低：反查能力已现成）

`ToolCallDao::execute` 入口处仅当 call_id 为**业务指定**时（自动生成不查，新 UUID 永不命中，避免每次调用多一次 JSONL 扫描）：

- 调 `ToolCallLogger::get().read_call_by_id(Some(tool_id), call_id)`（src/pkg/tool_tracing/logger.rs 已实现，限定 tool 目录扫描）
- 命中且 `status=Completed` → 直接返回历史 output 与历史 entry（entry.metadata 标 `deduplicated=true`），不重复执行
- 命中且 `status=Failed` → 允许重试，正常执行（失败不该永久钉死）
- 未命中 → 正常执行；本轮 awakening 链路 call_id 均为自动生成，不会命中防重，行为无变化

关联链路：`ToolCallEntry.call_id`（JSONL）↔ 日志文件名 `{call_id}.log` ↔ `ProcessEntry.call_id + pid` ↔ 工具返回 JSON 的 pid，任一端均可反查其余。

生成/使用语义（明确）：
- **生成端**：业务指定优先（ctx 已携带 `tool_call_id` 则复用，这是防重的前提）；未指定时由 `ToolCallDao::execute` 单点生成（每次一个新 UUID v7），工具自身不生成追踪 call_id
- **消费端**：后续所有需要关联 id 的工具（shell_exec 及未来任何 spawn 进程/产文件的工具）一律优先取 `ctx.tool_call_id()`；仅当 ctx 未注入（测试直接调 CoreTool、绕过 execute 链路的场景）才回退 `ctx.log_id`

## pkg/process 注册中心（新增，纯基础设施）

新增 `src/pkg/process/mod.rs`（并在 `src/pkg/mod.rs` 注册）：

- `ProcessEntry { pid: u32, tool_id, call_id, agent_id: Option<String>, project_id: Option<String>, task_id: Option<String>, command, working_dir, log_path, background: bool, started_at: u64, status: ProcessStatus(Running/Exited), exit_code: Option<i32>, finished_at: Option<u64> }`（call_id 取自 ctx.tool_call_id，见上节）
- `ProcessRegistry` 全局单例（once_cell + Mutex<HashMap<u32, ProcessEntry>>，pid 为键）：`register / get / list / mark_exited / refresh(pid)`（探活并更新状态）/ `tail_log(path, n)`
- 进程原语（`#[cfg(unix)]` 用 libc；Windows 桩返回 unsupported）：`is_alive(pid)` = kill(pid, 0)；`terminate(pid)` = SIGKILL
- Cargo.toml 根 workspace 新增 `libc` 依赖
- pid 复用风险在文档注释中声明（v1 接受）；entry 携带 started_at 供人工甄别

## SystemDomain ProcessManager（领域层）

`src/service/domain/system/mod.rs`（可拆 `process.rs` 子文件）：

- 新增 trait `ProcessManager`，由 `SystemDomainImpl` 实现：
  - `get_process(ctx, pid)` / `list_processes(ctx)` / `kill_process(ctx, pid)` / `process_status(ctx, pid, tail_lines)`
- scope 规则：`ctx.agent_id()` 为 Some 时必须与 entry.agent_id 匹配（Agent 只能管理自己启动的进程，不匹配返回 `PermissionDenied`）；ctx 无 agent_id（人类用户/管理面调用）放行
- kill 走 registry `terminate` 原语后 `mark_exited`；status 先 `refresh` 探活再返回 entry + 日志尾部

## shell_exec 重构（超时 detach + 统一日志流式模型）

`src/pkg/tool_registry/shell_exec.rs`：

- **统一执行模型**：sync 与 background 都从 spawn 起就把 stdout/stderr 重定向到日志文件（现 sync 用管道捕获是两套逻辑），日志文件名改用 `{call_id}.log`。sync 等待结束后从日志文件读取输出做摘要（受 `max_output_size_bytes` 截断），全量留盘
- **超时语义改为 detach**：超时不再 kill，更新 registry 状态后返回 `{ status: "timeout", call_id, pid, log_path, message: "进程仍在运行，可用 shell_status 查询或 shell_kill 终止" }`；新增可选参数 `timeout_action: "detach" | "kill"`（默认 `detach`，保留显式 kill 能力），parameters_schema 同步更新
- **进程注册**：spawn 成功后（sync/background 均注册）写入 ProcessRegistry，携带 `ctx.agent_id() / ctx.project_id() / ctx.task_id()` 与 `ctx.tool_call_id()`；进程退出时 `mark_exited(exit_code)`；正常返回 JSON 同样补充 `call_id` 字段
- 沙箱（working_dir 白名单）与环境变量过滤逻辑保持不变

## shell_status / shell_kill 工具（双露）

- common 层：`common/src/api/` 新增（或复用现有分组）参数/响应 DTO：
  - `ShellStatusParams { pid: u32, tail_lines: Option<usize> }` → `ShellStatusResponse { pid, alive, exit_code, started_at, command, log_path, log_tail }`
  - `ShellKillParams { pid: u32 }` → `ShellKillResponse { pid, killed }`
- Handler：`src/handlers/system/process/` 新增 `shell_status.rs` / `shell_kill.rs`，使用 `#[register_handler_tool(id = "shell_status"/"shell_kill", tags = "shell", ...)]` + `#[generate_http_handler]`，内部调 `system::domain().process_manage()`（或等价 manager 方法）；`src/handlers/system/mod.rs` 与 `router.rs` system nest 注册路由
- `shell_exec` 的 `create_po()` tags 增加 `"shell"`，三个工具可按 tag 分组绑定

## 文档修订

- `docs/design/tool_design.md` 追加「2026-08-09 更新」章节（遵循 design/ 只追加不改旧文）：同步默认 + Agent background 调用级决策 + dispatch_mode=async 仅显式配置；shell_exec 超时 detach 语义；统一进程管理模块架构（pkg/process + SystemDomain + 双露工具）
- `AGENTS.md`：最后更新行、功能表（新增「统一后台进程管理」行）、测试统计数字同步

## 测试计划

- pkg/process 单测：register/get/list/tail_log；真实 spawn `sleep` 进程验证 is_alive/terminate/mark_exited（native 环境执行）
- shell_exec：新增真实子进程用例——`sleep 2` + timeout_ms=100 → 返回 timeout + pid 存活 + registry 有条目；`timeout_action=kill` 用例；现有 7 个解析类测试保持通过
- call_id 关联断言：经 `ToolCallDao::execute` 调用 shell_exec，断言 `entry.call_id == 日志文件名主干 == ProcessEntry.call_id`，且返回 JSON 含同一 call_id 与 pid；`ctx.tool_call_id` 缺失时回退 log_id 的兼容用例
- 幂等防重用例：业务指定 call_id 首次执行落盘后，同 call_id 再次调用 → 直接返回历史结果（不重复执行，metadata 标 deduplicated）；历史 Failed 同 call_id → 允许重试重新执行；自动生成 call_id 路径不触发防重查询
- SystemDomain ProcessManager：scope 校验（agent 不匹配拒绝 / 匹配放行 / 无 agent ctx 放行）+ kill 生效
- 全量门槛：`cargo test`（后端全绿）、`cargo clippy --all-targets -- -D warnings`、`cargo fmt --all --check`

## 假设与边界

- 本轮不含 HTTP 工具前端创建表单、PUT/DELETE 方法放开（此前评估的其它 Gap，另行安排）
- Windows 下进程探活/终止为 unsupported 桩（CI 与开发环境为 macOS/Linux）
- 进程注册中心不做 DB 持久化；前端进程管理页面不在本轮范围
- 集成测试数量如新增需同步更新 AGENTS.md 统计口径