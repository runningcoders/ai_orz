---
kind: 设计决策
module: 02-Agent运行时
name: runtime_data_bus_design
title: 运行时数据总线设计（收集、移交与写穿）
status: 现行
scope:
- src/service/domain/runtime/think_trace.rs
- src/service/domain/runtime/collected.rs
- src/service/domain/runtime/think_round/
- src/service/domain/runtime/legacy_awaken/context.rs
---
# 运行时数据总线设计（收集、移交与写穿）

> 定位：回答一个反复出现的问题 —— **「节点侧收集的数据，最后怎么交出去？」**
>
> 结论先给：不看数据长什么样，看**读者在哪**。四种读者对应四种载体，没有第五种。
>
> 硬红线：**进程内禁止 `static … HashMap<run_id, _>` 形式的总线/登记表**。

---

## 一、问题的由来

「在节点里攒一点东西，循环结束吐出去」这种需求，过去用三种不同办法实现过：

| 历史实现 | 载体 | 现状 |
|---|---|---|
| 思考状态（messages / 计数 / 轮次轨迹） | `RoundState`（`Arc<Mutex>` + 强类型字段） | 保留，是轮级图的上下文 |
| 唤醒级上下文（prompt / budget / artifacts） | `ThinkBus`（`serde_json::Value` + `ContextKey`） | 保留，是唤醒图的上下文 |
| 小脑留痕 | `CEREBELLUM_BUFFER`：`static OnceLock<Mutex<HashMap<run_id, _>>>` | **已删除**，改为写穿 |
| 本轮附件 / 产物 | `PRODUCED_BUFFER`：同上 | **已删除**，改为随结果移交 |

两个全局表之所以出现，是因为「生产者与消费者不在同一条调用栈上」，而当时为了不动结构体，
选择了「进程级登记」。代价是可见的：

- 键对不上（`trace_ids.first()` 隐含「一次唤醒一个 run」，却没有断言）；
- 取消 / 早退路径不消费 ⇒ 残留，只能靠 `map.clear()` 静默全清；
- 容量截断不可见（「轨迹看着完整、其实少了 26 条」）；
- 全局状态让测试互相污染、无法并行。

**根因不是「缺少总线」，而是「所有权没有出口」。**

---

## 二、判据：读者在哪，载体就在哪

| # | 读者位置 | 生命周期 | 载体 | 典型数据 |
|---|---|---|---|---|
| 1 | 图内后续节点 | 随图 | `RoundState` / `ThinkBus` 字段 | messages、budget、artifacts |
| 2 | 图的调用方（进程内） | 到图返回 | **返回值** typed 字段 | `AwakeningResult.collected` |
| 3 | 调用方，且 Err / 超时 / 取消路径也要 | 长于图 | **调用方持有**的 `Arc` 句柄 | `ThinkLoopProgress`、`WakeUsage` |
| 4 | 进程外（前端 / 查询接口 / 排障 cat 文件） | 不关心图 | **写穿落盘** | 轮次轨迹、小脑留痕、tool_tracing |

两条附加规则：

- **决策类数据只准走 1/2**：路由、投递、是否回复这些结论必须是强类型字段，
  禁止经由任何「尽力而为」的载体传递（丢失会改变行为，不可接受）。
- **「活过图」≠「登记进进程表」**：需要长于图的数据，把所有者升级到调用方（`Arc`），
  所有权仍然显式、仍然随用随 drop。

---

## 二·补：图级运行总线实例（`GraphRuntime`）

跨节点共享状态由 `src/service/domain/runtime/graph_runtime.rs` 的 `GraphRuntime` 持有；
它站在整张图的视角，把独立功能模块包装成自己的方法（首片是 inbox）：

```text
节点 → runtime.inbox_collect(ctx, judge_ctx, brain) → inbox::collect(共享参数自己取)
节点 → runtime.inbox_record_judged(judged, used)   → 总账扣减
节点 → runtime.inbox_advance_*(batch)              → 游标 / 吸收 / 修订 / 尾插
```

- **共享参数由总线取**：锚点、游标、已判定集合、剩余预算不出现在节点签名里；
  节点只传本次任务参数（判定现场、brain）。这是「模块独立 + 传参解耦」的落点。
- **两图共享同一实例**：`AwakenDeps.runtime` → `ThinkInput.runtime` →
  `ThinkLoopParams.runtime` → `RoundRun.runtime` → `RoundDeps.runtime`，
  全程 `Arc` 透传，不重建、不用进程级表。
- **已迁入的五片（2026-11-28）**：`inbox` / `RoundCollected` / `RetrievalState`+
  `retrieve_searches` / `CompactionSlot` / `intent_seed`。五者都是「跨节点可变状态」，
  从 `AwakenDeps` 直连字段改为运行时持有、节点经方法读写；`AwakenDeps` 其余字段
  暂不动，后续按同一判据逐片迁（下一个候选是 `trace`，但它是单写者、写完即落库，
  属节点局部，暂不迁）。`RetrievalState` / `CompactionSlot` 的定义也从
  `legacy_awaken::context` 下沉到 `graph_runtime.rs`，消除 runtime → legacy_awaken
  的反向依赖。
- **CompactionSlot / intent_seed 的读写口**：`compaction_slot()` / `set_compaction_slot()`
  （CompactNode 写 → Prepare / Business 读）；`intent_seed_slot()`（执行缝写回句柄，
  意图 domain 函数仍只认 `&Mutex`，保持通用）/ `take_intent_seed()`（Prepare 读走即清）
  / `intent_seed_messages()`（poll 偷看副本，不清空）。⚠️ `intent_seed_slot` 暴露句柄
  是执行缝回写口，不是节点直接读写共享数据，故不违反「节点不摸 Mutex」。
- ⚠️ `inbox::collect` 仍是独立模块（收剩余、返实际用量）；运行时只做共享数据的
  「取出来 → 传进去 → 收结果」，不把模块逻辑搬进图节点。
- ⚠️ **小脑判定 DAL 可替换**：`inbox::collect` 收可选 `judge_dal`，生产走
  `brain::dal()` 真实单例，集成测试经 `GraphRuntime.set_judge_dal` 注入脚本替身。
  原因：相关性判定若走真实单例，测试靠 provider `base_url` 残缺才没打网络——
  换配置就会等超时。替身无 weights 答案 ⇒ fail-open 放行全部候选。

---

## 三、载体一：`RoundCollected`（业务产出，走返回值）

```rust
// 图级运行时：GraphRuntime.collected: Arc<Mutex<RoundCollected>>
// 节点写入：dispatch 成功后 runtime.record_resources(produced)
//           （内部小循环无 runtime ⇒ 跳过写入，其产出本就不作交付物）
// 出口移交：runtime.take_collected() → LegacyAwakenOutcome.collected → AwakeningResult.collected
// 消费：consumer/message.rs 落库前查漏补缺
```

载体二 —— `RetrievalState`（检索 visited / discover 软预算）：

```text
节点 → runtime.queried_terms()             → 差集输入（锁中毒取旧值）
节点 → runtime.record_queried_terms(terms) → 真检索后追加新词
节点 → runtime.discovery_rounds()          → 软预算判据
节点 → runtime.note_discovery_round()      → discover 成功一轮后 +1
节点 → runtime.note_retrieve_search()      → 真检索次数（短路不计，返回累计值）
```

⚠️ `queried_terms`（查过哪些词）与 `retrieve_searches`（查了几次库）分工不可混：
「回环有没有真再查一遍」只能看次数——短路的空 pack 与「查了但零命中」形态相同，
靠 pack 反推会假绿。

- 与 `usage` / `progress` 同一范式：run 级、显式共享、跨周期累计。
- 新增一类业务事实 = **加一个字段 + 一处写入 + 一处读取**，不需要穿结构体。
- 为什么不用返回值直传：`run_think_loop` 的返回值有 4 个变体、调用方 3 处，
  而 `Arc` 句柄在 Err / 超时路径上也能保住已收集的部分。
  **当前边界（审核确认）**：句柄在这些路径确实没被 drop，但 `AwakeningResult`
  只在成功路径构造，`abort_summary` 不读 `collected` —— 即异常路径的产出
  **保住但不移交**，不会补进 final 引用。若将来要求异常也交付已产出资源，
  需在 abort 路径显式读同一 `Arc`；在那之前不要把「保住」理解成「已送达」。
- 为什么不能只走全局表：这是**用户可见交付物**，丢失 = 用户拿不到文件。
- **「产出了什么」也归执行层**：`collect_produced`（解析工具返回）在本模块，
  值对象 `ProducedResource` 在 `models::produced_resource`（跨层共用），
  消息域只留「怎么拼进正文」
  （`message::resource_refs::append_missing_resource_refs`）。
- **域间零调用**：值对象放任一域都会制造域间边（挂 runtime ⇒ `message → runtime`；
  挂 message ⇒ `runtime → message` 且执行层反向依赖消息协议），因此当前不存在
  `domain → domain` 的引用，两个域各自引用 `models`。

## 四、载体二：`TraceSink`（观测数据，写穿）

```rust
let scope = TraceSink::new(agent_id, run_id, scene)
    .with_scope(project_id, task_id);

scope.cerebellum(item);                       // 一条小脑留痕 = 一行
scope.round(THINK_ROUND_BLUEPRINT_ID, &r);    // 逐轮增量行
scope.fact("retrieval", json!({ … }));        // 将来任何观测品类
```

- 它是**身份信封**，不是收集器：不持有数据，只回答「这条事实属于哪次 run」。
- 数据产出即落盘，内存里不攒批 ⇒ 没有「谁来 flush」的问题，也没有残留。
- 构造点就是 run 入口（`run_think_loop` / 各节点的 `deps.trace_sink()`），所有权显式。
- **扩展方式**：`fact(名字, 载荷)` 写一行 `kind=fact, fact=<名字>, …载荷平铺`。
  旧读者忽略未知字段；将来消费侧在 `merge_into` 里按 `fact` 名追加即可，不需要 schema 迁移。

### 为什么落在 `runtime` 而不是 `pkg`

`think_trace`（含 sidecar 索引）原先在 `pkg`，2026-11-19 随本设计一起下沉到
`service/domain/runtime/`。判据是**谁拥有这份数据的语义**：

| | `pkg::jsonl` | `runtime::think_trace` |
|---|---|---|
| 内容 | 通用通道（按日切 JSONL、旁车索引） | 轨迹 schema（run / round / cerebellum / close 的语义与合并） |
| 改动理由 | 存储机制变化 | 观测品类 / 合并规则变化 |
| 复用者 | 工具轨迹、记忆轨迹等多家 | 只有 Agent 运行时 |

「通用管道下沉 `pkg`、领域格式留域内」这条在本仓是既有惯例。副产物：`pkg` 不再反向
依赖 domain（原先 `pkg::init_all` 要调 `think_trace::init`，现在由 `runtime::init`
在 `config` 就绪后注入基址，测试缺配置时保持 no-op）。同目录的
`think_trace_query` 继续负责查询作用域校验，`TraceScope` 这个名字归它
（写穿句柄叫 `TraceSink`，两者不再是同名异物）。

### 轨迹行的种类（`TraceLineKind`）

| kind | 语义 | 合并行为 |
|---|---|---|
| `legacy` | 存量行（无 `kind`） | 按 `partial` 推断是否收尾 |
| `round` | 逐轮增量 | 累积轮次，计入 `partial_round_count` |
| `close` | run 收尾 | 摘掉本片段增量轮次再拼接；`partial = false` |
| `fact` | 写穿观测事实 | 只追加自己的字段，**不碰**轮次与半成品标记 |

> ⚠️ 为什么不能用 `partial` 一职两用：写穿的事实行**根本不带** `partial` 字段
> （落盘只有 `kind=fact` + 载荷，反序列化拿到默认 `false`）。若拿它判两件事，既会把
> 「只写了入口门留痕、还没收尾」的 run 误判成已完成（事实行常常就是该 run 的首行，
> 且 `merge_into` 只在收尾行置 `false`、从不置 `true`，一旦判错整条 run 都翻不回来），
> 又会因收尾之后到达的事实行（chain_reply 留痕）把已完成轨迹重新标成「在跑」。
>
> 因此**收尾语义只由 `kind` 决定**（`TraceLineKind::is_close`）：合并入口按首行 `kind`
> 定片段的初始半成品状态，只有 `close` 能关闭它。`partial` 仅作为合并结果的对外读数。
>
> 扩展观测品类时注意：`fact` 的载荷会平铺到行上，因此身份字段（`run_id` / `agent_id` /
> `scene` / `project_id` / `task_id` / `created_at`）与**合并语义字段**（`partial` /
> `path` / `steps` / `advisor_calls` / `fingerprint` / `rounds` / `occurrences` / `routes`）
> 都在保留键里，载荷撞上会被忽略；`cerebellum` 是刻意例外（留痕正是靠它落行）。

### 收尾行的覆盖：RAII 兜底

`close` 必须覆盖**所有**退出路径，否则只写了 `kind=fact` 的 run 会被查询侧
永久标成「进行中」（入口门事实行常常就是首行，`partial` 初值由首行 `kind`
决定）。图正常跑完由 `persist_trace` 写含完整轮次的 `close`；小脑 Direct 直回、
零可用轮次、预算耗尽、LLM 错误预算耗尽、外部 timeout 等路径由
`TraceFinishGuard`（Drop 时调 `TraceSink::close_once`）补一行最小 `close`，
带 `close_reason`、空轮次列表。两者共享 `TraceSink.closed` 位，**只写一次**。
新增早退分支时不要手写 close，guard 会兜底；只需 `set_reason` 说明原因。

### 同步写的约束

`TraceSink::round` 刻意同步写盘：JSONL 是 append-only，行序即轮次序，
`merge_into` 的「增量行累积 → 收尾行替换」完全依赖顺序。改为后台队列会让后一轮
先落盘、轨迹错序。单次小写入的阻塞代价，远小于错序的排障代价。

---

## 五、迁移记录与边界

- `CEREBELLUM_BUFFER` / `record_cerebellum` / `take_cerebellum` /
  `flush_cerebellum_patch` 全部删除；chain_reply 的「判完补一行补丁」时序前提消失。
- `PRODUCED_BUFFER` / `record_produced` / `take_produced` 全部删除；
  消费侧直接读 `awaken_result.collected.resources`。
- 测试侧：`TraceSink::memory(...)` 把写穿重定向到内存，
  用例不再依赖全局落盘基址（`OnceLock` 进程内只能 init 一次），可并行。
- 已知边界：
  - 每唤醒会多出 3–8 行事实行（入口门 / 强度 / 精选），查询合并后仍是一条轨迹，
    但 JSONL 行数与旁车索引行数会相应增加；合并结果的 `created_at` 取**首行**
    （即 run 开始留痕的时刻，而非收尾时刻），列表排序据此；
  - 一次唤醒仍按 `trace_ids.first()` 取 run（当前恒为 1 个），
    多 run 场景需要遍历 `trace_ids` 逐个消费；
  - `awakening.rs` 之外若新增 awaken 入口，必须同步构造 `TraceSink` 与 `collected`。
  - **轨迹 schema 不回滚兼容**：删表前的旧二进制用 `!partial` 判收尾，会把新格式的
    事实行（无 `partial` 字段）当成收尾行，从而摘一次增量轮次。回滚只能整版本回滚，
    且回滚后新产生的事实行留痕在旧读者眼里不可见（观测数据，不影响业务交付）。
