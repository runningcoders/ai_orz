# 思考生命周期设计（Think Lifecycle）

> 定位：把「Agent 怎么想一次」从散落在 `awakening` / `think_loop` / `consumer` 三处的平铺代码，
> 收敛为一个**图引擎**：节点 + 边 + 策略随配置组合，一次运行 = 一个独一无二的执行实例。
>
> 本文描述**理想态**；当前实现作为第一个落地蓝图 `legacy_awaken`，行为等价、可灰度回退。

---

## 一、为什么需要它

现状（详见 `src/service/domain/runtime/awakening.rs` 与 `think_loop.rs`）：

| 问题 | 证据 |
|------|------|
| 主流程平铺 650 行 | `awakening.rs#L429-1081` 的 `awaken` |
| 三层循环嵌套，无阶段概念 | 压缩外循环 `awakening.rs#L558` → 思考内循环 `think_loop.rs#L366` → LLM 重试内内循环 `#L388` |
| 快的排在慢的后面 | 小脑快判断在 `think_loop.rs#L297-343`，但 93 行上下文装配（`awakening.rs#L583-675`）**已经跑完了** |
| 循环状态是局部变量网 | `messages` + 7 个计数器散在 `think_loop.rs#L293-364`，无法被任何外部节点读写 |
| 收尾散落 4 文件 9 处 | trace 写库、记忆沉淀、事件发布、统计上报各写各的 |
| 加一个「反射弧」要改主函数体 | 无插入点，只能继续往 650 行里塞 |

结论：**退出决策已可插拔（`pkg/policy`），思考流程本身不可插拔。** 本设计补的是后者。

---

## 二、概念模型

### 2.1 三层响应（响应成本阶梯）

| 层 | 机制 | 延迟 | 是否调 LLM | 现状 |
|----|------|------|-----------|------|
| 反射弧 Reflex | 规则 / 关键词 / 缓存命中 / 本地向量相似度 | 毫秒 | **否** | 缺失 |
| 小脑 Cerebellum | 一次轻量 LLM（System One） | ~800ms | 是（轻量） | 已有 `cerebellum_router` |
| 大脑 Cortex | 完整多轮循环 + 工具 | 秒~分钟 | 是（完整） | 已有 `run_think_loop` |

**硬边界**：反射弧**零 LLM**。一旦反射也调 LLM，它就退化成第二个小脑，三层塌成两层。
三层的正交性在于「确定性 → 轻量概率 → 完整概率」——只有最底层能做到真正不占用大脑。

### 2.2 图引擎：节点 / 边 / 信号

流程不是管道，是**图**。核心切分：

> **节点只产出 Signal，不知道下一跳是谁。跳转由边决定。**

这条是「可组合」的实质：改连接关系不需要动节点代码。
（早期设计中节点直接返回跳转目标 `Short(target)`，已废弃——它把拓扑硬编码进了节点。）

```rust
pub trait ThinkNode: Send + Sync + Debug {
    fn id(&self) -> NodeId;
    /// 阶段标签：只用于契约分组与可视化，不构成顺序约束
    fn phase(&self) -> PhaseId;
    /// 读写契约（见 §四）
    fn contract(&self) -> NodeContract;
    async fn run(&self, bus: &mut ThinkBus) -> Result<NodeSignal>;
}

/// 节点产出：只表达「我发生了什么」，不表达「接下来去哪」
pub enum NodeSignal {
    Done,
    Yield(SignalTag),           // 如 overflow / need_compact / reflex_hit
    Fail(ThinkAbort),
}

pub struct ThinkEdge {
    pub from: NodeId,
    pub matcher: SignalMatcher,             // 匹配哪个 signal
    pub candidates: Vec<NodeId>,            // 候选目标，运行时由 guard 选（见 §2.5）
    pub guard: Option<Arc<dyn Policy>>,     // 边策略：路由条件（见 §五）
}
```

> 单候选时 `candidates` 退化为确定跳转，等价于普通 DAG —— **静态是动态的特例**。

**阶段（PhaseId）只是标签。** 它的作用是约束「同类节点的输入输出契约」与可视化分组，
**不决定执行顺序**——顺序完全由边决定。

### 2.3 蓝图 vs 实例

```
Blueprint（静态 · 可序列化 · 可复用 · 无行为）
    │  instantiate(run_id)
    ▼
Instance（独一无二 · 数据总线 + 执行轨迹 + 预算消耗）
```

```rust
/// 蓝图：可持久化、可版本化、可按名加载。不含任何行为。
#[derive(Serialize, Deserialize)]
pub struct ThinkPathBlueprint {
    pub id: String,
    pub version: u32,
    pub nodes: Vec<NodeSpec>,      // 节点 id + phase + 策略参数
    pub edges: Vec<EdgeSpec>,      // from / matcher / to / guard 参数
    pub limits: PathLimits,        // max_steps / timeout / retry_budget
}

/// 实例：一次运行的运行时实体
pub struct ThinkRun {
    pub run_id: String,
    /// 蓝图不可变快照：蓝图改了，存量实例仍按当时版本可回放
    pub blueprint: Arc<ThinkPathBlueprint>,
    /// 刺激：入站消息携带，随 run 构造即存在，零 I/O（见 §3.1）
    pub stimulus: Stimulus,
    pub bus: ThinkBus,             // 数据总线（见 §三）
    pub trace: RunTrace,           // 执行轨迹（见 §六）
    pub started_at: Instant,
}
```

**实例的三个工程价值**：

1. **trace 收敛**：现在 trace 写了四份（`MemoryTrace` / `ThinkLoopProgress` / 4 处事件 / 5 处统计）。
   实例对象化后 trace 是它的**投影**。
   ——`ThinkLoopProgress` 存在的根因注释（`types.rs#L14-18`：「`messages` 是局部变量，
   Err / 超时路径拿不到」）在实例模型下自然消解。
2. **可回放**：`(节点, 输入快照, 输出快照, signal)` 序列支持重放。
3. **Reflex / Review 的前置条件**：事后复盘的输入就是实例档案。没有它，大脑无从知道自己「反射」了什么。

### 2.4 Builder

```rust
let bp = ThinkPathBuilder::new("legacy_awaken").version(1)
    .node(PrepareNode).node(DeliberateNode).node(ConsolidateNode)
    .edge(Prepare, Deliberate).on(Done)
    .edge(Deliberate, Consolidate).on(Done)
    .edge(Deliberate, Prepare).on(Signal("overflow")).guard(CompactGuard)
    .limits(PathLimits { max_steps: 64, timeout_secs: 0, retry_budget: 3 })
    .build()?;
```

### 2.5 图是活的：中断 / 动态路由 / 演化

> **现实**：人的思考不是照图走一遍。可能中途被打断，带着当前上下文跳到别处；
> 下一步去哪常常是临时定的；同一个「小脑」第二次进入时，上下文与时序已不同——
> 它不是简单回溯，而是一次**新的出现**。
>
> 大多数流程是明确的（模板），但要逼近理想的思考链路，必须支持**在模板上的动态偏移**。

#### ① 节点定义 vs 节点出现（NodeOccurrence）

区分二者，是「复用」与「每次都不同」的唯一调和方式：

```rust
pub struct NodeOccurrence {
    pub def: NodeId,        // 节点定义：静态、可复用、可测试
    pub seq: usize,         // 本次 run 内该定义的第几次出现
    pub entered_at: Instant,
    pub input_depth: Vec<(ContextKey, LoadDepth)>,   // 进入时的上下文档位快照
}
```

轨迹记的是 occurrence 序列，不是节点序列：

```
Triage#1 → Prepare#1 → Deliberate#1 → Triage#2 → Consolidate#1
```

⇒ 「小脑第二次介入」不需要新建节点定义，它是 `Triage#2`——
**定义可复用，出现是新的**。这比「运行时动态创建节点」简单一个量级，且保持可测试。

#### ② 两类边

| 类型 | 触发 | 目标如何确定 |
|------|------|-------------|
| **节点边** | 节点正常返回 signal | guard 运行时从**候选集**中选 |
| **中断边** | 外部抢占（新消息 / 取消 / 超时 / 预算耗尽） | 由中断处理器决定，与当前节点无关 |

```rust
pub struct ThinkEdge {
    pub from: NodeId,
    pub matcher: SignalMatcher,
    /// 候选目标：运行时由 guard 决定走哪个（节点仍不知道下一跳）
    pub candidates: Vec<NodeId>,
    pub guard: Option<Arc<dyn Policy>>,
}

pub struct InterruptEdge {
    pub on: InterruptKind,          // NewStimulus / UserCancel / Timeout / BudgetExhausted
    pub to: NodeId,
    pub carry: CarryPolicy,         // 带什么过去
}

pub enum CarryPolicy {
    Full,                           // 整条总线
    Only(Vec<ContextKey>),          // 只带指定分区（反射式跳转常用）
    Snapshot,                       // 转不可变快照，防后续节点污染前序上下文
}
```

节点执行必须可被中断（tokio `select!` + cancel token），中断后总线状态**保留**——
这正是「带着上下文走到下一个节点」的落地。

#### ③ 演化的三个强度

| 强度 | 含义 | 可预测性 | 建议 |
|------|------|---------|------|
| **参数级** | 调边 guard 的阈值 | 高 | 一期做 |
| **路径级** | 从预置候选分支里选（candidates） | 中 | 一期做 |
| **结构级** | 运行时加边 / 加节点 / 改目标 | **低** | 只留接口，不做实现 |

**立场：不做「运行时自动改写蓝图」。** 运行时自修改 = 不可预测、不可测试、不可回滚；
同样的效果可以由「运行时受控选择 + 离线依据统计反馈优化蓝图」达成，且安全得多。

若未来确需结构级变更，必须配套：

- **图变更日志**：任何 `GraphMutation`（AddEdge / Retarget / SetGuard）写入 `RunTrace`，
  否则**可回放性直接失效**——你不知道当时跑的是哪张图。
- **路径指纹**：run 结束时记录实际走过的 occurrence 序列并聚合统计，
  才能回答「最近 100 次 awaken 里有多少走了 `Triage#2`」。
  动态性越强，这个越不是可选项。

---

## 三、数据总线与统一上下文加载

> **设计诉求**：业务上下文（记忆 / 工具 / 技能 / 项目 / 任务 / 画像 / 资源 / 提示词）
> 由**统一出口**读取并写入数据总线，后续节点与边策略从总线消费，各自不再重复取数。

### 3.1 分区键

总线按 `ContextKey` 分区，防上帝对象：

```rust
pub enum ContextKey {
    Memory,      // 短期 / 长期记忆检索结果
    Tools,       // 场景过滤后的工具描述符
    Skills,      // 技能副本
    Runtime,     // 工作空间路径 / 资源清单
    Business,    // project / task / user_profile
    Prompt,      // 已装配的 initial_messages
    Budget,      // 预算与计数
    Artifacts,   // trace_ids / raw_input / raw_output / exit_reason
}
```

> ⚠️ **刺激（Stimulus）不在 `ContextKey` 里。**
> 它是 run 的**构造参数**（`run.stimulus`），由入站消息直接携带，零 I/O、零装配。
> 区分二者的意义：反射弧要读的是**刺激**，不是**业务上下文**——见 §3.3。

### 3.2 统一供给器

上下文加载**不是普通节点**，而是总线的按需供给层：

```rust
#[async_trait]
pub trait ContextProvider: Send + Sync + Debug {
    fn key(&self) -> ContextKey;
    /// depth 见 §3.4：同一 key 可按轻量/完整两档取数
    async fn load(&self, run: &ThinkRun, depth: LoadDepth) -> Result<LoadedValue>;
    /// 取数失败语义：可降级（记 warn，填空）还是硬失败（中断 run）
    fn on_failure(&self) -> LoadFailureMode;   // Degrade | HardFail
}
```

当前 `awakening.rs#L583-675` 那 93 行取数 + 装配逻辑，整体收敛为若干个 `ContextProvider`
（记忆 / 工具 / 技能 / 工作空间 / 业务实体 / Prompt）。

### 3.3 ⚠️ 惰性加载（关键：否则反射弧短路失效）

**问题**：若上下文必须在一个前置节点里全量读完，那么反射弧命中时那一大坨业务数据已经白读了
——这正是当前「小脑快判断排在 93 行装配之后」（`think_loop.rs#L297-343`）的老毛病，换个形式复发。

**解法**：节点**声明**自己依赖哪些 key，引擎在执行前按需确保已加载（幂等）：

```rust
async fn ensure_loaded(run: &mut ThinkRun, wants: &[(ContextKey, LoadDepth)]) -> Result<()> {
    for (k, depth) in wants {
        // 已达成的深度 ≥ 请求深度则跳过；否则重新加载并抬升档位
        if run.bus.depth_of(k) >= *depth { continue; }
        run.bus.insert(k, *depth, run.providers.load(k, run, *depth).await?);
    }
    Ok(())
}
```

效果：

- **反射节点 `requires: []`：零取数，但不是零输入。**
  它读的是 `run.stimulus`（消息原文，已在内存里，零 I/O），
  而不是「记忆 / 工具 / 项目 / 画像」这些**需要 I/O 才能拿到**的业务上下文。
  生物学上正好对应：膝跳反射的感受器信号是**局部、原始**的，不经过皮层准备。
  ⇒ 反射的「廉价」来自**不做取数**，不是来自「什么都不看」。
- 主路径节点 `requires: [Memory, Tools, Business, Prompt]` → 全量装配，行为与现状等价
- 边策略同样声明依赖，只读它需要的那一小块

### 3.4 加载深度分级

并非所有反射都是纯字面匹配——「老规矩」「照上次那样」这类需要**一点点**上下文才能判定。
若为此触发全量装配就得不偿失，故 Provider 支持深度分级：

```rust
pub enum LoadDepth {
    Shallow,   // 轻量：如最近 1-3 条记忆摘要，不做向量检索
    Full,      // 完整：如最近 20 条 + 语义检索 + 本体词表
}

#[async_trait]
pub trait ContextProvider {
    async fn load(&self, run: &ThinkRun, depth: LoadDepth) -> Result<LoadedValue>;
}
```

节点按档声明，总线按 `ContextKey` 记住**已达成的深度**，只有向更深的档位请求时才真正重新加载：

```rust
pub struct NodeContract {
    pub requires: Vec<(ContextKey, LoadDepth)>,
    pub reads:    Vec<ContextKey>,
    pub writes:   Vec<ContextKey>,
}
```

—— 这让「反射需要一点上下文」与「反射必须廉价」不再二选一。

### 3.5 失效与刷新

总线内默认是 **run 级快照**（run 内不重复取数）。节点若通过工具修改了业务实体，
须在契约里声明 `writes`（契约定义见 §3.4），触发对应 key 失效，后续 `requires` 时重新加载：
失效后深度归零，重新按声明档位加载。

> ⚠️ 不声明 `writes` 却实际修改了业务实体的节点，会让后续节点读到陈旧快照。
> debug 模式下总线应对「未声明的写入」fail fast。

---

## 四、节点契约

节点是通用的，但**不是无约束的**。契约是可扩展性的保证。引擎在每个节点执行前做三件事：

1. `ensure_loaded(contract.requires)`
2. 打点开始（节点 id + 输入 key 快照摘要）
3. 执行后校验 `writes` 声明，未声明的变更 fail fast（debug 模式）

**阶段标签的用途**：同 `phase` 的节点共享一组默认契约模板，新增节点实现时只需声明差异。
这让「阶段」从顺序约束变成**契约模板**——也正因如此，阶段不构成顺序。

---

## 五、策略二分

策略挂在两个正交的位置，**职责不重叠**：

| 位置 | 职责 | 约束 |
|------|------|------|
| **边策略（guard）** | **路由**：决定走哪条边 | 纯函数、同步、无副作用；吃总线只读快照 → bool |
| **节点策略** | **执行语义**：决定跑不跑、跑几次、超时怎么办 | 前置门闩 / 超时 / 重试 / 熔断 / 后置校验 |

**复用现有 `pkg/policy` 的 `Policy` trait 与 `policy_set!` 宏，不另起炉灶。**
它当前吃 `HashMap<String, Value>`（`pkg/policy/mod.rs#L90-135`），作为边 guard 的输入正好够用。

现有 8 个退出策略（取消 / Final / 溢出 / 轮次 / 超时 / 无进展 / token / LLM 错误）
在图上表达为：**`Deliberate` 节点的多条出边**，每条挂一个 guard。语义不变，形式升级。

### 5.1 仲裁三层与 LLM 顾问（只选不定）

只有节点池和数据总线还不够：候选多个时谁来决定？答案不是写一套通用规则引擎——
**我们本来就有大脑和小脑**，动态判断可以由它们承担。仲裁因此分三层，成本递增：

| 层 | 干什么 | 成本 |
|----|--------|------|
| **L0 合法化** | 按准入规则 + `ActivationBudget` 算出**合法候选集** | 零 |
| **L1 唯一化** | 候选唯一 → 直接选（覆盖绝大多数流转） | 零 |
| **L2 顾问** | 候选多个 → 构造 `RouterView` 问小脑（廉价）/ 大脑（昂贵） | 一次 LLM |

> 工程化一套通用规则引擎成本极高且难以覆盖长尾；而「给模型看什么」是可收敛的问题。
> 所以本设计真正的难点从「规则怎么写」变成 **§5.2 的 `RouterView` 契约**。

⚠️⚠️ **顾问只「选」，不「定」**：它只能在 L0 给出的合法候选集里挑，
返回越界节点或调用失败一律降级为确定性选择（`RouteVia::Fallback`）。
**终止性因此仍由 `ActivationBudget` 保证，不交给概率**——否则「图 + LLM」就是
一个无法证明会停的东西。

**协议层护栏**：`RouteAdvice.choose` 必须以候选 id 的 **schema enum** 下发，
不允许模型自由输出字符串。从协议层杜绝非法选择，别靠事后校验兜底
（与 `function.name` LCD 红线同源：模型侧的宽容是运气，不是契约）。

**成本护栏**：`PathLimits.advisor_calls_budget` 限制单次 run 的顾问调用次数。
每步都问模型 = 每步 +800ms，一轮思考十步就不可接受。

### 5.2 `RouterView`：给大小脑看什么

这张表就是「引擎数据流转是否足够全面顺滑」的落地。它同时决定两件事：
顾问能做出多好的判断，以及每次选路要烧多少 token。

```rust
pub struct RouterView {
    pub stimulus_brief: String,               // 刺激摘要（截断，非全量）
    pub step: usize, pub steps_left: usize,   // 进度与剩余预算
    pub visited: Vec<String>,                 // occurrence 序列 Triage#1>Prepare#1>...
    pub bus_brief: Vec<(String, String)>,     // 各分区**截断**摘要
    pub candidates: Vec<CandidateBrief>,      // id / phase / purpose / why_eligible
    pub last_signal: Option<String>,
}
```

只给摘要不给全量：分区摘要由 `ThinkBus::summary(key, max_chars)` 截断，
刺激文本同样截断。**全量上下文塞进路由判断是成本灾难，也是注意力稀释。**

### 5.3 准入条件 = 策略集（复用 `pkg::policy`）

§五开头说「复用 `pkg/policy`，不另起炉灶」，落地方式如下：
**准入规则是声明（`ActivationRule`），运行期判定编译成 `Policy`。**

```rust
// 声明（可读、可覆盖、可序列化）
pub struct ActivationRule {
    pub after: Vec<NodeId>,      // 允许的前驱
    pub on:    Vec<String>,      // 允许的前驱信号
    pub when:  Vec<BusPredicate>,// 总线谓词
    pub priority: i32,
}

// 编译产物（NodePool::build 时定稿，运行期只求值）
compile_guard(node, rule, budget) -> ActivationGuard  // And(一组正向 Policy)
```

| 策略 | 算子 | 命中含义 |
|------|------|----------|
| `InSetPolicy::after` | `prev_node` | 前驱在允许集合内 |
| `InSetPolicy::signal` | `signal` | 前驱信号在允许集合内 |
| `BusPredicatePolicy` | `bus.<key>.loaded` / `.depth` / `flag.<key>#<ptr>` | 总线谓词成立 |
| `BudgetPolicy` | `activations` / `since_last` | **未**达上限且**不在**冷却期 |

⚠️ 语义约定：**准入策略一律正向表述**（命中 = 该条成立）。`pkg::policy` 没有 `Not`
组合子，所以「未达上限」「不在冷却」都写成正向条件，由 And 组合成整体准入。
不要让某条策略表达「拒绝」，否则 And 的语义会反直觉。

**算子按 `required_metrics()` 惰性投影**（`policy.rs::metrics_for`）：每次仲裁对每个
候选都要一张算子表，全量投影会随总线分区增长变贵；只投影被声明的 key，
成本与策略数量挂钩而非总线规模。

**收益不只是复用**：`Policy` 自带 `id / name / condition_desc`，
`condition_desc` 正是「为什么没走到这个节点」的答案。动态路由下可解释性只能运行时
给出，事后补不上——`ActivationGuard::rejections()` 逐条评估并返回未命中项的自然语言
描述，`CandidateBrief.why_eligible` 直接把准入描述喂给顾问。

---

## 六、执行引擎

```rust
pub struct ThinkPath {
    nodes: HashMap<NodeId, Arc<dyn ThinkNode>>,
    edges: Vec<ThinkEdge>,
    limits: PathLimits,
    providers: ContextProviderSet,
}

impl ThinkPath {
    pub fn instantiate(&self, run_id: String) -> ThinkRun { /* 快照蓝图版本 */ }

    pub async fn run(&self, run: &mut ThinkRun) -> Result<()> {
        let mut current = self.entry;
        let mut steps = 0usize;
        while let Some(node) = self.nodes.get(&current) {
            if steps >= self.limits.max_steps { return Err(err!(...)); }   // 硬护栏
            steps += 1;

            ensure_loaded(run, &node.contract().requires).await?;
            let occ = run.trace.enter(node.id());          // occurrence 序号

            // 节点执行可被中断：中断边优先于节点自身的返回
            let signal = tokio::select! {
                s = node.run(&mut run.bus) => s?,
                it = run.interrupts.recv() => {
                    let edge = self.resolve_interrupt(&it)?;
                    current = edge.apply(run, &mut run.bus)?;   // 按 CarryPolicy 携带上下文
                    continue;
                }
            };
            run.trace.leave(&occ, &signal);

            match self.resolve_edge(current, &signal, run)? {
                Some(next) => current = next,
                None => break,
            }
        }
        Ok(())
    }
}
```

> 中断后总线**保留**（不重置），这正是「带着上下文走到下一个节点」的落地。
> `CarryPolicy::Snapshot` 用于需要冻结前序上下文、防后续节点污染的场景。

**红线**：

1. 节点不持有跨节点可变状态 —— 全在数据总线。
2. 节点不知道后继；拓扑只在边里。
3. 任何节点的 `Fail` 统一进 `Consolidate` 的异常分支（复用现有 `abort_summary`），
   不允许节点自己吞错误。
4. 每节点进出打点 + 轨迹留痕 —— 流程变数据后这是唯一排障手段，**必须同步建**。
5. 图 + 回环必须靠 `PathLimits.max_steps` 硬性封顶，防死循环。
6. **图引擎不管并发**：实例 per-run 独占、单线程推进；Agent 级并发仍由 `try_set_busy` 状态机负责。
7. **任何运行时图变更必须写入 `RunTrace`**（`GraphMutation` 日志），否则可回放性失效——
   没有它就无法回答「当时跑的是哪张图」。见 §2.5③。

---

## 七、第一版落地：legacy_awaken

**目标：行为与当前 `awaken` 完全等价，只是换了个组织方式。** 不改语义、不加功能。

| 节点 | phase | 来源 |
|------|-------|------|
| `Prepare` | Prepare | 搬 `awakening.rs#L563-675`（93 行取数 + 装配 → 拆成 ContextProvider） |
| `Deliberate` | Deliberate | 搬 `think_loop.rs#L265-743`（内部保留 `pkg/policy` 策略组不动） |
| `Consolidate` | Consolidate | 搬 `awakening.rs#L971-1080` + 4 分支结果处理 `#L702-968` |

边：`Prepare → Deliberate`(Done)、`Deliberate → Consolidate`(Done)、
`Deliberate → Prepare`(Signal "overflow" + guard)，`retry_budget = 3`（现 `COMPACT_MAX_ROUNDS`）。

**待建节点（后续蓝图）**：`Reflex`、`Triage`（后者搬 `think_loop.rs#L297-343`，
搬到正确位置后自动修正「快判断排在慢准备之后」的错位）。

> `Stimulus` 通常**不是节点**：它是 run 的构造参数（§2.3），由入站消息直接填充。
> 只有当多渠道归一化逻辑复杂到需要独立编排时，才把它升级为节点——
> 且它的产出写入 `run.stimulus` 而非总线，仍不计入 `ContextKey`。

**反射弧在新模型下是一条独立蓝图**，不是同蓝图加节点：

```
reflex_fast:  Stimulus → Reflex → Consolidate
```

——正好落实「不同路径 = 不同的思考行为方式」。

**Deliver 故意不入图**：交付路由在 `consumer/message.rs#L865-989`，与 `try_set_busy` 抢占、
nack 重投耦合。run 结束时在 `bus.artifacts` 里产出**结构化交付意图**，仍由 consumer 执行。
边界：图管「想出什么」，consumer 管「怎么送」。

### 7.1 ⚠️⚠️ 信号 Tag 必须单一真相（否则边匹配静默失效）

把控制流从 `match` 搬成图的边，收益是「分支穷尽性变成可断言的边」；
但前提是**边的 `on` 与节点实际产出的那个 Tag 同源**。落地时踩到一次：

同一个 `"overflow"` / `"max_rounds"` / `"cancelled"` 曾同时存在于四处，

| 位置 | 作用 |
|------|------|
| `Deliberate` 内联 `match` 的 Arms | 产出 Tag |
| `blueprint.rs` 的 `EdgeSpec.on` | 匹配 Tag |
| `Artifacts.exit` 的字面量 | 语义标签，`Consolidate` 凭它分支 |
| 测试里的字面量数组 | 断言「这些 Tag 有边」 |

互不相干 ⇒ 改一处漏一处时**测试依旧全绿**，但运行时边匹配不上，
run 因无候选而**静默终止**：不写 Trace、不报统计、不发事件，日志里看不出来。
这正是「Match 漏一个分支就静默丢路径」那个老毛病换了新马甲。

处置：

1. 抽出纯函数 `map_think_result(ThinkLoopResult, ..) -> ExitMapping`，
   它是 Tag 的**唯一真相来源**；
2. Tag 与 exit 各自定义 `SIGNAL_*` / `EXIT_*` 常量，四处统一引用；
3. 测试**从真实映射穷举**（`sample_results` → `map_think_result` → `signal.tag()`），
   不再维护手写数组。

并且断言是**双向**的（`tests.rs`）：

- `every_deliberate_exit_signal_has_an_edge`：产出的 Tag 都得有边
- `every_deliberate_edge_signal_is_reachable`：蓝图声明的 Tag 都得有人产出
  （反过来同样危险：配置了一条永远走不到的路径）

> ⇒ 凡是「节点产出 Signal + 蓝图边匹配 Signal」的组合，
> **Tag 必须走同一个常量**，任何内联字面量都是隐患。

---

## 八、目录结构

```
src/service/domain/runtime/lifecycle/
├── mod.rs                ThinkPath / ThinkRun / ThinkBus / NodeSignal / NodeId
├── bus.rs                ThinkBus / ContextKey / ContextProvider / LoadFailureMode
├── node.rs               ThinkNode trait / NodeContract / PhaseId
├── edge.rs               ThinkEdge / SignalMatcher
├── engine.rs             执行引擎（ensure_loaded + 轨迹 + PathLimits 护栏）
├── blueprint.rs          ThinkPathBlueprint / ThinkPathBuilder / NodeRegistry
├── trace.rs              RunTrace（实例档案，可回放）
├── nodes/
│   ├── prepare.rs
│   ├── deliberate.rs
│   └── consolidate.rs
└── blueprints/
    └── legacy_awaken.rs  第一版蓝图装配（拓扑的唯一声明处）
```

落在 `src/service/domain/runtime/` 下，不新增顶层模块，符合现有分层。

---

## 九、迁移路线（每步独立可验证、可回退）

| 步 | 内容 | 验收 | 风险 |
|----|------|------|------|
| 1 | 定义 `ContextKey` / `ThinkBus` / `ThinkNode` / `NodeContract` 骨架，不接业务 | 编译通过，无调用点 | 无 |
| ~~1~~ | ✅ **已完成**（见 §十一）：骨架落在 `pkg/think_path/`，含仲裁三层、`RouterView` 与准入策略集，11 个单测通过 | — | — |
| 2 | 抽 `ThinkBudget`：`think_loop.rs#L354-364` 七个计数器对象化入总线 | 现有测试全绿，**零行为变化** | 低 |
| ~~3~~ | ✅ **已完成**：`legacy_awaken` 蓝图 5 节点 7 边（Prepare/Deliberate/Compact/ForceCompact/Consolidate），行为等价地委托 `run_think_loop` | — | — |
| ~~4~~ | ✅ **已完成（2026-10-22）**：`RuntimeAwakening::awaken` 主体换成图编排调用，老那条 650 行循环删除；`awaken` 只剩入参转换 + 结果回填 | 既有 4 个 `awaken` sqlx 用例（含 429 兜底存档）改走新链路仍全绿 | 见 §11.3 的取舍说明 |

### 9.1 测试现状（截至 first landing）

| 层 | 位置 | 用例数 | 覆盖什么 |
|----|------|--------|----------|
| 引擎机制 | `pkg/think_path/engine_tests.rs` | 5 | 线性流转 / 回环预算封顶 / 顾问选路与降级 / 惰性加载 |
| 策略算子 | `pkg/think_path/policy_tests.rs` | 6 | 准入三条 Policy 的算子语义 |
| 业务拓扑 | `runtime/legacy_awaken/tests.rs` | 9 | 蓝图 Shape + **Signal↔边双向一致** + 轮次累加 |
| 业务流转 | `runtime/legacy_awaken/integration_tests.rs` | 5 | 真装配 + 真收尾，四条退出路径 + 失败冒泡 |

### 9.2 集成测试的前提：`ThinkBackend` 这个缝

集成用例能成立，靠的是 `backend.rs` 抽出的注入缝 —— `Deliberate` / `Compact` /
`Consolidate` 里**唯二会触碰 LLM 的两个调用**（`think` / `compact`）走了 trait：

```rust
pub trait ThinkBackend: Send + Sync + Debug {
    async fn think(&self, deps: &AwakenDeps, input: ThinkInput) -> Result<ThinkLoopResult>;
    async fn compact(&self, deps: &AwakenDeps, input: CompactInput) -> Result<CompactOutcome>;
}
```

- `LegacyBackend`：转调 `RuntimeDomainImpl` 的老函数，**行为与 `awaken` 完全等价**
- 测试注入 `ScriptedBackend`：按队列返回预设的 `ThinkLoopResult`

三个理由，缺一不可：

1. **红线要求**：集成测试报 `connect error ... api.openai.com` 默认当真 bug。
   若裸调 `domain`，「测试通过」的含义就取决于本机能否连上 OpenAI —— 典型的假绿。
   实测判据：**5 个用例合计 1.1s**。真打模型会是几十秒。
2. **用退出信号驱动流转**，而不是靠构造特定 LLM 响应去「诱使」链路走到某个分支。
   前者测的是图，后者测的是模型。
3. 它同时是**后续把 480 行 `run_think_loop` 下沉成图内节点的替换点** ——
   届时只是多一个 `ThinkBackend` 实现，节点代码一行不用动。

⚠️ 装配（`Prepare`）与收尾（`Consolidate`）**刻意保持真实实现**：那两段是纯 I/O，
全桩掉了就退化成「第二个 `tests.rs`」。真跑起来的代价是 fixture 要完整 ——
注意用 `dao::init_all()` / `dal::init_all()` 而不是逐个补单例：
漏一个会在链路深处 panic（`unwrap on None`），报错位置与缺席的那个毫无关系。

✅ **已移植**：老链路那 4 个 DB 集成用例（`awakening_tests.rs`）的价值被覆盖，
方式不是改断言，而是把 Stub BrainDal 换成 **Scripted Deliberate** ——
用信号序列让 `overflow` / `max_rounds` / `cancelled` / `final` 各走一次，
再验 `Consolidate` 的收尾。其中 `overflow` 那条跑出
`prepare#1>deliberate#1>compact#1>prepare#2>deliberate#2>consolidate#1`，
正是旧代码里「溢出压缩后重开一轮」的等价表达。
| 4 | 把 `run_think_loop` 的 480 行逐块下沉成节点（小脑路由 / 工具二分 / 8 个退出策略） | 委托痕迹消失 | 中 |
| 4 | `awaken` 改为跑 run，旧实现保留一段时间做对照 | 集成测试全绿 | 中 |
| 5 | 把小脑从 `think_loop` 搬到 `Triage` 节点 | 小脑行为不变，但**顺序提前** | 中 |
| 6 | 加 `Stimulus` / `Reflex` 节点与 `reflex_fast` 蓝图 | 见 §十 | 高 |

**步 3 最大难点**：`awakening.rs#L702-968` 的 4 分支结果处理与压缩回环强耦合，
且依赖 `prompt` / `raw_output` 的 Rust 延迟初始化技巧（`#L541-542` 声明、`#L675` 和 4 个分支分别赋值）。
搬之前先把这两个变量改为 `Option` 显式初始化——「哪个分支赋过值」的真相目前被语言技巧掩盖着。

**前置清洁**：`awakening.rs#L5`、`mod.rs#L283`、`intent_analyze.rs#L1-3` 三处注释仍宣称
「两阶段唤醒 IntentAnalyze → Awaken」在跑，实际已移除（墓碑注释在 `awakening.rs#L546-556`，
已降级为 prompt 里的 `§0 审题 SOP`）。重构前必须先修正，否则新抽象会被旧概念污染。

**ID 统一**：实例引入 `run_id`，现有 `trace_id` 必须由其派生或作为关联字段，禁止各自生成。

---

## 十、未决问题（需拍板）

1. **Reflex 的触发源**：纯配置规则表，还是从历史 trace 自动沉淀命中模式？
   后者需要命中率反馈闭环，成本高一个量级。
2. **Reflex 短路与 busy 抢占**：反射在 Prepare 之前短路，`try_set_busy` 的 busy 会瞬间翻转，
   与并发唤醒、nack 重投如何相处？
3. **事后复盘（Review）**：是否作为**独立蓝图**（由 Deliver 投递事件触发）？
   反射产生的回复是既成事实（用户已看到），复盘只能追加、不能撤回——
   这与「Discard 路径消息救不回」的既有红线直接交互，需先定语义。
4. **总线持久化粒度**：热态全量 / 归档摘要 + 关键快照的分层阈值。
   现状参考：`ROUND_DIGEST_MAX_CONTENT = 800`（`think_loop.rs#L148`）的截断即因全量 clone 太贵。
5. **策略引擎接入层**：现在是 `HashMap<String, Value>` 弱类型，`ThinkBus` 分区后是否升级为强类型？
   建议二期做，一期保持兼容。
6. **动态演化的强度上限**（§2.5③）：一期只做参数级 + 路径级（候选分支），
   结构级（运行时改图）只留接口。是否接受这个边界？
   若要求结构级，必须先落地「图变更日志 + 路径指纹统计」两项前置能力。
7. **中断源清单**：除「新消息 / 用户取消 / 超时 / 预算耗尽」外，
   是否纳入「Agent 主动自我打断」（模型输出中途改主意）？这会显著增加语义复杂度。
8. **顾问的层次选择**：候选多个时默认问小脑还是大脑？建议默认小脑；
   仅当小脑连续 `Fallback` 或 run 已消耗过半预算时才升级大脑。待实测校准。
9. **选路结果缓存**：相同 `(visited 前缀, last_signal, 预算档位)` 是否命中缓存直接复用？
   能显著降本，但会让「同样的状态走出不同路径」变得难以解释，需先建路径指纹统计。

---

## 十一、新引擎并行建设策略（当前执行路线）

**决策：不改老链路，新建引擎，影子对比，逐步切换。**

老链路（`awakening.rs` 650 行 + `think_loop.rs` 480 行）是三层嵌套循环加局部变量网，
原地重构风险高、收益慢；新引擎独立建设可随时停、随时验、随时回退。

### 11.1 代码落位

**引擎放 `src/pkg/think_path/`（通用机制），业务放 `src/service/domain/runtime/`（后续）。**

理由是它不认识业务：`ThinkNode` / `ThinkBus` / 仲裁器 / 蓝图只按契约搬运
`serde_json::Value`，不关心「记忆 / 工具 / 技能」具体是什么。与 `pkg::aop`（事件引擎）、
`pkg::background_task`、`pkg::policy`（判定引擎）同类。后续 `sleep_and_settle` /
消息路由等非思考流程可复用同一套图引擎，而不必反向依赖 `domain/runtime`。

```
src/pkg/think_path/                    ← 通用引擎（零业务）
├── types.rs         NodeId / PhaseId / ContextKey / LoadDepth / Stimulus / NodeSignal
├── bus.rs           ThinkBus（分区 + 深度分级）+ ContextProvider / ProviderSet
├── node.rs          ThinkNode / NodeContract / ActivationRule / ActivationBudget / BusPredicate
├── strategy.rs      StrategyPatch / StrategyOverlay（元认知的受控出口）
├── policy.rs        ★ 准入策略集：把规则编译成 pkg::policy::Policy（见 §5.3）
├── blueprint.rs     ThinkPathBlueprint / NodeSpec / EdgeSpec / PathLimits / Builder
├── run.rs           ThinkRun / NodeOccurrence / RunTrace / RouteVia
├── arbiter.rs       NodePool（编译守卫）+ Arbiter 三层仲裁 + RouterView
├── engine.rs        ThinkPathEngine 主循环（ensure_loaded → run → 记 occurrence → 仲裁）
├── testkit.rs       测试夹具（占位节点 / 脚本化顾问 / 计数供给器）
├── engine_tests.rs  引擎行为测试
└── policy_tests.rs  准入策略语义测试
```

⚠️ **新引擎与 `think_loop` / `awakening` 零互相引用**，仅参考其逻辑点。
业务侧（具体节点 / 上下文供给器 / 顾问实现 / 蓝图常量）建在
`runtime/legacy_awaken/` 下，**单向依赖**本模块，反向禁止。

```
src/service/domain/runtime/legacy_awaken/   ← 第一个业务实例
├── context.rs     AwakenDeps + 4 个 ContextProvider + build_deps
├── nodes.rs       Prepare / Deliberate / Compact / Consolidate
├── blueprint.rs   legacy_awaken 蓝图（5 节点 7 边）
├── runner.rs      run_legacy_awaken / shadow_run + ShadowReport
└── tests.rs       拓扑不变量测试（6 个）
```

### 11.1.1 ⚠️ 数据总线的承载边界（落地实测结论）

总线只装 `serde_json::Value`，而 `Memory` / `Message` / `Agent` / `Project` / `Task`
这些业务实体**只有 `Debug + Clone`，没有 Serialize**。所以分工是：

| 类别 | 例子 | 去向 |
|------|------|------|
| 可无损 JSON 往返 | 工具描述符、技能、工作空间路径、记忆摘要、资源行、图像 parts、Prompt、初始消息序列 | **总线 + ContextProvider** |
| 不可序列化的实体 | `Vec<Memory>`（`builder.history()` 要的就是它）、Message、Agent、ThinkingOptions | **节点构造期注入的依赖**（`AwakenDeps`） |

推论：装配不是单一 Provider 的活。`ContextProvider` 只负责「一个分区的取数」，
**跨分区合成是节点的职责**——所以 `PrepareNode` 亲自取 `Vec<Memory>` 并装配 Prompt，
其余五个分区由 Provider 供给。（这与 §三「统一读上下文」不冲突：统一的是**出口**，
不是「一切都要经过总线」。）

### 11.1.2 `run_into`：异常中断也要拿得到现场

老链路为了解决「`messages` 是局部变量，`?` 一传播就没了」专门发明了
`ThinkLoopProgress`。实例对象化后这个问题的正解是**让调用方持有 run**：

```rust
pub async fn run_into(&self, run: &mut ThinkRun) -> Result<()>
```

节点报错时依旧返回 `Err`，但总线、轨迹、occurrence 序列都还在调用方手上。
所有需要「异常兜底存档」的调用方都应该用这个入口，不再另写一份进度快照。

### 11.2 三条已在代码中锁死的边界

1. **节点不知道后继**：`ActivationRule` 挂在 `to` 上，静态边 `from → to` 在
   `NodePool::build` 时**编译进**目标节点的 `after / on`。上层 builder 保留边语法糖
   （可做可达性 / 孤儿节点静态分析），底层统一跑准入仲裁——**静态是动态的特例**。
2. **节点只能提案改参数**：`NodeOutcome.patches: Vec<StrategyPatch>`，
   由引擎落到 `StrategyOverlay`。节点改不了规则结构。
   护栏：补丁把所有候选都禁用时，仲裁**忽略 `disabled` 覆盖**，避免「改主意」把 run 掐死。
3. **顾问只选不定**：见 §5.1。

### 11.3 切换方式：直接替换（实际采用）

**没有走影子双跑**，原因要在变更前说清楚，否则后来人会把这次的取舍当成疏忽：

- 图编排与老 `awaken` 是**同一批实现函数的重排**（`ThinkBackend` 仍委托老函数），
  双跑比的是同一份代码的两种调用方式，信息量很低；
- 仓库里已有 4 个 `awaken` 的 `#[sqlx::test]`（技能注入 / 无技能 / 用户画像 /
  429 兜底存档），它们天然就是这次替换的**回归基线** —— 改走新链路后仍全绿；
- 真正有风险的是那些**测试覆盖不到的口径**：统计标签、Trace 元数据、
  异常退出路径。这些逐个做了对齐而不是靠双跑兜底（详见下方「替换时的对齐项」）。

如需灰度，仍可对单个 Agent/组织回退：保留调用 `run_legacy_awaken` 的那一行就够了。

#### 替换时的对齐项（改动点必须与此表一致）

| 口径 | 老 `awaken` | 新链路实现 |
|------|------------|-----------|
| Trace 元数据 `exit_reason` | `Final` / `MaxRoundsExceeded` | `EXIT_*` 常量，取值逐字节相同 |
| `AgentAwakeEvent.exit_reason` | 小写形式（`maxroundsexceeded`） | `Artifacts.exit.to_lowercase()` |
| Trace 落库失败 | `?` 传播 | `?` 传播（不能用 `let _ =` 吞） |
| Final 的累计轮次 | 不更新 `total_rounds` | `map_think_result` 原样写回 |
| 异常退出的 `total_rounds` | 累计轮次 | 从总线读 `BudgetState`，不是 `run.trace.steps` |
| 溢出回环次数 | 无上限（只靠 max_rounds 间接收敛） | `MAX_LOOP_ROUNDS = 8`，耗尽即**报错**（非静默） |

口径提示：若真要对比，期间「新引擎无结果」不等于失败——要看 `RunTrace.fingerprint()`
落在哪里，而不是只看有没有产出。
