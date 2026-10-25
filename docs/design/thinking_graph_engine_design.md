# 思考图引擎改造设计（Awaken 主思考路径节点化）

> 状态：落地中快照 · 2026-10-25
> 代码为 SSOT，本文只记「为什么、边界、契约」。

## 1. 背景与痛点

当前 Awaken 主思考路径（`legacy_awaken` 图）本质仍是「一次装配大 prompt + 让模型自己找路」：

- `PrepareNode` 把人设/技能/工作空间/项目/历史/资源等十余个区块一次性全量拼进 prompt（src/service/domain/runtime/legacy_awaken/nodes.rs#L171-L265）——**上下文过长、全是重点 = 没有重点**；
- 意图识别（IntentAnalyze）已实现但**零调用**（`analyze_input_intent` 无生产调用方），builder 的意图区块渲染位（src/service/dal/agent/builder/default.rs#L738、#L1084）从未被喂过数据；
- 知识/技能检索完全交给主执行循环里模型自觉调工具，不可控、不稳定。

## 2. 现状代码事实（改造基础）

| 事实 | 锚点 |
|---|---|
| 图引擎已存在：ThinkNode trait + NodeContract + ThinkBus + 蓝图信号路由 + ActivationBudget 终止护栏 | src/pkg/think_path/node.rs#L111、src/pkg/think_path/mod.rs |
| Awaken 已节点化：prepare → deliberate → compact/force_compact → consolidate，MAX_LOOP_ROUNDS=8 | src/service/domain/runtime/legacy_awaken/blueprint.rs#L33 |
| 主执行内部已是图：think → dispatch → evaluate(→reflect) 轮级环图，工具循环/退出策略齐全 | src/service/domain/runtime/think_round/blueprint.rs#L46 |
| IntentAnalyze 半成品：inner 实现 + 六级 JSON 降级解析 + builder prompt 方法齐备 | src/service/domain/runtime/intent_analyze.rs#L30、#L197 |
| IntentAnalysis 七字段契约（key_terms 可直接喂检索） | src/service/domain/runtime/types.rs#L430 |
| 检索基建现成：MemorySearch 混合搜索 / traverse / recommend_seed_nodes | src/service/domain/runtime/memory.rs#L75、#L203 |
| traverse 的 visited/edge_cache 防环先例 | src/service/dal/memory/graph.rs#L204、#L351 |
| ThinkBackend 可替换（测试不打模型） | src/service/domain/runtime/legacy_awaken/context.rs#L360 |
| 预置技能 11 份 + sync_seed_skill 增量同步 | src/service/domain/system/seed/skills/、tools/src/bin/sync_seed_skill.rs |
| ThinkRoundEvent / AgentLoopEvent / fingerprint 观测链 | src/models/events/think_round.rs#L8、legacy_awaken/runner.rs#L244 |

## 3. 设计决策

### 3.1 不新建引擎，扩展既有蓝图

`pkg/think_path` 就是思考图引擎；本次改造 = 把 `legacy_awaken` 蓝图扩成**完整思考图**。不新建模块、不保留旧路径开关（旧代码在 git；不做灰度）。

### 3.2 目标拓扑

```text
intent ──done/degraded──▶ retrieve ──done──▶ prepare ──done──▶ deliberate
                              ▲                                              │
                              │                 new_findings                ├─final─────▶ consolidate
                              └───────────────── discover ◀──max_rounds────┤
                                                        │                    ├─cancelled──▶ consolidate
                                                        │                    └─overflow───▶ compact ──done──▶ prepare
                                                        │
                          no_new_findings / 检索预算尽 ──▶ force_compact ──done──▶ consolidate
```

- `intent`（判断节点）：包 `analyze_input_intent_inner` 跑 IntentAnalyze 场景小循环；**失败降级为空 IntentAnalysis，不阻断**（与 types.rs#L427 既有降级语义一致）；intent-analyze started/finished 事件由节点发布（原 trait 外层逻辑移入）。
- `retrieve`（机械节点）：输入 = `IntentAnalysis.key_terms/summary`（或 discover 新词）；`MemorySearch` 混合搜索知识 + 技能检索（`category=thinking_method` 优先）；机械 Top-K 裁剪（v1 不加模型判断）；产出 `RetrievalPack` 写总线。
- `prepare`（机械节点，增强）：既有装配 + 意图理解区块（`builder.intent_analysis()`）+ 检索产物区块 + 上轮思考摘要区块——**重点前置，通用上下文后置**。
- `deliberate`（判断节点，不动）：仍是 `run_think_loop`（工具循环在 think_round 图内部，天然覆盖「结果是调用工具」分支）。
- `discover`（判断节点，新增）：max_rounds 退出后跑一次**短聚焦无工具单轮**模型调用，从思考轨迹提取 `new_key_terms` + `prior_thinking_summary`；有新词且预算未尽 → 回 retrieve（「重新做识别、重新检索知识」分支）；否则 → force_compact。
- `compact/force_compact/consolidate`：不动。

### 3.3 节点分型与结合机制

- **机械节点**（不调模型）：retrieve、prepare——确定性、可测、零 token。
- **判断节点**（调模型、短聚焦 prompt）：intent（已有五步 SOP prompt）、discover（新增短 prompt：思考轨迹 + 已检索词 → 提取新线索 JSON）。
- 模型判断只加在 discover 一处；候选裁剪 v1 用机械 Top-K（后续视效果再升级）。
- discover 的 JSON 解析复用 `extract_first_json_object` + 宽容字段提取模式（与 parse_intent_analysis_json 同套路，不另造解析框架）。

### 3.4 产物契约（结构化上下文传递）

| 产物 | 载体 | 生产者 → 消费者 |
|---|---|---|
| `IntentAnalysis` | 总线新分区 `ContextKey::Intent` | intent → retrieve/prepare |
| `RetrievalPack`（知识摘要 + 命中技能 + 查询词） | 总线新分区 `ContextKey::Knowledge` | retrieve → prepare |
| `RetrievalState`（已查词集合 + 已命中节点 id，防重) | `AwakenDeps` 新增 `Mutex<RetrievalState>`（同 compaction 槽位先例） | retrieve/discover 读写 |
| `DiscoveryOutcome`（new_key_terms + prior_thinking_summary) | 并入 `ContextKey::Knowledge` 分区 | discover → retrieve/prepare |

`ContextKey` 八分区扩展为十分区（Intent/Knowledge），引擎枚举同步 `as_str/ALL/parse`。

### 3.5 终止三防线

**卡点哲学：图维度管回环总数，周期内部自决。** 思考轮次不再跨周期累计限制——每个思考周期满额重启（`start_round=0`），某个周期内部「需要多想几轮」由 think loop 自己决定；图真正要卡的点是整条唤醒链最多绕多少个回环。

1. **visited 去重**：retrieve 只查「已查词集合的差集」；discover 新词与已查词求差，差集空 = 无新息。
2. **回环预算（图维度卡点）**：蓝图 `ActivationBudget` 显式放开 retrieve/prepare/deliberate/discover 次数（对齐 MAX_LOOP_ROUNDS=8）+ `PathLimits.max_steps` 硬护栏 + runner `ensure_consolidated` 终止护栏（未到收尾节点即报错，不静默）。discover 双层预算：软预算 `MAX_DISCOVERY_ROUNDS`=2 轮再检索，硬预算 = 软预算 + 1——多出的那次激活留给软预算零模型调用优雅退出（no_new_findings → force_compact）；retrieve 预算跟着 discover 配对（N 轮再识别 = N 次回检索 + 1 次初始检索）。
3. **无新息退出**：discover 输出空 new_key_terms / 软预算尽 → 走 force_compact 既有收尾。

累计轮次口径：`BudgetState.total_rounds` 仍然跨周期累计（溢出/耗尽/取消都累加本周期用量），但只用于收尾回复与统计（Trace `rounds_used`、兜底存档），**不回灌成下一周期的起始限制**。

### 3.6 数据源：thinking_method 元技能

- 不新增实体：思维方法 = `category = "thinking_method"` 的技能（SkillPo.category 为 String，src/models/skill.rs#L39）。
- 新增预置技能（seed skills 目录 + sync_seed_skill 同步），首个：问题求解策略（熟悉域→规划执行 / 陌生域→检索先行）。
- 图谱登记约定：settle 沉淀技能节点时 `node_tags` 携带 category（含 thinking_method），检索侧按 tag/category 可召回。
- 技能检索入口复用 HR 域 search_skills 背后的技能 DAL（runtime 域经 DAL 层访问，遵守分层单向）。

### 3.7 观测

- 节点进出日志沿用 `log_info!` + 模块名（legacy_awaken）；
- 事件口径不动：ThinkRoundEvent / AgentAwakeEvent.exit_reason / fingerprint 照旧；
- intent 节点发布既有 `AgentLoopEvent("intent-analyze")` started/finished；discover/retrieve 节点用日志 + fingerprint 覆盖（不新增事件类型，避免消费者侧扩散）。

## 4. 不做的事

- ❌ 不新增「思维链」实体，不做 Agent 运行时改图结构（静态骨架 + 动态路由，自发性来自循环边与路由信号）。
- ❌ 不做灰度/影子对比开关（旧实现在 git 历史可回溯）。
- ❌ 不重构 think_round 轮级图、不动 settle 图（settle 不加意图识别，目标明确）。
- ❌ IntentAnalysis 七字段不加字段；discover 产物是独立小契约。
- ❌ v1 候选裁剪不上模型。

## 5. 影响面

| 位置 | 变更 |
|---|---|
| src/pkg/think_path/types.rs | ContextKey +Intent/Knowledge |
| src/service/domain/runtime/legacy_awaken/ | blueprint 扩展；nodes 拆新文件（intent/retrieve/discover）；context 加 RetrievalState 槽位 |
| src/service/dal/agent/builder/default.rs | 新增检索产物区块渲染、思考摘要区块、build_discover_prompt；意图区块已有 |
| src/service/domain/system/seed/skills/ | +thinking_method 预置技能 |
| settle 技能沉淀路径 | node_tags 携带 category 约定 |

## 6. 验证口径

- 全量 `cargo test` 通过、双端 `clippy -D warnings` 零告警；
- legacy_awaken 集成测试用脚本后端跑通新拓扑（含降级路径：intent 失败 / 检索空结果 / discover 无新词）；
- 蓝图单测：节点数、边、预算、无出边信号自然终止；
- 用户实测：真实模型下意图→检索→执行→（必要时）再检索→回复全链路。
