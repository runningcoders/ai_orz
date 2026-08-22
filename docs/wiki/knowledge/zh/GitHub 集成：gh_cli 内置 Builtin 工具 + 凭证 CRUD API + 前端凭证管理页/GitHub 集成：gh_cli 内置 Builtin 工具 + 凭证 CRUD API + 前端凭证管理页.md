---
kind: wiki_knowledge_card
name: GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页
category: 外部集成
scope:
- src/pkg/tool_registry/gh_cli.rs
- src/service/domain/github_integration/**
- src/handlers/github_integration/**
- frontend/src/pages/integrations/github.rs
- src/service/domain/identity_credential/**
source_files:
- src/pkg/tool_registry/gh_cli.rs#L1-L260
- src/service/domain/github_integration/mod.rs#L1-L90
- src/service/domain/github_integration/pr.rs#L1-L200
- src/service/domain/github_integration/repo.rs#L1-L180
- src/handlers/github_integration/list_repos.rs#L1-L120
- src/handlers/github_integration/create_pr.rs#L1-L140
- frontend/src/pages/integrations/github.rs#L1-L300
- frontend/src/api/github_integration.rs#L1-L100
- common/src/api/github_integration.rs#L1-L150
- src/service/domain/identity_credential/crud.rs#L200-L280
- docs/archive/design-archive/github_integration_subsystem.md
- docs/archive/plan-archive/github_integration_gh_cli_credential_crud_and_frontend.md
- docs/wiki/zh/content/功能模块/外部集成/GitHub 集成.md
- docs/wiki/knowledge/zh/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动
  + CredentialDetail 类型无关下沉）/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动
  + CredentialDetail 类型无关下沉）.md
- docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool
  宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool
  宏 + 神经工具免绑定三层校验.md
- docs/wiki/knowledge/zh/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例.md

---

# GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页

## §1 整体方案

d04cd9a3 变更落地完整的 GitHub 外部集成三层（凭据链路 2026-08-21 工厂化改造，见下方增量更新）：
1. **gh_cli Builtin 工具**：`gh_cli` 注册为 Agent 可调用内置工具（CoreTool trait），schema 为自由 `command` 字符串（gh 子命令与参数，不含二进制名，如 `repo list --limit 20`）+ 可选 `timeout_ms` / `working_dir`；底层通过 `std::process::Command` 调系统安装的 `gh` CLI，token 由凭据编排注入（见下），运行在调用者隔离的用户 HOME 工作区。
2. **凭证 CRUD API**：复用身份凭证 Domain（b4f9a560 统一链路）的 `credential_type = "GITHUB_PAT"` 类型，`detail = { "pat_token": "ghp_xxx", "github_host": "github.com" (企业版可填) }`，create/update/delete/list 全部走统一 Handler，不再独立造 GitHub-only 凭证表。
3. **前端凭证管理页**：`/integrations/github` 页面（Dioxus），展示「当前 org 已配置的 GitHub PAT 凭证列表（hash + 更新时间）」+ 「新增凭证表单」+ 「调用 gh_cli repo_list 测试连接」按钮 + 错误 toast 反馈；API 走 frontend/src/api/github_integration.rs 复用 common DTO。

**现行凭据链路**（2026-08-21，D17/D22）：模块级静态 `credential_requirements()` 单点声明 `[GithubToken → Internal { field: "token" }]`（工厂与实例同源）→ domain `call_tool` 编排层 `resolve_tool_credentials` 取数（user dal `find_default(GITHUB_TOKEN)` 单轨；**env `GITHUB_TOKEN` fallback 已废除，凭据无任何非凭证库来源**）→ `CoreTool::check(&mut self, resolved)` 注入 token 实例字段 → `call` 内 `ensure_gh_auth` 用户 HOME 幂等登录（token 走 stdin 不进进程参数）。未绑定 → 编排层 `credential_missing_json` 结构化引导；直调漏 check 为防御路径返回绑定提示 JSON。

**2026-08-21 增量更新（提交链 a22eede3 → 67012420，共享工具凭据增强器）**：`GhCredentialResolver` trait / RESOLVER OnceLock / `set·get_credential_resolver` 整体删除（D17）；`GhCliTool { identity_domain }` 改 `GhCliCoreTool { po, config, token }`（D22 check 注入）；`gh` 二进制名进 PO config `config.command` 缺省 `GH_CLI_BIN`（D28）。本卡 §3/§4 中 d04cd9a3 时期的凭证优先级与 env fallback 描述为历史快照，现行链路详见 [共享工具凭据增强器卡](docs/wiki/knowledge/zh/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例.md) 与 [GitHub 集成长文](docs/wiki/zh/content/功能模块/外部集成/GitHub 集成.md)。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/宏/入口 |
|------|------|----------------|
| [src/pkg/tool_registry/gh_cli.rs](src/pkg/tool_registry/gh_cli.rs) | 【d04cd9a3 核心，2026-08-21 工厂化】gh_cli 工具（GhCliToolFactory + GhCliCoreTool）| 模块级 `credential_requirements()` 单点声明 `[GithubToken → Internal { field: "token" }]`（工厂与实例同源）；`GhCliCoreTool { po, config: GhCliConfig, token: Option<String> }`（check 注入，D22）；`schema().input = { command: string, timeout_ms?, working_dir? }`；`call()` 内 ① 取 check 注入 token（缺失 → 绑定引导 JSON 防御路径）→ ② 用户 HOME 隔离 + `ensure_gh_auth` 幂等登录（token 走 stdin，marker 指纹检测轮换）→ ③ 二进制读 `config.command` 缺省 `GH_CLI_BIN`（D28）+ `command_available` 探测 → ④ spawn gh 子进程（不经 shell，kill_on_drop，默认 60s 超时 + 1MB 输出截断）；输出经 `sanitize_gh_output` 脱敏（token/secret 关键字行 → `[REDACTED]`）|
| [src/service/domain/github_integration/repo.rs](src/service/domain/github_integration/repo.rs) | Domain：Repo 相关业务封装（非 gh_cli 工具 HTTP 侧调用用） | Handler 调 `list_repos(ctx, org_login) -> Vec<RepoView>`：**不重复造命令**——也是内部 new GhCliTool → invoke("repo_list", args) 复用工具逻辑；保证 Handler 查询 & Agent 工具调用底层走同一套 gh 子命令（不会出现「前端能看到的 repo 比 Agent 工具多/少」字段漂移） |
| [src/service/domain/github_integration/pr.rs](src/service/domain/github_integration/pr.rs) | Domain：PR 业务封装 | create_pr 同样复用 GhCliTool.invoke(pr_create, {repo, base, head, title, body})；返回 `PrView { number, html_url, state, draft }`（common DTO） |
| [src/handlers/github_integration/list_repos.rs](src/handlers/github_integration/list_repos.rs) | Handler：GET /integrations/github/repos | 调 GitHubDomain.list_repos → 返回 ApiResponse；凭证权限：Member 只能取 user 级凭证查自己的 repo，Admin 可切 org 级 |
| [src/handlers/github_integration/create_pr.rs](src/handlers/github_integration/create_pr.rs) | Handler：POST /integrations/github/prs | 调 GitHubDomain.create_pr；body = CreatePrRequest 从 common DTO；create_pr 含大 body 时防止 HTTP 超时，工具侧 timeout = 120s |
| [frontend/src/pages/integrations/github.rs](frontend/src/pages/integrations/github.rs) | 前端：GitHub 管理页 Dioxus | 3 个 Tab：① 凭证管理（列表 + 新增表单，复用 IdentityCredentialDomain create API + `type=GITHUB_PAT`）② 测试工具（repo_list 按钮 + 结果表格）③ PR 助手（填 repo/base/head/title/body → 提交 create_pr → 展示 html_url 链接）；所有按钮点击 toast 反馈 |
| [common/src/api/github_integration.rs](common/src/api/github_integration.rs) | common DTO：前后端 + 工具共用 | `CreatePrRequest { repo_owner, repo_name, base_branch, head_branch, title, body, draft: bool }`；`RepoView { full_name, html_url, default_branch, description, private, updated_at }`；`PrView { number, html_url, state, draft }`；**gh_cli 工具 invoke 返回值 = Handler API 返回值**，强制一致（单测 `gh_cli_tool_resp_eq_handler_dto` 校验） |
| 【兄弟卡 Level3】身份凭证统一链路总卡 | GITHUB_PAT 凭证存储（复用身份 Domain）| [身份凭证统一总卡](docs/wiki/knowledge/zh/身份凭证统一链路（总卡：模型层%20+%20Domain%20层%20CRUD%20+%20Handler%20层%20API%20+%20外部集成联动%20+%20CredentialDetail%20类型无关下沉）/身份凭证统一链路（总卡：模型层%20+%20Domain%20层%20CRUD%20+%20Handler%20层%20API%20+%20外部集成联动%20+%20CredentialDetail%20类型无关下沉）.md) |
| 【兄弟卡 Level3】工具系统三层调用架构卡 | gh_cli 是 Builtin 协议的典型实现 | [工具系统三层调用卡](docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验.md) |
| 【Wiki 长文】GitHub 集成系统.md | 系统化上下文 + Troubleshooting | [GitHub 集成系统长文](docs/wiki/zh/content/功能模块/外部集成/GitHub%20集成系统.md) |
| 【Design 占位】github_integration_subsystem.md | （未来 doc-maintainer 落地）| docs/archive/design-archive/github_integration_subsystem.md（占位）|
| 【Plan 占位】github_integration_gh_cli_credential_crud_and_frontend.md | （未来 doc-maintainer 落地）| docs/archive/plan-archive/github_integration_gh_cli_credential_crud_and_frontend.md（占位）|

## §3 架构约定

本卡与 [身份凭证统一总卡](docs/wiki/knowledge/zh/身份凭证统一链路（总卡：模型层%20+%20Domain%20层%20CRUD%20+%20Handler%20层%20API%20+%20外部集成联动%20+%20CredentialDetail%20类型无关下沉）/身份凭证统一链路（总卡：模型层%20+%20Domain%20层%20CRUD%20+%20Handler%20层%20API%20+%20外部集成联动%20+%20CredentialDetail%20类型无关下沉）.md) + [工具系统三层调用卡](docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验.md) 构成 **GitHub 集成 + 凭证统一 + 工具注册** 体系的 业务实现 / 凭证存储 / 工具注册 互补视角；按 AGENTS §2.1.3 Level 3 保留平行卡。

1. **gh CLI 命令通过子进程调，不调 Octocat crate**：刻意选型 `std::process::Command` + 官方 `gh` CLI（而不是 Rust `octocrab` crate 直接调 GitHub REST API）——原因：gh CLI 自带 retries / rate limit 友好提示 / 企业版 ghes 兼容 / pagination 友好；Agent 工具只要 parse gh 的 JSON output（加 `--json` flag）即可，不用手写 REST client 的 pagination 逻辑。
2. **HTTP Handler 与 Agent 工具复用同一套 gh_cli 执行代码**：HTTP list_repos 接口不是手写 gh call → 而是内部 new 一个 GhCliTool → invoke("repo_list", args)。保证「前端测试工具」和「Agent 调工具」100% 相同结果（字段完全一致，不会出现：前端看到的 updated_at = ISO8601 而 Agent 工具返回 unix_ms）。
3. **凭证来源单一化（2026-08-21 D17 后）**：取数上移 domain 编排层 `resolve_tool_credentials`（user dal `find_default` 单轨）——禁止 gh_cli.rs 自建取数（per-tool resolver / OnceLock 注册 / env fallback 全部废除，凭据无任何非凭证库来源）；`CoreTool::check(&mut self, resolved)` 注入 token 实例字段（D22，check 注入的实例禁缓存复用）；未绑定 → 编排层 `credential_missing_json` 结构化引导，禁止 panic。
4. **所有 gh CLI 命令统一加 `--json` flag + 固定字段清单**：如 `gh repo list owner --json fullName,htmlUrl,defaultBranchRef,description,isPrivate,updatedAt`，字段清单写在 gh_cli.rs 常量里，未来前端要加字段只改常量 + common DTO 同步加字段。禁止 agent 随意传 `--json any,field`（字段开放范围不可控导致日志泄露私密字段如 primaryLanguage 里的 license 私有信息？）。
5. **命令执行强制 timeout**：gh_cli 子进程最长 60s，大 PR create 放宽到 120s；timeout 通过 `tokio::time::timeout()` 包裹，超时后 kill 子进程，返回工具错误 "gh CLI timeout after 60s"。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 gh_cli 把 PAT token 写进任何日志/进程参数**（2026-08-21 D22 后实现形态）：token 经 `ensure_gh_auth` 的 **stdin** 送入 `gh auth login --with-token`（不进进程参数列表、不设 GITHUB_TOKEN 环境变量）；所有 gh 输出（含 auth login 失败 stderr）过 `sanitize_gh_output`——token/secret/password/authorization 关键字行 + gho_/ghu_/ghs_/ghr_/ghp_ 前缀行整行替换 `[REDACTED]`；token 指纹 marker 只存 sha256 前 16 位（不可逆推）。
2. ⚠️ **schema 为自由 `command` 字符串（无枚举白名单）**：现行实现不设子命令白名单——风险控制靠四层：① token 即权限边界（凭据未绑定直接引导，无匿名调用）；② 用户 HOME 隔离（`{base}/users/{user_id}`，gh 登录态/hosts.yml 按用户分目录，凭证删除时 `clear_gh_auth` 清登录态）；③ 子进程不经 shell 按空白切分参数（无 shell 注入面）+ kill_on_drop 超时终止；④ 输出脱敏 + 1MB 截断。工具 description 明确警示 destructive 命令（如 repo delete）不可逆。
3. ✅ **凭据注入与引导测试锁定**：`check_injects_token_from_resolved_requirement`（check 注入 token 实例字段）+ `factory_and_instance_requirements_are_consistent`（工厂声明与实例声明同源防漂移）+ `call_without_check_returns_guidance`（漏 check 防御路径返回绑定引导 JSON）+ `call_with_empty_command_returns_error_json`（空命令报错）。
4. ✅ **凭证来源单轨测试**（2026-08-21 D17 后）：无 env fallback 分支——user dal `find_default(GITHUB_TOKEN)` 单轨取数在 domain `resolve_tool_credentials`；未绑定 → 编排层 `credential_missing_json` 结构化引导（Agent 可读自愈），call 层防御路径见上条 `call_without_check_returns_guidance`。
5. ❌ **禁止 gh_cli.rs 手工 decrypt 凭证密文（跨层 + 跨边界）**（2026-08-21 D17 后）：明文 PAT 只能来自 domain 编排层 `resolve_tool_credentials` 加工后的注入值（解密单点在 `pkg::credential::decrypt_detail`，由编排链调用）——gh_cli 经 `check(resolved)` 收到即用值，作为调用方不自己做解密，违反分层。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 Wiki 长文 1 篇（GitHub 集成）+ 3 张兄弟 RAG 卡（身份统一总卡 + 工具系统三层调用卡 + 共享工具凭据增强器卡）+ Design/Plan 占位各 1；Wiki 长文 cite 区回链本卡 + 兄弟卡 + Design/Plan 占位路径。
7. ❌ **禁止前端 github 页面把用户保存的 PAT token 通过 `value=` 回显到 `<input>`**：和身份凭证总卡 §4.2 对齐——编辑表单显示「当前凭证 hash: xxx，如要修改请重新输入完整 PAT」，编辑 API 每次提交都要求重输（不会意外泄露旧值到 HTML source 或被浏览器 autocomplete 缓存）。
8. ✅ **强制 CLI 存在性检查测试**：gh_cli invoke 时**第一步**先 `which gh` → 若 Path 不存在 → 返回结构化错误 "`gh` CLI not installed, install from https://cli.github.com"，避免 panic "program not found" 直接抛给用户/Agent。
