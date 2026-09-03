---
kind: rag_card
name: @提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染
category: module
scope:
  - 'common/src/mention.rs'
  - 'frontend/src/utils/mention.rs'
  - 'frontend/src/components/mention_picker.rs'
  - 'src/service/dal/agent/builder/default.rs'
  - 'src/service/dal/agent/prompt_builder_test.rs'
  - 'frontend/src/components/markdown.rs'
source_files:
  - common/src/mention.rs#L1-L326
  - common/src/mention.rs#L115-L142
  - common/src/mention.rs#L191-L214
  - common/src/mention.rs#L249-L306
  - frontend/src/utils/mention.rs#L1-L133
  - frontend/src/components/mention_picker.rs#L1-L135
  - frontend/src/components/mention_picker.rs#L137-L334
  - src/service/dal/agent/builder/default.rs
  - docs/wiki/zh/content/功能模块/消息系统/@提及功能（mention）.md
  - docs/wiki/zh/content/功能模块/消息系统/Agent消息集成.md
  - docs/wiki/knowledge/zh/脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏/脱敏引擎下沉 common：fail-closed + ValueRule + 边界统一 + 闭环脱敏.md
---

# @提及功能（mention）：文本协议 + 选择器 + prompt 注入 + Markdown 渲染

## §1 概述

AI Orz 的消息 @ 提及功能采用**标准 CommonMark 链接语法**承载，dest 为 `type:id`：`[@张伟](agent:agt_7f3)` / `[@数据清洗](task:tsk_a91)` / `[@客户数据平台](project:prj_2c8)`。核心价值：(a) 降级安全——未被提及渲染器识别时退化为普通链接而非暴露原始 `agent:agt_7f3` 串；(b) 消息体自包含——复制/转发/导入导出都不丢提及信息，零存储 migration；(c) 前后端协议单一事实源在 `common/src/mention.rs`，前端渲染（`frontend/src/utils/mention.rs`）和后端 prompt 注入（agent prompt builder）走同一套解析。

## §2 关键文件表

| 文件 | 职责 |
|------|------|
| `common/src/mention.rs` | **协议核心**：`MentionKind` 三类型枚举（Agent/Task/Project）、`MentionRef`（kind+id 协议层结构）、`ResolvedMention`（含 name+summary 的后端消费结构）、`parse_mention_dest`（dest → MentionRef）、`format_mention`（生成语法 + 名字转义）、`detect_mention_query`（光标处 @ 查询检测）、`apply_mention_pick`（插入替换 + 光标恢复）、`extract_mentions[_with_text]`（后端 prompt 注入提取）、`resolve_display_name`（实时名优先，快照名回退） |
| `frontend/src/utils/mention.rs` | **前端渲染层**：`transform_mentions`（pulldown-cmark 事件流拦截，把提及链接替换为 chip inline HTML）、`render_mention_chip`（chip HTML 生成 + XSS 转义）。`pub use common::mention::*` 把协议 API 原样转发 |
| `frontend/src/components/mention_picker.rs` | **@ 选择器 UI**：`MentionPicker` 多级菜单组件（All/Agent/Task/Project Tab + 候选搜索 + 键盘导航）、`MentionState` 外置状态（Signal + use_effect 候选加载 + req 序号丢弃过期响应）、`MentionPickedBar` 已提及胶囊条 |
| `src/service/dal/agent/builder/default.rs` | **后端 prompt 注入**：从消息正文提取 `extract_mentions_with_text`，解析出 Agent/Task/Project 实体后注入到 prompt 上下文 |
| `frontend/src/components/markdown.rs` | **Markdown 渲染**：接入 `transform_mentions` 把提及链接转为 chip |

## §3 架构约定

- **提及协议层**（common/src/mention.rs）是纯文本解析，**不依赖任何渲染库或 DOM**，前后端天然一致。
- **光标检测**：`detect_mention_query` 要求 `@` 前字符属于可触发边界（行首/空白/开括号），字母数字（邮箱 `a@b.com`）不触发。刚插入的 `[@张伟](agent:agt_1)` 光标停在末尾时不得再次弹菜单——靠 `is_query_char` 黑名单挡掉括号字符。
- **名字转义**：`format_mention` 对 `[` `]` `\` 做最小转义，避免破坏 Markdown 链接语法。
- **实时名覆盖**：`resolve_display_name` 命中 Agent 目录时覆盖文本快照名——改名后历史消息自动同步。Task/Project 无全局目录缓存，直接用快照名。
- **mousedown 而非 click**：`MentionPicker` 用 `onmousedown + prevent_default` 避免点击时 textarea 失焦导致光标位置丢失（组件 doc comment L9 有历史教训记录）。

## §4 硬约束

1. **协议迭代只能改 `common/src/mention.rs`**：前后端复用同一模块，改这里两边自动同步。禁止前端单独维护一套 mention 解析逻辑。
2. **token 边界安全**：`apply_mention_pick` 替换 `[start..caret]` 后末尾补一个空格，避免与后续文字粘连导致 `is_query_char` 误判。调用方必须把光标设回返回的新位置。
3. **候选范围分层**：项目会话 `mention_kinds_for` 限定为 `Agent + Task`（Agent 限于项目协作人），默认对话放开为 `Agent + Task + Project`（Agent 取组织全量由关键词收窄）。单一事实源在 `mention_picker.rs` L111-L119。
4. **前端纯 textarea**：MentionPicker **不碰光标 DOM**，输入框始终是受控 textarea，插入的是纯文本语法。避免中文输入法组合期间被重渲染打断（历史教训 `0644609c`）。
5. **XSS 防护**：`transform_mentions` 顺带承担源文 HTML 降级（Html/InlineHtml → Text），`render_mention_chip` 对展示名与 id 必过 `escape_html`。不能因为加了提及链路就放开 XSS。
