---
kind: rag_card
name: ChatMessage::System 消息角色：人设/指令/规则 与 对话内容 分层传递给 Chat Completions API
category: Cortex API 协议层
scope:
- src/models/cortex_types.rs
- src/service/dao/cortex/native/http.rs
- src/models/prompt_builder.rs
source_files:
- src/models/cortex_types.rs#L135-L195
- src/models/cortex_types.rs#L160-L181
- src/models/cortex_types.rs#L192-L195
- src/service/dao/cortex/native/http.rs#L42-L44
- src/models/prompt_builder.rs#L155-L209
- docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Runtime 领域编排.md

---

# ChatMessage::System 消息角色

## §1 整体方案

`ChatMessage` 枚举原本只有 `User / Assistant / Tool` 三种变体，导致 Agent 的人设、能力清单、技能规范、回复规则全部挤在 User 角色里传给模型——模型无法区分"必须遵守的 System 指令"vs"用户输入"，行为不可控。

**95a0b1bf 修复**：新增 `ChatMessage::System { content: String }` 变体 + `system()` 构造器 + `to_summary_text` 处理，PromptBuilder trait 新增 4 个构建方法（`build_initial_messages / build_sleep_initial_messages / build_summary_initial_messages / build_intent_analyze_initial_messages`）统一输出 `Vec<ChatMessage>` 而非扁平字符串。

分层后的消息结构：
- **ChatMessage::System**：人设 + 全量技能规范 + 回复规则 §0-§5（审题 SOP + 何时直接回复 + send_message 正确用途 + 闲聊豁免 + 检索空结果 + 禁止假忙）
- **ChatMessage::User**：当前用户信息（含 user_id / 偏好）+ 历史对话 20 条 + 上下文（compaction 压缩后的近期摘要 / 长期沉淀知识图谱引用）+ 当前消息

修复后首次调用模型时，System 角色以 Chat Completions API 规范传递，指令约束力显著提升，告别"Agent 误以为必须调用 send_message 才能结束任务"的心理陷阱。

## §2 关键文件路径表格（读代码直接跳）

| 文件锚点 | 角色 | 核心契约 |
|---------|------|---------|
| [src/models/cortex_types.rs](src/models/cortex_types.rs#L135-L195) | ChatMessage 枚举 + System 变体 | `ChatMessage::System { content: String }` 第四种变体；`system()` 构造器；`to_summary_text` 处理 System 消息 |
| [src/service/dao/cortex/native/http.rs](src/service/dao/cortex/native/http.rs#L42-L44) | HTTP 序列化 | System 角色序列化为 `{"role":"system","content":"..."}` |
| [src/models/prompt_builder.rs](src/models/prompt_builder.rs#L155-L209) | PromptBuilder trait 扩展 | `build_initial_messages / build_sleep_initial_messages / build_summary_initial_messages / build_intent_analyze_initial_messages` — 输出 `Vec<ChatMessage>`，System + User 各一条 |
| 【平行卡】PromptBuilder workspace_context | trait 扩展全貌 | [PromptBuilder workspace_context](docs/wiki/knowledge/zh/PromptBuilder 工作空间上下文注入：workspace_context 方法让 Agent 感知文件操作路径/PromptBuilder 工作空间上下文注入：workspace_context 方法让 Agent 感知文件操作路径.md) |

## §3 架构约定

1. **System 消息只放"必须遵守的规则"**：人设、能力清单、技能规范、回复规则 §0-§5 放 System；用户信息、历史对话、上下文、当前消息放 User。
2. **回复规则 §0-§5 必须拼在 System 尾部**：`build_final_response_guidance()` 返回的 70+ 行硬规则（审题 SOP / 直接回复豁免 / 闲聊免检索 / 检索空结果 / 禁止假忙）全部写入 System 消息尾部，保证模型最高注意力覆盖。
3. **ThinkLoopResult 的 Final 语义不变**：System 消息里的回复规则告诉模型"有足够信息直接输出自然文本 = Final 回复"，think_loop 遇到 `ThinkResult::Final { content }` 自动退出——Framework 层 consumer 负责把 Final 文本回传给对端，不需要 Agent 自己调 send_message。

## §4 约束清单

1. ❌ **禁止把对话内容写进 System 消息**：System = 规则层（Agent 视角），User = 对话层（外部视角），Role 混用 = 行为失控。
2. ✅ **所有 think_loop 调用场景（awaken / sleep / summary / intent_analyze）必须使用新 trait 方法**：不得再用单字符串 prompt 调用 think_loop；`capturingBrainDal` 等测试桩需同步适配（按序拼接所有消息 content 等价旧版输出）。
3. ✅ **System 消息序列化必须在 Cortex DAO 层显式处理**：`cortex/native/http.rs` 里必须有 System 角色的 JSON 分支，漏写 = 运行时 panic。
