---
kind: wiki_knowledge_card
name: GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页
category: 外部集成
scope:
  - "src/pkg/tool_registry/gh_cli.rs"
  - "src/service/domain/github_integration/**"
  - "src/handlers/github_integration/**"
  - "frontend/src/pages/integrations/github.rs"
  - "src/service/domain/identity_credential/**"
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
  - docs/design/github_integration_subsystem.md（占位：待 ai-orz-doc-maintainer 落地后回填真实路径）
  - docs/plan/github_integration_gh_cli_credential_crud_and_frontend.md（占位：待 ai-orz-doc-maintainer 落地后回填真实路径）
  - docs/wiki/zh/content/功能模块/外部集成/GitHub 集成.md
  - docs/wiki/knowledge/zh/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）.md
  - docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md
---

# GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页

## §1 整体方案

d04cd9a3 变更落地完整的 GitHub 外部集成三层：
1. **gh_cli Builtin 工具**：`tool://builtin/github_cli` 注册为 Agent 可调用工具（CoreTool trait），schema 支持 `subcommand in [repo_list, pr_create, pr_list, issue_create, issue_comment, clone]` 六大子命令；底层通过 `std::process::Command` 调系统安装的 `gh` CLI（需 Agent 所在节点提前 `gh auth login` 或配置 `GITHUB_TOKEN` 环境变量，**也支持从身份凭证 Domain 取 GitHubPAT 作为 auth**——二选一，优先凭证 Domain）。
2. **凭证 CRUD API**：复用身份凭证 Domain（b4f9a560 统一链路）的 `credential_type = "GITHUB_PAT"` 类型，`detail = { "pat_token": "ghp_xxx", "github_host": "github.com" (企业版可填) }`，create/update/delete/list 全部走统一 Handler，不再独立造 GitHub-only 凭证表。
3. **前端凭证管理页**：`/integrations/github` 页面（Dioxus），展示「当前 org 已配置的 GitHub PAT 凭证列表（hash + 更新时间）」+ 「新增凭证表单」+ 「调用 gh_cli repo_list 测试连接」按钮 + 错误 toast 反馈；API 走 frontend/src/api/github_integration.rs 复用 common DTO。

凭证优先级 gh_cli 工具执行时：`IdentityCredentialDomain.get_by_type(GITHUB_PAT)`（org 级 → user 级，按 scope 匹配）→ 若未找到 → fallback 环境变量 `GITHUB_TOKEN` → 若仍未找到 → 返回工具错误 "GitHub PAT not configured, save via /integrations/github or set GITHUB_TOKEN env"。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/宏/入口 |
|------|------|----------------|
| [src/pkg/tool_registry/gh_cli.rs](src/pkg/tool_registry/gh_cli.rs) | 【d04cd9a3 核心】gh_cli 工具（CoreTool impl + 注册函数）| `GhCliTool { identity_domain: Arc<dyn IdentityCredentialDomain> }`；`schema().input = { subcommand: enum[repo_list|pr_create|…], args: JSON object }`；`invoke()` 内 ① 取凭证（identity_domain → env fallback）→ ② 拼 `gh {sub} --arg1 v1 …` 命令 → ③ 设置 `GITHUB_TOKEN` env 到子进程 → ④ `output_with_timeout(ctx, cmd, 60s)` 执行，Stdout → tool 输出；错误写 log_error! + 返回 structured_err（含 gh 退出码 + stderr 摘要，避免 Agent 看到裸字符串）|
| [src/service/domain/github_integration/repo.rs](src/service/domain/github_integration/repo.rs) | Domain：Repo 相关业务封装（非 gh_cli 工具 HTTP 侧调用用） | Handler 调 `list_repos(ctx, org_login) -> Vec<RepoView>`：**不重复造命令**——也是内部 new GhCliTool → invoke("repo_list", args) 复用工具逻辑；保证 Handler 查询 & Agent 工具调用底层走同一套 gh 子命令（不会出现「前端能看到的 repo 比 Agent 工具多/少」字段漂移） |
| [src/service/domain/github_integration/pr.rs](src/service/domain/github_integration/pr.rs) | Domain：PR 业务封装 | create_pr 同样复用 GhCliTool.invoke(pr_create, {repo, base, head, title, body})；返回 `PrView { number, html_url, state, draft }`（common DTO） |
| [src/handlers/github_integration/list_repos.rs](src/handlers/github_integration/list_repos.rs) | Handler：GET /integrations/github/repos | 调 GitHubDomain.list_repos → 返回 ApiResponse；凭证权限：Member 只能取 user 级凭证查自己的 repo，Admin 可切 org 级 |
| [src/handlers/github_integration/create_pr.rs](src/handlers/github_integration/create_pr.rs) | Handler：POST /integrations/github/prs | 调 GitHubDomain.create_pr；body = CreatePrRequest 从 common DTO；create_pr 含大 body 时防止 HTTP 超时，工具侧 timeout = 120s |
| [frontend/src/pages/integrations/github.rs](frontend/src/pages/integrations/github.rs) | 前端：GitHub 管理页 Dioxus | 3 个 Tab：① 凭证管理（列表 + 新增表单，复用 IdentityCredentialDomain create API + `type=GITHUB_PAT`）② 测试工具（repo_list 按钮 + 结果表格）③ PR 助手（填 repo/base/head/title/body → 提交 create_pr → 展示 html_url 链接）；所有按钮点击 toast 反馈 |
| [common/src/api/github_integration.rs](common/src/api/github_integration.rs) | common DTO：前后端 + 工具共用 | `CreatePrRequest { repo_owner, repo_name, base_branch, head_branch, title, body, draft: bool }`；`RepoView { full_name, html_url, default_branch, description, private, updated_at }`；`PrView { number, html_url, state, draft }`；**gh_cli 工具 invoke 返回值 = Handler API 返回值**，强制一致（单测 `gh_cli_tool_resp_eq_handler_dto` 校验） |
| 【兄弟卡 Level3】身份凭证统一链路总卡 | GITHUB_PAT 凭证存储（复用身份 Domain）| [身份凭证统一总卡](docs/wiki/knowledge/zh/身份凭证统一链路（总卡：模型层%20+%20Domain%20层%20CRUD%20+%20Handler%20层%20API%20+%20外部集成联动%20+%20CredentialDetail%20类型无关下沉）/身份凭证统一链路（总卡：模型层%20+%20Domain%20层%20CRUD%20+%20Handler%20层%20API%20+%20外部集成联动%20+%20CredentialDetail%20类型无关下沉）.md) |
| 【兄弟卡 Level3】工具系统三层调用架构卡 | gh_cli 是 Builtin 协议的典型实现 | [工具系统三层调用卡](docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验.md) |
| 【Wiki 长文】GitHub 集成系统.md | 系统化上下文 + Troubleshooting | [GitHub 集成系统长文](docs/wiki/zh/content/功能模块/外部集成/GitHub%20集成系统.md) |
| 【Design 占位】github_integration_subsystem.md | （未来 doc-maintainer 落地）| docs/design/github_integration_subsystem.md（占位）|
| 【Plan 占位】github_integration_gh_cli_credential_crud_and_frontend.md | （未来 doc-maintainer 落地）| docs/plan/github_integration_gh_cli_credential_crud_and_frontend.md（占位）|

## §3 架构约定

本卡与 [身份凭证统一总卡](docs/wiki/knowledge/zh/身份凭证统一链路（总卡：模型层%20+%20Domain%20层%20CRUD%20+%20Handler%20层%20API%20+%20外部集成联动%20+%20CredentialDetail%20类型无关下沉）/身份凭证统一链路（总卡：模型层%20+%20Domain%20层%20CRUD%20+%20Handler%20层%20API%20+%20外部集成联动%20+%20CredentialDetail%20类型无关下沉）.md) + [工具系统三层调用卡](docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验/工具系统三层调用架构：CoreTool%20trait%20+%20Builtin%20HTTP%20MCP%20三协议路由%20+%20register_handler_tool%20宏%20+%20神经工具免绑定三层校验.md) 构成 **GitHub 集成 + 凭证统一 + 工具注册** 体系的 业务实现 / 凭证存储 / 工具注册 互补视角；按 AGENTS §2.1.3 Level 3 保留平行卡。

1. **gh CLI 命令通过子进程调，不调 Octocat crate**：刻意选型 `std::process::Command` + 官方 `gh` CLI（而不是 Rust `octocrab` crate 直接调 GitHub REST API）——原因：gh CLI 自带 retries / rate limit 友好提示 / 企业版 ghes 兼容 / pagination 友好；Agent 工具只要 parse gh 的 JSON output（加 `--json` flag）即可，不用手写 REST client 的 pagination 逻辑。
2. **HTTP Handler 与 Agent 工具复用同一套 gh_cli 执行代码**：HTTP list_repos 接口不是手写 gh call → 而是内部 new 一个 GhCliTool → invoke("repo_list", args)。保证「前端测试工具」和「Agent 调工具」100% 相同结果（字段完全一致，不会出现：前端看到的 updated_at = ISO8601 而 Agent 工具返回 unix_ms）。
3. **凭证来源强制「身份凭证 Domain 优先 → env fallback」**：禁止 gh_cli.rs 直接查 IdentityCredentialDao（跨层）→ 必须注入 Arc<dyn IdentityCredentialDomain>；domain.get_by_type(GITHUB_PAT, scope=当前user→当前org) → 返回 Some 则用 detail.pat_token → None 则读 std::env::var("GITHUB_TOKEN")；双空 → 工具返回结构化错误，禁止 panic。
4. **所有 gh CLI 命令统一加 `--json` flag + 固定字段清单**：如 `gh repo list owner --json fullName,htmlUrl,defaultBranchRef,description,isPrivate,updatedAt`，字段清单写在 gh_cli.rs 常量里，未来前端要加字段只改常量 + common DTO 同步加字段。禁止 agent 随意传 `--json any,field`（字段开放范围不可控导致日志泄露私密字段如 primaryLanguage 里的 license 私有信息？）。
5. **命令执行强制 timeout**：gh_cli 子进程最长 60s，大 PR create 放宽到 120s；timeout 通过 `tokio::time::timeout()` 包裹，超时后 kill 子进程，返回工具错误 "gh CLI timeout after 60s"。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 gh_cli 把 PAT token 写进任何日志**：构造 Command 时设置 GITHUB_TOKEN 环境变量**仅作用于子进程**，父进程内 `log_debug!` / `log_info!` 打印命令前必须 scrub：`let cmd_display = cmd_str.replace(pat_token, "***MASKED***")`；测试 `gh_cli_pat_never_logged` 模拟 10 次 invoke 扫所有日志文本——含 "ghp_" / "github_pat_" 前缀的一律 fail。
2. ❌ **禁止 GhCliTool schema 中允许用户传任意 subcommand 字符串**：input.subcommand 是 `#[serde(rename_all = "snake_case")] enum GhCliSubcommand { RepoList, PrCreate, PrList, IssueCreate, IssueComment, Clone }`——**枚举白名单**；如果 Agent 传未定义字符串 = schema 校验失败直接 reject，防止 Agent 尝试 `gh auth token` / `gh api user -H Accept:application/vnd.github+json` 等不在白名单的命令做越权。
3. ✅ **强制子命令白名单 + 字段白名单测试**：6 个允许的子命令各 1 条成功测试；**故意传 2 条非法子命令**（"auth_status", "admin_org_add_member"）→ schema 校验失败返回 400 + 错误信息含 "invalid subcommand"（2 条非法必须全命中拒绝）。
4. ✅ **强制凭证优先级测试**：测试 3 条：① Both（Domain 里有 + 环境变量也有）→ 使用 Domain 里的（assert GITHUB_TOKEN 进程值 = Domain 凭证）；② Only Env（Domain 里无 + 环境变量有）→ 使用环境变量；③ None → 返回工具错误信息正确。
5. ❌ **禁止 gh_cli.rs 手工 decrypt IdentityCredentialDao 的 encrypted_detail（跨层 + 跨边界）**：明文 PAT 只能来自 IdentityCredentialDomain 返回的 Entity.plaintext_detail.pat_token——DAL 解密 + Domain 返回明文；gh_cli 作为调用方不自己做解密，违反分层。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 Wiki 长文 1 篇（GitHub 集成系统）+ 2 张兄弟 RAG 卡（身份统一总卡 + 工具系统三层调用卡）+ Design/Plan 占位各 1；Wiki 长文 cite 区回链本卡 + 2 兄弟卡 + Design/Plan 占位路径。
7. ❌ **禁止前端 github 页面把用户保存的 PAT token 通过 `value=` 回显到 `<input>`**：和身份凭证总卡 §4.2 对齐——编辑表单显示「当前凭证 hash: xxx，如要修改请重新输入完整 PAT」，编辑 API 每次提交都要求重输（不会意外泄露旧值到 HTML source 或被浏览器 autocomplete 缓存）。
8. ✅ **强制 CLI 存在性检查测试**：gh_cli invoke 时**第一步**先 `which gh` → 若 Path 不存在 → 返回结构化错误 "`gh` CLI not installed, install from https://cli.github.com"，避免 panic "program not found" 直接抛给用户/Agent。
