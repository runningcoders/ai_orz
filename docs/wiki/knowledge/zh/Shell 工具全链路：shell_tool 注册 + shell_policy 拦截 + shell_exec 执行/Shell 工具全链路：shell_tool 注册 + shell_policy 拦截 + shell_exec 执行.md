---
kind: rag_card
name: Shell 工具全链路：shell_exec 内置执行 + shell_policy 策略拦截 + shell_tool 声明式注册
category: pkg层基础设施
scope:
- src/pkg/tool_registry/shell_exec.rs
- src/pkg/tool_registry/shell_policy.rs
- src/pkg/tool_registry/shell_tool.rs
- src/pkg/policy/mod.rs
source_files:
- src/pkg/tool_registry/shell_exec.rs#L24-L552
- src/pkg/tool_registry/shell_policy.rs#L1-L359
- src/pkg/tool_registry/shell_tool.rs#L1-L223
- src/pkg/policy/mod.rs#L17-L83
- common/src/enums/tool.rs
- docs/wiki/zh/content/基础设施/工具注册表/内置工具系统/Shell执行工具.md
- docs/wiki/zh/content/核心模块/服务层/领域层/策略引擎与 Shell 拦截层.md
- docs/wiki/knowledge/zh/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层 + PolicyAction 动作上浮 + Shell 拦截层/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层 + PolicyAction 动作上浮 + Shell 拦截层.md
---

# Shell 工具全链路（内置执行 + 策略拦截 + 声明式注册）

## §1 概述

Shell 工具是 Agent 操作文件系统、版本控制、构建工具的核心执行层，**三层架构**：

1. **shell_exec 内置执行工具**——模型传命令字符串走 `/bin/sh -c` 执行（传统 Builtin 工具，control_mode=Manual），长进程支持 background 模式 + 统一日志落盘 + 进程注册中心（shell_status / shell_kill 管理）。
2. **shell_policy 拦截层**——复用通用策略引擎（ShellRulePolicy 实现 Policy trait），5 条静态规则表（scope 结构判定 + 命令类 Regex/Subcommand 匹配），Or 组按声明顺序上浮首个阻断动作（Confirm 短路）或收集放行场景的审计（git commit = 产物锚点时刻）。
3. **shell_tool 声明式 Shell 工具**——数据库注册 + 配置驱动（ToolPo.config 存 ShellToolConfig），与 HTTP 工具同构；argv 逐项传递不经 `sh -c`（命令注入结构性不可能）；固定 program + args 模板占位符渲染 + 复用 shell_policy 同一拦截层。

三条链路统一：shell_exec 和 shell_tool **都调用同一个 shell_policy::evaluate**，env 注入（AI_ORZ_TASK_ID / AI_ORZ_AGENT_ID）也统一在两条执行链路的出口步骤执行，供 git commit-msg hook 消费产物锚点。

> 本卡为策略引擎的 Shell 子主题，与【策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层 + PolicyAction 动作上浮 + Shell 拦截层】互为兄弟卡——策略引擎负责通用框架，本卡负责 Shell 工具的注册 + 拦截 + 执行完整链路。

## §2 关键文件表

| 文件 | 角色 | 核心入口/约束 |
|------|------|---------------|
| [shell_exec.rs](src/pkg/tool_registry/shell_exec.rs) | 内置 Shell 执行工具（传统 Builtin） | `ShellExecToolFactory.create_po/create`；`ShellExecCoreTool.call`：resolve_working_dir（默认 Agent 工作区）→ shell_policy::evaluate → ensure_workspace_repo 惰性 git init → env 过滤白名单 + AI_ORZ_TASK_ID/AGENT_ID 注入 → Stdio 重定向到日志文件 → spawn → sync/background 二选一；超时策略 detach（默认，交还进程）/ kill（强制终止） |
| [shell_policy.rs](src/pkg/tool_registry/shell_policy.rs) | Shell 命令策略拦截层 | `ShellRulePolicy`（声明式结构，实现 Policy trait + 覆写 action() 返回 PolicyAction）；`CommandMatcher` 四 matcher（ScopeOutsideAllowedPaths / ScopeIdentityBoundary / Regex / Subcommand）；`RULE_DEFS` 静态规则表 5 条；`evaluate` 入口（构造 Metrics → Or 组 action 上浮 → blocking 短路 + audits 收集） |
| [shell_tool.rs](src/pkg/tool_registry/shell_tool.rs) | 声明式 Shell 工具（数据库注册 + 配置驱动） | `ShellCoreTool::from_po`（从 ToolPo.config 反序列化 ShellToolConfig）；`execute_shell_call`（模板渲染 args → shell_policy::evaluate → process::exec argv 逐项传递）；validate_config 校验（program 无空白、working_dir 绝对路径、timeout_ms 1..600000） |
| [pkg/policy/mod.rs](src/pkg/policy/mod.rs) | 通用策略引擎（本卡依赖） | Policy trait 定义 + PolicyAction 枚举 + PolicyGroup Or 组合 + PolicyBuilder；ShellRulePolicy 即此 trait 的 shell 领域实现 |
| 【兄弟卡】策略引擎 | Policy trait + PolicyAction + think_loop 全策略化 | [策略引擎 RAG 卡](docs/wiki/knowledge/zh/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法%20+%20PolicyAction%20动作上浮%20+%20Shell%20拦截层/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法%20+%20PolicyAction%20动作上浮%20+%20Shell%20拦截层.md) |
| 【Wiki 长文】Shell 执行工具 | 系统化安全机制 | [Shell执行工具](docs/wiki/zh/content/基础设施/工具注册表/内置工具系统/Shell执行工具.md) |
| 【Wiki 长文】策略引擎与 Shell 拦截层 | Mermaid 架构图 + Shell 拦截完整链路 | [策略引擎与 Shell 拦截层](docs/wiki/zh/content/核心模块/服务层/领域层/策略引擎与%20Shell%20拦截层.md) |

## §3 架构约定

1. **shell_exec 与 shell_tool 双执行通道同策略管线**：两条链路都调用 `shell_policy::evaluate(ShellPolicyInput)`，拦截逻辑完全一致（scope 越界 Confirm / 身份边界越界 Confirm / 破坏性命令 Confirm / git 审计 Audit）。
2. **ShellPolicyInput 适配层统一填充 Metrics**：command / working_dir / base_root / additional_allowed_paths / user_id / agent_id 六个算子键，shell_policy::keys 模块定义 SSOT。scope 类规则（ScopeOutsideAllowedPaths / ScopeIdentityBoundary）不解析命令字符串，是结构性判定。
3. **env 注入是拦截层固定出口步骤**：shell_exec 两条链路都注入 AI_ORZ_TASK_ID / AI_ORZ_AGENT_ID（不可绕），供 git commit-msg hook（pkg/git_workspace.rs）读取追加 Task-Id / Agent-Id trailer。shell_tool 目前未显式注入——后续扩展需对齐。
4. **shell_tool 安全边界**：argv 逐项传递不经 `sh -c` → 参数里的空白/元字符无解释歧义；program 固定配置化，模型只能在 args 模板占位符里填参，不能指定要跑什么程序。这与 shell_exec（模型传完整命令字符串）的安全边界不同——shell_exec 靠 shell_policy 策略管线兜底，shell_tool 靠 argv 传递 + 策略管线双重兜底。
5. **RULE_DEFS 声明顺序 = 阻断优先级**：Or 组按声明顺序上浮首个 Some（PolicyGroup.action 实现），所以破坏性命令 Confirm 在 git commit Audit 前面——复合命令 `git commit -m x && git push` 时阻断优先于审计。审计规则放末尾，只有放行场景才收集。
6. **shell_exec 统一日志流式模型**：每条工具调用一个日志文件 `{call_id}.log`（按天分区 YYYYMMDD），Stdio 直接重定向到文件；shell_tool 复用 `pkg/process::exec`，输出经 stdout/stderr 截断返回（截断上限 100000 chars）。

## §4 硬约束（红线）

1. ❌ **禁止 shell_tool 绕开 shell_policy**：shell_tool.execute_shell_call 必须调用 shell_policy::evaluate；禁止手工 scope 检查或命令正则匹配。
2. ❌ **禁止 shell_policy 里做命令内容修补**：Shell 拦截层只做「策略判断」，不做内容替换（如 `rm -rf /` → `rm -rf home`）。内容修补走 git hooks 这类原生扩展点。
3. ✅ **Scope 规则是结构性判定，不解析命令字符串**：ScopeOutsideAllowedPaths / ScopeIdentityBoundary 只看 Metrics 里的路径，不看 command 内容。路径逃逸类攻击（如 `rm -rf ../../etc`）由路径归一化 + base_root 边界判定覆盖，不在命令字符串层面匹配。
4. ✅ **Subcommand 匹配是粗粒度护栏**：`git -C <path> push` 这类全局 flag 后置子命令可能漏拦，`cd x&&git push`（无空格）会漏拦。Confirm 是护栏不是安全边界，嵌套绕过漏拦可接受（YAGNI 不做完整 shell argv 解析）。
5. ✅ **env 注入是结构性步骤，不可绕**：shell_exec 在 spawn 前必须注入 AI_ORZ_TASK_ID / AI_ORZ_AGENT_ID，哪怕 ctx 里没有 task_id/agent_id 也要尝试注入 None（shell_policy 里 try_get）。
6. ❌ **禁止在 RULE_DEFS 之外散落 Shell 拦截规则**：所有拦截规则必须在 static RULE_DEFS 数组里声明，由 OnceLock 初始化 Or 组。禁止在 shell_exec / shell_tool 的 call 方法里手工加 if/else scope 检查。
7. ✅ **shell_tool program 固定 + 无空白**：validate_config 检查 program.trim().is_empty() / program.chars().any(|c| c.is_whitespace())；working_dir 必须绝对路径；timeout_ms 1..600000。

---

## §5 历史演进（变更摘要）

- **原始 shell_exec（仓库早期）**：纯 builtin 工具，命令直接 spawn `/bin/sh -c`，无 scope 检查，env 过滤白名单但无 AI_ORZ_TASK_ID/AGENT_ID 注入。
- **shell_policy 拦截层（本轮 51d944f8 新增）**：ShellRulePolicy 实现通用 Policy trait + 覆写 action()；5 条静态规则表（scope 越界 Confirm / 身份边界越界 Confirm / 破坏性 fs 命令 Confirm / git push-reset-clean Confirm / git commit Audit）。
- **git_workspace.rs 产物锚点（本轮 51d944f8 新增）**：Agent 工作区惰性 git init → commit-msg hook 追加 Task-Id / Agent-Id trailer；shell_exec 调用前 ensure_workspace_repo best-effort；checkpoint_task_commit 兜底收尾提交。
- **shell_tool 声明式工具（本轮 51d944f8 新增）**：数据库注册 + ShellToolConfig 驱动，argv 逐项传递不经 sh -c；与 shell_exec 共享 shell_policy 拦截层。
