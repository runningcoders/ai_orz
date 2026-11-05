---
kind: 设计决策
module: 04-对话与消息
name: collaboration_pressure_design
title: Agent 间协作压力（链深度 + 信息疲劳度）可复用组件设计
status: 现行
scope:
- common/config/ai_orz.toml
- common/src/config.rs
- src/consumer/message.rs
- src/consumer/message_route_policy.rs
- src/consumer/message_route_policy_tests.rs
- src/models/prompt_builder.rs
- src/service/dal/agent/builder/default.rs
- src/service/dal/agent/builder/flat.rs
- src/pkg/collaboration_pressure/
- src/pkg/collaboration_pressure/mod.rs
- src/pkg/collaboration_pressure/policy.rs
- src/pkg/mod.rs
- src/pkg/policy/
- src/service/domain/runtime/awakening.rs
- src/service/domain/runtime/legacy_awaken/nodes.rs
- src/service/domain/runtime/types.rs
related:
- 02-Agent运行时/A-设计/thinking_task_policy_engine_design.md
- 02-Agent运行时/A-设计/two_stage_append_only_prompt_design.md
---

# Agent 间协作压力组件设计

> 2026-11-04 定稿。取代 2026-09-18 的「链深度 ≥ 5 机械兜底」决策；旧决策的理由保留在 C-知识《消息领域》的更新摘要里，不抹掉。

---

## 一、问题：软停止与硬墙之间缺一段过渡带

旧机制只有两层，中间是断崖：

| 层 | 实现 | 特征 |
|----|------|------|
| 模型侧软停止 | `NO_REPLY` 哨兵（全等词表）+ `AgentNotify` 知会 | 二值、单条、靠模型自觉 |
| 硬墙 | `MAX_AGENT_REPLY_CHAIN = 5`，`ThresholdPolicy` 固化在 `OnceLock` | 编译期常量不可配；命中即 `EscalateToOwner`，`Ok(())` ack 终结，无过渡、无续接入口 |

实际症状：两个 Agent 正常协作到 5 轮被确定性掐断，用户侧只收到一条「已自动中止协作」告警。防浪费的目的达到了，但代价是协作被误伤且没有任何缓和余地。

**同时存在的另一个事实**：链遇到 User/System 来源消息就断（用户介入是自然终止点），所以「新输入重置」的骨架本来就有，缺的是链**内部**出现新信息时的回血，以及让用户把协作续回去的入口。

---

## 二、设计目标

1. **慢性红线**：重复信息量的往返连续累积疲劳度，压力带内先施压，硬阈值才升级；不是一次性硬墙。
2. **新鲜感可续**：新信息注入能降低疲劳度并重置连续窗口，使协作合法超过深度上限。
3. **信号随回复带上**：每次唤醒前把链深度 / 疲劳度 / 档位 / 配套指引注入 prompt，让**当前正在思考的 Agent** 主动收敛。
4. **可复用**：抽成 `pkg/` 下零 I/O 组件，后续思考链路（工具循环、多 Agent 协商、任务收敛）可用同一套口径。
5. **提示词与行为动作同源**：因为存在回血机制，档位、动作、提示词、回血话术必须在一处声明，避免「改了动作忘了改提示」。
6. **可记录可打印**：判定走策略引擎，命中 id / 条件描述 / 事实快照一行可打、可序列化。

---

## 三、组件边界（为什么放 `pkg/collaboration_pressure/`）

照 `pkg/policy/` 的先例：纯框架层、独立组件、不感知业务语义。

- **零 I/O**：不读 DB、不发消息、不写日志、不读全局配置（除 `PressureConfig::from_system()` 这个显式便利构造函数）。
- **零上层依赖**：不认识 `Message` / `MessagePo`。调用方把领域对象翻译成 `ChainHop { from_agent, novelty }` 两个语义标志；「什么算新信息、回多少血、是否重置窗口」留在组件内。
- **判定复用策略引擎**：`STAGE_DEFS` 每条实现 `Policy`，由 `PolicyBuilder::or()` 组装，声明顺序即优先级；命中 id 由 `Metrics` 事实桶驱动，天然可记录、可打印。

文件分工：`mod.rs` 类型与累加器 / `policy.rs` 档位规则表 / `tests.rs` 护栏。

---

## 四、核心模型：两个深度 + 一个回血枚举

| 量 | 语义 | 是否参与硬墙判定 |
|----|------|------------------|
| `depth` | 绝对链深，单调递增 | ❌ 仅展示 / 日志 |
| `window_depth` | **自最近一次有效回血以来**的连续无新信息跳数 | ✅ |
| `fatigue` | 信息疲劳度 0..`max_fatigue`，每跳 +`per_hop_fatigue` | ✅ |

**这是本设计最关键的一条**：只让回血降 `fatigue`、不重置 `window_depth` 的话，绝对深度仍会单调触顶，「有新鲜感就允许继续聊」就是空话。因此 `Novelty::ToolResult` / `NewParticipant` / `ExternalInput` 都会把窗口清零；`DistinctContent` 是弱新信息，只微量降疲劳度、不重置窗口；`ExternalInput` 额外整链重置。

新信息刻意用**结构化锚点**（工具/任务结果、附件、非 Text 消息类型、第三个及以后的参与方），不做内容相似度 / embedding：向量是落库后的检索能力、依赖模型且可降级跳过，引入它会带来延迟、成本与不确定性（YAGNI）。

⚠️ **能力边界**：`Novelty::DistinctContent`（弱新信息，只扣 8 点、不重置窗口）目前是**组件预留枚举**，消息链路的 `classify_chain_novelty` 只产出 `None` / `ToolResult` / `NewParticipant`。因此当前「疲劳度」是**结构化代理量**（每跳 +12、遇结构化新信息回血），不是按正文信息量逐条打分；纯文本往返即使内容完全不同也按同一速率累积。真要接内容判重属于二期，须同步重估软/硬阈值。

默认口径（每跳 12 点、软 36、硬 100、窗口 8）：纯文本往返第 3 跳进入压力带，第 8 跳撞窗口硬墙；期间每出现一次工具结果就回血 40 并清零窗口，协作可合法超过 8 跳。护栏测试 `fresh_input_lets_chain_outlive_hard_window` 钉住这一点。

---

## 五、档位规则表：判据 / 动作 / 提示词 / 回血话术同一行

`policy.rs` 的 `STAGE_DEFS` 按优先级声明，`PolicyGroup(Or)` 的 `evaluate` 按声明序返回命中列表，取 `first()` 即最高档位：

| 优先级 | 策略 id | 判据 | 档位 | 动作 |
|:---:|---|---|---|---|
| 1 | `pressure_fatigue_exhausted` | `fatigue >= hard_fatigue` | Exhausted | Escalate |
| 2 | `pressure_window_exhausted` | `window_depth >= hard_window_depth` | Exhausted | Escalate |
| 3 | `pressure_nudge` | `fatigue >= soft_fatigue` | Pressured | Nudge |
| 4 | `pressure_recovered` | `recovery_count > 0` | Recovered | Continue |
| — | `pressure_fresh`（fallback） | 未命中 | Fresh | Continue |

**每一行同时写着 `prompt_hint` 与 `recovery_hint`**：找「Escalate 会让模型看到什么」「回血后提示语怎么变」只需看那一行。`every_hit_binds_action_and_prompt_to_the_same_rule` 是这条约束的护栏测试。

`Nudge` 不改变路由（仍 Peer），它的效力来自唤醒前注入的施压提示词；只有 `Escalate` 才改变路由：映射为 `CloseChainAsNotify`，把 Final 作为知会送达对方并终止链条（见 §5.1）。

⚠️ **阈值不写进规则对象**：`FatigueAtLeast` / `WindowDepthAtLeast` 按 key 从 `Metrics` 双键读取，规则本身无状态，因此规则组可以安全 `OnceLock` 缓存而配置改动立即生效。旧 `chain_policy()` 把 `MAX_AGENT_REPLY_CHAIN` 固化进 `OnceLock<ThresholdPolicy>` 是明确反例。系统级配置在 `[agent].collab_*`（`common/config/ai_orz.toml`），`PressureConfig::sanitized()` 对非法配置回退默认值，避免一条坏配置把所有协作钉死在升级区；`max_fatigue` 自动抬到 `>= hard_fatigue`，否则把硬阈值放宽到 100 以上会被当成非法配置、连带软阈值与窗口一起静默回退（`hard_fatigue_above_default_max_is_not_silently_rejected`）。

### 5.1 硬墙收口：终止链条，但不丢最后一轮信息

硬墙命中时**不再「直接停」**。旧形态（`EscalateToOwner`）只通知归属用户，对方永远等不到最后一轮结果——协作被切断，信息凭空消失；而「照常回发」又正是硬墙要拦的乒乓。收口是第三种形态：

| 维度 | 旧：直接停 | 新：知会收口 |
|---|---|---|
| 最后一次 Final | 丢弃（用户只收到一条告警说「没回发」） | 作为 `AgentNotify` 投给来源方，信息完整送达 |
| 链条怎么断 | 靠「没发出去」断 | 靠知会语义断：对端 `ReplyObligation::NotifyOnly` ⇒ 静态路由 Discard |
| 对端感知 | 只知道自己发了消息、永远等不到 | 提示词明确「本条是上游链最后一跳，链已终止」+ 疲劳度事实 |
| 想继续 | 无入口 | 对端自己主动 `send_message_to_agent` 开新消息（新链从第 1 跳重新计数） |
| 归属用户 | 「已暂停自动回发」告警 | 「已把最后一轮作为知会送达并终止该链」的知会（结果已给对端，不再承载内容） |

两个条件必须同时成立，缺一个就退回成「丢消息」或「继续乒乓」：

1. **发送侧**：`judge_chain_reply_route` 的 `Escalate` → `AutoReplyRoute::CloseChainAsNotify`，consumer 用 `MessageType::AgentNotify` 投递 Final（收口仍走 `Ok(())` ack 终结，不产生 nack 重投）；
2. **接收侧**：知会消息也做一次链观测（但**不施压**——它本来就不回），命中硬墙档位时追加 `PressureDecision::chain_closed_prompt_lines()`：疲劳度事实 + 「默认 NO_REPLY / 要推进就自己开新链」。

⚠️ **知会必须同时是回溯边界**（`MessageConsumer::is_chain_boundary`）。否则「对端主动开新链」发出的那一跳会顺着 `reply_to_id` 穿过那条知会，把旧链疲劳度整段继承过来 ⇒ 新链第一跳立刻再撞墙，「换个新链继续」当场变成空话。护栏：`notify_is_a_chain_boundary_so_reopened_chain_starts_fresh`。

代价与取舍：知会消息多一次回溯查库（≤ `MAX_CHAIN_TRAVERSAL` 跳，仅 Agent 来源消息）；换来的是收口机制闭环——信息传到与链必断同时成立。

---

## 六、接入链路

```
handle_agent_message（唤醒前）
  ├─ 预筛：ReplyObligation::of = Some（Agent 来源 && from != agent_id）
  │    · Expected（对端在等答复）：施压 + 硬墙裁决用
  │    · NotifyOnly（知会）：不施压，只判断「本条是不是上游触顶收口的那一跳」
  │        （与静态路由规则同源 ⇒ 用户对话主干零额外 DB 读）
  ├─ collect_chain_pressure：沿 reply_to_id 回溯收集 ChainHop（新 → 旧）
  │    · 非 Agent 来源 / 断链 / 父消息不存在 / 查询失败 → 停止（宁少勿错）
  │    · 父消息是知会 → 链边界，不继续回溯（见 §5.1）
  │    · MAX_CHAIN_TRAVERSAL = 32 硬上限，防异常长链放大 DB 读
  │    · observe_backward_chain：归一化为时间正序后再逐跳 observe（见 §6.2）
  ├─ log_info! 打印 decision.log_line()（档位 / 命中策略 / 条件 / 全部事实）
  └─ ThinkingOptions.with_turn_guidance_lines(回复仲裁行 + 压力行 / 收口行)
        ↓ fill_prompt_base / legacy_awaken 重建路径 / Phase2 执行段 → set_turn_guidance_lines
        ↓ DefaultPromptBuilder 渲染 append-only 独立区块
          （当前消息 / 尾插输入 / 技能提醒之后，收尾指令之前）
        ↓ 本轮 Agent 看到「要不要回」+ 链深、疲劳度、档位动作、本轮指引、回血规则
awaken → raw_output
  └─ resolve_auto_reply_route（纯函数，不再查库）
       静态段命中 → Peer / Discard
       静态未命中 → judge_chain_reply_route(&pressure)：Escalate → 知会收口，其余 Peer
```

归属用户的通知文案不再是「已暂停自动回发」，而是「已把最后一轮结果作为知会送达对方并终止该链」，并给出续接入口（补新信息 / 让相关 Agent 重新发起）。压力数据缺失属异常，按「宁少勿错」放行 Peer，不改成 Err / nack（会触发重投雪崩）。

### 6.1 提示词注入位置与 prefix cache 边界

**压力值每次唤醒都变**（链深 +1、疲劳度 +12、档位可能跳档），因此它属于最易变内容，只能 append-only 收尾：

- **落点**：`DefaultPromptBuilder` 把 `turn_guidance_lines` 渲染成独立区块（内容含回复仲裁行 + 【协作压力】行），位于 `current_message` / 尾插输入 / 技能提醒**之后**、收尾指令之前。其后只剩一句收尾语，插入点之前的所有字节与没有指引时逐字节一致。
- **三个禁止**：❌ 禁止前移到【消息链上下文】/【资源上下文】等稳定区块之前（provider prefix cache 从请求首字节逐字节匹配，易变内容前移会作废其后全部内容）；❌ 禁止并入【资源上下文】的 `- ` 列表项（语义错配，把行为指令降级成参考资料）；❌ 禁止进 System 或 tools（那是跨唤醒唯一可复用的前缀，本次完全未改动）。
- **两阶段 I1/I2**：`analyze_opts = options.clone()` 携带同一份压力行，`fill_prompt_base` 是唯一 base 装配点，Phase1 seed 与 Phase2 base 逐字节一致；`compact_context` 复用 `work_messages` 只 push 指令，压缩循环的前缀也不变。
- **跨唤醒的现实边界**：A↔B 协作交替唤醒的是**不同 Agent**，System 人设与技能不同，前缀天然无法跨唤醒复用；能复用的是同一 Agent 多次唤醒的 System + tools，本次已保持完全静态。把压力信号「加到消息列表前面」不会带来复用，只会把本来可命中的稳定前缀一起作废。
- **护栏**：`guidance_block_is_append_only_and_preserves_stable_prefix`（前缀逐字节不变 + 压力值变化也不变 + 不进 System）、`guidance_lines_keep_intent_prefix_identical`（带指引行时两阶段仍一致）、`forwards_turn_guidance_lines_into_flat_prompt`。
- ⚠️ **外部 Agent 覆盖**：`FlatPromptBuilder`（A2A / Codex）内部包 `DefaultPromptBuilder`，但只转发部分方法；必须显式转发 `set_turn_guidance_lines`，否则 trait 默认空实现会**静默吞掉整段指引**（压力信号 + 「可以回 NO_REPLY」的出口），只剩 consumer 层硬墙兜底、失去主动收敛能力。（其 `prompt_template` 若不含 `{user}` 占位符仍会丢，属模板配置范畴。）

### 6.2 回溯方向与回血时序（提交审核时修正）

`reply_to_id` 回溯天然是**新 → 旧**，而 `observe` 里的回血会把 `window_depth` 清零、把 `fatigue` 扣减，是**有状态且有方向**的运算。逐跳直接 `observe` 会让 `window_depth` 的含义反转成「回血点之前的老跳数」：

| 场景 | 时间正序（正确） | 倒序直接观测（旧实现） |
|---|---|---|
| 回血点在链中部，其后 5 跳纯文本 | `window=5` | `window=3`（统计成回血之前的跳） |
| 回血点在链头，其后 10 跳纯文本 | `window=10` → Escalate | `window=0` → 只 Nudge，**硬墙被整段绕过** |

第二种正是「防无限互发」最不能接受的情形：最近一次新信息越早、之后的重复往返越长，倒序越会把累积值清零。

- **修法**：consumer 只负责按查库顺序收集 `Vec<ChainHop>`；链断点（非 Agent 来源）**不入链**（反转后会落到链头导致整条链被截断）；组件 `observe_backward_chain` 内部 `reverse` 后走 `observe_chain`，把方向归一化收在单一真相处，后续思考链路复用时不会各自漏掉。
- **护栏**：`backward_chain_observation_is_normalized_to_chronological_order`（正序与归一化结果逐项一致 + 倒序确实算错）、`normalization_keeps_the_hard_wall_after_an_early_recovery`（硬墙不得被回血绕过）。

---

## 七、已确认决策

| # | 问题 | 决策 | 理由 |
|---|------|------|------|
| 1 | 是否按文本相似度算「信息量」 | 否，用结构化锚点 | 无额外模型调用 / 延迟 / 不确定性；YAGNI |
| 2 | 回血是否重置绝对链深 | 否，只重置连续窗口 | 绝对深度用于展示与审计，必须单调 |
| 3 | 提示词给谁看 | 当前被唤醒 Agent 的 prompt（唤醒前注入），不塞进消息正文 | 污染正文会连带影响 FTS / trace / 用户可见内容 |
| 4 | 新增 DB / SSE / 前端字段 | 一期一律不加 | `SsePushPayload` 与 `MessageListItem` 对齐需动 6 层约 9~10 处，先验证压力模型够用 |
| 5 | 硬墙数值 | 5 → 8（可配），且由压力带决定实际停在哪轮 | 直接抬高上限只是把浪费往后推，渐变施压才是目的 |
| 6 | 静默结束 | 禁止；每次观测都打 `log_info!`，升级必给用户可见通知 | 项目已有「引擎无合法候选当 Ok 结束会静默」的教训 |
| 7 | 哨兵优先级 | 保持不变，`NO_REPLY` 恒在 Or 组最前、trim 后全等 | 2026-10-16 实证 29 条泄漏；新增规则不得插到它前面 |

---

## 八、红线

1. **档位新增/修改只准改 `STAGE_DEFS` 一行**，动作、提示词、回血话术必须同行声明；禁止在 consumer 里另写一套压力判断。
2. **回血必须重置 `window_depth`**，否则「新鲜感允许继续聊」失效；`fresh_input_lets_chain_outlive_hard_window` 与 `chain_route_escalates_at_pressure_wall_but_recovery_keeps_peer` 是双重护栏。
3. **阈值必须从 `Metrics` 注入**，禁止固化进 `OnceLock` 策略对象或写死在分支里。
4. **压力观测必须在 awaken 之前**，否则本轮 Agent 看不到信号，只剩机械掐断。
5. **查库失败 / 断链一律放行 Peer**，禁止改成 Err；升级也保持 `Ok(())` ack 终结。
6. **`NO_REPLY` 哨兵恒在最前且只做归一化全等**，禁止 includes 子串匹配。
7. **回溯序列必须先归一化为时间正序再观测**（2026-11-05 审核新增）：禁止把新→旧的 `ChainHop` 直接喂进 `observe` / `observe_chain`；回血清零窗口是有方向的运算，倒序会让硬墙永不触发（§6.2）。链断点不入链，改在回溯处停止收集。
8. **硬墙必须收口为知会，禁止「直接停」或「照常回发」**（2026-11-05 新增）：`Escalate` 只准映射为 `CloseChainAsNotify`（Final 以 `AgentNotify` 送达 + 通知归属用户）；接收侧必须看到 `chain_closed_prompt_lines()`；**知会必须同时是回溯边界**，否则「主动开新链」的第一跳会继承旧链疲劳度而立刻再撞墙（§5.1）。
