---
kind: wiki_knowledge_card
name: 工具详情页配置编辑：HTTP Shell 子表单复用创建页 + MCP 只读 + 内置工具配置表单 id 匹配修复
category: 前端应用 / 页面模块
scope:
- frontend/src/pages/finance/tool_detail.rs
- frontend/src/components/create_tool_http.rs
- frontend/src/components/create_tool_shell.rs
source_files:
- frontend/src/pages/finance/tool_detail.rs#L104-L160
- frontend/src/pages/finance/tool_detail.rs#L425-L462
- frontend/src/pages/finance/tool_detail.rs#L870-L885
- frontend/src/components/create_tool_http.rs#L157-L260
- frontend/src/components/create_tool_http.rs#L323-L360
- frontend/src/components/create_tool_shell.rs#L37-L143
- docs/wiki/zh/content/前端应用/页面模块/Finance 管理页面/工具管理/工具管理.md
- docs/wiki/zh/content/前端应用/页面模块/Finance 管理页面/工具管理/HTTP工具创建界面.md
- docs/wiki/knowledge/zh/HTTP 工具创建表单限定 method 白名单为 GET_POST/HTTP 工具创建表单限定 method 白名单为 GET_POST.md
- 【关联卡：后端视角】docs/wiki/knowledge/zh/工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆/工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆.md

---

## §1 概述

**本卡角色**：Finance 工具详情页的「工具配置编辑」能力知识卡。回答「详情页到底能改哪些工具的 config、怎么复用创建页表单、内置工具表单为什么按 id 匹配」。**定位：改工具详情页配置区、排查『内置工具配置表单不显示』或『HTTP/Shell config 改不动』时读。**

- **内置工具结构化表单**：`builtin_config_form(id)` 按 **稳定标识 `t.id`**（`"gh_cli"` / `"shell_exec"` / …）匹配 `BuiltinConfigForm` 枚举分支；不匹配（MCP / HTTP / 未知 Builtin）→ `None` 回退只读 JSON textarea。
- **修复的 bug**：原按**显示名** `t.name` 匹配恒为 `None`——内置工具入库时 `name` 是「GitHub CLI」等人类可读名，导致结构化表单从不显示。
- **用户自建工具（HTTP / Shell）配置编辑放开**：详情页直接复用创建页协议子表单 `HttpToolSubForm` / `ShellToolSubForm`（字段集与校验一致，不另写一套）。
- **MCP 保持只读**：MCP 工具的 config 源自 Server 同步（`McpToolConfig` 仅存 server_id + tool_name），不接受页面直接编辑。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| [frontend/src/pages/finance/tool_detail.rs](frontend/src/pages/finance/tool_detail.rs#L104-L160) | 内置工具表单匹配 + 状态 | `builtin_config_form(id)` 按 id 枚举分支；`BuiltinConfigFormState` 字段名与 `CliToolConfig` / `ShellExecConfig` 对齐 | `#L104-L160` |
| [frontend/src/pages/finance/tool_detail.rs](frontend/src/pages/finance/tool_detail.rs#L425-L462) | 详情页编辑状态装配 | HTTP/Shell 各自 `Signal<*FormState>`；`use_effect` 内按 `tool.protocol` 用 `*_form_from_config` 反填 | `#L425-L462` |
| [frontend/src/pages/finance/tool_detail.rs](frontend/src/pages/finance/tool_detail.rs#L870-L885) | 渲染分支 | HTTP → `HttpToolSubForm` / Shell → `ShellToolSubForm`，MCP 走只读 JSON | `#L870-L885` |
| [frontend/src/components/create_tool_http.rs](frontend/src/components/create_tool_http.rs#L157-L260) | HTTP 表单双向转换 | `http_config_from_form` / `http_form_from_config`（创建页与详情页共用） | `#L157-L260` |
| [frontend/src/components/create_tool_http.rs](frontend/src/components/create_tool_http.rs#L323-L360) | method `<select>` 修复 | 每个 `option` 显式绑定 `selected`，避免反填后只绑 `value` 丢选中 | `#L323-L360` |
| [frontend/src/components/create_tool_shell.rs](frontend/src/components/create_tool_shell.rs#L37-L143) | Shell 表单 | `shell_config_from_form` / `shell_form_from_config` / `ShellToolSubForm`，与 `ShellToolConfig` 字段一一对齐 | `#L37-L143` |
| 测试 | 回归锁 | `builtin_config_form_matches_factory_ids_not_display_names`（显示名→None 负例） + HTTP/Shell 表单往返幂等 | `#L1128` |

**章节来源**
- [frontend/src/pages/finance/tool_detail.rs:L104-L160](frontend/src/pages/finance/tool_detail.rs#L104-L160)
- [工具管理.md](docs/wiki/zh/content/前端应用/页面模块/Finance 管理页面/工具管理/工具管理.md)

---

## §3 架构约定

本卡与 [工具 config 结构体下沉 common 前后端 SSOT](docs/wiki/knowledge/zh/工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆/工具 config 结构体下沉 common 前后端 SSOT：8 类结构体 + 后端薄转发 + from_value 强类型读取 + 表单构造反填互逆.md) 构成「工具配置」体系的 前端消费侧 / 后端 SSOT 侧 双端视角；HTTP method 白名单细粒度约定见 [HTTP 工具创建表单限定 method 白名单为 GET/POST](docs/wiki/knowledge/zh/HTTP 工具创建表单限定 method 白名单为 GET_POST/HTTP 工具创建表单限定 method 白名单为 GET_POST.md)。

- **表单渲染分派**：内置工具 → 结构化字段表单；HTTP / Shell 自建工具 → 复用创建页协议子表单；MCP → 只读 JSON。
- **反填路径**：详情响应 `tool.config` → `builtin_form_from_config` / `http_form_from_config` / `shell_form_from_config` → `Signal` → 子表单渲染。
- **保存路径**：子表单状态 → `*_config_from_form` → `serde_json::to_value` → 提交；因 `credential_requirements` 带 `skip_serializing_if`，按结构体整体覆盖，避免 base 合并残留旧值。

---

## §4 硬约束与回归红线

1. **内置工具表单必须按 `t.id` 匹配，禁止用 `t.name`**：`name` 是人类可读显示名（「GitHub CLI」），按它匹配恒 `None`。回归测试 `builtin_config_form_matches_factory_ids_not_display_names` 已锁死。
2. **详情页禁止为 HTTP/Shell 另写一套配置表单**：必须复用 `HttpToolSubForm` / `ShellToolSubForm`，字段集与校验与创建页保持一致；否则两处字段漂移。
3. **`<select>` 的每个 `option` 必须显式绑定 `selected`**：仅绑 `value` 在反填初值 / 重渲染时会丢选中（如 HTTP method 显示首个选项，再保存即写坏）。
4. **MCP config 保持只读**：MCP 工具 config 由 Server 同步派生，详情页不提供编辑入口。
5. **新增 config 字段需同步三处**：`common` 结构体 → 后端消费点 → `*_config_from_form` / `*_form_from_config` 互逆对（含幂等测试）。

---

## §5 历史演进

- **2026-09-30（base 4ad13f2e→HEAD，commit 0762e1d8 + 917cfe7d）**：修复内置工具结构化配置表单因按显示名 `t.name` 匹配而恒不显示的问题，改用 `t.id`；放开 HTTP/Shell 自建工具 config 编辑（复用创建页子表单，MCP 保持只读）；`<select>` 补显式 `selected`；新增「显示名→None」负例与表单往返幂等测试。