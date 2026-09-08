---
kind: rag_card
name: Shell 工具全链路：shell_exec 内置执行 + shell_policy 策略拦截 + shell_tool 声明式注册
category: pkg层基础设施
scope:
- src/pkg/tool_registry/shell_env.rs
- src/pkg/tool_registry/shell_exec.rs
- src/pkg/tool_registry/shell_policy.rs
- src/pkg/tool_registry/shell_tool.rs
- src/pkg/policy/mod.rs
source_files:
- src/pkg/tool_registry/shell_env.rs#L1-L348
- src/pkg/tool_registry/shell_env_tests.rs#L1-L255
- src/pkg/tool_registry/shell_exec.rs#L24-L552
- src/pkg/tool_registry/shell_policy.rs#L1-L359
- src/pkg/tool_registry/shell_tool.rs#L1-L223
- src/pkg/policy/mod.rs#L17-L83
- common/src/config.rs#L270-L342
- common/src/models/tool.rs#L23-L47
- common/src/enums/tool.rs
- docs/wiki/zh/content/基础设施/工具注册表/内置工具系统/Shell执行工具.md
- docs/wiki/zh/content/核心模块/服务层/领域层/策略引擎与 Shell 拦截层.md
- docs/wiki/knowledge/zh/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层 + PolicyAction 动作上浮 + Shell 拦截层/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层 + PolicyAction 动作上浮 + Shell 拦截层.md
---

# Shell 工具全链路（内置执行 + 策略拦截 + 声明式注册）

## §1 概述

Shell 工具是 Agent 操作文件系统、版本控制、构建工具的核心执行层，**四层架构**：

1. **shell_env 统一环境出口**——（新增）三条起子进程的链路（shell_exec / shell_tool / MCP stdio）统一走 `shell_env::resolve()` 构造 env；PATH 补全（`completed_path` / `inherited_path`，追加存在目录 + `~` 前缀 + 单段 `*` 通配）、HOME 策略（`home_for`：Isolated=`{base}/users/{user_id}` / Inherit=继承）、工具链根经官方变量指回真实 HOME（`toolchain_env_injections`：cargo/nvm/pyenv 等 7 条映射）、SSH known_hosts 补齐（`git_ssh_command_injection`）、env 白名单 + 敏感子串剔除（`filter_inherited_environment`）。
2. **shell_exec 内置执行工具**——模型传命令字符串走 `/bin/sh -c` 执行（传统 Builtin 工具，control_mode=Manual），长进程支持 background 模式 + 统一日志落盘 + 进程注册中心（shell_status / shell_kill 管理）。env 构造完全委托 shell_env::resolve；HOME 策略可配置（默认 Isolated），隔离 HOME 下工具链根目录自动指回真实 HOME；git over SSH known_hosts 自动补齐。
3. **shell_policy 拦截层**——复用通用策略引擎（ShellRulePolicy 实现 Policy trait），5 条静态规则表（scope 结构判定 + 命令类 Regex/Subcommand 匹配），Or 组按声明顺序上浮首个阻断动作（Confirm 短路）或收集放行场景的审计（git commit = 产物锚点时刻）。
4. **shell_tool 声明式 Shell 工具**——数据库注册 + 配置驱动（ToolPo.config 存 ShellToolConfig），与 HTTP 工具同构；argv 逐项传递不经 `sh -c`（命令注入结构性不可能）；固定 program + args 模板占位符渲染 + 复用 shell_policy 同一拦截层；PATH 补全 + 身份变量注入也走 shell_env 统一出口。

三条链路统一：shell_exec 和 shell_tool **都调用同一个 shell_policy::evaluate**，env 构造也统一在 shell_env::resolve 出口执行（AI_ORZ_TASK_ID / AI_ORZ_AGENT_ID 身份变量注入、PATH 补全、HOME 策略），供 git commit-msg hook 消费产物锚点。MCP stdio 零继承场景也经 shell_env::inherited_path() 补 PATH。

> 本卡为策略引擎的 Shell 子主题，与【策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层 + PolicyAction 动作上浮 + Shell 拦截层】互为兄弟卡——策略引擎负责通用框架，本卡负责 Shell 工具的注册 + 拦截 + 执行完整链路。

## §2 关键文件表

| 文件 | 角色 | 核心入口/约束 |
|------|------|---------------|
| [shell_env.rs](src/pkg/tool_registry/shell_env.rs) | Shell 环境统一出口（新增） | `resolve()` 三条链路共用的 env 构造出口（shell_exec / shell_tool / MCP stdio）；`completed_path()` / `inherited_path()` PATH 补全（追加存在目录 + `~` 前缀 + 单段 `*` 通配）；`home_for()` HOME 策略（Isolated=`{base}/users/{user_id}` / Inherit=继承服务进程 HOME）；`toolchain_env_injections()` 隔离 HOME 下经官方环境变量把 cargo/nvm/pyenv 等工具链根指回真实 HOME（7 工具链映射常量见 `common::models::tool::SHELL_TOOLCHAIN_HOME_VARS`）；`git_ssh_command_injection()` 隔离 HOME 下 known_hosts 补齐（SSH_AUTH_SOCK 继承 + known_hosts 存在时注入 GIT_SSH_COMMAND）；`filter_inherited_environment()` 白名单 + 敏感子串剔除。MCP stdio 零继承时也经 inherited_path() 补 PATH |
| [shell_exec.rs](src/pkg/tool_registry/shell_exec.rs) | 内置 Shell 执行工具（传统 Builtin） | `ShellExecToolFactory.create_po/create`；`ShellExecCoreTool.call`：resolve_working_dir（默认 Agent 工作区）→ shell_policy::evaluate → ensure_workspace_repo 惰性 git init → **env 构造完全委托 shell_env::resolve**（PATH 补全 + HOME 策略 + 工具链 env 注入 + git_ssh_command）→ Stdio 重定向到日志文件 → spawn → sync/background 二选一；超时策略 detach（默认，交还进程）/ kill（强制终止）。新增配置项 path_additions / home_mode / toolchain_envs（ToolPo.config）；HOME 策略默认 Isolated，可切换 Inherit |
| [shell_policy.rs](src/pkg/tool_registry/shell_policy.rs) | Shell 命令策略拦截层 | `ShellRulePolicy`（声明式结构，实现 Policy trait + 覆写 action() 返回 PolicyAction）；`CommandMatcher` 四 matcher（ScopeOutsideAllowedPaths / ScopeIdentityBoundary / Regex / Subcommand）；`RULE_DEFS` 静态规则表 5 条；`evaluate` 入口（构造 Metrics → Or 组 action 上浮 → blocking 短路 + audits 收集） |
| [shell_tool.rs](src/pkg/tool_registry/shell_tool.rs) | 声明式 Shell 工具（数据库注册 + 配置驱动） | `ShellCoreTool::from_po`（从 ToolPo.config 反序列化 ShellToolConfig）；`execute_shell_call`（模板渲染 args → shell_policy::evaluate → process::exec argv 逐项传递）；**PATH 补全 + 身份变量注入也走 shell_env 统一出口**（不再手工拼装 env）；validate_config 校验（program 无空白、working_dir 绝对路径、timeout_ms 1..600000） |
| [pkg/policy/mod.rs](src/pkg/policy/mod.rs) | 通用策略引擎（本卡依赖） | Policy trait 定义 + PolicyAction 枚举 + PolicyGroup Or 组合 + PolicyBuilder；ShellRulePolicy 即此 trait 的 shell 领域实现 |
| [common::config::ShellConfig](common/src/config.rs#L270-L342) | Shell 配置内置默认值 | `ShellConfig`：内置默认 PATH 补全目录列表（不出现在 ai_orz.toml）+ HOME 策略（HomeMode 枚举：Isolated / Inherit）；`default_shell_path_additions` 定义核心工具链默认路径 |
| [common::models::tool::SHELL_TOOLCHAIN_HOME_VARS](common/src/models/tool.rs#L23-L47) | 7 工具链 HOME 映射常量 | nvm/cargo/rustup/pyenv/rbenv/go/npm 的官方环境变量名 → HOME 根目录映射；`is_supported_toolchain()` 判断工具链是否在支持列表内 |
| 【兄弟卡】策略引擎 | Policy trait + PolicyAction + think_loop 全策略化 | [策略引擎 RAG 卡](docs/wiki/knowledge/zh/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法%20+%20PolicyAction%20动作上浮%20+%20Shell%20拦截层/策略引擎：Policy%20trait%20+%20PolicyGroup%20嵌套组合%20+%20policy_set!%20宏声明式写法%20+%20PolicyAction%20动作上浮%20+%20Shell%20拦截层.md) |
| 【Wiki 长文】Shell 执行工具 | 系统化安全机制 | [Shell执行工具](docs/wiki/zh/content/基础设施/工具注册表/内置工具系统/Shell执行工具.md) |
| 【Wiki 长文】策略引擎与 Shell 拦截层 | Mermaid 架构图 + Shell 拦截完整链路 | [策略引擎与 Shell 拦截层](docs/wiki/zh/content/核心模块/服务层/领域层/策略引擎与%20Shell%20拦截层.md) |

## §3 架构约定

1. **shell_exec 与 shell_tool 双执行通道同策略管线**：两条链路都调用 `shell_policy::evaluate(ShellPolicyInput)`，拦截逻辑完全一致（scope 越界 Confirm / 身份边界越界 Confirm / 破坏性命令 Confirm / git 审计 Audit）。
2. **ShellPolicyInput 适配层统一填充 Metrics**：command / working_dir / base_root / additional_allowed_paths / user_id / agent_id 六个算子键，shell_policy::keys 模块定义 SSOT。scope 类规则（ScopeOutsideAllowedPaths / ScopeIdentityBoundary）不解析命令字符串，是结构性判定。
3. **shell_env 是三条链路的统一 env 出口**：shell_exec、shell_tool、MCP stdio 都不得手工拼装 env；PATH 补全、HOME 策略、身份变量注入（AI_ORZ_TASK_ID / AI_ORZ_AGENT_ID）、工具链 env 注入、git_ssh_command_injection 全部走 `shell_env::resolve()`。shell_tool 也统一经 shell_env 出口（不再手工组装 env）；MCP stdio 零继承场景经 `shell_env::inherited_path()` 补 PATH。
4. **PATH 补全语义**：`completed_path()`（shell_exec 用，先继承再补）和 `inherited_path()`（MCP stdio 零继承用，从零构造）—— 只追加文件系统上**实际存在**的目录；支持 `~` 前缀展开到服务进程 HOME；支持单段 `*` 通配（glob 匹配），用于工具链版本目录（如 `/usr/local/cargo/registry/*/bin`）。不做子段通配。
5. **HOME 策略可配置**：`HomeMode` 枚举（`common::config::HomeMode`）两种模式——Isolated（默认，`{base}/users/{user_id}`）和 Inherit（继承服务进程 HOME）。隔离模式下工具链根目录经官方环境变量指回真实 HOME（见 SHELL_TOOLCHAIN_HOME_VARS），确保 cargo/nvm/pyenv 等仍能找到全局安装的二进制和依赖。
6. **toolchain 映射常量集中定义**：`common::models::tool::SHELL_TOOLCHAIN_HOME_VARS` 是 7 条映射的 SSOT（nvm→NVM_DIR、cargo→CARGO_HOME、rustup→RUSTUP_HOME、pyenv→PYENV_ROOT、rbenv→RBENV_ROOT、go→GOROOT、npm→NPM_CONFIG_CACHE）。新增工具链必须在此常量注册，`shell_env::toolchain_env_injections()` 遍历此表注入。
7. **git over SSH known_hosts 自动补齐**：隔离 HOME 下，若服务进程 HOME 的 `~/.ssh/known_hosts` 存在，则注入 `GIT_SSH_COMMAND="ssh -o UserKnownHostsFile={path}"`（SSH_AUTH_SOCK 继承）。Inherit 模式下不需要此注入（HOME 本身就是服务进程 HOME）。
8. **shell_tool 安全边界**：argv 逐项传递不经 `sh -c` → 参数里的空白/元字符无解释歧义；program 固定配置化，模型只能在 args 模板占位符里填参，不能指定要跑什么程序。这与 shell_exec（模型传完整命令字符串）的安全边界不同——shell_exec 靠 shell_policy 策略管线兜底，shell_tool 靠 argv 传递 + 策略管线双重兜底。
9. **RULE_DEFS 声明顺序 = 阻断优先级**：Or 组按声明顺序上浮首个 Some（PolicyGroup.action 实现），所以破坏性命令 Confirm 在 git commit Audit 前面——复合命令 `git commit -m x && git push` 时阻断优先于审计。审计规则放末尾，只有放行场景才收集。
10. **shell_exec 统一日志流式模型**：每条工具调用一个日志文件 `{call_id}.log`（按天分区 YYYYMMDD），Stdio 直接重定向到文件；shell_tool 复用 `pkg/process::exec`，输出经 stdout/stderr 截断返回（截断上限 100000 chars）。
11. **env 兼容优先不做白名单**：`filter_inherited_environment()` 采用白名单 + 敏感子串剔除而非黑名单——白名单保留已知安全变量（PATH / LANG / TERM / SSH_AUTH_SOCK 等），再剔除显式注入集合里的敏感前缀（如包含 KEY / TOKEN / PASSWORD 的变量）。避免误伤用户依赖的变量，同时隔离密钥类信息。

## §4 硬约束（红线）

1. ❌ **禁止 shell_tool 绕开 shell_policy**：shell_tool.execute_shell_call 必须调用 shell_policy::evaluate；禁止手工 scope 检查或命令正则匹配。
2. ❌ **禁止 shell_policy 里做命令内容修补**：Shell 拦截层只做「策略判断」，不做内容替换（如 `rm -rf /` → `rm -rf home`）。内容修补走 git hooks 这类原生扩展点。
3. ✅ **Scope 规则是结构性判定，不解析命令字符串**：ScopeOutsideAllowedPaths / ScopeIdentityBoundary 只看 Metrics 里的路径，不看 command 内容。路径逃逸类攻击（如 `rm -rf ../../etc`）由路径归一化 + base_root 边界判定覆盖，不在命令字符串层面匹配。
4. ✅ **Subcommand 匹配是粗粒度护栏**：`git -C <path> push` 这类全局 flag 后置子命令可能漏拦，`cd x&&git push`（无空格）会漏拦。Confirm 是护栏不是安全边界，嵌套绕过漏拦可接受（YAGNI 不做完整 shell argv 解析）。
5. ❌ **禁止 shell_exec / shell_tool / MCP stdio 手工拼装 env**：三条链路必须委托 `shell_env::resolve()` 或 `shell_env::inherited_path()`；禁止绕过 shell_env 直接 filter + inject。身份变量（AI_ORZ_TASK_ID / AI_ORZ_AGENT_ID）PATH 补全、HOME 策略、工具链 env、git_ssh_command 都是 shell_env 的职责。
6. ❌ **禁止在 RULE_DEFS 之外散落 Shell 拦截规则**：所有拦截规则必须在 static RULE_DEFS 数组里声明，由 OnceLock 初始化 Or 组。禁止在 shell_exec / shell_tool 的 call 方法里手工加 if/else scope 检查。
7. ✅ **shell_tool program 固定 + 无空白**：validate_config 检查 program.trim().is_empty() / program.chars().any(|c| c.is_whitespace())；working_dir 必须绝对路径；timeout_ms 1..600000。
8. ✅ **PATH 补全只追加存在目录**：`completed_path()` / `inherited_path()` 必须对每个候选目录做 filesystem 存在性检查；不存在的跳过。禁止把不存在的路径塞进 PATH，避免工具链静默失效。
9. ❌ **禁止跳过 HOME 策略**：shell_exec 必须走 `shell_env::home_for()` 决定 HOME（默认 Isolated）；禁止直接用服务进程 HOME 或硬编码路径。HomeMode 可配置但不可跳过。
10. ✅ **env 兼容优先不做激进白名单**：`filter_inherited_environment()` 用白名单 + 敏感子串剔除而非黑名单——白名单保留已知安全变量 + 用户自定义注入变量，敏感子串仅剔除显式注入集合（包含 KEY / TOKEN / PASSWORD / SECRET / CREDENTIAL 等前缀的变量）。避免误伤用户依赖的代理、证书、语言运行时变量。
11. ✅ **敏感子串仅剔除显式注入集合**：不会剔除非显式注入的继承变量。如 `GOPROXY=https://...` 即便包含 URL 也不会被当作敏感信息——只剔除显式注入的密钥类变量。
12. ✅ **新增工具链必须注册 SHELL_TOOLCHAIN_HOME_VARS**：`common::models::tool.rs` 是 SSOT；新增工具链（如 rbenv 之外的版本管理器）必须在此常量追加映射，否则隔离 HOME 下找不到根目录。

---

## §5 历史演进（变更摘要）

- **原始 shell_exec（仓库早期）**：纯 builtin 工具，命令直接 spawn `/bin/sh -c`，无 scope 检查，env 过滤白名单但无 AI_ORZ_TASK_ID/AGENT_ID 注入。
- **shell_policy 拦截层（本轮 51d944f8 新增）**：ShellRulePolicy 实现通用 Policy trait + 覆写 action()；5 条静态规则表（scope 越界 Confirm / 身份边界越界 Confirm / 破坏性 fs 命令 Confirm / git push-reset-clean Confirm / git commit Audit）。
- **git_workspace.rs 产物锚点（本轮 51d944f8 新增）**：Agent 工作区惰性 git init → commit-msg hook 追加 Task-Id / Agent-Id trailer；shell_exec 调用前 ensure_workspace_repo best-effort；checkpoint_task_commit 兜底收尾提交。
- **shell_tool 声明式工具（本轮 51d944f8 新增）**：数据库注册 + ShellToolConfig 驱动，argv 逐项传递不经 sh -c；与 shell_exec 共享 shell_policy 拦截层。
- **shell_env 统一环境出口（本轮新增）**：从 shell_exec / shell_tool / MCP stdio 三条执行链路里抽离 env 构造，形成 `shell_env::resolve()` 单一出口。新增 HOME 策略可配置（HomeMode 枚举：Isolated 默认 / Inherit）、PATH 补全语义化（completed_path / inherited_path，只追加存在目录 + `~` 前缀 + 单段 `*` 通配）、7 工具链映射常量（SHELL_TOOLCHAIN_HOME_VARS）、git over SSH known_hosts 自动补齐、env 白名单 + 敏感子串剔除。shell_exec 重构：env 构造完全委托 shell_env；shell_tool 也对齐 shell_env 统一出口（PATH 补全 + 身份变量注入）；common 层新增 ShellConfig（内置默认 PATH 补全目录）、HomeMode 枚举、SHELL_TOOLCHAIN_HOME_VARS 常量（7 条映射）。
