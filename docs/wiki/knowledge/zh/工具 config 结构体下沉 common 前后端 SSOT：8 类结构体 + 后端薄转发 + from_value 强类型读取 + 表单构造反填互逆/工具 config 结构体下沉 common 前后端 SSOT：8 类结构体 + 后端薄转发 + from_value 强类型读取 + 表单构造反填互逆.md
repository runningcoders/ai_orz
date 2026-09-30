---
kind: wiki_knowledge_card
name: 工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆
category: 基础设施 / 工具注册表
scope:
- common/src/config.rs
- src/pkg/tool_registry/**
source_files:
- common/src/config.rs#L374-L475
- common/src/config.rs#L737-L900
- src/pkg/tool_registry/http.rs#L42
- src/pkg/tool_registry/shell_exec.rs#L31
- src/pkg/tool_registry/mcp.rs#L30
- src/pkg/tool_registry/fs_read.rs#L16
- src/pkg/tool_registry/gh_cli.rs#L59
- src/pkg/tool_registry/browser.rs
- docs/design/web_search_and_browser_tools_design.md
- docs/wiki/zh/content/功能模块/工具生态系统/内置工具集/内置工具集.md
- docs/wiki/zh/content/核心模块/工具注册表/工具注册表.md
- 【视角兄弟卡】docs/wiki/knowledge/zh/Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位/Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位.md
- 【关联卡：前端视角】docs/wiki/knowledge/zh/工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复/工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复.md

---

## §1 概述

**本卡角色**：工具行为配置（存于 `ToolPo.config`）的结构体定义与读取约定知识卡。回答「工具 config 的字段长什么样、定义在哪、后端怎么读、前端表单怎么对齐」。**定位：新增/修改任一内置工具 config 字段、排查「前后端键名漂移」、理解 `ToolPo.config` 强类型读取时读。**

- **问题背景**：内置/自建工具的 config 结构体原本各自定义在 `src/pkg/tool_registry/*`，而前端是独立 WASM crate（只能依赖 `common`），无法引用 `src/` 的类型 → 前后端各写一套字段，键名必然漂移。
- **解法**：统一下沉到 `common::config`，作为前后端共享的 **SSOT（单一事实来源）**。后端各工具文件顶部用 `pub use common::config::X;` 做薄转发，既有引用点零改动。
- **8 类结构体**：`FsToolConfig`（fs_read/fs_write 去重）、`CliToolConfig`（gh_cli/lark_cli 合并，缺省二进制名由调用方以工具常量兜底）、`HttpToolConfig`、`ShellToolConfig`（声明式 shell）、`ShellExecConfig`（shell_exec）、`McpToolConfig`、`BrowserConfig`（键名对齐 `cli_*` 泛型访问器）、`SearchToolConfig`（tavily_search/doubao_search 合并）。
- **读取方式反转**：由自由键 `po.config.get("timeout_ms")` 改为 `from_value::<X>()`（先 `is_null` 短路回缺省），解析失败留 `warn` 痕，**不再静默回落**。
- **前端对齐**：创建页与详情页共用同一构造点 `*_config_from_form` / `*_form_from_config`，并新增「表单产物反序列化回 common 结构体」+ 往返幂等测试。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| [common/src/config.rs](common/src/config.rs#L737-L900) | 工具 config 结构体 SSOT | 8 类结构体集中定义 + 缺省常量 `CLI_TOOL_DEFAULT_TIMEOUT_MS` / `CLI_TOOL_DEFAULT_MAX_OUTPUT_BYTES`；`McpToolConfig` 带 `#[serde(deny_unknown_fields)]` | `#L737-L900` |
| [common/src/config.rs](common/src/config.rs#L374-L475) | `ShellExecConfig` | `shell_exec` 专用配置 + `Default`/访问器（`default_timeout_ms()` 30 万 ms、`path_additions()` 回退 `ShellConfig` 默认、`home_mode()` 回退 `isolated`） | `#L374-L475` |
| [src/pkg/tool_registry/http.rs](src/pkg/tool_registry/http.rs#L42) | HTTP 工具运行时 | `pub use common::config::HttpToolConfig;` 薄转发 + 按结构体读取 config | `#L42` |
| [src/pkg/tool_registry/shell_exec.rs](src/pkg/tool_registry/shell_exec.rs#L31) | shell_exec 运行时 | `pub use common::config::ShellExecConfig;`，路径补全 / HOME 策略消费 | `#L31` |
| [src/pkg/tool_registry/mcp.rs](src/pkg/tool_registry/mcp.rs#L30) | MCP 工具绑定 | `pub use common::config::McpToolConfig;`（只存 server_id + tool_name，禁止复制 Server 连接信息） | `#L30` |
| [src/pkg/tool_registry/fs_read.rs](src/pkg/tool_registry/fs_read.rs#L16) | FS 工具 | `pub use common::config::FsToolConfig;`（fs_read / fs_write 共用同一结构体） | `#L16` |
| [src/pkg/tool_registry/gh_cli.rs](src/pkg/tool_registry/gh_cli.rs#L59) | CLI 工具 | `pub use common::config::CliToolConfig;`（gh_cli / lark_cli 合并，缺省 bin 由调用方传入） | `#L59` |
| [src/pkg/tool_registry/browser.rs](src/pkg/tool_registry/browser.rs) | browser 内置工具 | `BrowserConfig`（`command` / `timeout_ms` / `max_output_bytes` / `install_hint`，键名对齐 `ToolPo::cli_command` 等泛型访问器） | 见 browser.rs |

**章节来源**
- [common/src/config.rs:L737-L900](common/src/config.rs#L737-L900)
- [common/src/config.rs:L374-L475](common/src/config.rs#L374-L475)
- [内置工具集.md](docs/wiki/zh/content/功能模块/工具生态系统/内置工具集/内置工具集.md)

---

## §3 架构约定

本卡与 [Handler 宏元数据契约 + ToolPo config/parameters_schema 字段分离](docs/wiki/knowledge/zh/Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位/Handler 宏工具 ToolPo config 与 parameters_schema 字段分离：运行时行为配置（无进展限制）与参数 JSON Schema 各归其位.md) 构成 **ToolPo.config 体系** 的 结构体定义与前后端共享 / 字段语义契约 互补视角；按 AGENTS §2.1.3 Level 3 保留平行卡。前端消费侧见关联卡 [工具详情页配置编辑](docs/wiki/knowledge/zh/工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复/工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复.md)。

```
common/src/config.rs（SSOT：8 类工具 config 结构体）
        │  pub use 薄转发（所有权在 common，src 侧零改动）
        ▼
src/pkg/tool_registry/{http,shell_exec,mcp,fs_read,fs_write,gh_cli,lark_cli,
                       shell_tool,browser,tavily_search,doubao_search}.rs
        │  from_value::<X>(&po.config)  ← is_null 先短路回缺省，失败留 warn
        ▼
工具运行时：字段通过结构体访问器带缺省兜底
        ▲
        │  同一字段键名（表单构造/反填互逆）
frontend/src/components/create_tool_http.rs / create_tool_shell.rs
frontend/src/pages/finance/tool_detail.rs
```

- 缺省值策略分两种：`CliToolConfig` / `BrowserConfig` / `SearchToolConfig` 的缺省由**各工具自身常量**兜底（因此字段为 `Option`）；`ShellExecConfig` 的缺省回退 `ShellConfig` 内置默认。
- `McpToolConfig` 只存「标准工具记录 → MCP server/tool」绑定；Server 连接/凭据/命令属 `McpServerPo.config`，不得复制进工具 config。

---

## §4 硬约束与回归红线

1. **新增/修改工具 config 字段必须三处同步**：`common/src/config.rs` 结构体 → 对应后端工具文件消费点 → 前端表单（`*_config_from_form` 与 `*_form_from_config` 互逆对）。只改一侧 = 键名漂移回归。
2. **后端读取统一 `from_value::<X>()`，禁止再散落自由键 `.get("snake_case_key")`**：读取前先 `is_null` 短路回缺省；反序列化失败必须留 `warn` 日志，禁止静默回落（静默回落会掩盖前后端键名不一致）。
3. **`pub use` 薄转发保持既有引用点不变**：结构体所有权在 `common`，`src/pkg/tool_registry/*` 只做转发；禁止在工具文件里再定义同名本地结构体副本（会重新引入双份 SSOT）。
4. **`credential_requirements` 带 `skip_serializing_if`**：保存 config 时按结构体**整体覆盖**，不能让 base 合并逻辑把已清空的旧值保留（旧值删除不掉）。
5. **`McpToolConfig` 禁止携带 Server 连接信息**：`#[serde(deny_unknown_fields)]` 强制；config 只保留 `server_id` + `tool_name`。
6. **表单产物必须能反序列化回 common 结构体且往返幂等**：新增字段必须补「表单 → 结构体 → 表单」幂等测试，锁死字段集漂移。

---

## §5 历史演进

- **2026-09-30（base 4ad13f2e→HEAD，commit 917cfe7d）**：原 `src/pkg/tool_registry/*` 内各自定义的 config 结构体统一下沉 `common::config`；fs_read/fs_write 去重为 `FsToolConfig`，gh_cli/lark_cli 合并为 `CliToolConfig`（缺省 bin 提为 `command(default_bin)` 参数），tavily/doubao 合并为 `SearchToolConfig`，新建 `BrowserConfig`；后端反向读取由自由键 `get` 改为 `from_value::<X>()`；前端表单键名对齐并新增往返幂等测试。