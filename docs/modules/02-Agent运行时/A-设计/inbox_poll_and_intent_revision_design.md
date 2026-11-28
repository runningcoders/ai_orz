---
kind: 设计决策
module: 02-Agent运行时
name: inbox_poll_and_intent_revision_design
title: 思考循环中的新输入收集与意图回退设计（可中断唤醒）
status: 现行
scope:
- src/service/domain/runtime/legacy_awaken/blueprint.rs
- src/service/domain/runtime/legacy_awaken/intent_node.rs
- src/service/domain/runtime/legacy_awaken/nodes.rs
- src/service/domain/runtime/think_round/blueprint.rs
- src/service/domain/runtime/think_round/nodes.rs
- src/service/domain/runtime/think_round/state.rs
- src/service/domain/runtime/think_round/deps.rs
- src/service/domain/runtime/intent_analyze.rs
- src/service/domain/runtime/types.rs
- src/service/dal/agent/builder/default.rs
- src/service/dao/message/mod.rs
- src/consumer/message.rs
- src/pkg/think_path/types.rs
- frontend/src/pages/message/chat.rs
- frontend/src/components/chat/cancel_thinking_button.rs
related:
- 02-Agent运行时/A-设计/agent_loop_engine_design.md
- 02-Agent运行时/A-设计/thinking_graph_engine_design.md
- 02-Agent运行时/A-设计/intent_aware_two_stage_awaken_design.md
- 02-Agent运行时/A-设计/two_stage_append_only_prompt_design.md
- 02-Agent运行时/A-设计/runtime_design.md
---

# 思考循环中的新输入收集与意图回退设计（可中断唤醒）

> 🎯 **本文档定位**：设计意图权威。回答「用户在 Agent 思考过程中继续发消息，怎么收集、怎么合并、什么时候回退重识别意图」。
> 现状以 `C-知识` 与代码为准；本文与代码冲突时，以代码为准并回头修正本文。
> 状态：新增（2026-11-02），**未实现**。落地后另出 `B-落地方案`，本文保持现行。

## 一、设计目标与关键决策

### 1.1 目标

1. **不丢输入**：唤醒进行中到达的用户消息，要么被本轮吸收，要么留在队列等下一次唤醒，永不静默丢弃。
2. **可中断转向**：用户补充的新约束能在下一轮思考中生效，而不是等本次唤醒吐完答案再由队列触发第二次唤醒。
3. **意图对齐**：在产生副作用（检索装配、工具执行、发答复）之前发现新输入，回到意图识别重新对齐目标，允许复用上一轮结论做增量修订。
4. **零成本空转**：没有新输入时不产生任何额外开销——不查库、不改 Prompt、不影响前缀缓存。

### 1.2 关键决策

| 问题 | 决策 | 理由 |
|---|---|---|
| 新输入从哪里取 | 从 DB 按 `Pending` + 游标拉取，不改 AOP 队列核心 | 队列已按 `order_key = agent_id` 门闩 FIFO，同一 Agent 同时只有一条事件在飞，DB 的 `Pending` 行就是现成待办清单 |
| 消息被吸收后怎么收尾 | 唤醒成功后才批量置 `Processed`；失败保持 `Pending` | 后续出队事件命中消费者已有的 `status == Processed` 幂等守卫自动 ack；失败时兄弟消息仍会各自唤醒，不会丢 |
| 在哪些位置检查新输入 | 三处挂点：`poll_front`（意图后）、`poll_round`（每轮边界）、`poll_exit`（发答复前） | 三处对应「还没开始干 / 干到一半 / 已经想给结论」三种时点，动作不同 |
| 什么时候回到意图识别 | 由 `inbox_intent_shift` 的**小脑二选一判据**决定：`reinterpret`（用户否定了当前意图）→ 回 `intent`；`supplement` → 尾插追加。判据拿到不结果（无小脑 / 配额耗尽 / 降级）一律 `supplement`。**与挂点位置无关** | 图本身具备回到任意节点的能力；回 `intent` 时 `IntentRevision` 带旧结论 + 新输入做增量修订，不必因「已检索」就禁止回退；追加成本最低，故作为降级默认 |
| 意图怎么复用 | 增量修订，一次 LLM 调用产出完整 `IntentAnalysis` | 纯结构合并（只并 `key_terms`）无法处理「别用 X，改用 Y」这类翻转；复用会倒是会害事 |
| 复用哪些字段 | `intent_type` / `confidence` / `key_terms` / `resolutions` / `summary` 作为修订输入 | 这些是「理解结论」，新输入可以在此基础上增量修改 |
| 哪些字段必须重算 | `direct_answer` / `need_clarification` / `retrieved_context` | 它们是**针对上一条消息的最终判断**，新输入一到即失效；直答尤其危险，发出去就是错误回复 |
| 新内容放进 Prompt 哪里 | 一律尾部（当前消息之后） | 前缀（人设 / 技能 / 词表 / 历史 / SOP）保持字节稳定，缓存全量命中 |
| 额外轮次怎么算 | 回 intent 与尾插共用同一个吸收预算（上限 3 次），不动 `MaxRounds` 策略口径 | 方向已由小脑二选一判定、默认追加，无需再分「回退次数」「注入次数」两套计数；统一上限把额外轮次封死在常数内 |
| 话题相关性怎么判 | 结构判据先筛同上下文，候选再交给 Agent 携带的小脑做语义多选判断 | 结构判据零成本且可解释；同上下文消息本就少，一次 Choice 多选（含「都不相关」哨兵）即可找出真正相关的，单条时退化为判断题；判定结果按「锚点 + 候选 + 稳定现场指纹」双向缓存，降级放行也按已判定缓存，避免每轮重判 |

## 二、架构思路

### 2.1 统一的新输入收集器

三处挂点共用同一个收集函数，只有后续动作不同。收集器本身是纯查询 + 纯过滤：

| 条件 | 取值 | 说明 |
|---|---|---|
| `to_id` | 当前 Agent | 队列门闩已保证同一 Agent 串行 |
| `from_id` / `from_role` | 与锚点一致 | **不限 User**：同一发送者的连续发言都可合并，Agent / System 同理 |
| `project_id` / `task_id` | 与锚点**相等（含 None）** | 业务上下文不同即串味；无项目 / 无任务同样是一种上下文，不能用「跳过空值」代替相等 |
| `status` | `Pending` | 只捞还没人领的 |
| `message_type` | 锚点仅 `Text`；候选同类型，或 `Mixed` 且正文带附件引用 | `ToolCall*` / `Confirm*` / 各类通知 / 旁路附件消息不合并；`Mixed` 的图片合并进锚点视觉输入（2026-11-28） |
| `thread_id` | **只作否决项** | 两端都有且不相等 → 拒绝；任一端缺失 → 不据此否决 |
| `created_at` | `> 游标`，**无时间上界** | 同上下文的补充消息隔多久都该参与判断；SQL 按时间正序每批拉 10 条、循环至取尽或预算触顶，批条数只决定单次判据大小 |
| 条数与字符 | 每批取 ≤ 10 条**循环分批**直至取尽或预算触顶；吸收不设条数上限，只受 token 预算约束 | 一批 10 条是小脑单次判定的大小，不是总量上限：20 条待处理就分 10+10 两批判完，不把后 10 条留到下一次唤醒；判定结果双向缓存，后续批次只追加、不重判头部；刷屏输入由 token 预算（模型上下文占比，跨批累加）兜住 |

**限制的是类型，不是角色**：合并的前提是同一发送者的连续「发言」，与发送者是不是人无关。真正不能混的是协议类型——`ToolCall*` 与工具一一配对、`Confirm*` 是请求/应答配对、系统通知各自独立、`AgentNotify` 明确声明无需回复；并进同一轮会让「要不要回、回给谁」失真。所以白名单按 `message_type` 收紧，而不是按 `from_role`。`Mixed` 是唯一例外：它仍是同一发送者的连续发言，只因正文带附件引用被归一为 `Mixed`，图片并入锚点视觉输入（见红线 8）。

**`thread_id` 只作否决项**：它是渠道侧「同一话题」的强证据，不是必要条件——用户完全可能不在话题内发下一条（私信、跨话题、客户端差异），缺失不能推出无关。规则：

- 两端都有且**不相等** → 否决合并（确定的「不同话题」信号）；
- 任一端缺失（`None`）→ 不据此否决，交由 `from_id` / 项目 / 任务 / 时间游标继续判断；
- 两端都 `None`（普通私信）→ 与缺失同义，不作否决。

游标由轮次状态持有：`after_created_at` + `after_id`（同秒并列时的稳定 tie-break）。每吸收一批就前移，保证同一条消息在一轮唤醒内只被吸收一次。

**空值语义（必须显式处理）**：`project_id` / `task_id` 的匹配是 `Option` 相等，不是「空值就不过滤」。锚点为 `None` 时只吸收同样为 `None` 的消息（SQL 用 `IS NULL`），锚点为 `Some(x)` 时要求 `= x`。要覆盖三种上下文：

| 上下文 | 锚点取值 | 匹配要求 |
|---|---|---|
| 默认对话（无项目、无任务） | 两者皆 `None` | 候选消息的 `project_id`、`task_id` 也必须皆 `None` |
| 有项目、未挂任务 | `project_id = Some(x)`、`task_id = None` | 要求同项目且 `task_id IS NULL` |
| 项目 + 任务 | 两者皆 `Some` | 要求两端都相等 |

若对空值直接跳过过滤条件，会把另一话题下挂在任务上的消息吸进本轮——这是「想支持空值」与「想少写条件」最容易混淆的地方。

**收集不等于预占**：收集器只读，不改消息状态。消息的 `Processed` 由唤醒收尾统一提交（见 §2.4），这样中途失败也不会丢。

### 2.2 三处挂点

```
intent ──done────▶ poll_front ──new_input──▶ intent        (回边，与尾插共用吸收预算 ≤3 次)
                        └──none──▶ retrieve ──▶ prepare ──▶ deliberate
                                                        │
                                                        ├─continue──▶ poll_round ──continue──▶ think
                                                        │                  └─new_input──▶ (尾插后继续)
                                                        └─final/exit──▶ poll_exit ──new_input──▶ intent 或再思考一轮
                                                                           └──none──▶ consolidate

intent ──direct/clarify──▶ poll_exit ──new_input──▶ intent
                              └──none──▶ consolidate
```

| 挂点 | 时机 | 有新输入 | 无新输入 | 额外成本 |
|---|---|---|---|---|
| `poll_front` | 意图识别完成、检索未开始 | `reinterpret` → `set_revision` + 回 `intent`（清空 `Knowledge`/`Prompt`/`Artifacts`）；`supplement` → 尾插后继续 | 直接进 `retrieve` | 一次 DB 查询 + 一次二选一小脑判断（有小脑时） |
| `poll_round` | 每轮思考边界 | 尾插一条带序号的用户消息，继续思考 | 什么都不做 | 一次 DB 查询 |
| `poll_exit` | `final` / `direct` / `clarify` 落地前 | `reinterpret` → 回 `intent`（**不论是否已检索**）；`supplement` → 尾插转向（`rethink` 回 `prepare`） | 正常收尾 | 一次 DB 查询 + 一次二选一小脑判断（有小脑时） |

> `poll_front` / `poll_exit` 的「回 intent 还是追加」不再由位置（`retrieved` / `rewinds`）决定，而是同一份二选一判据 [`inbox_intent_shift`](../../../../../src/service/domain/runtime/inbox_intent_shift.rs) 的输出。判据输入复用 `InboxJudgeContext`（已含【输入理解结果】即当前意图）+ 本批新消息正文；Brain 优先 `Agent.brain`，隐式启用（有小脑即判断）。两个方向共用 `InboxState.absorptions` 一个计数与 `MAX_ABSORPTIONS_PER_WAKE = 3` 一个上限，各记一次、同池封顶；取不到判断（无小脑 / 降级 / 无答案）一律追加。

**`poll_exit` 不可省略**：`direct_answer` 与 `need_clarification` 是图上的两条短路边（`intent --direct/clarify--> consolidate`）。若在出口不检查，用户补一句「等一下，改成 X」时，Agent 会把已经作废的直答或已经不需要的澄清问题发出去。这是本设计最优先要解决的错误形态。

**`poll_round` 的粒度**：只在轮边界注入。工具批量执行中途不插队——那需要在 dispatch 节点内部做检查，v1 不做；模型在下一轮看到新输入即可重新规划。

### 2.3 意图增量修订

修订输入 = 上次完整结论 + 按时间正序的新增输入（带序号）。修订输出 = **完整的同一个结构体**，下游（`retrieve` / `prepare` / Prompt 的【输入理解结果】区块）零改动。

写进意图 SOP 的合并协议：

1. 新输入**回答**了 `need_clarification` 中的问题时，清空已解决的条目；
2. 新输入**改变目标 / 对象 / 约束**时，重写 `summary` 与 `key_terms`，并**清空 `direct_answer`**；
3. 新输入只是补充细节时，合并进 `key_terms` / `resolutions`，其余保持不变；
4. 无法判断是否改变意图时，保守地清空 `direct_answer`，让流程回到完整链路；
5. 始终输出完整结构体，不输出增量补丁。

**Prompt 位置**：System 段（人设 + 技能 + 意图 SOP）与首次意图识别完全一致，公共上下文段（画像 / 项目 / 任务 / 历史）也一致；「上次结论 + 新增输入」拼在 `current_message` 之后的尾部区块。这样前缀全量命中，只多付尾部几十到几百 token。严禁为「让它看到旧结论」把结论插到历史之前。

**失败降级**：修订调用失败时保留旧结论，只把新输入尾插为转向消息。宁可让模型在下一轮自行处理，也不阻塞、不丢输入。

**下游失效**：回退必须同时清空 `ContextKey::Knowledge` 与 `ContextKey::Prompt`（`retrieve` 重跑后覆盖，`prepare` 重新装配），但**不得重置 `Budget`**——已消耗的轮次、token、墙钟继续累计。

### 2.4 状态与收尾

吸收清单需要从轮级执行回传到消费者：

```
RoundState.injected_ids
  → ThinkLoopResult.injected_ids
  → awaken 返回值
  → consumer::handle_agent_message
```

- **成功**：批量把 `injected_ids` 置 `Processed`。它们的队列事件后续出队时命中幂等守卫，返回 Ok → 框架 ack → 生产者再写一次 `Processed`，幂等。
- **失败**：不改状态。兄弟消息仍是 `Pending`，本次唤醒结束后各自作为独立唤醒处理。
- **不需要新状态、不需要迁移**：`Processed` 的既有语义（已消费）足够表达「已被本轮吸收」。

### 2.5 预算与护栏

| 护栏 | 建议值 | 作用 |
|---|---|---|
| `MAX_ABSORPTIONS_PER_WAKE` | 3 | 回 intent 与尾插共用的吸收次数上限，防止用户持续输入导致永不收敛 |
| token 预算（`INBOX_BATCH_BP`） | 上下文 5%（@200K ⇒ 10_000 token；2026-11-28 由 1% 上调） | 封顶**整个唤醒**的吸收量、跨批累加（不在每批重置）；不另设条数硬上限，判定相关的消息全收 |
| 剩余轮次检查 | `available_rounds >= 1` | 只剩最后一轮时不再注入，避免「听完就没轮次回答」 |

**预算总账契约**（2026-11-28 审核后定稿）：预算在 `build_deps_with_backend` 按模型上下文容量派生**一次**，存进唤醒级 `InboxState.remaining_inbox_tokens`（`None` = 无上限）。`collect` 只接受一个「剩余预算」入参、返回「本次实际用量 `used_tokens`」；三个挂点（`poll_front` / `poll_round` / `poll_exit`）各自读当前剩余值传入，完成后在外部做 `剩余 -= 实际用量` 扣减（`InboxState::spend_inbox_tokens`）。返回「实际用量」而非「剩余量」，让预算为零 / 无预算上限等情形仍有明确语义。批次内的退出判据用「是否有候选被预算挡下」，不依赖 `used >= budget`（后者在 token 数不整除预算时永不成立，会空耗小脑刷完整库）。

图引擎的**激活预算必须同步调整**，否则不是降级而是 `runner` 报错（「无合法候选」）：

| 节点 | 现值 | 新值 |
|---|---|---|
| `intent` | 1（默认） | `1 + MAX_ABSORPTIONS_PER_WAKE` |
| `retrieve` | `MAX_DISCOVERY_ROUNDS + 1` | `MAX_DISCOVERY_ROUNDS + 1 + MAX_ABSORPTIONS_PER_WAKE` |
| `deliberate` | `MAX_LOOP_ROUNDS` | `MAX_LOOP_ROUNDS + MAX_ABSORPTIONS_PER_WAKE` |
| `prepare` / `compact` | 各自回环预算 | 同步叠加吸收轮次 |
| `poll_front` / `poll_exit` | — | `1 + MAX_ABSORPTIONS_PER_WAKE` |
| `poll_round` | — | 随 `deliberate` 预算 |

`MaxRounds` / `TokenBudget` / `Timeout` / `NoProgress` 策略口径**保持不变**：每次吸收消耗的轮次照常计入，由统一次数上限把最坏情况封在常数内。

## 三、涉及文件清单

| 文件 | 角色 | 改动 |
|---|---|---|
| [legacy_awaken/blueprint.rs](src/service/domain/runtime/legacy_awaken/blueprint.rs#L64-L121) | 唤醒全图拓扑 | 新增 `poll_front` / `poll_exit` 节点与回边；调整各节点激活预算 |
| [legacy_awaken/intent_node.rs](src/service/domain/runtime/legacy_awaken/intent_node.rs#L53-L64) | 意图节点契约 | 支持重入；`writes` 补齐 `Knowledge` 失效语义 |
| [legacy_awaken/nodes.rs](src/service/domain/runtime/legacy_awaken/nodes.rs) | 装配节点 | `prepare` 读取修订后的意图；注入清单透传 |
| [think_round/blueprint.rs](src/service/domain/runtime/think_round/blueprint.rs) | 轮级环图 | 插入 `poll_round` 节点与边 |
| [think_round/state.rs](src/service/domain/runtime/think_round/state.rs) | 轮次状态 | 新增吸收游标与 `injected_ids` |
| [think_round/deps.rs](src/service/domain/runtime/think_round/deps.rs) | 轮次依赖 | 注入场景开关与收集器所需依赖 |
| [think_round/nodes.rs](src/service/domain/runtime/think_round/nodes.rs) | 轮内节点 | `poll_round` 实现；`evaluate` 出口检查 |
| [runtime/intent_analyze.rs](src/service/domain/runtime/intent_analyze.rs) | 意图分析 | 支持「带上次结论 + 新增输入」的修订入参 |
| [runtime/types.rs](src/service/domain/runtime/types.rs#L543-L580) | `IntentAnalysis` 契约 | 不新增字段；修订语义靠 SOP 与入参表达 |
| [dal/agent/builder/default.rs](src/service/dal/agent/builder/default.rs) | Prompt 装配 | 意图修订区块拼在尾部，保持前缀稳定 |
| [dao/message/mod.rs](src/service/dao/message/mod.rs#L169) | 消息查询/更新 | 复用 `MessageQuery`；必要时补批量置状态 |
| [consumer/message.rs](src/consumer/message.rs) | 唤醒入口与收尾 | 回传吸收清单，成功批量置 `Processed` |
| [pkg/think_path/types.rs](src/pkg/think_path/types.rs) | 图契约 | 若需跨节点共享吸收清单，补一个 `ContextKey` |
| [pages/message/chat.rs](frontend/src/pages/message/chat.rs) | 对话输入区 | 忙碌不拦截发送；输入区固定只放「发送」，状态提示由 placeholder 与置底气泡承担 |
| [cancel_thinking_button.rs](frontend/src/components/chat/cancel_thinking_button.rs) | 停止思考按钮 | 只保留气泡内 `Inline` 一个落点；输入区的 `Block` 变体已删除，避免同一动作出现两个入口 |

## 四、边界与行为红线

1. **禁止插 index 0**：任何「有新输入」「路由提示」都只能尾插。插头会让 System 前缀整体后移，跨唤醒缓存归零。
2. **禁止越过 `deliberate` 回退意图**：已在执行阶段时不回退，只做尾插转向。
3. **禁止预占消息状态**：收集阶段只读，`Processed` 只在唤醒成功后提交。
4. **禁止跨来源合并**：`from_id` / `from_role` / `project_id` / `task_id` 任一不同即不合并；`None` 也是一种取值，必须同为 `None`。`thread_id` 仅作否决项——两端都有且不同才拒绝，缺失不算冲突。不同话题的消息留在队列，等本次唤醒结束后独立处理。
5. **禁止复用终态字段**：`direct_answer` / `need_clarification` / `retrieved_context` 不得原样带进修订结论。
6. **禁止重置预算**：回退不得清空 `Budget`，轮次/token/墙钟继续累计。
7. **场景开关**：仅 `awaken` 启用三处挂点；`intent` 单轮与 `settle` / `summary` 场景关闭，避免无意义查库与被无关消息打扰。
8. **多模态合并到锚点，不另起多模态消息**（2026-11-28 决策）：
   - 候选 `Mixed` + 正文附件引用与 `Text` 锚点兼容（`file_type` 仍必须为 `None`，旁路附件消息不合并）；`InjectedMessage.attachment_ids` 只传引用，不传 base64。
   - 锚点与本批吸收消息的附件由 `resolve_message_resources` 统一解析，共用单条 4 张、单图 ≤10MB、`image/*` 白名单；超限留文本占位行。`InboxState.absorbed_attachment_ids` 跨 `pending_turn` / `revision` 文本消费仍保留，Phase 1 意图修订与 Phase 2 三条装配路径（续跑追加 / seed 续接 / 重建）按 base64 去重合并，不产生第二条多模态用户消息。
   - 视觉能力门禁：provider 未显式配 `supports_vision=true` 时，`NoVisionGuardDao` 在 Chat Completions / Responses 两种协议发送前把 `UserMultimodal` 降级为文本占位，避免 400 或静默忽略。
9. **撤回竞态可接受但不扩大**：收集前已撤回的消息（`find_by_id` 过滤 `Recalled`）不吸收；吸收后被撤回的，本轮仍会连同其内容答复，不额外补偿。

## 五、观测与验收

### 5.1 观测字段

| 字段 | 位置 | 用途 |
|---|---|---|
| `injected_count` | 唤醒统计事件 | 每次唤醒实际吸收几条 |
| `injected_chars` | 同上 | 吸收量分布，调上限 |
| `intent_revisions` | 意图子 trace 元数据 | 回退次数 |
| `rescued_direct_answer` | 意图修订结果 | 作废直答被救回的次数 |
| `batch_started_at` | trace 元数据 | 推断注入发生在第几轮 |

### 5.2 验收口径

| 场景 | 期望 |
|---|---|
| 无新输入 | Prompt 与改造前逐字节一致，查库不改变任何状态 |
| 默认对话（无项目、无任务）连发 2 条 | 正常吸收，空值不跳过过滤、不误吸其它话题 |
| 有项目但未挂任务时连发 | 正常吸收，`task_id IS NULL` 语义正确 |
| Agent 忙碌时前端发送 | 消息正常入队；置底「正在思考中…」与 placeholder 提示保留 |
| 首轮前到达 2 条同源消息 | 一次唤醒吸收两条，只回复一次 |
| 直答判定后、发送前到达新约束 | 不发送旧直答，回意图修订 |
| 已执行工具后到达新输入 | 不回退，尾插后在下一轮生效 |
| 修订调用失败 | 旧意图保留 + 新输入尾插，流程继续 |
| 唤醒失败 | 兄弟消息仍为 `Pending`，后续各自唤醒 |
| 吸收次数达到上限（回 intent 与尾插共用） | 图以正常路径收尾，不报「无合法候选」 |

### 5.3 单测清单

- 收集器判据（来源 / 项目 / 任务 / 类型 / 游标 / 上限，含三种空值上下文）；
- 同源不限角色：Agent / System 连续文本可合并；
- 跨类型不合并（`Text + AgentNotify`、`ToolCallResult`）；
- `thread_id` 两端冲突不合并、缺失不否决；
- 意图修订降级（模型失败、空结构、清空直答）；
- `poll_round` 无输入时零写入；
- 收尾幂等（重复 ack 不产生第二次回复）；
- 图预算（吸收 3 次后仍能走到 `consolidate`）；
- 前端忙碌态发送（发送键可用、「停止」仍在、旧拦截提示不残留）。

## 六、扩展模式

1. **小脑语义判据（已实现）**：结构判据（同源 / 同项目 / 同任务 / 同线程 / 同类型）先筛出同上下文候选，再交给 `domain/runtime/inbox_relevance` 做语义判断——全部候选作为一次 `Choice` 多选选项（附「都不相关」哨兵），权重过阈值且高于哨兵的才合并；单条候选退化为判断题。判据入口 `rank_related` 从参数注入 `BrainDal`（生产包装 `rank_related_with_default_dao` 注入默认单例，测试注入 mock 覆盖全部降级路径）；Brain 由节点从 Agent 携带的实例优先取得（Agent 绑定自己的小脑时自动跟随），不另开「默认小脑」旁路。权重缺失与哨兵缺失同为答案不完整，统一 fail-open 放行。判定结果按 `(anchor_id, message_id, 现场语义指纹)` 双向缓存（`runtime.inbox_relevance`）；指纹只取任务 / 项目 / 输入理解三类稳定摘要，**不取**每轮都变的「消息条数 / 最近动作」，否则缓存永不命中，而任务上下文真变了会自然生成新键。小脑未装配 / 配额耗尽 / 失败 / 无答案时 fail-open，整批按「已判定为相关」写缓存并注入。已判定 ID 随唤醒级 `InboxState.judged_ids` 下推给下一批 SQL 排除；拉取注入时**不立刻改消息状态**，注入成功的消息在唤醒成功后才由消费侧批量置 `Processed`（取消/失败保持 Pending），判定缓存与排除列表负责唤醒过程中的防重，状态提交负责跨唤醒防重。不设时间窗：同上下文补充消息隔多久都参与判断。**循环分批**：每次 `collect` 以 `INBOX_SCAN_LIMIT = 10` 为一批，小脑判定后把相关消息追加进同一份结果，再用本轮新判定的 ID 扩展 `exclude_ids` 拉下一批，直到 SQL 取不满一批（DB 已无更多 `Pending` 候选）或 token 预算触顶；预算跨批累加（同一份「思考空间占比」，不在每批重置），因此 20+ 条待处理消息能在一次唤醒内全部判完、而不是只吃前 10 条。每批判定结果都进双向缓存，后续批次只追加新候选、复用已判定的缓存，不重复调用小脑。收集器与挂点拓扑不变——三处挂点（`poll_front` / `poll_exit` / `poll_round`）共用这一份分批逻辑与同一套过滤条件。
2. **前置批量合并统一**：唤醒开始前的批处理与 `poll_front` 共用同一收集器，区别只是「合并进 `current_message`」还是「作为修订输入」。两者不要各写一套判据。
3. **群聊多用户**：当前限制同 `from_id`。放开时需要在收集结果里携带来源人，Prompt 中按发送者分段标注。
4. **工具执行中途注入**：在 dispatch 节点内按工具边界检查新输入，可进一步缩短转向延迟；与轮边界语义相同，只是粒度更细。
