---
kind: 设计决策
module: 02-Agent运行时
name: cerebellum_decision_strategy_design
title: 决策型策略设计（策略引擎承载小脑场景）
status: 现行
scope:
- src/pkg/policy/mod.rs
- src/pkg/policy/builtin.rs
- common/src/api/cerebellum.rs
- src/service/dal/brain.rs
- src/service/domain/runtime/cerebellum_router.rs
- src/service/domain/runtime/think_loop.rs
related:
- 02-Agent运行时/A-设计/thinking_task_policy_engine_design.md
- 02-Agent运行时/A-设计/runtime_design.md
- 02-Agent运行时/C-知识/小脑快判断（System One）.md
---

# 决策型策略设计（策略引擎承载小脑场景）

> 🎯 **本文档定位**：设计意图权威。回答「需要外部决策模型的判断，怎么以策略的形态表达」。
> 现状以 `C-知识` 与代码为准；本文与代码冲突时，以代码为准并回头修正本文。
> 状态：新增（2026-11-01）。已落地「策略接口 + 组合 + 答案口」与「协议 DTO 迁 common + DAL `decide`」，**尚未接生产场景**（还没有任何决策策略挂进策略组，默认行为与改造前逐字节一致）。

## 一、问题与边界

### 1.1 策略是什么

策略引擎（`src/pkg/policy/`）里，一个策略 = **一个有约束的判断单元**：

- 输入：`Metrics`（本轮算子快照）；
- 输出：命中的策略 id 列表（空 = 未命中）；
- 附带元数据：`id` / `name` / `condition_desc` / `required_metrics`，供日志与前端展示；
- 可组合：`PolicyGroup` 的 And / Or 可任意嵌套。

它**不产生自由文本**，只产出结构化结论。这正是它和「大脑」的分工边界：常规思考是文本进文本出，交给大脑；只有需要给出**受约束的结论**的地方，才用策略表达。

### 1.2 现有策略的局限

9 个内置策略全是「能直接算」的判断：读 `Metrics` 里一个键，和阈值比一下。`Policy::evaluate` 是同步纯函数、零 I/O，这是整个引擎的设计前提。

但有一类判断算不出来，必须问外部决策模型（小脑）：给定 `state`（上下文）+ `questions`（题型 + 指令 + 判据），返回结构化结论（选哪个 / 打几分 / 是否相关 + 概率）。它带网络延迟、会超时、会失败、结果带不确定度。

今天这两类判断在同一处并存却互不知道：`run_think_loop` 入口硬编码调一次 `cerebellum_router`，紧接着跑 `policy_set!` 组装的 8 个同步策略。

### 1.3 本文不做的事

**不让策略引擎去「运行」小脑。** 把网关依赖注入每个策略、让策略自己发请求，代价是：

- 每个策略无法纯函数单测；
- `PolicyGroup` 的 And/Or 要重做一套异步递归；
- 策略从「契约」退化成「执行体」，不再能脱离小脑复用。

这里复用的是策略引擎的**策略部分**——元数据、命中语义、组合能力——而不是它的执行模型。执行只在编排层发生一次。

## 二、核心判断：答案作为 Metrics 的一部分

小脑结论的唯一去处是 `Metrics`。这是本文最关键的一条，后面所有接口都由它推出来。

```
① 同步遍历策略树 → 收集 prepare() 声明的决策请求
② 交给 BrainDal::decide 执行（唯一 async 点）
③ 答案写入 Metrics.decisions
④ 同步 evaluate(Metrics) → 决策策略与阈值策略走同一套判断路径
```

> ⚠️ **编排的落点分两处**：`retrieve` 节点内的记忆精选已经走通完整链路（`prepare → decide → with_decisions → pick_ranked`，默认关）；`think_loop` 的 `policy_exit` 附近仍未接线——那里要等真实小脑与第一个信号型策略（如 `CortexNeededPolicy`）一起做。所以 §3 / §5 描述的接口在**场景侧已跑通**，在**信号侧仍只是接口**。

之所以是「写进 Metrics」而不是「策略自己 await」，是因为：

| 方案 | 改动面 | 组合语义 | 纯函数性 |
|---|---|---|---|
| 策略内 `await`（async trait） | 改 `Policy`，9 个策略全动，And/Or 重做 | 需重写 | ❌ 测试要 async |
| 策略内 `RwLock` 缓存 | 不动 trait | 语义变形（evaluate 有隐藏状态） | ❌ |
| **答案写进 Metrics** | **只加一个默认空方法** | **天然正确** | **✅ 不变** |

第三条的关键收益：`Policy::evaluate(&Metrics) -> Vec<String>` 签名一个字都不用改，`PolicyGroup` 的 And/Or 合并逻辑零改动就对新策略生效——因为决策策略和阈值策略在**同一次 evaluate 里**被评估，命中列表本来就该一起合并。

## 三、接口设计

### 3.1 契约类型（`pkg/policy/decision/`）与决策分支（`pkg/policy`）

```rust
/// 一类决策请求：策略约定输入的唯一出口
pub struct DecisionRequest {
    pub scene: &'static str,                          // 计量与日志口径（如 "route"）
    pub state: serde_json::Value,                     // 上下文，轻量
    pub questions: BTreeMap<String, QuestionSpec>,    // 题 id → 题型/指令/判据
}

/// 题型（与协议三型一一对应，但不引用协议 DTO）
pub enum QuestionSpec {
    Pick { instructions: String, options: BTreeMap<String, String> },
    Grade { instructions: String, levels: Vec<String> },
    Presence { instructions: String },
}

/// 决策结论（引擎侧通用表示，不感知具体协议）
pub enum DecisionValue {
    Pick { chosen: String, confidence: f32, weights: BTreeMap<String, f32> },
    Grade { score: f32, confidence: f32 },
    Presence { value: f32 },
}
```

`pkg::policy::decision` 只依赖 serde（`Metrics` 在这里，它是「一次判断的输入事实桶」）。这几块类型只用 `String` / `f32` / `BTreeMap` / `serde_json::Value`，**保持这个纯粹性**：协议 DTO（`CerebellumRequest` / `CerebellumAnswer`）在 `common/src/api/cerebellum.rs`（跨进程线格式，前端可复用），转换在 DAL 做。`pkg::policy` 通过 `pub use` 转发这些名字，外部引用路径不变。

> 📌 **为什么不开独立的 `pkg::decision` 包**：`Metrics` 本来就是策略的输入桶（`Policy::evaluate(&Metrics)`），决策契约也只有策略与它的执行方（DAL）在认。拆成同级包会让同一个概念分居两个目录，并多一条 `policy → decision` 的包内依赖；合并为子模块后对外仍是 `pkg::policy::{DecisionRequest, Metrics}`，DAL 一行不改。

场景抽象**不是**独立 trait，而是策略树上的一个分支（定义在 `pkg::policy`，因为它要引用 `Policy`）：

```rust
/// 决策型策略：通用策略在「需要外部决策」这一方向上的分支
pub trait DecisionPolicy: Policy {
    // === 场景必须实现 ===
    fn scene(&self) -> &'static str;
    fn request(&self, metrics: &Metrics) -> Option<DecisionRequest>;   // 问什么
    fn judge(&self, metrics: &Metrics, answers: &DecisionAnswers) -> Vec<String>; // 算不算命中

    // === 通用默认实现 ===
    fn qid(&self, name: &str) -> String { format!("{}.{}", self.scene(), name) }
    fn primary_qid(&self) -> String { self.qid("primary") }
    fn decision_prepare(&self, metrics: &Metrics) -> Vec<DecisionRequest> { self.request(metrics).into_iter().collect() }
    fn decision_evaluate(&self, metrics: &Metrics) -> Vec<String> { /* 无答案 → 未命中，有答案 → judge */ }
    fn pick_ranked(&self, metrics: &Metrics, options: &[String], threshold: f32, keep: usize) -> Option<Vec<String>>;
}
```

所有「要问外部」的策略都实现这一个 trait，因此它们：

- 能被 `PolicyGroup` 当普通 `Policy` 组合（继承关系在，And/Or 语义不变）；
- 能被编排层用同一套 `prepare → decide → evaluate` 驱动，不需要按消费形态分派；
- 只在三个点上各自发挥，其余（请求摊平、缺答案语义、按权重重排）共享一份实现。

> 📌 为什么不设成「两个互不继承的 trait」（信号型 / 取值型）：那等于在策略树上开了两条没关系的分支，同一个场景（如检索精选：既要判「答案可用」又要拿权重筛数据）会被劈成两半写，编排层还得知道该走哪条。**统一在 `DecisionPolicy` 下，信号与取值只是它的两个出口**：`judge` 出信号，`pick_ranked` 出保留集。
>
> 📌 **依赖方向**：`pkg::policy::decision` 零内部依赖（只 serde），`DAL → pkg::policy`（只取数据契约，不取 trait）。`DecisionPolicy: Policy` 只影响**要提问的策略**，既有的 18 个本地 `impl Policy` 一行不改。
>
> 📌 契约不放 `common`：它是**进程内**抽象（`Pick/Grade/Presence` 只有引擎认识），进 `common` 会把引擎绑死在 common 版本上。


### 3.2 Metrics 增加答案口

```rust
pub struct Metrics {
    data: HashMap<String, serde_json::Value>,
    decisions: Option<Arc<DecisionAnswers>>,   // 新增；Default = None（等价于「没有决策可用」）
}

impl Metrics {
    pub fn with_decisions(self, answers: Arc<DecisionAnswers>) -> Self;
    pub fn decision(&self, qid: &str) -> Option<&DecisionValue>;
    pub fn has_decisions(&self) -> bool;
}
```

`decision(qid)` 返回 `Option`：**没问过 / 问了但答案缺失 / 形态不符，一律是 `None`**，由策略自己决定 `None` 怎么处理——但见 3.4 的红线。

### 3.3 两种落地方式：形态实现类（首选）或自己实现

策略层不该只给骨架。业务决策的**输入输出是确定的**，就那几种形态，
所以 `pkg::policy::forms` 直接给实现类：

| 形态 | 题型 | 业务提供 | 结论 |
|---|---|---|---|
| `RankPolicy` | `Pick`（全选项权重） | 候选 `Candidate`（id + 判据文本） | 有序保留集 `Vec<String>` |
| `PickOnePolicy` | `Pick` | 固定选项 + 上下文 | `PickOne`（选中项 + 置信度） |
| `ScorePolicy` | `Grade` | 等级表 + 上下文 | `Grade`（分数 + 置信度） |
| `PresencePolicy` | `Presence` | 上下文 | 0~1 概率 |

- **排序**与**多选**是同一个形态：排序后按阈值截断即多选（`RankPolicy::ranked`）；
- **单选**不是排序特例的代码复用，而是消费方式不同——只要选中项 + 置信度门槛，不要全序；
- **分级**用于强度/质量这类连续判断，**概率**用于「是不是/要不要」这类二值判断。

业务只构造实例，**不写任何 `impl`**：

```rust
let policy = PickOnePolicy::new(
    "route", "小脑路由", "首轮快速判断是否需要完整思考",
    "根据用户消息判断应走模板直回还是完整思考主循环",
    [(CHOICE_TRIVIAL_DIRECT, "琐碎，可模板直回"), (CHOICE_CORTEX_NEEDED, "需要完整思考")],
    0.85,                                    // 置信度门槛
).with_state_keys(&["agent_id", "agent_name", "last_user_message"]);
```

需要后处理（把选中项翻成文案、把 id 映射回实体）时，读 `Metrics` 即可——
route 就是这么做的；需要非同寻常的问法（多题、动态题型）才自己实现 `DecisionPolicy`：

```rust
// ① 通用描述 + 两行骨架转发
impl Policy for MyPolicy {
    fn id(&self) -> &str { "my_scene" }
    fn name(&self) -> &str { "……" }
    fn condition_desc(&self) -> &str { "……" }
    fn required_metrics(&self) -> Vec<String> { vec!["……".into()] }

    fn prepare(&self, m: &Metrics) -> Vec<DecisionRequest> { self.decision_prepare(m) }
    fn evaluate(&self, m: &Metrics) -> Vec<String> { self.decision_evaluate(m) }
}

// ② 场景只填三个点
impl DecisionPolicy for MyPolicy {
    fn scene(&self) -> &'static str { "my_scene" }
    fn request(&self, m: &Metrics) -> Option<DecisionRequest> { /* 构造请求 */ }
    fn judge(&self, _m: &Metrics, a: &DecisionAnswers) -> Vec<String> { /* 答案可用即命中 */ }
}
```

> 📌 两行转发是 Rust trait 继承的语法限制（子 trait 的默认体不会被 `impl Policy` 采用），不是设计冗余。用形态类就不必写这两行。

形态类之上还有三个**结论读取器**（`DecisionPolicy` 默认方法），自定义场景也能用：
`pick_ranked`（全序 + 阈值 + 上限）、`pick_one`（单选 + 置信门槛）、
`grade`（分数 + 置信门槛）、`presence`（0~1 概率）。全部是「没有结论就返回 `None`」，
不允许压成 `false` / `0`。

`evaluate` 恒返回空不是瑕疵，是**显式声明「本地不可判」**。决策策略的命中在下面的 `interpret` 里。

### 3.4 决策策略仍要实现 `evaluate`

决策结论必须在 `evaluate` 里被消费（而不是另开一个 `interpret` 接口），否则 `PolicyGroup` 的 And 语义就失效——「本地策略命中但决策未命中」时，And 组必须整体未命中，而这件事只有在同一次 `evaluate` 里才能判定。

因此决策策略的 `evaluate` 形态固定为：

```rust
fn evaluate(&self, metrics: &Metrics) -> Vec<String> {
    match metrics.decision("route") {
        Some(DecisionValue::Pick { chosen, confidence, .. })
            if *confidence >= THRESHOLD && self.accepts(chosen) => vec![self.id().to_string()],
        _ => vec![],
    }
}
```

⚠️ **红线：`None` 一律视为「未命中」，绝不能视为「判断为否」。** 用 `unwrap_or(false)` / `unwrap_or_default()` 把「没问出来」压成「否」，是这类系统最典型的静默错误（与 `tool-args-null-silent-degrade` 同形）。拒绝语义与降级语义必须在日志里可区分。

### 3.5 题型选型

| 形态 | 题型 | 适用 | 注意 |
|---|---|---|---|
| `Pick` | `choice` | 「下一步走哪个节点」「选哪个工具」 | 选项 1~255 |
| `Grade` | `score` | 「相关性打几级」 | 2~10 级，分数可落级间 |
| `Presence` | `noul` | 「这条产物相关吗」 | 0~1 概率，**无 confidence** |

三条边界：

1. **要「带置信度的二值判断」，用 `Pick` 不用 `Presence`**。`noul` 只给概率没有 confidence，无法套统一阈值；用 `choice` 给两个选项即可拿到 `choice` + `confidence` + `probabilities`。
2. **协议没有「排序」题型**。排序只能是复合策略：对 N 个候选各发一个 `Grade`（一次请求内并行），本地按分数排。成本随候选线性增长，候选数必须有硬上限（建议 ≤10），超限先本地粗筛。
3. **`Presence` 不得携带判据**（`criteria`），违反会让**整批请求**失败而不只是该题失败。

## 四、组合

`PolicyGroup::prepare` 在 And / Or 下都**收集全部子策略的请求**——每个子判断都需要自己的资料，收集阶段与关系无关：

```rust
fn prepare(&self, metrics: &Metrics) -> Vec<DecisionRequest> {
    self.policies.iter().flat_map(|p| p.prepare(metrics)).collect()
}
```

命中合并不需要任何改动：`PolicyGroup::evaluate` 本来就逐个调用子策略的 `evaluate`，决策策略的结论会自然汇入。

于是三种组合都成立：

| 组合 | 例子 | 说明 |
|---|---|---|
| 纯决策 | `OR { CortexNeededPolicy, ... }` | 多个决策点任一命中即触发 |
| 纯阈值 | `AND { MaxRoundsPolicy, TimeoutPolicy }` | 现有行为不变 |
| 混合 | `AND { FinalAnswerPolicy, RelevanceGatePolicy }` | 本地完成 + 决策确认都满足才退出 |

## 五、执行编排

唯一 async 点在 **DAL**，不是 trait 方法，也不是独立网关模块：

```rust
// src/service/dal/brain.rs
async fn decide(
    &self,
    ctx: RequestContext,
    brain: &Brain,                       // provider 从 brain.cerebellum 取
    requests: Vec<DecisionRequest>,      // 纯数据，不是策略集
) -> DecisionOutcome;                    // 永不 Err

pub struct DecisionOutcome {
    pub answers: DecisionAnswers,        // 空表 = 无结论
    pub degraded: Option<DegradedReason>,// None = 正常
    pub latency_ms: u64,
    pub usage: CerebellumUsage,
}
```

**为什么不返回 `Result`**：小脑是可选增强，失败必须收敛为 `degraded`，让调用方**在类型上就无法**用一个 `?` 把「没配小脑」升级成主流程故障。这与 `wake_brain` 里「小脑装配失败不能阻断唤醒」是同一条原则。

**为什么不用独立网关模块**：`BrainDal` 已有 `wake_brain`（装配 cerebellum provider）与 `think`（大脑调用），小脑调用放同一个门面才有单一的 provider 来源；单独建模块反而要再传一次 provider。

DAL 侧收敛的四道门 + 一件事：

| 门 | 判定 | 结果 |
|---|---|---|
| 1 | 请求为空（策略组本轮无决策诉求） | `degraded: None` + 空答案 —— **正常无操作，不是降级** |
| 2 | `brain.cerebellum` 为 None | `NoProvider` |
| 3 | `api_key` 为空 | `NoProvider`（不发请求） |
| 4 | DAO 单例未初始化 | `NoProvider`（`dao()` 是 `expect` panic） |
| 超时 | `provider.config.timeout_ms` 优先，缺省 800；整批共享一个预算 | `Timeout` |
| 其他错误 | 网络 / 协议 / 服务端 | `CallError` |

协议 DTO（`CerebellumQuestion` 等）**只出现在 DAL**：`DecisionRequest → CerebellumRequest` 与 `CerebellumAnswer → DecisionValue` 两个转换函数都是 `brain.rs` 内的私有函数，Domain 完全看不到协议类型。

`DegradedReason` 因此从 Domain 移到 `common/src/api/cerebellum.rs`（跨层共享、前端可展示），并把原先混在一起的 `NoCerebellum` 拆成 `NoProvider`（没配）与 `Disabled`（人为关掉）——排查时这两件事必须能分开看。

批量语义（实现细节，易被误读为「合并成一次 HTTP 调用」）：

- **逐条顺序发送**，一个 `DecisionRequest` = 一次 `POST /v1/systemone`。不同场景的 `state` 结构不同，跨场景合并在协议层没有干净的表示（`state` 是整批共享的单份上下文）。
- **超时预算是整批共享**的，不是每题一份：慢场景会挤掉后续场景的时间。多场景真正落地时应改为按题预算或并发发送，见 §八。
- **`questions` 为空**的请求会被跳过并告警——服务端会 400，而 client「任一题不合法即整批失败」，不清掉它会连累同批其它场景。
- **qid 命名空间是全局的**（`Metrics::decision(qid)` 不带 scene），两个场景撞同一个 qid 会互相覆盖，DAL 在覆盖时告警。

`think_loop::policy_exit` 的签名不变；但「调用前 metrics 已被填充」这件事**还没发生**，等第 5 步把编排接上才成立。

## 六、接线点

| 消费场景 | 题型 | 位置 |
|---|---|---|
| 检索结果精选（记忆相关性） | `Pick`（候选逐条为选项，权重即排序） | `retrieve` 节点，宽召回后阈值截断 |
| 琐碎请求直回 | `Pick`（trivial / needed） | think_loop 入口，改造现有 `cerebellum_router` |
| 图节点选路 | `Pick`（候选节点） | `RouteAdvisor`（arbiter L2），替换现在的空位 |
| 产物相关性 | `Grade` / `Presence` | 依赖产物判断的调用点 |
| 检索是否再挖 | `Presence` | `discover` 节点（现为一次完整 cortex 调用） |
| 记忆压缩阈值 | `Grade` | `CompactionThresholdPolicy` 的策略组替换位 |

`CompactionThresholdPolicy` 的注释早已预告这条路：「后续换『小脑』判断时只需替换策略组，调用方不动」——这正是决策型策略想要的性质。

## 七、实施顺序

1. ✅ 契约收敛：`pkg::policy::decision` 只留数据形状（`DecisionRequest` / `QuestionSpec` / `DecisionValue` / `DecisionAnswers` / `Metrics`）；策略分支 `DecisionPolicy: Policy` 落在 `pkg::policy`，把请求摊平、缺答案语义、按权重重排写成默认实现
2. ✅ 单测：组合语义（混合组 And/Or）、答案缺失走未命中、无请求时零开销、`qid` 前缀、`pick_ranked` 的阈值/上限/幻觉过滤/同分兜底
3. ✅ 协议 DTO（`cerebellum_types.rs` 205 行）迁入 `common/src/api/cerebellum.rs`；`DegradedReason` 同步迁移并拆分 `NoProvider` / `Disabled`
4. ✅ `BrainDal::think_fast` → `decide`（收 `DecisionRequest` 批次、出 `DecisionOutcome`、永不 Err）；`cerebellum_router` 退化为「构造一个 Pick 请求 + 解释结论」，行为等价
5. 🔶 场景策略逐个接入，每个场景独立开关，默认关
   - ✅ **第一个：检索结果精选**（`legacy_awaken/memory_relevance.rs`）。直接构造 `RankPolicy`（不写 impl），候选池 = 渲染预算 × 3（15 条，与开关无关），一次 `Pick` 排全序，`ranked()` 按阈值 + Top-K 截回 5 条；`enable_memory_rerank` 默认关，关闭/降级/无答案 → 按机械 Top-K 截断（不丢召回）。
   - ✅ **第二个：琐碎请求路由**（`cerebellum_router.rs`）。手搓的 `DecisionRequest` 换成 `PickOnePolicy`（选项 + 判据 + `state_keys` 都在形态里），router 只留三分类翻译；qid 从 `route` 变为 `route.primary`（前缀统一）。
   - ⬜ 形态覆盖度：`ScorePolicy` / `PresencePolicy` 已有实现与单测，但**尚无业务场景**；等「检索是否再挖」「回答质量分级」这类需求出现再接线，别为用而用。
6. ⬜ 补 `record_event!` 计量（`DecisionOutcome.usage` 已备好，尚未上报）

每步行为不变、可独立提交、可回滚。

## 八、未决

- **决策请求的频次控制**：`run_think_loop` 每轮都会评估策略，若 `prepare` 每轮都声明请求，一轮唤醒最多 8 次网络调用。短期靠决策策略自身限定触发条件（如只在首轮或特定信号后 `prepare`），长期可能需要 `Metrics` 携带「本唤醒已问过 N 次」的预算键。
- **场景 prompt（`instructions` / `criteria`）的存放**：先集中在策略实现里；组织级配置等真实需求出现再定，现在建表属过度设计。
- **真实端点未做连通性验证**（`cerebellum_types.rs` 已注明协议口径来自公开文档、未经实测），上述题型边界都待一次真实调用背书。
- **观测缺口**：`ThinkFastResult.usage` 现在只进日志、无 `record_event!`，多场景后成本不可见。DAL 是唯一执行点，计量应集中在那里（`DecisionOutcome.usage` 已备好）。
- **批量超时是共享预算**：`decide` 用一个 `timeout` 包住整批顺序调用，慢场景会挤掉后续场景。真实多场景落地前应改为「按题预算」或并发发送（并发要另配并发上限，否则打爆 provider）。
- **`DecisionRequest` 没有统一的构造期校验**：形态类各管一段（`RankPolicy` 候选 < 2 → `None`，`ScorePolicy` 等级不在 2~10 → `None`，`PickOnePolicy` 选项为空 → `None`）；但手写 `DecisionRequest` 仍可能造出空 `questions`（DAL 跳过）或越界 levels（client 整批拒）。等出现第二个手写场景时再抽统一 `build`。
- **qid 是全局命名空间**：`Metrics::decision(qid)` 不带 scene。`DecisionPolicy` 用 `qid()` / `primary_qid()` 统一加前缀（已落地的是 `memory_relevance.primary`），但这是 trait 的纪律不是类型保证——手写 `DecisionRequest.questions` 仍可能漏前缀。多场景后可考虑在 `DecisionAnswers` 里改为二级键。
- **检索精选的阈值尚未标定**：`WEIGHT_THRESHOLD = 0.08` 是按「prompt 预算不变」推的保守值，没有真实模型输出分布背书。接入真实小脑后应先用离线历史召回集跑一遍，看权重分布再定。
- **召回宽度与渲染预算解耦**：候选池 = `KNOWLEDGE_TOP_K × RECALL_WIDTH_FACTOR`（当前 5 × 3 = 15），与精选开关**无关**。召回是「便宜但漏一次就没了」的一侧，所以先宽召回、再筛；关掉精选只是少了语义筛选，不改变候选面。
  - ⚠️ `MemorySearch` 的 `top_k` 只管向量路，关键词（FTS）路的条数由 `filters.limit` 决定（DAO 缺省 50）。两个字段必须写同一个 `recall_width`，否则「宽召回 N 条」语义脱钩。
  - **后续优化方向（暂不做）**：交叉召回——用向量路命中的节点去补关键词路、用关键词命中的节点文本去补向量路，把两路的候选面互相扩张。它的前提是「宽召回 + 精选」这条路已经跑通；现在 15 条的小候选池上，交叉召回换来的增量大概率抵不过多一次查询 + 一次 embed 的复杂度。
- **`filters.limit` 只截实体、不截边**：混合搜索里关系边是节点的派生视图，且图批次不变式要求边两端同在批内。若边参与名额争夺，它按排序键恒排最后，会被整批砍掉（`apply_entity_limit` 就是为此存在）。
