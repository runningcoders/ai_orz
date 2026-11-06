---
kind: 设计决策
module: 02-Agent运行时
name: two_stage_append_only_prompt_design
title: 两阶段唤醒 Prompt 追加化与前缀缓存设计
status: 现行
scope:
- src/service/domain/runtime/intent_analyze.rs
- src/service/domain/runtime/memory.rs
- src/service/domain/runtime/awakening.rs
- src/service/domain/runtime/inbox.rs
- src/service/domain/runtime/legacy_awaken/context.rs
- src/service/domain/runtime/legacy_awaken/backend.rs
- src/service/domain/runtime/legacy_awaken/intent_node.rs
- src/service/domain/runtime/legacy_awaken/nodes.rs
- src/service/domain/runtime/legacy_awaken/poll_node.rs
- src/service/domain/runtime/legacy_awaken/blueprint.rs
- src/service/domain/runtime/think_round/nodes.rs
- src/service/dal/agent/builder/default.rs
- src/models/prompt_builder.rs
- src/service/domain/runtime/types.rs
- common/src/enums/thinking_scene.rs
related:
- 02-Agent运行时/A-设计/runtime_design.md
- 02-Agent运行时/A-设计/thinking_graph_engine_design.md
- 02-Agent运行时/A-设计/inbox_poll_and_intent_revision_design.md
- 02-Agent运行时/A-设计/agent_loop_engine_design.md
---

# 两阶段唤醒 Prompt 追加化与前缀缓存设计

> 🎯 **本文档定位**：设计意图权威。回答「意图识别（Phase 1）与正式执行（Phase 2）如何共用同一段 Prompt 前缀，让第二次调用尽量命中缓存」。
> 现状以代码与 `C-知识` 为准；本文与代码冲突时，以代码为准并回头修正本文。
> 状态：新增（2026-11-03），**已实现**；落地快照见 [两阶段唤醒Prompt追加化](../B-落地方案/两阶段唤醒Prompt追加化.md)。本文保持现行。
>
> 同步记录：2026-11-06 将【历史对话】从「固定 20 条 `ShortTermMemoryIndexPo` 摘要」改为预算驱动的阶梯式索引加载（见 §2.9 表格「历史对话」行与红线）；具体算法与常量见 `src/models/memory.rs` 的 `MemoryIndexPlan` / `memory_index_budget_tokens` 与 `src/service/domain/runtime/think_loop.rs` 的 `memory_index_budget_for` / `context_overflow_threshold`。

## 一、设计目标与关键决策

### 1.1 目标

1. **前缀可复用**：Phase 2 的请求前缀必须与 Phase 1 的初始 Prompt **逐字节一致**，让 provider 的 prompt cache 命中到 Phase 1 指令末尾。
2. **思考连续**：Phase 2 不是「另起一段相似 Prompt」，而是同一会话的下一轮——保留 Phase 1 的审题过程与结论，模型不需要重新进入语境。
3. **信息只追加**：意图结果、检索知识、后续新输入、执行指令一律追加在尾部，不插入已有前缀，不重排已有区块。
4. **行为不变**：工具白名单的允许集合、意图分析七字段、直答/澄清短路、降级语义、取消语义全部保持现状；只改变「在哪里拦截」和「消息怎么摆放」。
5. **可验证**：用消息数组前缀相等 + `TokenUsage.cached_tokens` 两项指标验收，而不是靠体感。

### 1.2 关键决策

| 问题 | 方案 | 理由 |
|------|------|------|
| 两阶段 Prompt 的关系 | 同一会话追加：Phase 2 的初始消息 = Phase 1 初始消息原样 + 追加消息 | 只有原样复用才能让缓存命中整段初始 Prompt；重排区块会让插入点之后全部失效 |
| 共用部分（base）的边界 | System（人设 + 全量技能 + 最终回复规范）+ 公共上下文 + 历史/压缩 + 消息链 + 资源/视觉 + Trace + 当前消息 | 这些在两阶段产生时都已确定，且占 token 大头，最适合做稳定前缀 |
| 意图专用指令位置 | 追加在 base 的**同一条 user 消息尾部**，不放进 System | System 必须与 Phase 2 完全一致；同角色内追加既不破坏角色交替，也不影响前缀 |
| 工具列表 | 两阶段使用**完全相同的全量工具描述符**（顺序、序列化一致） | tools 属于请求前缀的一部分，差一个工具就整段不命中 |
| 工具白名单执行时机 | 描述符阶段不再过滤；在工具调用分发时按 `ThinkingScene::IntentAnalyze` 判定 | 这是「拦截点放在调用时」的直接落地；既保留原允许集合，又让工具列表两边一致 |
| Phase 2 复用范围 | 只复用 Phase 1 的**初始消息**，不复用其工具调用轨迹 | 被拦截的调用、检索工具结果不该带进执行阶段；初始消息仍是其请求前缀，缓存照样命中 |
| 意图结果的载体 | 规范化的 `IntentAnalysis` JSON，作为 assistant 消息追加 | 让模型把审题结论当作「自己刚说过的话」，思考连续；序列化确定，便于前缀校验 |
| 新消息的处理 | 执行段重新追加「本次新消息」：锚点消息 + 收集器吸收批次；不重发历史 | 历史已在 base 里；重发只针对本次任务，成本可控，且强化「现在处理的是这一条」 |
| Trace 的 Prompt 口径 | 两阶段都写主 `trace_id`；`intent-{trace}` 只用于事件子轨迹 | Trace 行位于当前消息之前，口径不同会让共享前缀在最后一段断开 |
| 分析/知识在 Phase 2 的位置 | 全部放执行段尾部，不插入当前消息之前 | 保持 base 字节稳定；这是放弃原「重点前置」换取缓存命中率的明确取舍 |
| 失败降级 | 仍追加一份规范化 `IntentAnalysis`（空/旧结论）+ 执行段说明，不省略 assistant 消息 | 保持消息形态稳定；下游仍能按原约定自行自愈 |
| 直答/澄清 | 不进入 Phase 2，seed 直接丢弃 | 原短路语义优先，不为了缓存强行多跑一轮 |
| 与既有 inbox 追加功能的关系 | 收集规则、游标、合并判据、状态提交全部不变；只把新输入进入 Prompt 的方式统一为追加 | 复用已验证的新输入收集能力，避免另起一套判定 |
| 轮间新输入的落点 | `poll_round` 直接 append；`poll_exit` 的 rethink 改为在既有对话上追加后续跑；`poll_front` 仍回退重跑意图 | 只有 `poll_exit` 原来走 `prepare` 重建，会破坏追加契约，必须随本设计一起改 |
| 压缩回环 | 保持 `overflow → compact → prepare` 重建 | 压缩是有损变换，无法追加；这是本设计唯一允许重建的路径 |
| 外部 Agent（Cli / Remote） | 不进入 seed 续接与续跑追加，统一走重建 | `FlatPromptBuilder` 把初始消息压成单条 user，而执行层只取最后一条 user；追加后 base 原文会整体丢失。外部链路也没有 provider 级 prompt cache，追加化收益为零 |
| 缓存验收 | Phase 2 的 `cached_tokens` 应覆盖 Phase 1 初始 Prompt 的绝大部分 | 指标可观测，避免「看起来一样」但字节不同 |

### 1.3 非目标

- 不改模型、provider、`run_think_loop` 的轮次/超时策略。
- 不取消意图识别阶段，也不减少其七字段产出。
- 不改变工具白名单的**允许集合**，只改变判定发生的时机。
- 不优化沉默、总结、睡眠等其他场景的 Prompt。
- 不承诺所有 provider 都命中（是否缓存由 provider 决定）；本设计只保证「请求前缀具备可缓存形状」。

## 二、架构思路

### 2.1 两阶段消息契约

```text
Phase 1（意图识别）
  system    : 人设 + 全量技能 + 最终回复规范
  user      : 公共上下文 + 历史/压缩 + 消息链 + 资源/视觉 + trace
              + 当前消息
              + "本段只做意图识别，输出 IntentAnalysis JSON"

Phase 2（正式执行）
  system    : 与 Phase 1 逐字节一致
  user      : 与 Phase 1 逐字节一致
  assistant : IntentAnalysis 规范 JSON
  user      : "以上为意图识别部分，下面开始真正处理本次新消息："
              + 重新追加的新消息（当前消息 + 吸收批次）
              + 相关知识 / 思维方法 / 上一轮思考摘要
              + 执行要求
```

- Phase 1 与 Phase 2 使用同一套 `ChatMessage` 角色交替：`system → user → assistant → user`，避免连续同角色消息在严格 provider 上被合并或拒绝。
- 允许 Phase 1 内部多轮工具调用；这些轮次不进入 Phase 2 的初始 seed。
- Phase 2 之后若再吸收新输入，继续走现有的轮级 `poll_round` / `poll_exit` 尾部追加，不回头改 base。

### 2.2 唯一 base 装配

新增一个**唯一**的 base 装配入口，供意图节点与装配节点共用。它负责：

1. 读取同一份数据源：Agent 人设与技能、工作空间、用户画像、项目/任务、本体词表、消息链与资源行、视觉 parts、压缩槽、历史记忆、工具失败统计、主 trace、当前消息。
2. 按固定顺序写入同一个 PromptBuilder。
3. 保证所有集合顺序确定（技能按 `name + id`、工具按注册顺序），禁止 HashMap 迭代顺序进入 Prompt。
4. 意图阶段与正式阶段**都不得**在这段 base 中放入意图分析结果、检索知识或执行指令。

意图阶段在 base 之后追加意图指令；正式阶段把 base 当作 seed，再追加结果与执行段。装配节点在「已存在 seed」时必须跳过 `build()` / `build_initial_messages()` 的重新渲染，否则会把分析结果插回当前消息之前，前缀立即失效。

### 2.3 前缀不变量

落地后必须同时满足以下六条，任何一条破坏都视为设计失败：

| 编号 | 不变量 |
|------|--------|
| I1 | Phase 1 与 Phase 2 的 System 消息内容完全相等 |
| I2 | Phase 1 与 Phase 2 的 base user 消息内容完全相等（含空白与分隔符） |
| I3 | 两阶段的工具描述符集合、顺序与序列化结果完全相等 |
| I4 | Phase 2 初始消息的前 N 条与 Phase 1 初始消息逐条相等 |
| I5 | 新输入、意图结果、检索知识、执行指令只出现在第 N+1 条之后 |
| I6 | base 中不出现任何随时间变化且两边不一致的值（尤其是 trace 口径） |

### 2.4 工具调用时拦截

- 两阶段都使用正式执行的全量工具描述符。
- `run_dispatch` 在解析出工具后、执行前判定：`ThinkingScene::IntentAnalyze.is_tool_allowed(tags)`。
- 判定不通过时：
  - **不执行**任何副作用；
  - 追加一条 tool message，明确「当前为审题阶段，该工具不可执行，请继续完成理解并输出 JSON」；
  - 该轮计入正常的轮次预算，不额外回退。
- 判定通过时行为与现状一致，允许集合保持不变（神经 / 记忆 / 查询 / 检索类）。
- 拦截发生在 seed 之外，因此不会把被拒绝的调用带进正式执行。

### 2.5 执行段（Phase 2 尾部）的顺序

执行段固定按以下顺序拼接，全部位于 base 之后：

1. 分隔与切换语：说明「以上为意图识别部分」。
2. 重新追加的本次新消息：当前消息 + 收集器吸收批次；图片等附件已在 base 中，这里只放文本，避免重复视觉 token。
3. 相关知识 / 思维方法 / 上一轮思考摘要（检索节点产出）。
4. 执行要求：按意图结论推进任务，不要再次输出 IntentAnalysis JSON。

历史对话、压缩摘要、消息链、资源行、视觉 parts 不再重复出现在执行段。

### 2.6 失败、修订与中途新输入

- **意图失败降级**：仍追加当前使用的 `IntentAnalysis`（旧结论清空终态字段或空结构）作为 assistant 消息，再追加执行段说明；不省略 assistant，避免消息形态在两阶段间漂移。
- **意图修订**：Phase 1 因新输入回退重跑时，seed 取**最后一次**意图调用的初始消息（含修订尾部）；Phase 2 复用该 seed。首次调用与修订调用之间的缓存会部分失效，但 base 仍然命中。
- **Phase 1 与 Phase 2 之间到达的新输入**：进入收集器的 `pending_turn`，由执行段第 2 步重新追加；不插入 base。
- **Phase 2 开始后到达的新输入**：沿用 `poll_round` / `poll_exit` 的尾部追加与信号回退，不修改 seed。

### 2.7 观测与验收

- 在意图与正式执行两侧记录 `TokenUsage.cached_tokens`。
- 验收口径：在 provider 上报缓存的前提下，Phase 2 的 `cached_tokens` 应覆盖 Phase 1 初始 Prompt 的绝大部分；若接近 0，优先排查 I1–I6 哪一条被破坏。
- 同时保留现有的消息级 Trace 指纹，确认两阶段的消息数与角色序列符合契约。

### 2.8 与既有 inbox 追加功能的衔接

既有功能（`inbox_poll_and_intent_revision_design.md`）负责「哪些消息算新输入、何时吸收、吸收后如何影响行为」；本设计只负责「吸收后的消息怎么进入 Prompt」。两者边界如下：

| 时机 | 既有行为 | 本设计后的行为 |
|------|----------|----------------|
| 意图之前 / 意图之后、检索之前 | `poll_front`：能回退则带修订尾部重跑意图；不能回退则暂存 `pending_turn` | 保留。修订尾部仍是追加；重跑意图的新请求仍复用同一 base |
| 思考轮内 | `poll_round`：直接向当前消息列表追加 user 消息 | 保留，且天然满足追加契约 |
| 检索后 / 思考结束后 | `poll_exit` 的 rethink 原走 `prepare` 重建 Prompt | **改为追加模式**：以 `Artifacts.messages` 为已有对话，尾部追加新输入后继续跑 |
| 上下文压缩回环 | `overflow → compact → prepare` 重建 | 保留重建；压缩是有损变换，是唯一例外 |
| 唤醒成功后置 `Processed` | 消费者按 `injected_ids` 批量提交 | 保留；取消或失败仍保持 `Pending` |

`poll_exit` 追加模式的判定不能靠「`Artifacts` 存在」——压缩回环也会产生 `Artifacts`。必须由 `poll_exit` 写入一个显式的续跑标记（`InboxState.resume_append` 或等价总线上标记），`prepare` 读取该标记：

- 标记为真且 `Artifacts.messages` 非空：`PromptPack.initial_messages` = 既有消息 + 尾部新输入，不重跑 base 装配；
- 标记为真但 `Artifacts.messages` 为空（脚本后端 / 外部执行器）：退回原重建路径，保证兼容；
- 标记为假（首次装配 / 压缩回环）：走原重建路径。

`poll_front` 与 `poll_round` 不设置该标记。所有吸收、游标推进、`injected_ids` 记录仍由原收集器完成，本设计不复制这套逻辑。

### 2.9 历史消息与消息链的边界（现状对齐）

统一 base 装配里存在两个独立来源，**不合并、不去重**，因为语义不同：

| 项 | 现状 | 本设计 |
|----|------|--------|
| 历史对话（短期记忆索引块） | 原：`get_recent_context` 取固定 20 条 `ShortTermMemoryIndexPo`（每条 `summary` 摘要），只按 `agent_id + ShortTerm + limit`，倒序翻转成正序。新（2026-11-06）：装配节点按「记忆索引块」渲染——预算 = 压缩阈值 × 10%，四档配额 `4:3:2:1`（`Full` / 3 段 / 2 段 / 1 段，由近到远），复用总结阶段 `estimate_tokens` 从新往旧累加切档，**不设条数上限** | 改为预算驱动阶梯式加载；元信息不分级（id / 绝对时间 / `ts=` Unix 秒级 / 相对时间 / tags / task_id 四档逐字节一致，仅摘要正文随档衰减）；`T0` = 入站消息时间作锚点冻结索引块，保证 Phase 1 / Phase 2 base 逐字节一致；按需翻页（每页 20、最多 50 页）而非固定 20 条；仍只按 agent 级口径，不引入 project / thread 过滤 |
| 同上下文 | 短期记忆 PO 只有 `agent_id / task_id`，无 project / thread / root；`MemoryQuery` 有 `task_id` 但调用方未传。新输入同上下文由收集器按 project / task 的 `Option` 相等判定 | 短期记忆查询不改；同上下文由「当前消息 + 项目/任务区块 + 收集器」表达 |
| 同 thread | 独立 `fetch_message_thread`：`root_id + organization_id`，`created_at ASC` + `limit 60`，排除当前消息与 `Recalled`，取链头 + 最近 6 条 | 保留为**唯一**消息链来源；仍然只进 base，不进执行段 |
| 与新输入的重叠 | 查询没有时间上界，晚于锚点的消息可能已落库并被链查询命中 | 链查询加 `created_at <= 锚点消息` 上界（同毫秒按 id 去重）；晚于锚点的消息只由收集器进入执行段 |
| 长链截断 | `ORDER BY created_at ASC LIMIT 60` 取的是**最早 60 条**，链超长时「尾部 6 条」并非最近 6 条 | 改为「链头 + 最近 6 条」：先 `DESC` 取 K 再反转，或分两次查询 |

落地后实际实现（2026-11-03）：`fetch_message_thread` 以 `root_id + organization_id` 查询，链头单独按 id 取；尾部窗口以 `created_before = 锚点 + 1`、`ORDER BY created_at DESC, id DESC`、每页 `LIMIT K + 1` 按 `offset` 向更早翻页，边扫描边过滤当前消息 / 同毫秒 id 更大 / `Recalled`，凑满 K 条或链耗尽即停；内存反转后按 `(created_at, id)` 正序，最终输出「链头 + 最近 6 条」。分页扫描而非单页 K+1 的原因：单页可能被过滤掉若干条，导致「最近 6 条」不足 6。

红线：base 中的【历史对话】（预算驱动的短期记忆索引块）与【消息链上下文】必须分别渲染、各自稳定排序；不得把消息链原文塞进【历史对话】冒充索引，也不得用索引替代链内原文。

### 2.10 上次意图结论在新消息后的携带

新消息到达后重新思考时，上一轮意图结论必须仍在消息列表里，且**不得删除、重排或重复整段**：

| 路径 | 现状 | 本设计 |
|------|------|--------|
| `poll_front` 回退重跑意图 | ✅ 已携带：意图节点读 `ContextKey::Intent` 作为 `previous` 做增量修订 | 保留；seed 覆盖为**最后一次**意图调用的初始消息 |
| `poll_exit` rethink → `prepare` | ⚠️ 弱携带：只失效 `Prompt`，`prepare` 把结论重新渲染成【输入理解结果】参考区块，不是 assistant 消息 | 改为以 `Artifacts.messages` 续跑；Phase 1 的 assistant JSON 与后续轨迹原样保留 |
| Phase 2 首次执行 | — | seed 已含 Phase 1 assistant JSON，天然携带 |

补充约束：

- 新 user turn 只加一句「上一条 assistant JSON 是本次意图识别结论，下面为新输入，请据此继续」，**不重复 JSON 正文**。
- 回退路径（`poll_front` / 检索前 `poll_exit`）会清空 `Artifacts`，因此结论必须由 `Intent` 分区重新写入，不能依赖 `Artifacts.messages` 恢复。
- 修订重跑时 seed 只保留最后一次调用的初始消息；旧 seed 不得与新 seed 混用。

## 三、涉及文件清单

| 文件 | 角色 | 改动要点 |
|------|------|----------|
| [intent_analyze.rs](../../../../src/service/domain/runtime/intent_analyze.rs#L31-L189) | Phase 1 入口 | 改为调用唯一 base 装配；System 换成 `awaken_system_part`；意图指令尾插；工具改全量 |
| [default.rs](../../../../src/service/dal/agent/builder/default.rs#L1135-L1230) | Prompt 渲染 | 提供 base 渲染与执行段渲染；意图断言不再生成独立 System |
| [default.rs](../../../../src/service/dal/agent/builder/default.rs#L1593-L1660) | Phase 1 初始消息 | 改为「base + 意图尾部」，不再过滤技能、不再拼专用 System |
| [prompt_builder.rs](../../../../src/models/prompt_builder.rs#L218-L240) | Builder 契约 | base 渲染入口与执行段入口的 trait 定义 |
| [context.rs](../../../../src/service/domain/runtime/legacy_awaken/context.rs#L90-L150) | 唤醒依赖 | 增加 Phase 1 seed 槽（初始消息快照）与压缩槽访问 |
| [inbox.rs](../../../../src/service/domain/runtime/inbox.rs#L150-L210) | 新输入状态 | 增加续跑标记；既有游标、批次、`injected_ids` 不变 |
| [poll_node.rs](../../../../src/service/domain/runtime/legacy_awaken/poll_node.rs#L90-L251) | 三处挂点 | `poll_exit` 的 rethink 写入续跑标记；`poll_front` / `poll_round` 不写 |
| [blueprint.rs](../../../../src/service/domain/runtime/legacy_awaken/blueprint.rs#L120-L190) | 图拓扑与预算 | rethink 边保持指向 `prepare`，由标记决定追加或重建；预算无需新增节点 |
| [intent_node.rs](../../../../src/service/domain/runtime/legacy_awaken/intent_node.rs#L47-L119) | 意图节点 | 写入 seed；修订时覆盖 seed；失败时仍写可追加结论 |
| [backend.rs](../../../../src/service/domain/runtime/legacy_awaken/backend.rs#L161-L178) | LLM 调用缝 | 意图调用返回分析结果 + 本次初始消息，供节点保存 |
| [awakening.rs](../../../../src/service/domain/runtime/awakening.rs#L700-L760) | 消息链取数 | 链查询加锚点上界；长链改为「链头 + 最近 K 条」 |
| [memory.rs](../../../../src/service/domain/runtime/memory.rs#L20-L45) | 历史记忆取数（记忆索引窗口） | 改为预算驱动阶梯式加载（`MemoryIndexPlan` + `memory_index_budget_for`），按需翻页，不再固定 20 条；仍只按 agent 级口径，不引入 project / thread 过滤 |
| [nodes.rs](../../../../src/service/domain/runtime/legacy_awaken/nodes.rs#L280-L315) | 装配节点 | 有 seed 时直接以 seed 为初始消息；续跑标记为真时以 `Artifacts.messages` 追加；两者都没有时保留旧路径 |
| [think_round/nodes.rs](../../../../src/service/domain/runtime/think_round/nodes.rs#L548-L657) | 工具分发 | 白名单判定从描述符构造挪到调用前；被拦截时回 tool message |
| [thinking_scene.rs](../../../../common/src/enums/thinking_scene.rs#L111-L126) | 场景矩阵 | 复用现有 `is_tool_allowed`，不改允许集合 |

## 四、边界与行为红线

1. **不得在 Phase 2 重新渲染 base**：一旦重新拼装并把分析/知识插入当前消息之前，I2/I4 立即失效，缓存收益归零。
2. **不得让不确定顺序进入 Prompt**：技能、工具、知识命中、消息链都必须稳定排序；任何 HashMap 迭代顺序都视为缺陷。
3. **不得让 trace、时间戳、随机数出现在两边不一致的位置**：base 内的 trace 必须用主 trace；子轨迹只进事件。
4. **工具拦截必须发生在执行前**：被拒绝的工具不得产生任何外部副作用、状态变更或消息发送。
5. **允许集合不变**：移动判定位置不等于扩大权限；意图阶段仍然只允许原矩阵中的标签。
6. **只重发新消息**：执行段不得重复历史、压缩摘要或消息链，否则每轮都会线性膨胀。
7. **直答/澄清不进入 Phase 2**：短路优先，避免为了缓存多跑一次执行。
8. **provider 不负责任**：缓存是否生效由 provider 决定；本设计只保证前缀形状，不以命中率替代正确性。
9. **取消与失败语义不变**：取消不提交吸收消息；意图失败降级不阻塞主流程。
10. **`poll_exit` 的 rethink 不得重建 base**：必须走续跑标记 + 既有消息追加；否则 I5 失效，这正是既有实现需要随本设计一起调整的唯一一处。
11. **压缩回环是唯一重建例外**：`overflow → compact → prepare` 必须有损压缩后重建；不得为了缓存把压缩摘要塞回原 base。
12. **续跑标记必须显式**：不能用「`Artifacts` 存在」推断，否则压缩回环会被误判成追加；`Artifacts.messages` 为空时必须退回重建，兼容脚本后端与外部执行器。
13. **历史索引与消息链不得混源**：短期记忆索引块仍是 agent 级摘要（预算驱动阶梯加载、不设条数上限），不扩成原始消息查询；同 thread 由 `root_id` 链单独表达。
14. **消息链不得越过锚点**：链查询必须限制在当前消息之前；晚于锚点的新消息只能由收集器进入执行段，避免重复渲染。
15. **上次意图结论不得被删改**：新输入到来后，仅在其后追加新的 user turn；上一轮 assistant JSON 保持原位。

## 五、扩展模式

本设计把两阶段关系固化为「同一会话追加」，后续扩展遵循同一形状：

- **新增阶段**（如执行前的风险审查）：在该阶段之后继续追加 assistant/user，不回改前面的消息。
- **新增上下文来源**（如新的检索器）：一律进入执行段或后续尾部，不进入 base；若必须进入 base，则两阶段都必须同时获得该数据。
- **新增工具类别**：只需维护 `ThinkingScene` 矩阵，工具描述符保持全量，拦截仍发生在分发处。
- **新增缓存观测**：复用 `TokenUsage.cached_tokens`，不另造指标口径。
- **换 provider**：只要其支持前缀缓存或提示缓存断点，消息契约不变；若不支持，本设计退化为「行为更连续」而不会破坏正确性。
