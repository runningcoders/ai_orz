---
kind: 落地方案
module: 02-Agent运行时
name: 两阶段唤醒Prompt追加化
title: 两阶段唤醒Prompt追加化
status: 已完成
scope: []
related: []
---

🎯定位：把「意图识别 → 正式执行」从两套独立装配改成同一段 base 上的**逐字节追加**，让 Phase 2 命中 provider 的 prompt cache；同时修掉消息链截断缺陷与 `poll_exit` 重建对追加契约的破坏
状态：v1.0（落地快照）
触发场景：长对话 / 高并发唤醒下 Phase 1 与 Phase 2 重复装配同一份 base，缓存命中率接近 0；长消息链只取到最早 60 条；思考结束后吸收新输入时整段 Prompt 被重建
关联文档：
- 对应 design 文档：[two_stage_append_only_prompt_design.md](../A-设计/two_stage_append_only_prompt_design.md)
- 上层规范：[CODE_STANDARDS.md](../../../CODE_STANDARDS.md) / [ARCHITECTURE.md](../../../ARCHITECTURE.md)

---

## 一、目标

| 问题 | 方式 |
|------|------|
| Phase 1 与 Phase 2 各拼一套 System / User，即使内容相近也字节不同 | 抽出唯一 base 装配 [`fill_prompt_base`](../../../../src/service/domain/runtime/awakening.rs#L797)，两阶段共用 |
| 意图 SOP 被渲染进独立 System 区块，无法复用 Phase 1 前缀 | 意图指令改为**同一条 user 消息的尾部追加**（[`intent_analyze_tail`](../../../../src/service/dal/agent/builder/default.rs#L1614)） |
| 工具描述符按场景过滤，两阶段工具集不一致（破坏 I3） | 两阶段统一使用 Awaken 全量描述符；白名单判定挪到**调用前**（[`intent_analyze_blocks_tool`](../../../../src/service/domain/runtime/think_round/nodes.rs#L859)） |
| `poll_exit` 吸收新输入后走 `prepare` 重建，意图 JSON 与既有轨迹被重排 | 新增显式续跑标记 `InboxState.resume_append`，`prepare` 以 `Artifacts.messages` 为既有对话尾部追加 |
| 消息链 `created_at ASC + LIMIT 60` 取到**最早** 60 条，长链下「最近 6 条」失效 | 改为「链头 + `DESC` 分页扫描直到凑满 K 再反转」，同时加锚点上界（[`fetch_message_thread`](../../../../src/service/domain/runtime/awakening.rs#L893)） |
| 链查询没有时间上界，晚于当前消息的新消息会重复进入 base | 加 `created_before = 锚点 + 1`，同毫秒按 `id` 裁定（[`select_thread_items`](../../../../src/service/domain/runtime/awakening.rs#L949)） |

收敛一句话：base 只装配一次，意图结论与执行段只追加；外部 Agent 因扁平化协议退回重建，其余路径全部满足 I1–I6。

---

## 二、架构思路

```text
Phase 1（意图识别）                    Phase 2（正式执行）
┌──────────────────────────┐          ┌──────────────────────────┐
│ system : base system     │          │ system : 同一份（逐字节）│
│ user   : base user       │  ──────▶ │ user   : 同一份（逐字节）│
│        + 意图指令尾部     │   seed   │ assistant : Intent JSON  │
└──────────────────────────┘          │ user   : 执行段（新增）  │
                                      └──────────────────────────┘
```

三条装配路径（[`PrepareNode`](../../../../src/service/domain/runtime/legacy_awaken/nodes.rs#L308-L400)）：

| 顺序 | 触发条件 | 行为 |
|------|----------|------|
| A 续跑追加 | `resume_append` 为真且 `Artifacts.messages` 非空 | 既有消息 + 尾部一条 user（仅描述新输入，不重复 JSON） |
| B seed 续接 | `intent_seed` 有快照且未压缩 | seed + `assistant(Intent JSON)` + `user(执行段)` |
| C 重建 | 压缩回环 / 无 seed / 外部 Agent / `Artifacts.messages` 为空 | 原 `builder.build()` + `build_initial_messages()` |

外部 Agent（Cli / Remote）例外：`FlatPromptBuilder` 把初始消息压成单条 user，而 `BrainDal::think` 只取最后一条 user；追加后会丢掉 base 原文（人设、当前消息、消息链），且它们没有 provider 级 prompt cache，因此 A/B 两条路径都以 `AgentKind::Local` 为前提，外部 Agent 统一走 C。seed 无论是否使用都会被一次性消费，避免后续 rethink 复用旧快照。

执行段固定顺序（[`build_execution_segment`](../../../../src/service/dal/agent/builder/default.rs#L1279)）：切换语 → 当前消息 + 吸收批次 → 相关知识 / 思维方法 / 上一轮思考摘要 → 执行要求。历史、压缩摘要、消息链、资源行、视觉 parts 不重复进入执行段。

---

## 三、涉及文件清单

| 层次 | 文件路径 | 职责 |
|------|----------|------|
| Domain 意图入口 | [intent_analyze.rs](../../../../src/service/domain/runtime/intent_analyze.rs#L40) | 调唯一 base 装配；意图指令尾插；使用 Awaken 全量工具；回写 seed |
| Domain 取数 | [awakening.rs](../../../../src/service/domain/runtime/awakening.rs#L702) | `workspace_path_args` / `PromptBaseInput` / `fill_prompt_base` / `fetch_message_business_blocks` 的唯一入口 |
| Domain 消息链 | [awakening.rs](../../../../src/service/domain/runtime/awakening.rs#L893) | 锚点上界 + 链头 + 最近 K 条（`DESC` 按页扫描直到凑满 K 再反转） |
| Model 契约 | [prompt_builder.rs](../../../../src/models/prompt_builder.rs#L233) | 新增 `build_execution_segment` 默认空实现 |
| Model 消息 | [cortex_types.rs](../../../../src/models/cortex_types.rs#L276) | `ChatMessage::assistant` 构造器 |
| DAL 渲染 | [default.rs](../../../../src/service/dal/agent/builder/default.rs#L553) | 意图 Prompt 改为 `base + tail`；`build_intent_analyze_initial_messages` 复用 base |
| DAL 渲染 | [default.rs](../../../../src/service/dal/agent/builder/default.rs#L1279) | 执行段渲染 |
| DAL 外部 Agent | [flat.rs](../../../../src/service/dal/agent/builder/flat.rs#L330) | 转发 `intent_revision` / `injected_inputs`，保证外部 Agent 重建时也带上修订与追加输入 |
| Domain 依赖 | [context.rs](../../../../src/service/domain/runtime/legacy_awaken/context.rs#L114) | `intent_seed` 槽位；Business 取数改调共享入口 |
| Domain 装配 | [nodes.rs](../../../../src/service/domain/runtime/legacy_awaken/nodes.rs#L308-L400) | Prepare 三路分支 + 外部 Agent 退回重建 |
| Domain 收件箱 | [inbox.rs](../../../../src/service/domain/runtime/inbox.rs#L236) | `mark_resume_append` / 一次性 `take_resume_append` |
| Domain 挂点 | [poll_node.rs](../../../../src/service/domain/runtime/legacy_awaken/poll_node.rs#L223) | `poll_exit` rethink 置续跑标记；`poll_front` / `poll_round` 不置 |
| Domain 工具拦截 | [think_round/nodes.rs](../../../../src/service/domain/runtime/think_round/nodes.rs#L859) | 执行前按 `ThinkingScene::IntentAnalyze` 拦截并回 tool message |
| Domain 意图调用 | [backend.rs](../../../../src/service/domain/runtime/legacy_awaken/backend.rs#L168-L178) | 主 trace 传入 + seed 槽接入 |
| 测试 | [awakening_tests.rs](../../../../src/service/domain/runtime/awakening_tests.rs#L907) | 消息链选择 4 个用例（长链、锚点之后、当前消息/撤回、链头去重） |
| 测试 | [prompt_builder_test.rs](../../../../src/service/dal/agent/builder/prompt_builder_test.rs) | 意图 SOP 追加在 base 之后 |
| 测试 | [inbox.rs](../../../../src/service/domain/runtime/inbox.rs#L735) | 续跑标记一次性 |
| 集成测试 | [federation_delegation_test.rs](../../../../tests/integration/federation_delegation_test.rs#L362) | Cli/Remote 退回重建后的回显完整性 |

---

## 四、分发点速查表

| 分发场景 | 入口 | 装配路径 | 是否命中缓存 |
|----------|------|----------|--------------|
| Local Agent 首次唤醒 | `Intent` → `Prepare` | B（seed 续接） | 是，Phase 2 复用 Phase 1 前缀 |
| Local Agent 思考后吸收新输入 | `poll_exit` → `Prepare` | A（续跑追加） | 前缀继续命中，仅尾部增长 |
| 意图前 / 检索前吸收新输入 | `poll_front` → `Intent` | 重跑意图并覆盖 seed | base 命中，修订尾部部分失效 |
| 思考轮内新输入 | `poll_round` | 直接追加 user | 天然追加 |
| 上下文溢出 / 轮次耗尽压缩 | `overflow → compact → prepare` | C（重建） | 有损变换，唯一允许重建 |
| 无 seed / 脚本后端 / `Artifacts` 为空 | `Prepare` | C（重建） | 不回退缓存 |
| Cli / Remote Agent | 任意唤醒 | C（重建，含修订与追加输入） | 无 provider 缓存，保持既有回显行为 |
| 意图阶段调用非白名单工具 | `run_dispatch` | 不执行，回 tool message | 拦截在 seed 之外 |

---

## 五、验收清单

| 验收项 | 结果 |
|--------|------|
| Phase 1 / Phase 2 的 System 与 base user 逐字节一致（I1/I2） | ✓ 单测 `intent_initial_messages_reuse_awaken_base_as_prefix` |
| 两阶段工具描述符全量一致，白名单判定移到调用前（I3） | ✓ `intent_analyze_blocks_execution_tools_at_call_time` |
| Phase 2 初始消息前 N 条等于 Phase 1，追加内容只在 N+1 之后（I4/I5） | ✓ seed 续接路径 + 单测断言 |
| trace 使用主 trace，不带随时间变化的差异化值（I6） | ✓ `IntentInput` 移除独立 `trace_id` |
| 消息链锚点上界 + 最近 K 条严格凑满（长链不再取最早，也不因过滤而不足 K） | ✓ `message_thread_selection` 4 个用例 + 按页扫描 |
| `poll_exit` rethink 走追加，压缩回环仍重建 | ✓ `resume_append` 标记 + Prepare 分支 |
| 外部 Agent 追加丢 base 原文的问题 | ✓ Cli/Remote 统一退回重建，联邦委派 e2e 回显恢复 |
| 外部 Agent 重建时仍带意图修订与追加输入 | ✓ 单测 `forwards_intent_revision_into_flat_prompt`、`forwards_injected_inputs_into_flat_prompt` |
| 全量后端测试 | ✓ `cargo test` 通过（含 `federation_delegation_test` 5 项） |
| 后端格式与静态检查 | ✓ `cargo fmt --check`、`cargo clippy --all-targets -D warnings` |
| 前端门禁 | ✓ `make clippy-fe`、`make test-fe`（387 通过） |
| 知识库门禁 | ✓ `make wiki-index`、`make wiki-check` |

---

## 六、执行结果摘要

| 指标 | 值 |
|------|----|
| 修改文件数 | 15 个（Domain 9 / DAL 2 / Model 2 / 测试 2） |
| 新增共享入口 | 4 个（`workspace_path_args`、`PromptBaseInput`、`fill_prompt_base`、`fetch_message_business_blocks`） |
| 新增 trait 方法 | 1 个（`build_execution_segment`） |
| 消息链查询 | 链头 1 次（按 id）+ 尾部按页 `DESC` 扫描（页大小 K+1，过滤后不足 K 继续翻页），替代原单次 `ASC + LIMIT 60` |
| 新增单测 | 9 个（消息链 4、意图前缀 1、续跑标记 1、工具拦截 1、Flat 转发 2） |
| 全量后端测试 | 100% 通过 |
| 前端测试 | 387 通过 |
| clippy 零警告 | backend + frontend 均通过 |

---

## 七、后续扩展路径

1. **Domain 层**：把「Phase 1 初始消息快照」持久化到本轮 trace 元数据，离线对比 `cached_tokens` 与基线，量化缓存收益。
2. **Provider 层**：对支持的 provider 显式设置缓存断点（如 Anthropic `cache_control`），把 base 末条 user 标为断点，减少依赖隐式前缀匹配。
3. **Domain 层**：为 Remote（A2A）定义追加语义的消息契约（多 part 文本），届时可让外部 Agent 也享受两阶段连续性，而不仅是退回重建。
4. **可观测**：在统计事件中新增 `prompt_prefix_reused` 布尔位，区分「seed 续接 / 续跑追加 / 重建」三种装配路径，便于定位缓存失效来源。
