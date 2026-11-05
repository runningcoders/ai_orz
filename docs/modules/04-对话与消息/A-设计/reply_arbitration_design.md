---
kind: 设计决策
module: 04-对话与消息
name: reply_arbitration_design
title: 回复仲裁：是否回复由 Agent 自决，Final 是唯一权威
status: 现行
scope:
- src/consumer/message.rs
- src/consumer/message_route_policy.rs
- src/consumer/message_route_policy_tests.rs
- src/models/prompt_builder.rs
- src/service/dal/agent/builder/default.rs
- src/service/dal/agent/builder/flat.rs
- src/service/domain/runtime/types.rs
- src/service/domain/runtime/awakening.rs
- src/service/domain/runtime/legacy_awaken/nodes.rs
related:
- 04-对话与消息/A-设计/collaboration_pressure_design.md
- 02-Agent运行时/A-设计/thinking_task_policy_engine_design.md
---

# 回复仲裁：是否回复由 Agent 自决，Final 是唯一权威

> 2026-11-05 定稿。补齐 [协作压力组件](collaboration_pressure_design.md) 之外的另一个循环源头：
> 压力解决「回太多次」，本文解决「**这一轮到底该不该回**」。

## 一、问题：框架替 Agent 假设了「一问一答」

`handle_agent_message` 在唤醒完成后把 `raw_output` 自动回发给来源方（见
`src/consumer/message.rs` 的「Framework 主动把 Final 文本转为对等回复」段）。
历史原因是 raw_output 曾被直接丢弃，Agent 被迫在 think loop 里额外调
`send_message` 才能交付，造成「必须走工具才算完成」的死循环。

自动回发保留了，但引入一个**框架级假设**：把「Agent 的输出」等同于「对来源方的答复」。
两个 Agent 各自被这种自动回发唤醒、又各自自动回发，就形成不需要任何一方同意的
一问一答闭环——直到撞上疲劳度硬墙。这不是模型想聊，是框架替它们约好了每轮必答。

### 1.1 被废弃的方案：反查「本轮是否已经发过消息」

第一版尝试用**客观交付事实**兜底：唤醒后查一次消息表

```sql
SELECT 1 FROM messages
 WHERE reply_to_id = <入口消息 id> AND from_id = <本 Agent> AND to_id = <来源方>
 LIMIT 1
```

命中则 `Discard`（判定规则 `agent_self_replied`）。逻辑上能挡住「已经交付过、
Final 只是内部收尾」的场景，但语义与成本都不成立：

| 问题 | 说明 |
|---|---|
| 语义错位 | 「发过消息」≠「已答复」：先发进度同步、Final 才是完整答案时，用户只看到半截进展。只能再加「仅 Agent 来源生效」的守卫，规则越叠越绕 |
| 覆盖不全 | 发给了别人 / 别的会话、出站投递失败（异步静默丢）、任务派发不写 `reply_to_id`，全都查不到，仍然会重复回 |
| 判定权错位 | Agent 已经可以显式说「这轮不用回」（`NO_REPLY`），框架再替它推断一次，两者冲突时无法解释、也无法观测 |
| 成本 | 每次跨 Agent 唤醒一次额外索引查询，失败还要选边（宁可多发则等于没挡） |

结论：**副作用推断不是仲裁**。仲裁的依据应当是 Agent 在思考中给出的显式决定。

## 二、决策：把「是否回复」交还思考过程

```
思考（模型看到义务 + 压力证据） → 自决 → Final
                                        ├─ 答复正文 → 框架回发给来源方
                                        └─ NO_REPLY → 框架丢弃，不再唤醒对端
```

两条硬约束：

1. **统一出口**：要回复就在思考完成后由 Final 交付，正文即答复全文；不要在过程中
   用发送工具重复投递同一内容（工具仍保留给「接收方不在本条对话里」的场景）。
2. **有权不回**：判断无需回复时，Final 恰好输出哨兵，框架据此丢弃。

### 2.1 协议枚举（代码与提示词同一份定义）

`src/consumer/message_route_policy.rs`：

```rust
/// 本轮消息对「是否需要回复」的要求（只看入口消息语义，不参与路由判定）
pub enum ReplyObligation {
    Expected,   // 对端在等答复（跨 Agent 对等协作 / 任务交接）
    NotifyOnly, // 知会消息：发送方已声明无需回复（MessageType::AgentNotify）
}

/// Agent 自决结果（Final 的唯一权威形态）
pub enum ReplyDecision {
    Reply,   // 正文投递给来源方
    NoReply, // 哨兵，丢弃
}
```

- `ReplyObligation::of(from_role, from_id, agent_id, message_type)` 只对 **Agent 来源且
  非自触发** 的消息返回 `Some`；用户 / 系统来源返回 `None`（投递方向由来源规则决定，
  用户对话 Prompt 与路由零变化）。
- `classify_reply_decision(raw_output)` 是哨兵判定的唯一入口（内部复用
  `is_no_reply_sentinel` 的封闭词表 + 归一化全等），路由与日志共用，避免两处口径漂移。

| Final 形态 | 语义 | 路由 |
|---|---|---|
| 任意正文 | 回复 | `Peer`（用户 → `send_to_user`；Agent → `send_to_agent`） |
| `NO_REPLY` 及封闭词表变体 | 不回复 | `Discard { reason: 哨兵 }` |
| 空字符串 | 无交付（Cancel / 纯工具任务） | 不进入路由 |

## 三、提示词契约（`ReplyObligation::prompt_lines`）

**Expected**（对等协作）：

```
【回复仲裁】本条来自另一个 Agent，在等你答复；是否回复由你决定。
- 回复：思考完成后由 Final 统一答复，正文即答复全文——不要再用发送消息工具重复投递同一份内容。
- 不回复：Final 恰好只输出 NO_REPLY（独占全文，不加引号、标点或解释），框架不会回发。
- 判据：有结论/交付物、需要对方决策或补充信息 → 回复；只是确认收到、复述上一轮、已完成且无补充、后续与对方无关 → NO_REPLY。
```

**NotifyOnly**（知会）：

```
【回复仲裁】本条是知会消息（发送方已声明无需回复），框架不会回发你的 Final。
- 默认不回复：Final 恰好只输出 NO_REPLY 收尾。
- 确需回应：用发送消息工具主动发给来源方，不要依赖 Final。
```

「判据」那行就是当前手写策略：把**什么时候该回 / 不该回**写成可执行的判断，
再把疲劳度等量化证据交给 `collaboration_pressure` 的区块。后续接小脑时替换判据段，
提示词位置与协议不变。

### 3.1 注入位置与缓存

- 两组行统一经 `ThinkingOptions.turn_guidance_lines` → `PromptBuilder::set_turn_guidance_lines`
  渲染为 **User 消息末尾的 append-only 独立区块**，顺序固定：
  回复仲裁（跨轮稳定）→ 协作压力（每轮变化）→ 收尾指令。
- 禁止并入【资源上下文】、禁止插到消息链/历史之前、禁止进 System：
  prefix cache 从请求首字节逐字节匹配，易变内容前移会作废其后全部命中。
- 收尾指令在有指引时由 `请回复：` 变为
  `请输出本轮结果（无需回复时只输出 NO_REPLY）：`——老文案本身就在暗示「必须回」，
  与仲裁区块的出口自相矛盾。**无指引时逐字节保持原样**，用户 ↔ Agent 主流对话零变化。
- Phase 2 执行段（`build_execution_segment`）会把同一份指引再挂一次：执行段才是真正的
  决策点，模型不该为了看清「能不能不回」回头翻 Phase 1。两处同源（同一份
  `ThinkingOptions`），数值不会出现两个口径。

## 四、判定顺序（三段）

```
① 静态规则表（零查库，声明顺序即优先级）
   哨兵 → 用户来源 Peer → 系统来源 → 自触发 → 知会
② 协作压力档位（awaken 前已观测）
   Exhausted → CloseChainAsNotify（Final 作知会送达 + 终止链），其余 → Peer
③ 未来：小脑（读同一份证据）
```

哨兵仍在最前：保留字任何来源都不投递（任务派发经用户身份中继后 `from_role=User`，
若让来源规则先命中会把哨兵原文发给用户，2026-10-16 实证泄漏 29 条）。

**②的兜底形态是「收口」而不是「丢弃」**：命硬墙时 Final 仍作为 `AgentNotify` 送达
来源方，靠知会语义终止链条，对端另有一条「链已终止 + 疲劳度」的提示词。细节见
[collaboration_pressure_design.md](<collaboration_pressure_design.md>) §5.1（含「知会必须是
回溯边界」这条不变量）。

## 五、小脑接入点（预留，本文不实现）

接入位置固定在「静态规则未命中 → 压力未触顶」之后、默认 `Peer` 之前：

```rust
fn resolve_auto_reply_route(...) -> AutoReplyRoute {
    if let Some(route) = judge_static_reply_route(evidence) { return route; }
    if pressure.exhausted() { return CloseChainAsNotify; }
    // ← 小脑接在这里：输入同一份 evidence + pressure 快照 + Final 形态
    //    输出 Peer / Discard{reason}，reason 必写日志
    Peer
}
```

硬约束：

- 小脑**只做仲裁，不做投递**；投递仍由 `AutoReplyRoute` 分支执行。
- 输入必须是 `RouteInput` + `ChainPressure` 快照（可序列化），禁止让小脑自己查库——
  否则证据口径会分裂成三套（手写 / 小脑 / 路由）。
- 小脑判定 `Discard` 必须落 `log_info`（`auto-reply decision: ...`），否则线上无法区分
  「判不回」与「消息丢了」。
- 超时 / 失败一律降级为 `Peer`（保交付），不得降级为 `Discard`（吞答复）。

## 六、已知边界

| 边界 | 现状 | 影响 |
|---|---|---|
| 模型坚持每轮都回 | 提示词 + 疲劳度施压只能降低概率 | 硬墙 `CloseChainAsNotify` 兜底（最后一轮作知会送达、链必然终止）；后续小脑接管 |
| 哨兵后追加解释（`NO_REPLY: 不用回`） | 归一化全等不命中 ⇒ 当正文投递 | 提示词已要求「独占全文」；确定性更强的前缀规则暂不做（会把「正文恰好以 NO_REPLY 开头」误伤，收益低于风险） |
| 知会消息 Final 被丢弃 | 要回必须显式调发送工具 | 提示词已给出口；若模型仍只输出正文，属于可观测的协议违约（`auto-reply decision` 日志） |
| 过程中已用工具发过消息，Final 又输出正文 | 会重复投递 | 由 Agent 自决（提示词要求此时 Final 输出 NO_REPLY）；框架不再反查——见 §1.1 |
| 用户来源 | 不进入仲裁 | Final 必须回到用户；用户对话 Prompt 与路由行为零变化 |

## 七、护栏

- `reply_obligation_follows_message_semantics`：只有 Agent 对等消息产生 `Expected`，
  知会为 `NotifyOnly`，用户 / 系统 / 自触发为 `None`。
- `reply_protocol_prompt_states_both_forms`：提示词必须同时给出「正文即答复」与
  「NO_REPLY 独占全文」，并验证路由真的兑现哨兵出口。
- `notify_obligation_prompt_points_to_explicit_send`：知会消息必须给出「要回走工具」的通道。
- `reply_decision_only_fails_closed_on_sentinel`：只有封闭词表判 `NoReply`，带解释的写法按正文处理。
- `guidance_block_is_append_only_and_preserves_stable_prefix`：指引区块 append-only，
  收尾指令必须暴露 NO_REPLY 出口。
- `reply_tail_stays_unchanged_without_guidance`：无指引时收尾指令逐字节不变（主流路径零变化）。
- `execution_segment_carries_turn_guidance_and_reply_tail`：Phase 2 决策点必须看到同一份指引。
- `forwards_turn_guidance_lines_into_flat_prompt`：A2A / Codex 的 flat builder 不得吞掉指引。
