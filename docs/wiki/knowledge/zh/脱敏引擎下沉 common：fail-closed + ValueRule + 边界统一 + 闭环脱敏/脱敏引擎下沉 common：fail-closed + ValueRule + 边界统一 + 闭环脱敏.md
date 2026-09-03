---
kind: rag_card
name: 脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏
category: infra
scope:
  - 'common/src/redaction/*.rs'
  - 'common/src/redaction/mod.rs'
  - 'src/pkg/mod.rs'
  - 'src/pkg/logging.rs'
  - 'src/consumer/message.rs'
  - 'src/middleware/api_notice.rs'
source_files:
  - common/src/redaction/mod.rs#L1-L103
  - common/src/redaction/engine.rs#L1-L357
  - common/src/redaction/engine.rs#L129-L260
  - common/src/redaction/policy.rs#L1-L65
  - common/src/redaction/rule.rs#L1-L171
  - common/src/redaction/rule.rs#L173-L212
  - common/src/redaction/redact.rs#L1-L116
  - src/pkg/mod.rs#L18-L20
  - docs/wiki/zh/content/核心模块/工具与技能/工具系统/脱敏引擎下沉 common.md
  - docs/wiki/zh/content/核心模块/工具与技能/工具系统/工具输出与安全治理.md
  - docs/wiki/knowledge/zh/工具输出与安全治理/工具输出与安全治理.md
  - docs/wiki/knowledge/zh/@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染/@提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染.md
---

# 脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏

## §1 概述

AI Orz 的脱敏引擎从 `pkg/redaction` 下沉到 `common/src/redaction/` 成为前后端共享的单一事实源。核心边界决策（2026-09-03）：**系统内部不脱敏，仅在对外接口输出时按需脱敏**。内部存储（JSONL trace、SQLite、日志）保持原文，风险由访问控制承担；需要脱敏的出口接口在返回前用 `redact!` 宏包一层，不做全局响应改写。引擎架构五层分层：`rule.rs`（规则注册表）→ `mask.rs`（脱敏样式）→ `policy.rs`（场景策略）→ `engine.rs`（JSON 遍历 + AC 预检文本扫描）→ `redact.rs`（`redact!` 宏的 autoref specialization 类型分派）。

## §2 关键文件表

| 文件 | 职责 |
|------|------|
| `common/src/redaction/mod.rs` | 模块入口；对外暴露 `redact_json` / `redact_text` 快捷函数（使用 EXPORT 策略）；分层架构 doc comment |
| `common/src/redaction/engine.rs` | JSON 递归遍历 + 文本级扫描；AC 预检快速路径（无敏感词时零拷贝返回 Cow::Borrowed）；裸凭证值形态兜底（`value_shape_boundary_match`）；性能优化（键名判定零分配字节窗口） |
| `common/src/redaction/policy.rs` | 场景化策略预设：`EXPORT`（对外输出，Partial 样式 + 自由文本扫描）、`PERSIST`（内部持久化，当前未启用）、`LOG`（日志输出，Full 样式 + 关闭自由文本扫描 + 压低深度上限） |
| `common/src/redaction/rule.rs` | 规则注册表：`KEY_RULES`（6 条键名规则 + LLM 用量字段 exclude 名单）+ `VALUE_RULES`（7 条值形态规则——sk-/ghp_/glpat-/xoxb-/AKIA/eyJ/sk_live_ 前缀）；`ValueClass::StringOnly` vs `Any` 类型感知；`match_key` / `match_value_shape` 匹配函数 |
| `common/src/redaction/redact.rs` | `redact!` 宏的 autoref specialization 类型分派：`RedactStrDispatch`（第一优先级 `T: AsRef<str>` → 文本扫描）vs `RedactSerdeDispatch`（第二优先级可 JSON 往返类型 → 序列化脱敏反序列化）；fail-closed：JSON 往返失败 → 全遮蔽重试 → 仍失败返回 Err，**绝不把原文带回给调用方** |
| `src/pkg/mod.rs` L18-L20 | 脱敏引擎路径兼容转发（`pub use common::redaction`）；`init_all` 启动时调 `redaction::warmup()` 预热预检自动机 |

## §3 架构约定

- **fail-closed 失败语义**：`redact!` 宏返回 `Result<T, serde_json::Error>`，脱敏失败不回退原文。common::error 已提供 `From<serde_json::Error>`，在返回 `common::error::Result` 的接口里直接 `?` 即可。
- **边界统一**：内部保持原文（SQLite 存储、JSONL trace、日志文件），对外出口按需 `redact!`。禁止在中间层做隐式脱敏。
- **闭环脱敏**：命令参数内明文凭证（`--token secret123`）在 JSON 字符串值的自由文本扫描里被捕获——`engine.rs` L412-L415 的测试用例验证了这条路径。
- **值形态兜底**：键名未命中但值长得像凭证时（`"sk-abcdef123456"` 放在泛型 `data` 键下），`match_value_shape` 兜底全量遮蔽。token 边界安全——必须前导为空白/引号/分隔符，避免把 `"ask-"` 里的 `sk-` 误判为凭证（`value_shape_boundary_match` L266-L301）。
- **LLM 用量字段 exclude 名单**：`KEY_RULES.token` 规则排除 `usage/count/total/max/limit/remaining/cost/prompt/completion/...` 等字段——避免把 `total_tokens: 1536` 这类数值统计值误伤。

## §4 硬约束

1. **键名判定零分配**：`engine.rs` L5-L6 记录：实测 10 万次调用 20.9ms → 1.7ms。禁止回退到 `key.to_lowercase()`。
2. **AC 预检快速路径**：文本扫描前先用 Aho-Corasick 判断全文是否含任一敏感词，无命中直接返回 `Cow::Borrowed`。禁止跳过预检直接扫描。
3. **禁止切片优化**：`engine.rs` L9-L11 明确——CLI flag 形态 `--token secret123` 识别依赖左侧 `-` 上下文，一旦把扫描区间切掉前导安全区，`--token` 的 `-` 就会丢失导致漏脱敏。
4. **JSON 字符串形态保留引号**：`text_json_shape_keeps_quotes` 测试（L453-L461）验证 `{"api_key":"sk-..."}` 脱敏后仍是合法 JSON——不能吞掉两侧引号。
5. **内部不做任何日志输出**：common::redaction 模块内部只返回 `Result` / 就地改写，观测与降级策略完全由调用方决定。
6. **新增凭证 = 加一行**：扩展 `KEY_RULES` 或 `VALUE_RULES` 表，其余逻辑零改动。禁止在扩展路径里修改 engine.rs 核心循环。
