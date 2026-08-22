# GitHub 集成

<cite>
**本文引用的文件**
- [src/pkg/tool_registry/gh_cli.rs](src/pkg/tool_registry/gh_cli.rs)
- [src/service/domain/github_integration/pr.rs](src/service/domain/github_integration/pr.rs)
- [frontend/src/pages/integrations/github.rs](frontend/src/pages/integrations/github.rs)

### 本文关联的三类文档（四类互引闭环）

**① 设计文档（Design）**：
- docs/archive/design-archive/github_integration_subsystem.md（占位：待 ai-orz-doc-maintainer 落地后回填真实路径）

**② 落地计划（Plan）**：
- docs/archive/plan-archive/github_integration_gh_cli_credential_crud_and_frontend.md（占位：待 ai-orz-doc-maintainer 落地后回填真实路径）

**④ RAG 原子知识卡**：
- [GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页](docs/wiki/knowledge/zh/GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页/GitHub 集成：gh_cli 内置 Builtin 工具 + 凭证 CRUD API + 前端凭证管理页.md) — §红线 1 禁止把 PAT token 写进任何日志；§红线 2 禁止允许用户传任意 subcommand 字符串
- [身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）](docs/wiki/knowledge/zh/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）.md) — Level3 兄弟卡：GITHUB_PAT 凭证存储复用身份 Domain
- [工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验](docs/wiki/knowledge/zh/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验/工具系统三层调用架构：CoreTool trait + Builtin HTTP MCP 三协议路由 + register_handler_tool 宏 + 神经工具免绑定三层校验.md) — Level3 兄弟卡：gh_cli 是 Builtin 协议典型实现
- [共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例](docs/wiki/knowledge/zh/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例/共享工具凭据增强器：类型级需求声明 + domain 编排注入 + check 单次实例.md) — gh_cli 凭据消费现行链路（2026-08-21 工厂化 + check 注入 + domain 编排取数，per-tool resolver 已删除）
</cite>

**【本次 2026-08-16 增量追加互引】**
#### ④ RAG 原子知识卡（本次追加 T3 身份凭证总卡 Level3 兄弟关联）：
- [身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）](docs/wiki/knowledge/zh/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）.md) — §红线 7 外部集成新增凭证类型必须走 CredentialDetail 新增 arm，禁止独立建凭证表
#### ③ Wiki 关联长文（本次追加 Level3 兄弟长文反向引用）：
- [身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md](docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md) — GITHUB_PAT 凭证存储、加密、生命周期副作用（删除时清 gh auth 登录态）完整链路说明
</cite>

## 更新摘要
**变更内容**
- d04cd9a3 变更：落地完整 GitHub 集成三层：gh_cli Builtin 工具 + 凭证 CRUD API（复用身份凭证 Domain）+ 前端凭证管理页
- 新增 gh_cli 工具六大子命令白名单：repo_list / pr_create / pr_list / issue_create / issue_comment / clone
- 凭证优先级：身份凭证 Domain GITHUB_PAT → env GITHUB_TOKEN → 错误提示
- 前端 /integrations/github 页面：凭证管理 Tab + 测试工具 Tab + PR 助手 Tab
- 更新日期：2026-08-16
**2026-08-16 增量补充互引**：追加 T3（身份凭证统一链路总卡 Level3 兄弟卡）RAG 卡互引；cite 区反向引用身份凭证管理长文（形成 GITHUB_PAT 凭证存储 ↔ GitHub 集成消费的双向链接闭环）；§5 详细实现分析 gh_cli 凭证获取优先级章节来源行号降级（因 identity_credential/crud.rs 新增 GITHUB_PAT 专用 match arm 导致行号范围漂移，优先降级为无行号范围引用）。

**2026-08-21 增量更新（提交链 a22eede3 → 67012420，共享工具凭据增强器）**：
- **gh_cli 工厂化**：`GhCredentialResolver` trait / RESOLVER OnceLock / `set·get_credential_resolver` 整体删除（D17）——取数上移 domain 编排层 `resolve_tool_credentials`（user dal `find_default` 单轨；`std::env::var("GITHUB_TOKEN")` fallback 一并废除，凭据无任何非凭证库来源）。
- **check 注入 token 实例字段**：CoreTool 实现改 D22 生命周期——模块级静态 requirements `[GithubToken]` 单点声明（工厂与实例同源）+ `check(&mut self, resolved)` 注入 token 实例字段，`call` 内取数段（原 resolve_github_pat 路径）删除；未绑定引导统一走编排层 `credential_missing_json`。
- **`gh` 二进制名进 PO config**（D28）：`po.config.command` 缺省 "gh"（工具管理页可改命令路径）；readiness 经 domain `tool_readiness` 数据驱动（CLI 型 `command_available` 探测）。
- §3「凭证获取优先级」代码块为 d04cd9a3 时期历史实现快照，现行链路详见 [共享工具凭据增强器](docs/wiki/zh/content/基础设施/工具注册表/共享工具凭据增强器.md)；关联文档追加共享工具凭据增强器 RAG 卡。

## 目录
1. [简介](#简介)
2. [核心数据结构与 API](#核心数据结构与-api)
3. [架构原理与流程](#架构原理与流程)
4. [边界与行为红线](#边界与行为红线)
5. [详细实现分析](#详细实现分析)
6. [代码组织与扩展点](#代码组织与扩展点)
7. [运维与监控](#运维与监控)
8. [常见故障排查](#常见故障排查)
9. [最佳实践](#最佳实践)
10. [结论与未来方向](#结论与未来方向)

## 简介
GitHub 集成（d04cd9a3 变更）是 AI Orz 外部集成体系的首个完整落地实现，覆盖三层能力：

1. **gh_cli Builtin 工具**：Agent 可在思考/对话过程中直接调用 `tool://builtin/github_cli` 完成 GitHub 常用操作——列仓库、建 PR、查 PR、开 Issue、评论 Issue、克隆仓库。底层通过 `std::process::Command` 调系统安装的官方 `gh` CLI（而非 octocrab crate），复用 gh CLI 自带的 retries、rate limit 友好提示、GHES 企业版兼容、pagination 自动处理等成熟能力。
2. **凭证 CRUD API**：不独立造 GitHub-only 凭证表，而是复用身份凭证统一链路（b4f9a560 总卡）的 `credential_type="GITHUB_PAT"` 类型，detail 为 `{ "pat_token": "ghp_xxx", "github_host": "github.com" | "ghes.example.com" }`，create/update/delete/list 全走统一 Handler 与加密存储。
3. **前端凭证管理页**：`/integrations/github`（Dioxus）提供三个 Tab——凭证管理（列表+新增，hash 回显不回显明文）+ 测试工具（一键 repo_list 验证连接）+ PR 助手（填 repo/base/head/title/body 提交 PR 后展示 html_url 链接）。

本章系统化阐述 GitHub 集成的 10 节完整结构。

章节来源
- [src/pkg/tool_registry/gh_cli.rs#L1-L260](src/pkg/tool_registry/gh_cli.rs#L1-L260)
- [src/service/domain/github_integration/mod.rs](src/service/domain/github_integration/mod.rs)
- [frontend/src/pages/integrations/github.rs#L1-L300](frontend/src/pages/integrations/github.rs#L1-L300)

## 核心数据结构与 API
### GhCliSubcommand 枚举（白名单）
```rust
#[serde(rename_all = "snake_case")]
pub enum GhCliSubcommand {
    RepoList,
    PrCreate,
    PrList,
    IssueCreate,
    IssueComment,
    Clone,
}
```
白名单枚举是安全红线的核心：Agent 传未定义字符串直接 schema 校验失败，防止 `gh auth token` / `gh admin_org_add_member` 等越权命令。

### 凭证 Detail 结构
GITHUB_PAT 凭证 detail（serde_json::Value，Domain 类型无关，消费方自行 into 强类型）：
```json
{
  "pat_token": "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxx",
  "github_host": "github.com"
}
```
- `github_host` 支持 GitHub Enterprise Server（GHES）自定义域名。
- `pat_token` 仅在 Entity 解密出明文后使用，响应 DTO 中永不返回。

### common DTO（前后端 + 工具共用）
`CreatePrRequest { repo_owner, repo_name, base_branch, head_branch, title, body, draft: bool }`
`RepoView { full_name, html_url, default_branch, description, private, updated_at }`
`PrView { number, html_url, state, draft }`
gh_cli 工具 invoke 返回值 = Handler API 返回值，由单测 `gh_cli_tool_resp_eq_handler_dto` 强制一致。

章节来源
- [src/pkg/tool_registry/gh_cli.rs#L1-L260](src/pkg/tool_registry/gh_cli.rs#L1-L260)
- [common/src/api/github_integration.rs#L1-L150](common/src/api/github_integration.rs#L1-L150)
- [src/service/domain/identity_credential/crud.rs#L200-L280](src/service/domain/identity_credential/crud.rs#L200-L280)

## 架构原理与流程
### 三层调用架构对齐
gh_cli 工具遵守工具系统三层调用架构：
- **CoreTool 层**：GhCliTool struct（注入 Arc<dyn IdentityCredentialDomain>），schema 校验 subcommand 白名单。
- **Builtin 协议路由**：`register_handler_tool!` 宏或显式 `tool_registry.register(Arc::new(gh_cli))`。
- **Domain 层复用**：HTTP Handler 侧 list_repos / create_pr 不是手写 gh call，而是内部 `GhCliTool::new(identity_domain).invoke("repo_list", args)`，保证前端与 Agent 工具 100% 相同字段。

### 凭证获取优先级
```
invoke(subcommand, args)
  │
  ├─► 1. IdentityCredentialDomain.get_by_type(GITHUB_PAT, scope=当前user→当前org)
  │      └─► Some(entity) → use entity.plaintext_detail.pat_token
  │
  ├─► 2. std::env::var("GITHUB_TOKEN")
  │      └─► Ok(v) → use v
  │
  └─► 3. 双空 → 工具返回结构化错误 "GitHub PAT not configured, save via /integrations/github or set GITHUB_TOKEN env"
```

> ⚠️ **2026-08-21 起上表为历史链路**（d04cd9a3 时期实现快照）：现行实现见 [共享工具凭据增强器](docs/wiki/zh/content/基础设施/工具注册表/共享工具凭据增强器.md)——`GhCredentialResolver` 与 env fallback 均已删除，取数统一走 domain `resolve_tool_credentials`（user dal `find_default` 纯单轨）→ `CoreTool::check` 注入 token 实例字段（D17/D22）；`gh` 二进制名读 `po.config.command`（D28）。
无论走哪条路径，PAT token 最终都设置为子进程的 GITHUB_TOKEN 环境变量（仅作用于子进程，父进程内日志打印前 scrub 掩码）。

### 命令执行与超时光照
所有 gh CLI 子命令统一加 `--json` + 固定字段清单（写死在 gh_cli.rs 常量），Agent 不可自定义字段清单。
超时：子命令默认 60s，create_pr 含大 body 放宽到 120s，`tokio::time::timeout()` 包裹后 kill 子进程。

章节来源
- [src/pkg/tool_registry/gh_cli.rs#L1-L260](src/pkg/tool_registry/gh_cli.rs#L1-L260)
- [src/service/domain/github_integration/repo.rs#L1-L180](src/service/domain/github_integration/repo.rs#L1-L180)

## 边界与行为红线
### 安全红线
1. ❌ **禁止 PAT 写日志**：构造命令时 GITHUB_TOKEN 环境变量仅给子进程，父进程 `log_debug!/log_info!` 打印命令前必须 `cmd_str.replace(pat_token, "***MASKED***")`；单测 `gh_cli_pat_never_logged` 模拟 10 次 invoke 扫所有日志文本。
2. ❌ **禁止任意 subcommand 字符串**：schema 必须是 GhCliSubcommand 枚举（6 白名单），非白名单 = schema 校验失败 reject。
3. ❌ **禁止 gh_cli 手工 decrypt DAO 层密文**：明文 PAT 只能来自 IdentityCredentialDomain 返回的 Entity.plaintext_detail——DAL 解密 + Domain 返回明文，gh_cli 作为调用方不自己做解密（跨层 + 跨边界）。
4. ❌ **禁止前端表单回显明文 PAT**：编辑页面展示「当前凭证 hash=xxx，如需修改请重新输入完整值」，与身份凭证总卡 §4.2 对齐。

### 架构红线
5. ✅ **HTTP Handler 与 Agent 工具复用同一 gh_cli 执行代码**：Handler 不手写命令，必须 new GhCliTool → invoke；字段清单 100% 对称。
6. ✅ **凭证优先级强制「身份 Domain 优先 → env fallback」**：测试 3 条 Both / Only Env / None 都符合预期。
7. ✅ **强制 CLI 存在性检查**：invoke 第一步先 `which gh` → 不存在返回结构化错误，避免 panic "program not found"。

章节来源
- [src/pkg/tool_registry/gh_cli.rs#L1-L260](src/pkg/tool_registry/gh_cli.rs#L1-L260)
- [frontend/src/pages/integrations/github.rs#L1-L300](frontend/src/pages/integrations/github.rs#L1-L300)

## 详细实现分析
### GhCliTool.invoke 核心流程
```rust
fn invoke(&self, ctx: &AgentCallContext, input: Value) -> ToolResult<Value> {
    // 1. CLI 存在性检查
    which("gh").map_err(|_| ToolError::new("`gh` CLI not installed, install from https://cli.github.com"))?;

    // 2. Schema 校验 + subcommand 枚举反序列化
    let input: GhCliToolInput = serde_json::from_value(input)?;

    // 3. 取凭证（优先级：身份 Domain → env）
    let pat_token = self.resolve_github_pat(ctx, &input).await?;

    // 4. 拼命令 + 固定 --json 字段清单 + args
    let mut cmd = self.build_command(&input);
    cmd.env("GITHUB_TOKEN", &pat_token);

    // 5. 日志掩码 scrub
    let cmd_display = self.scrub_cmd_for_log(&cmd, &pat_token);
    log_debug!(target: "gh_cli", "executing: {}", cmd_display);

    // 6. timeout 包裹执行 → stdout JSON 反序列化 → 结构化返回
    let output = tokio::time::timeout(timeout, cmd.output()).await??;
    if !output.status.success() {
        return Err(ToolError::structured(
            "gh CLI failed",
            json!({ "exit_code": output.status.code(), "stderr_summary": &String::from_utf8_lossy(&output.stderr)[..1000] })
        ));
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
```

### resolve_github_pat 凭证优先级
```rust
async fn resolve_github_pat(&self, ctx: &AgentCallContext, _input: &GhCliToolInput) -> Result<String> {
    // 1. 身份 Domain 优先
    if let Some(entity) = self.identity_domain
        .get_by_type(ctx, IdentityCredentialType::GithubPat, CredentialScope::user_then_org(ctx.uid(), ctx.org_id()))
        .await?
    {
        let detail: GitHubPATCredentialDetail = serde_json::from_value(entity.plaintext_detail)?;
        return Ok(detail.pat_token);
    }
    // 2. 环境变量 fallback
    if let Ok(v) = std::env::var("GITHUB_TOKEN") {
        if !v.is_empty() { return Ok(v); }
    }
    Err(anyhow!("GitHub PAT not configured, save via /integrations/github or set GITHUB_TOKEN env"))
}
```

### 前端页面三 Tab
frontend/src/pages/integrations/github.rs 使用 Dioxus router，三个 Tab 以 `use_state` 切：
- Tab1（凭证管理）：调 common API `create_identity_credential`（type=GITHUB_PAT），列表展示 hash + updated_at，删除按钮走 delete API。
- Tab2（测试工具）：点「Test Connection」→ 前端 API `list_repos` → Handler → GitHubDomain → new GhCliTool → invoke("repo_list") → 前端表格展示 RepoView 结果。
- Tab3（PR 助手）：输入 repo_owner/name + base/head + title + body + draft → 前端 API `create_pr` → 返回 PrView { number, html_url } → 前端 toast + 显示 `<a>` 链接。

章节来源
- [src/pkg/tool_registry/gh_cli.rs#L1-L260](src/pkg/tool_registry/gh_cli.rs#L1-L260)
- [frontend/src/pages/integrations/github.rs#L1-L300](frontend/src/pages/integrations/github.rs#L1-L300)

## 代码组织与扩展点
### 目录结构
```
src/
├── pkg/
│   └── tool_registry/
│       └── gh_cli.rs                 ← GhCliTool impl + 注册函数
├── service/
│   └── domain/
│       ├── github_integration/
│       │   ├── mod.rs                ← GithubIntegrationDomain trait
│       │   ├── repo.rs               ← list_repos 业务封装（new GhCliTool.invoke）
│       │   └── pr.rs                 ← create_pr 业务封装
│       └── identity_credential/
│           └── crud.rs               ← get_by_type(GITHUB_PAT) 复用
└── handlers/
    └── github_integration/
        ├── list_repos.rs
        └── create_pr.rs
frontend/src/pages/integrations/github.rs  ← 三 Tab 页面
frontend/src/api/github_integration.rs     ← 前端 API 调用封装
common/src/api/github_integration.rs       ← 前后端共用 DTO
```

### 扩展点
1. **新增 gh 子命令**：如 `pr_merge` / `release_create`：① GhCliSubcommand 枚举加变体 ② build_command 分支加 case ③ --json 字段清单常量加条目 ④ Handler + 前端 Tab 各新增一个。
2. **GitHub App 认证（替代 PAT）**：在 resolve_github_pat 中扩展第三档「GitHub App Installation Token」——身份凭证 Domain 新增 GITHUB_APP 类型存 app_id + private_key，每次调前用 jsonwebtoken crate 签短-lived token；当前 PAT 方案保持向后兼容。
3. **多仓库批量 PR**：create_pr 工具支持 batch 模式（`repo_owners: [a, b, c]`）→ 内部并行 invoke create_pr 三次 → 汇总结果数组返回，Agent 批量升级依赖场景使用。

章节来源
- [src/pkg/tool_registry/gh_cli.rs#L1-L260](src/pkg/tool_registry/gh_cli.rs#L1-L260)
- [src/service/domain/github_integration/mod.rs](src/service/domain/github_integration/mod.rs)

## 运维与监控
### 指标
- `gh_cli_invoke_count`（按 subcommand 分：6 子命令独立计数；按 source：Agent Tool / HTTP Handler）
- `gh_cli_invoke_duration_ms`（p50/p95/p99，超时 case 单独标）
- `github_pat_source_distribution`（IdentityDomain / Env / NotConfigured 三档比例）

### 日志
- `gh_cli_pat_never_logged` 单元测试 + 日志审计：日志系统中扫 `ghp_` / `github_pat_` 前缀，命中时告警（意味着 scrub 逻辑失效，PAT 被意外打印）。
- 速率限制告警：gh CLI 返回 stderr 含 "API rate limit exceeded" → 通过 DuckDB 日志告警通知管理员，Agent 工具需 sleep + retry。

### 健康检查
`GET /health/integrations/github`：
1. which gh 是否存在
2. GITHUB_TOKEN 环境变量是否已配置 或 身份凭证 Domain 中是否有至少一条 GITHUB_PAT
3. 两者都不满足 → health=DEGRADED，不 fail（因为允许用户稍后在 UI 中配置）

章节来源
- [src/pkg/tool_registry/gh_cli.rs#L1-L260](src/pkg/tool_registry/gh_cli.rs#L1-L260)
- [src/service/domain/github_integration/repo.rs#L1-L180](src/service/domain/github_integration/repo.rs#L1-L180)

## 常见故障排查
### Troubleshooting 1：gh_cli 工具错误 "gh CLI not installed"
**典型场景**：新部署环境未安装 gh CLI，或 PATH 没包含。Agent 调用任何 GitHub 子命令都失败。

**排查步骤**：
1. 在部署节点 `which gh` → 如果 not found → 按 https://cli.github.com 文档安装（Linux apt install gh / macOS brew install gh / Windows choco install gh）。
2. Docker 镜像构建时在 Dockerfile 中追加 `RUN apt-get update && apt-get install -y gh`，确保运行时存在。
3. 如果确认已安装但 PATH 不一致 → 在 start.sh 中 `export PATH="$PATH:/usr/local/bin"`（gh 默认安装位置），或在 gh_cli.rs 中用 `absolute_gh_path` 配置项显式指定。
4. 源码位置：[src/pkg/tool_registry/gh_cli.rs#L30-L55](src/pkg/tool_registry/gh_cli.rs#L30-L55)（CLI 存在性检查段）

### Troubleshooting 2：Agent 用 gh_cli repo_list 返回 "HTTP 401 Bad credentials"，但前端配置了 GITHUB_PAT 且 API list_repos 能成功
**典型场景**：前端 Tab2 测试工具能看到 repo 列表，但 Agent 工具调用报 401。说明身份 Domain 查凭证时两侧的 scope 过滤参数不一致。

**排查步骤**：
1. Handler 侧 `list_repos` 用的 scope=CredentialScope::Org(ctx.org_id)（管理员视角，允许用 org 级凭证），但 GhCliTool 侧可能默认 scope=CredentialScope::User(ctx.uid()) 只查用户级凭证 → 如果管理员在 UI 保存的是 org 级凭证（跨用户共享），而 Agent 是普通 Member 角色，resolve_github_pat 只查 user 级就查不到，fallback env 也没 → 最后报的其实是「没配凭证」但又因 env 为空时也可能意外拿到空字符串，导致 gh CLI 用空 PAT 请求 GitHub 报 401。
2. 修复：在 gh_cli.rs resolve_github_pat 中 scope 使用与 Handler 对称的 `user_then_org(ctx.uid(), ctx.org_id)`：先查用户级，没找到再查 org 级（且 org 级凭证要求 Agent 的角色是 Member，符合共享语义）。
3. 源码位置：
   - [src/pkg/tool_registry/gh_cli.rs#L100-L130](src/pkg/tool_registry/gh_cli.rs#L100-L130)（resolve_github_pat 段）
   - [src/service/domain/identity_credential/crud.rs#L200-L250](src/service/domain/identity_credential/crud.rs#L200-L250)（get_by_type scope 过滤段）

章节来源
- [src/pkg/tool_registry/gh_cli.rs#L1-L260](src/pkg/tool_registry/gh_cli.rs#L1-L260)
- [src/service/domain/identity_credential/crud.rs#L200-L280](src/service/domain/identity_credential/crud.rs#L200-L280)

## 最佳实践
1. **优先用 org 级凭证共享**：小团队在 /integrations/github 保存 GITHUB_PAT 时 scope=Org 级，所有 org 内 Agent 都能通过 user_then_org fallback 拿到；避免每个用户单独存一遍 PAT。
2. **create_pr 传 draft=true 先审阅**：Agent 自动建 PR 时建议 body 中输出「变更摘要 + 影响范围 + 测试覆盖率变化」，并先 draft=true，人类用户 review 再勾掉 draft，避免误合并半成品。
3. **字段清单按需扩展但始终白名单**：业务需要 RepoView 加 topics / license 字段时，只改 gh_cli.rs `REPO_LIST_JSON_FIELDS` 常量 + common DTO 同步加字段；禁止 Agent 自由传 `--json any,fields`。
4. **rate limit 告警 + 指数退避**：生产部署建议对 gh_cli_invoke 出现 rate limit 错误的 case 做指数退避重试（1s/2s/4s/8s，最多 4 次），避免 GitHub API 限流时空闲轮询失败。

章节来源
- [src/pkg/tool_registry/gh_cli.rs#L1-L260](src/pkg/tool_registry/gh_cli.rs#L1-L260)
- [frontend/src/pages/integrations/github.rs#L1-L300](frontend/src/pages/integrations/github.rs#L1-L300)

## 结论与未来方向
GitHub 集成通过 gh CLI 子进程 + 身份凭证统一链路复用，实现了「工具、API、前端 UI」三层对称的完整 GitHub 联动能力。选型上刻意避开了 octocrab crate 直接调 REST API，利用 gh CLI 成熟的 pagination / retry / GHES 兼容能力，减少了代码量与边界 case。

**未来方向**：
1. **GitHub Webhook 入站**：当前只有出站（Agent 工具 / UI 调用 gh），未来通过 GitHub Webhook（PR 评论、Issue assign 等事件）推送至 Lark / 站内消息，实现双向联动——外部 PR 有新评论时自动唤起对应 Agent 进行回复。
2. **多 Git 平台抽象**：当前只支持 GitHub，未来抽象 `GitProviderDomain trait`（接口 list_repos / create_pr / clone），追加 GitLab / Gitea 实现，PAT 凭证通过类型区分（GITHUB_PAT / GITLAB_PAT / GITEA_PAT）共用身份凭证 Domain 统一存储。
3. **自动依赖升级流水线**：结合 cron 任务 + gh_cli create_pr，每周自动 bump Cargo.toml / package.json 依赖版本，跑 CI，通过则自动建 PR 并 @ 负责 reviewer。

章节来源
- [src/pkg/tool_registry/gh_cli.rs#L1-L260](src/pkg/tool_registry/gh_cli.rs#L1-L260)
- [src/service/domain/github_integration/pr.rs#L1-L200](src/service/domain/github_integration/pr.rs#L1-L200)
