---
kind: rag_card
name: Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2
  正式执行串联
category: 领域建模
scope:
- src/service/domain/runtime/**/*.rs
- src/service/domain/runtime/*.rs
- src/models/prompt_builder.rs
- src/service/dal/agent/mod.rs
- common/src/enums/*.rs
- common/src/api/runtime.rs
- src/consumer/message.rs
source_files:
- src/service/domain/runtime/types.rs#L178-L195
- src/service/domain/runtime/intent_analyze.rs#L1-L189
- src/service/domain/runtime/awakening.rs#L702-L930
- src/service/domain/runtime/mod.rs#L203-L225
- src/service/domain/runtime/think_loop.rs#L117-L160
- src/service/domain/runtime/inbox.rs#L59-L342 (2026-11-02 增量：新输入统一收集器 + 2026-11-03 增量：resume_append)
- src/service/domain/runtime/legacy_awaken/poll_node.rs#L31-L242 (2026-11-02 增量：poll_front / poll_exit；2026-11-03 增量：rethink 置续跑标记)
- src/service/domain/runtime/legacy_awaken/blueprint.rs (2026-11-02 增量：挂点接线与预算)
- src/service/domain/runtime/legacy_awaken/runner.rs (2026-11-02 增量：injected_ids 回传)
- src/service/domain/runtime/legacy_awaken/nodes.rs#L308-L400 (2026-11-03 增量：Prepare 三路装配)
- src/service/domain/runtime/think_round/nodes.rs#L859-L890 (2026-11-03 增量：意图阶段工具调用时拦截)
- src/service/dal/agent/builder/default.rs#L553-L560 (2026-11-03 增量：意图 Prompt = base + tail)
- src/service/dal/agent/builder/default.rs#L1279-L1330 (2026-11-03 增量：执行段渲染)
- src/models/prompt_builder.rs#L233-L240 (2026-11-03 增量：build_execution_segment 契约)
- docs/modules/02-Agent运行时/A-设计/inbox_poll_and_intent_revision_design.md
- docs/modules/02-Agent运行时/A-设计/two_stage_append_only_prompt_design.md (2026-11-03 增量：两阶段 Prompt 追加化设计)
- src/service/dal/agent/mod.rs#L1080
- src/service/dal/agent/mod.rs#L1715
- src/models/prompt_builder.rs#L104-L123
- src/models/cortex_types.rs (2026-09 增量：DeltaToolCall.index 改 Option + resolve_tool_call_index)
- src/pkg/tool_registry/handler_adapter/mod.rs (2026-09 增量：check_stream_end + empty tool arguments 独立错误)
- src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md
- docs/modules/02-Agent运行时/A-设计/intent_aware_two_stage_awaken_design.md
- docs/modules/02-Agent运行时/A-设计/runtime_design.md
- docs/modules/02-Agent运行时/B-落地方案/两阶段唤醒Prompt追加化.md (2026-11-03 增量)
- docs/modules/02-Agent运行时/B-落地方案/唤醒上下文与睡眠约束.md
- docs/modules/02-Agent运行时/B-落地方案/运行时问题修复.md
- docs/modules/02-Agent运行时/C-知识/运行时领域.md
- docs/modules/00-总览与架构/C-知识/Runtime 领域编排.md
- docs/modules/12-基础设施与部署/C-知识/Agent 循环消费者.md
- docs/modules/05-任务与项目/C-知识/Agent 状态管理.md
- docs/modules/02-Agent运行时/C-知识/Agent处理器.md

---

# §1 概述（一句话定位 + 解决什么问题）

**定位**：Agent 唤醒流程的「先理解再执行」两阶段串联——Phase1 `IntentAnalyze` 专用子循环（工具描述符与 Phase2 **完全全量一致**，白名单在 `run_dispatch` 调用时拦截；轮次由 `intent_analyze_max_rounds` 配置、系统默认 3、Agent 可覆盖；0 取系统配置）产出结构化 7 字段 `IntentAnalysis`，Phase2 `Awaken` 以 Phase1 初始消息为 seed、追加 `assistant(Intent JSON)` + 执行段后进入正式干活；含 6 级 JSON 解析降级兜底绝不阻塞主流程。

**Prompt 契约（2026-11-03）**：两阶段共用同一段 base（`fill_prompt_base` 唯一装配），Phase1 只在 base user 尾部追加意图指令；Phase2 复用同一 System/base user，仅在其后追加 assistant 结论与执行段，保证请求前缀逐字节一致以命中 prompt cache。base 不含意图结果、检索知识、执行指令或随时间变化的差异化值。

**解决三类存量痛点**（对应 Design §1.1）：
1. 短消息/指代密集型输入无显式意图识别 → 消歧失败直接答偏
2. Agent 是否调用 `search_memory` 全凭自觉 → 跨任务跨天上下文失焦
3. 系统层无指代消歧辅助链路 → "上次那个方案" 类语句完全依赖 20 条历史窗口运气

---

# §2 关键文件与核心锚点速查表

| 文件锚点（点击跳转） | 角色 | 核心契约 / 红线 |
|---------------------|------|-----------------|
| [IntentAnalysis 7 字段定义](src/service/domain/runtime/types.rs#L178-L195) | **结构体契约（SSOT）** | intent_type / confidence / key_terms / resolutions / retrieved_context / need_clarification / summary；Agent 自由填值不做强枚举，解析失败退化为 `Default::default()` |
| [Phase1 analyze_input_intent_inner 实现](src/service/domain/runtime/intent_analyze.rs#L40-L189) | Phase1 子循环入口 | ThinkingScene=IntentAnalyze；工具描述符与 Phase2 全量一致，白名单在 `run_dispatch` 调用前拦截（neural/memory/query/search）；轮次上限由 intent_analyze_max_rounds 配置（默认 3）；Final 解析为 JSON |
| [base 唯一装配 fill_prompt_base](src/service/domain/runtime/awakening.rs#L797-L838) (2026-11-03 增量) | 两阶段共用前缀 | `workspace_path_args` / `PromptBaseInput` / `fetch_message_business_blocks` 是唯一取数入口；base 只含人设/技能/工作空间/上下文/历史/消息链等，不含意图/知识/执行指令 |
| [消息链锚点上界与最近 K](src/service/domain/runtime/awakening.rs#L893-L1010) (2026-11-03 增量) | 长链取最近 | 链头单独按 id 查；尾部 `created_before = 锚点 + 1` + `DESC` 按页扫描直到凑满 K 再反转（单页 K+1 过滤后可能不足 K）；同毫秒按 `id` 裁定 |
| [Prepare 三路装配](src/service/domain/runtime/legacy_awaken/nodes.rs#L308-L400) (2026-11-03 增量) | 续跑 / seed / 重建 | 续跑追加：`Artifacts.messages` + 尾部新输入；seed 续接：Phase1 初始消息 + assistant JSON + 执行段；压缩回环 / 无 seed / 外部 Agent 重建 |
| [resume_append 续跑标记](src/service/domain/runtime/inbox.rs#L118-L245) (2026-11-03 增量) | 区分续跑与重建 | `mark_resume_append` / 一次性 `take_resume_append`；`poll_exit` rethink 置位，`poll_front` / `poll_round` 不置 |
| [执行段 build_execution_segment](src/service/dal/agent/builder/default.rs#L1279-L1330) (2026-11-03 增量) | Phase2 尾部 | 切换语 + 当前消息/吸收批次 + 知识/方法/上一轮摘要 + 执行要求；不重复历史、压缩、消息链、资源行、视觉 |
| [意图阶段工具拦截](src/service/domain/runtime/think_round/nodes.rs#L859-L890) (2026-11-03 增量) | 调用时白名单 | `IntentAnalyze` 下不允许的 tag 直接 `continue` + 回 tool message + trace `"blocked"`，不产生副作用 |
| [Flat 转发修订与追加输入](src/service/dal/agent/builder/flat.rs#L330-L352) (2026-11-03 增量) | 外部 Agent 重建 | `intent_revision` / `injected_inputs` 转发给内层 `DefaultPromptBuilder`；只在初始消息阶段扁平化，其余区块不得丢 |
| [两阶段追加化 Design](docs/modules/02-Agent运行时/A-设计/two_stage_append_only_prompt_design.md) (2026-11-03 增量) | 为什么 / I1–I6 | 前置缓存不变量、工具全量、续跑标记、消息链边界 |
| [两阶段追加化 Plan](docs/modules/02-Agent运行时/B-落地方案/两阶段唤醒Prompt追加化.md) (2026-11-03 增量) | 怎么做 + 结果 | 15 文件改动、三路装配、验收与扩展路径 |
| [RuntimeDomain trait 方法签名](src/service/domain/runtime/mod.rs#L205-L225) | 领域层对外契约 | `analyze_input_intent(ctx, agent, message, options) -> Result<IntentAnalysis>`；可独立复用于：消息路由预分析 / 澄清追问 / Agent 协作消息分发 |
| [Phase2 awaken 两阶段串联](src/service/domain/runtime/awakening.rs#L150-L180) | 正式执行入口 | awaken 内部先调 analyze_input_intent → need_clarification 先渲染 Prompt → 正式 think loop；【输入理解结果】区块明确标注「仅供参考，不一致可推翻」 |
| [共享 run_think_loop 引擎](src/service/domain/runtime/think_loop.rs#L117-L160) | 两阶段复用的思考内核 | 超时控制 / 多轮迭代 / 策略评估 / 工具调用分发；Phase1 与 Phase2 走同一内核仅 scene 参数不同 |
| [两阶段唤醒 Design 总纲](docs/modules/02-Agent运行时/A-设计/intent_aware_two_stage_awaken_design.md) | 为什么 / 决策表 | §关键决策表 §4.2 降级兜底表 §5 非目标边界 |
| [唤醒上下文 Plan 落地快照](docs/modules/02-Agent运行时/B-落地方案/唤醒上下文与睡眠约束.md) | 怎么做 + 结果 | ThinkingOptions 统一参数 / PromptBuilder 公共方法复用 |
| [运行时领域 Wiki 长文](docs/modules/02-Agent运行时/C-知识/运行时领域.md) | 人类百科 | §5 两阶段唤醒流程详细说明 §8 故障排查 |
| [cortex_types.rs DeltaToolCall index 消歧](src/models/cortex_types.rs) (2026-09 增量) | LLM 流式 tool_call index 多路 fragment 消歧 | DeltaToolCall.index 改 Option；新增 `resolve_tool_call_index` 方法合并多路 fragment 的 index 消除歧义 |
| [handler_adapter check_stream_end](src/pkg/tool_registry/handler_adapter/mod.rs) (2026-09 增量) | 流式终止校验 | `check_stream_end` 无 `[DONE]` 或 `finish_reason=length` 一律判截断；empty tool arguments 独立错误路径 |
| [inbox.rs 统一收集器](src/service/domain/runtime/inbox.rs#L232-L342) (2026-11-02 增量) | 新输入合并判据 / 游标 / 批量预算 | `Option` 相等比较（None 也是取值）；`from_id + from_role` 相同；`thread_id` 仅否决；Text 白名单；≤5 条 / ≤2000 字 |
| [poll_node.rs 三处挂点](src/service/domain/runtime/legacy_awaken/poll_node.rs#L31-L242) (2026-11-02 增量) | 意图回退 / 尾插转向 / 落地前检查 | `poll_front` / `poll_exit`（轮级 `poll_round`）；回退 ≤2 次、转向 ≤3 次；出口信号 `exit_done` 必须唯一 |
| [inbox_poll_and_intent_revision_design.md](docs/modules/02-Agent运行时/A-设计/inbox_poll_and_intent_revision_design.md) | 为什么 / 决策表 | 合并判据、意图增量修订、状态提交、预算护栏 |

---

# §3 架构约定与数据流（业务语义层面，不贴实现代码）

**数据流总览（2026-11-03 追加化后）**：
```
外部消息 → MessageConsumer
  → Phase1 IntentAnalyze 子循环
     ThinkingScene=IntentAnalyze
     Prompt: system = base system；user = base user + 意图指令尾部
     工具：描述符与 Phase2 全量一致；run_dispatch 调用前按 tags 拦截
     输出：IntentAnalysis {7 字段}，同时回写 intent_seed = Phase1 初始消息
  → need_clarification 非空？→ 当前仅渲染提示，Agent 自行决定何时问
  → Phase1.5：PrepareNode 三路装配
     ① poll_exit 续跑：Artifacts.messages + 尾部新输入（resume_append）
     ② seed 续接：Phase1 初始消息 + assistant(Intent JSON) + user(执行段)
     ③ 重建：压缩回环 / 无 seed / 外部 Agent / Artifacts 为空
  → Phase2 Awaken 正式干活
     ThinkingScene=Awaken（所有工具可用）
     同一 System/base user 前缀 → 命中 prompt cache
     只有执行段与后续消息是新增 token
```

**6 级 JSON 降级兜底策略**（对应 Design §5.2）：
| 级别 | 故障场景 | 降级行为 |
|------|---------|---------|
| Level 1 | 完美路径 | Final 纯 JSON → 直接 serde_json 反序列化 |
| Level 2 | JSON 包裹 ```json``` 代码块 | 正则剥离三引号后反序列化 |
| Level 3 | JSON 前后有多余文本 | 提取首个 { ... } 完整块反序列化 |
| Level 4 | 单字段解析失败 | 对应字段用 Default，其余保留 |
| Level 5 | 整体反序列化失败 | summary 字段填原文本全文，其余 6 字段 Default |
| Level 6 | think loop 超限/抛错 | Result 捕获 warn，awaken 正常进入 Phase2（无增强区块），等价于单阶段流程 |

**三条关键行为红线**（§4 必守，回归必保）：
1. **IntentAnalyze 严格不允许执行类工具**：`is_tool_allowed` 匹配 neural/memory/query/search 任一 tag 才放行；禁止任何 messaging/project_management/external_agent 类工具
2. **理解结果仅供参考原则**：Phase2 Prompt 渲染必须写入「以下是你上一阶段自己得出的结论，如与当前判断不一致可以推翻」，避免 Agent 被错误前置理解带偏
3. **降级绝不阻塞主流程**：任何 Level 4-6 失败场景，只打 warn 日志 + 退化为 Default/单阶段，绝不返回 Err 中断 awaken 链路

**（2026-09 cortex 流式终止 + tool_call index 消歧约定）**：
4. **DeltaToolCall.index 必须 Option 化并由 resolve_tool_call_index 消歧**：多路 SSE fragment 并行到达时，不同 tool_call 的 index 可能交错拼接；直接拿 `delta.index.unwrap_or_default()` 会把多路 fragment 拼进第一路导致非法 JSON。必须在 cortex_types.rs 内用 `resolve_tool_call_index` 方法先合并再消歧
5. **check_stream_end 终止判定严格化**：无 `[DONE]` 信号或 `finish_reason=length` 一律判截断——空 `finish_reason` / `finish_reason=stop` 都不代表正常结束；截断事件统一上报 stats + 触发降级兜底

---

# §4 硬约束 / 必守红线 / 扩展入口

**§4.1 必守红线（8 条，违反 = FAIL）**

| # | 红线 | 验证方式 | 代码锚点 |
|---|------|---------|---------|
| 1 | IntentAnalysis 字段**绝对不强枚举** intent_type，允许 Agent 自由字符串扩展；解析失败用 Default | grep `enum IntentType` 应不存在；结构体 derive(Default) | [types.rs#L178-L195](src/service/domain/runtime/types.rs#L178-L195) |
| 2 | Phase1 think loop 轮次上限由 `intent_analyze_max_rounds` 配置（系统默认 3，Agent 可覆盖；0 取系统配置），超过强制截断 Final；不得与 Phase2 共享同一 think_runtime | options.max_rounds 取自 config_resolve::intent_analyze_max_rounds(agent)；AgentThinkRuntime 独立注册清理 | [intent_analyze.rs#L40-L60](src/service/domain/runtime/intent_analyze.rs#L40-L60) |
| 3 | `need_clarification` 字段**当前阶段绝不做短路 send_message**，仅渲染为 Phase2 Prompt 提示；短路机制为 P4 可选未来路径 | grep `send_message.*clarification` 在 awaken.rs 应为零 | [awakening.rs#L200-L240](src/service/domain/runtime/awakening.rs#L200-L240) |
| 4 | retrieved_context 字段渲染到 Prompt 时**最多 10 条，每条最多 200 字**；Token 安全网硬编码截断 | DefaultPromptBuilder.intent_analysis() 实现 | [dal/agent.rs](src/service/dal/agent/mod.rs) intent_analysis 方法 |
| 5 | 降级 Level 5/6 必须打 `log_warn!(&ctx, "intent_analyze_degrade", ...)` 含降级级别与原始 snippet 前 100 字 | grep `log_warn.*intent_analyze_degrade` | [intent_analyze.rs#L70-L80](src/service/domain/runtime/intent_analyze.rs#L70-L80) |
| 6 | 两阶段唤醒的 Phase1 / Phase2 **必须复用同一 run_think_loop 引擎**，仅 scene 参数不同；禁止复制粘贴两套 think loop 逻辑 | run_think_loop 仅一处定义 | [think_loop.rs#L117-L160](src/service/domain/runtime/think_loop.rs#L117-L160) |
| 7 | analyze_input_intent 作为通用方法**必须可独立被外部调用**（消息预路由 / 澄清重理解等），不能与 awaken 内部私有耦合；trait 定义在 RuntimeDomain 总接口 | `pub async fn analyze_input_intent` 在 trait 层公开 | [mod.rs#L205-L225](src/service/domain/runtime/mod.rs#L205-L225) |
| 8 | 方案 B（技能 SOP 版理解流程）与方案 A+（本卡 IntentAnalyze）**SOP 核心逻辑一致但强度不同**：Prompt 约束描述不得前后矛盾；Agent 读不到两个互相打架的方法论 | 对照 Design §5.3 分工边界表 | [intent_aware_two_stage_awaken_design.md §5.3](docs/modules/02-Agent运行时/A-设计/intent_aware_two_stage_awaken_design.md#L410-L422) |
| 9 | **禁止 awaken 对 Phase1 analyze 结果 `?` 冒泡**：错误吞掉降级 None；用 `?` 会把意图分析服务不可用升级为整次唤醒失败，P0 故障 | 集成测试模拟 cortex panick → awaken 仍返回 200 且正常响应 | [awakening.rs awaken Phase1 串联 match 分支](src/service/domain/runtime/awakening.rs#L230-L280) |
| 10 | **禁止 IntentAnalyze 执行类工具**：两阶段工具描述符全量一致，白名单判定必须在 `run_dispatch` **调用前**（`ThinkingScene::IntentAnalyze.is_tool_allowed(tags)`）；拦截后回 tool message 并标记 trace `"blocked"`，禁止在描述符层过滤（会破坏 I3） | thinking_scene_tool_whitelist 单测 + `intent_analyze_blocks_execution_tools_at_call_time` | [think_round/nodes.rs#L859-L890](src/service/domain/runtime/think_round/nodes.rs#L859-L890) |
| 11 | **Phase1 专用指令只能尾插在 base user 之后**：`DefaultPromptBuilder` 的 `awaken_system_part` / `awaken_user_part` 不得出现意图 SOP；`FlatPromptBuilder` 只做扁平化转发 | 单测断言 Phase1 user 以 base user 开头且 SOP 索引在 base 之后 | [default.rs#L1413-L1440](src/service/dal/agent/builder/default.rs#L1413-L1440) |
| 12 | **INTENT_ANALYSIS_START 锚点必须存在于 Prompt 末尾**：6 级 JSON 解析 Level 4 靠此锚点定位后提取 JSON，删除则误匹配 Prompt 示例代码 {} 的概率极高 | build_intent_analyze_prompt 单元测试：含 INTENT_ANALYSIS_START 字符串 | [default.rs#L1413-L1440](src/service/dal/agent/builder/default.rs#L1413-L1440) |
| 13 | **need_clarification=true 只做参考、当前不短路 Phase2**：短路是下一迭代功能；提前启用会导致每次都先问一句澄清再执行，响应翻倍 | awaken 入口 grep 不应有 send_message.*clarification 的短路 if 分支 | [awakening.rs awaken 入口 if need_clarification 检查位置](src/service/domain/runtime/awakening.rs#L200-L240) |
| 14 | **Template Communication SOP 章节（方案 B）与代码级 Phase1（方案 A+）并行双保险**：不要删除 skill.md 末尾的「理解用户消息 SOP」章节；即便 Phase1 流程被禁用，沟通技能也能驱动 Agent 在普通 think loop 中自行按五步理解 | TEMPLATE_COMMUNICATION/skill.md grep 「SOP」命中 | [TEMPLATE_COMMUNICATION skill.md 末尾章节](src/service/domain/system/seed/skills/TEMPLATE_COMMUNICATION/skill.md) |
| 15 | **DeltaToolCall.index 必须用 Option 类型并由 resolve_tool_call_index 消歧**（2026-09 新增）——多路 SSE fragment 并行到达时禁止直接拿 index（会把多路拼进第一路导致非法 JSON）；必须在 cortex_types.rs 内用 resolve_tool_call_index 方法先合并多路再输出统一 index | 单路 / 多路 SSE fragment 拼接测试断言生成的 tool_call JSON 合法；grep delta.index.unwrap_or_default 应不存在 | [cortex_types.rs DeltaToolCall](src/models/cortex_types.rs) |
| 16 | **check_stream_end 无 [DONE] 或 finish_reason=length 一律判截断**（2026-09 新增）——空 finish_reason / finish_reason=stop 都不代表正常结束；截断事件统一上报 stats + 触发降级兜底 | 模拟 SSE 流中途断连（无 [DONE]、无 finish_reason）应触发截断标记而非正常结束 | [handler_adapter/mod.rs check_stream_end](src/pkg/tool_registry/handler_adapter/mod.rs) |

**§4.2 扩展入口速查（按 4 步模板）**

| 扩展需求 | 改动位置（N 处同步） | 参考锚点 |
|---------|---------------------|---------|
| 新增 IntentAnalysis 字段（如 `ambiguity_level`） | ① types.rs struct 追加字段 → ② build_intent_analyze_prompt JSON Schema 追加 → ③ intent_analyze.rs serde 反序列化（兼容旧字段 Default）→ ④ PromptBuilder.intent_analysis() 渲染追加 → ⑤ 降级兜底 Default 兼容 | [types.rs#L178-L195](src/service/domain/runtime/types.rs#L178-L195) |
| Phase1 强制搜索策略更激进（例如必须调 recommend_seed_nodes + traverse 1 跳） | build_intent_analyze_prompt 的 Step 4 SOP 文本修改；降级兜底保持不变（Agent 不执行只是 retrieved_context 为空） | intent_analyze Prompt SOP Step 4 位置 |
| 短路澄清机制上线（need_clarification 非空直接发消息，不进入 Phase2） | awaken() 入口追加 if 分支；send_message 走与正常 Agent 调 send_message 同一通道；状态机正确回到 Idle | [awakening.rs awaken 方法入口](src/service/domain/runtime/awakening.rs#L150-L180) |
| 新增 ThinkingScene（如 CodeReview 专用前置分析场景） | ① ThinkingScene 枚举追加变体 → ② is_tool_allowed 追加分支 → ③ 对应专用 build_xxx_prompt → ④ RuntimeAwakening trait 新增通用复用方法 → ⑤ awaken 内部按需串联前置阶段 | common/src/enums/thinking_scene.rs + awakening.rs is_tool_allowed match |

## §4.3 2026-11-02 增量：思考中收到新输入（收集 + 增量修订）

**新增红线（与既有 16 条并列，违反 = FAIL）**

| # | 红线 | 验证方式 | 代码锚点 |
|---|------|---------|---------|
| 17 | **上下文过滤必须用 `Option` 相等比较**：`project_id` / `task_id` 的 `None` 也是取值，默认对话（无项目/无任务）必须参与匹配；禁止「空值就跳过过滤」的写法 | inbox 单测覆盖默认对话 / 有项目无任务 / 项目+任务三种上下文 | [inbox.rs#L232-L258](src/service/domain/runtime/inbox.rs#L232-L258) |
| 18 | **合并只要求同发送者（`from_id` + `from_role`）**，不限于 User；`thread_id` 只作否决项（两端都有且不等才拒绝），不能当必要条件 | inbox 单测：非 User 同发送者可合并；单端 thread 缺失不拒绝 | [inbox.rs#L245-L257](src/service/domain/runtime/inbox.rs#L245-L257) |
| 19 | **意图增量修订必须重算终态字段**：`direct_answer` / `need_clarification` / `retrieved_context` 针对旧消息，一有新输入即失效；只允许复用 `intent_type / confidence / key_terms / resolutions / summary` | 修订 Prompt 渲染在 `current_message` 之后；失败降级清空终态字段 | [default.rs#L1645-L1668](src/service/dal/agent/builder/default.rs#L1645-L1668) |
| 20 | **收集只读、成功才提交**：唤醒失败时吸收的消息必须保持 `Pending`，不得在收集阶段提前置 `Processed` | consumer 只在 awaken 成功后遍历 `injected_ids` 批量更新 | [consumer/message.rs](src/consumer/message.rs) |
| 21 | **图激活预算必须与新增挂点同步**：`poll_exit` 出口信号用唯一 `exit_done`（禁止复用 `done`，否则与 `prepare` 的入边竞争）；各节点预算漏算会在「无合法候选」处报错 | 蓝图单测断言节点/边/预算；`legacy_awaken` 集成测试跑通回退/转向路径 | [blueprint.rs](src/service/domain/runtime/legacy_awaken/blueprint.rs) |

**与既有红线的冲突说明**：红线 13「need_clarification 不短路 Phase2」描述的是旧唤醒入口；当前 legacy 思考图已通过 `poll_exit` 在 `direct` / `clarify` 落地前做新输入检查，澄清仍不会跳过整轮思考，但会在**确认没有新输入后**直接向用户提问。两者语义对齐：澄清不是跳过执行，而是缺少用户输入时不再白跑检索。

## §4.4 2026-11-03 增量：Prompt 追加化（I1–I6）

**新增红线（与既有 21 条并列，违反 = FAIL）**

| # | 红线 | 验证方式 | 代码锚点 |
|---|------|---------|---------|
| 22 | **Phase1 / Phase2 的 System 与 base user 必须逐字节一致（I1/I2）**：两阶段只能通过 `fill_prompt_base` 装配 base；禁止在 base 里塞意图结果、检索知识、执行指令、trace 子轨迹或任何随时间变化的差异化值 | `intent_initial_messages_reuse_awaken_base_as_prefix` 断言 System 相等、Phase1 user 以 base user 开头 | [default.rs#L1413-L1440](src/service/dal/agent/builder/default.rs#L1413-L1440) |
| 23 | **两阶段工具描述符全量一致（I3）**：白名单只在 `run_dispatch` 调用前判定；拦截必须回 tool message 并标记 trace `"blocked"`，禁止静默丢弃或在描述符层裁剪 | `intent_analyze_blocks_execution_tools_at_call_time` | [think_round/nodes.rs#L859-L890](src/service/domain/runtime/think_round/nodes.rs#L859-L890) |
| 24 | **追加内容只能出现在第 N+1 条之后（I4/I5）**：Phase2 以 Phase1 初始消息为 seed，依次追加 `assistant(Intent JSON)` 与执行段；禁止重排、重渲染或改写前缀 | seed 续接路径单测 + trace 对比两次请求前缀 | [nodes.rs#L330-L372](src/service/domain/runtime/legacy_awaken/nodes.rs#L330-L372) |
| 25 | **`poll_exit` rethink 必须续跑追加**：由 `InboxState.resume_append` 显式标记驱动，以 `Artifacts.messages` 为既有对话、只在尾部加新输入；**压缩回环是唯一允许重建的路径** | `resume_append_flag_is_one_shot` + prepare 分支单测 | [poll_node.rs#L214-L226](src/service/domain/runtime/legacy_awaken/poll_node.rs#L214-L226) |
| 26 | **外部 Agent（Cli / Remote）不得走 seed 或续跑追加**：`FlatPromptBuilder` 压成单条 user，执行层只取最后一条 user，追加会丢 base 原文；它们也没有 provider 级缓存，必须统一走重建。重建时 `FlatPromptBuilder` 仍须转发 `intent_revision` / `injected_inputs`，否则修订与追加批次丢失 | `federation_delegation_test` 回显包含原始 prompt + `forwards_intent_revision_into_flat_prompt` / `forwards_injected_inputs_into_flat_prompt` | [nodes.rs#L318-L328](src/service/domain/runtime/legacy_awaken/nodes.rs#L318-L328) |
| 27 | **消息链查询必须有锚点上界且取最近 K 严格凑满**：链头单独按 id 查；尾部 `created_before = 锚点 + 1` + `created_at DESC, id DESC` 按页（页大小 K+1）向更早扫描，过滤当前消息 / 同毫秒 id 更大 / `Recalled` 后仍不足 K 时继续翻页，凑满 K 或链耗尽即停，再反转；同毫秒按 `id` 裁定；晚于锚点的消息只能由收集器进入执行段 | `message_thread_selection` 4 个用例 | [awakening.rs#L893-L1010](src/service/domain/runtime/awakening.rs#L893-L1010) |
| 28 | **上次意图结论必须携带且不重复**：Intent JSON 只出现一次（assistant 消息）；执行段只描述新增输入与执行要求，不得整段复述 base 或历史 | seed 路径消息序列断言：`[System, User(base+tail), Assistant(JSON), User(segment)]` | [nodes.rs#L330-L372](src/service/domain/runtime/legacy_awaken/nodes.rs#L330-L372) |

## §5 历史演进（f300b5c0..HEAD 增量）

| commit | 事件 | 变更内容 |
|--------|------|---------|
| 2026-11-03 | 两阶段唤醒 Prompt 追加化 | 抽出 `fill_prompt_base` 唯一 base 装配（workspace / 业务块单一取数）；意图指令改为 base user 尾部追加；Phase2 以 Phase1 初始消息为 seed，追加 assistant(JSON) + 执行段；工具描述符两阶段全量一致、白名单移到 `run_dispatch` 调用前；`poll_exit` rethink 由 `resume_append` 驱动续跑追加；消息链加锚点上界并改为「链头 + DESC 按页扫描凑满 K 再反转」；Cli/Remote 统一退回重建，并转发 `intent_revision` / `injected_inputs` |
| 2026-11-02 | 思考中收到新输入：统一收集 + 意图增量修订 | 新增 `inbox.rs`（锚点/游标/Option 相等/线程否决/批量预算）；`legacy_awaken/poll_node.rs` 三挂点 + `poll_exit` 唯一出口信号 `exit_done`；`IntentRevision` 尾部修订 Prompt；成功后批量 `Processed`，失败保持 `Pending`；蓝图预算同步上调并 `max_steps=96` |
| 0e818092 | cortex 流式终止校验 + LLM 出站留痕 + empty tool arguments 独立错误 | DeltaToolCall.index 改 Option 类型 + resolve_tool_call_index 消歧（多路 fragment 并行时禁止直接拿 index）；check_stream_end 无 [DONE] 或 finish_reason=length 一律判截断；empty tool arguments 独立错误路径 |
