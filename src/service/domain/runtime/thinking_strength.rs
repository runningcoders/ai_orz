//! 思考强度选择（小脑判**相对偏移**，再折算回 0–100 绝对强度）
//!
//! 2026-11-12 AMan 拍板：让小脑凭空给「这条消息值多少分」不成立。生产实测
//! （11-12，73 次选中）分数全部落在 19–58、中位 46、置信中位 0.81，连 34K
//! token 的长任务也只给 58 —— 不是判错，是 11 级语义锚点的上端太重 + `score`
//! 概率加权天然向中回归，**绝对值本来就出不来**。更糟的是旧链路把小脑的绝对分
//! 排在显式配置前面（`runner.rs::resolve_call_options` 的 `.or(...)`），管理员
//! 配的 `thinking_intensity=100` 被静默替换成 46。
//!
//! ```text
//! base     = Agent 覆盖 → Provider 基准 → DEFAULT(50)    （配置权威；也是判断/折算锚点）
//! fallback = base.offset(场景固定偏移 + 意图动态偏移)     （默认建议值，仅无结论时下发）
//! delta    = 小脑相对偏移，量程 [-50, +50]                （score 0–100 减去中位 50）
//! result   = base.offset_with_headroom(delta, MAX_DELTA)  （**最终下发值**，恒在 0–100）
//! ```
//!
//! ⚠️ 场景/意图偏移是**重要度先验**，不是硬加成（2026-11-12 二次拍板）：它们和
//! 每个场景的重要度说明一起写进题面与 state，由小脑结合节点目标与输入要点决定
//! 本次是否兑现；**判断有结论时完全以它为准**，`fallback` 不参与折算也不二次叠加。
//! 只有降级（无结论 / 调用失败 / 超时 / 答案不可用）才回落到 `fallback`。
//! 此前把偏移在判断前硬并入锚点，基准顶格时向上 headroom=0，小脑判「更深」根本
//! 兑现不了；而题面又把它说成「不调整时真正下发的值」，模型自然倾向选「与基准
//! 相当」，于是 8 次实测全落在 98–100。现在先验与裁决分离，模型明确知道
//! 「默认建议不必照做」。
//!
//! 题面里写明当前 `base`、场景重要度与 `fallback`，并钉死「值域 0–100、
//! 100 是最大值」。偏移按**剩余空间**折算：基准越靠近极值，朝该极值方向越难动
//! （base=100 时正向完全不动、反向正常兑现），于是「基础越大向上越难」由代码
//! 保证，不靠模型自觉。
//!
//! # 上下文：节点位置 + 节点输入要点 + 原值（2026-11-12 续）
//!
//! 旧 state 只有「身份 + 裸 phase + 最后一条 User 文本 + 最近对话」，五个阶段
//! 共用同一判据「用户这条消息需要多深」。而 compact / summary / settle 的
//! 最后一条 User 其实是**整份拼装 Prompt**（实测 34K token），既不是用户原话、
//! 也看不出本节点在做什么；11-12 实测五个 phase 的中位分全是 46，完全没有
//! 区分度。故改走专用装配 [`render_strength_context`]：
//!
//! 1. **当前节点**：节点位置、该节点的真实目标与预期产出、轮次进度；
//! 2. **强度原值**：配置基准及其来源、场景重要度、场景固定偏移、意图动态偏移、
//!    默认建议值（仅兜底）、双向可调空间、可用工具数；
//! 3. **节点输入要点**：从传入该节点的 `initial_messages` 提取规模 /
//!    大脑已注入的背景区块 / 任务·项目摘要 / 已发生的工具调用 / 最近动作；
//! 4. 「当前用户消息」降级为输入之一，按容量截断并标明它在各节点的真实语义；
//! 5. 身份与最近对话殿后。
//!
//! ⚠️ 顺序不是排版偏好：[`context::ContextBundle::render_within`] 容量不足时
//! **从尾部整块砍**。旧装配把用户消息放中间、最近对话放尾，新增的高价值块若
//! 追加到末尾会最先被丢掉，等于白改。故专用装配把 1–3 顶到最前。
//!
//! 相对判断还有一层白拿的好处：`score` 的向中回归在绝对题里会把基准拉到 46，
//! 在相对题里只是 `delta≈0` ⇒ **保持配置基准**。判不准时不动，正是想要的行为。
//!
//! # 为什么不铺满协议的 255 级
//!
//! 255 是**允许的上限**，不是每次都该用的档数。实测 255 级纯数字刻度：
//! 概率摊薄到每级 0.01~0.02，服务端 `score` 与手算期望相差 73.5，归一后
//! 只有 40 分。等级越多、描述越没语义，判别力越差。这里保留 11 条**带语义**
//! 的偏移锚点，靠比例缩放拿到 0–100 的任意整数。
//!
//! # 降级（业务方忽略错误、走原逻辑）
//!
//! 无小脑 / 调用失败 / 超时 / 答案不可用 → `choice = None`，调用方退回
//! 原逻辑（Provider 基准 + Agent 覆盖 + 场景偏移 + 意图确定性偏移），即
//! [`strength_fallback`]。这是场景偏移**唯一真正被硬加进去**的时机。
//! 小脑是可选增强，绝不阻塞主流程，也不把「没判断出来」当成「判断为最低强度」。
//!
//! ⚠️⚠️ **不做单档置信门禁**：`score` 的 `confidence` 是打分置信度，只作观测。
//! 此前用 11 档 choice + 「top-1 `confidence >= 0.5`」当门禁，而连续量的
//! top-1 概率天然只有 0.18~0.25，导致强度决策 79 次全量降级、0 次选中
//! （生产日志 11-10~11-11）。换题型后这个门禁连同加权补丁一起删除。
//!
//! 返回 [`StrengthSelection`] 而不是 `Option`：调用方（思考轨迹）需要同时知道
//! **问没问**（`latency_ms`）与**为什么没成**（`degraded`）。几种成因若折叠成
//! 一个值，「配额耗尽」会被记成「答案不可用」，统计上无法归因。
//!
//! # Provider 未配置 `thinking_intensity` 时
//!
//! 分数照常算出并**写进思考轨迹**（`applied=false`），但请求体不下发该字段——
//! 能力开关的唯一判定点在 `think_round::runner::resolve_call_options`，这里
//! 不重复判。这样「算出了但没生效」是可观测的，不是静默失效。

use super::cerebellum_context as context;
use super::compaction::{estimate_tokens, truncate_to_tokens};
use crate::models::brain::Brain;
use crate::models::cortex_types::ChatMessage;
use crate::pkg::RequestContext;
use crate::pkg::policy::{Metrics, Policy, ScorePolicy};
use crate::service::dal::brain::{BrainDal, dal};
use common::enums::{ThinkingIntensity, ThinkingScene};
use std::sync::Arc;

/// 场景标识（日志 / 计量口径，也是 qid 命名空间前缀）
pub const SCENE: &str = "thinking_strength";

/// 轨迹里的阶段标签：`thinking_strength.<phase>`（2026-11-10 硬约束）
///
/// 强度决策在 awaken / intent-analyze / summary / compact / settle **每条链路各调
/// 一次**，但它们是**同一个决策 scene**（同一策略、同一问题、同一 800ms 预算，
/// 只是 `state.scene` 不同）。所以：
/// - `DecisionRequest.scene` 恒为 [`SCENE`]（不拆 scene，qid 命名空间唯一）；
/// - 轨迹/日志用本函数给出阶段后缀，否则看板读不出是哪一次决策（Q9 缺口：
///   P2 补上 intent 后，awaken / summary / intent 会三合一）。
pub const fn trace_scene(thought: ThinkingScene) -> &'static str {
    match thought {
        ThinkingScene::Awaken => "thinking_strength.awaken",
        ThinkingScene::IntentAnalyze => "thinking_strength.intent",
        ThinkingScene::Summary => "thinking_strength.summary",
        ThinkingScene::Compact => "thinking_strength.compact",
        ThinkingScene::Settle => "thinking_strength.settle",
    }
}

/// 业务上限：强度对外恒定 0–100 分（[`ThinkingIntensity`] 的值域）
///
/// `score` 题经 `with_upper(UPPER)` 读到 0–100 的**力度分**；中位 50 = 与基准
/// 相当，减去中位即得相对偏移 [`MAX_DELTA`]。
const UPPER: f32 = 100.0;

/// 偏移力度量程 = `UPPER / 2`：力度分 0..100 → `delta` -50..+50
///
/// 同时是「剩余空间充足」的阈值（见 `ThinkingIntensity::offset_with_headroom`）。
/// 写 50 而不是 `UPPER / 2` 的运行时表达式，是为了让量程是编译期常量；
/// 两者的一致性由 `delta_range_matches_upper` 护栏钉死。
const MAX_DELTA: i32 = 50;

/// 判据（本次运行的**基准 + 场景重要度 + 默认建议值**动态注入）
///
/// 四件事缺一不可：① 报裸配置基准（判断与折算都落在它上面）；② 把场景重要度
/// 与默认建议值写清楚，并明确「只是先验、可被覆盖、仅兜底生效」；③ 钉死
/// 0–100 值域、**100 是最大值**并说明按剩余空间折算；④ 明确 state 各块的
/// **优先级**。少了③，模型会把「+50」当成可以突破 100 的加分；少了④，它会
/// 退回「只看最后一条消息」—— 而那条在四个节点里根本不是用户原话。
///
/// ⚠️ `base` / `fallback` / `importance` 必须与 [`baseline_block`]、
/// [`strength_fallback`] 和 [`select`] 的折算逐位同源：题面说「基准 N 分」，
/// 裁决就得按 N 的剩余空间折算，不能各算一份。
fn instructions(base: ThinkingIntensity, fallback: ThinkingIntensity, importance: &str) -> String {
    format!(
        "请判断「相对当前配置基准，这个节点这一次需要更深还是更浅的思考」。\
当前基准强度 = {base} 分（配置原值，不含场景/意图偏移），合法值域 0–100 分，**100 是最大值**。\
场景重要度：{importance}；系统据此给出的默认建议值 = {fallback} 分（= 基准 + 场景固定偏移 + 意图动态偏移，已钳制）。\
⚠️ 默认建议只是先验、**不是必须兑现**：请结合「当前节点」的目标与「节点输入要点」判断本次是否真需要这么深；若不需要，直接给负值或选「与基准相当」，以你的判断为准。只有本次没给出可用结论时，系统才会回退到默认建议值。\
下面的等级是相对偏移：负数 = 比基准更浅，0 = 与基准相当，正数 = 比基准更深。\
偏移会按当前基准距边界的剩余空间折算：基准越接近 100，向上越难（到 100 时无法再增）；越接近 0，向下越难。\
判断结果**就是最终下发值**，不会再叠加场景或意图偏移，请直接对着这个数值给出深浅。\
state 按重要性给出：「当前节点」= 本次在链路中的位置、该节点的真实目标与轮次进度；\
「强度原值」= 配置基准及其来源、场景重要度、场景固定偏移、意图动态偏移、默认建议值与可调空间；\
「节点输入要点」= 从传入本节点的消息序列里提取的规模、大脑已注入的背景区块、任务/项目摘要、已发生的工具调用与最近动作；\
「当前用户消息」只是输入之一（部分节点里它是整份拼装 Prompt，不是用户原话），另有「会话身份」「最近对话」。\
请以「当前节点的目标 + 节点输入要点」为主要依据，不要只看用户消息的文字长短或措辞。\
只按任务相对基准的深浅判断，不要按身份、历史表现或情绪判断；拿不准就选「与基准相当」。",
        base = base.score(),
        fallback = fallback.score(),
        importance = importance,
    )
}

/// 11 条**相对偏移语义锚点**（不是输出档位）
///
/// `score` 题的等级是 0-based 序号，服务端返回的分数可以落在两级之间，
/// 经 [`UPPER`] 缩放得到 0–100 的力度分，再减去中位 50 得到 `delta`。
/// 锚点里的「N」是力度而非保证加 N 分：实际变化由
/// [`ThinkingIntensity::offset_with_headroom`] 按剩余空间折算。
/// 数量在协议 2~255 边界内，不写死在 `ScorePolicy` 里。
const LEVELS: [&str; 11] = [
    "-50：明显比基准更浅，几乎无需推理，应大幅减弱",
    "-40：比基准浅很多，简单确认即可",
    "-30：比基准浅，少量检索就能答",
    "-20：略浅于基准",
    "-10：稍微浅一点",
    "0：与当前基准相当，无需调整（拿不准选这一档）",
    "+10：稍微深一点",
    "+20：略深于基准",
    "+30：比基准深，需要多轮推理或多次工具调用",
    "+40：比基准深很多，复杂任务、反复验证",
    "+50：明显比基准更深，需要穷尽式排查与充分自证",
];

/// 透传给模型的上下文事实（单键 `context`，块序列见 [`render_strength_context`]）
///
/// 键名收敛到 [`context::STATE_KEY`] 一处（单出口）。原 `scene` 键放的是 **phase**
/// （`ThinkingScene::as_str()`），收敛后 phase 进「当前节点」块——维度信息不丢，
/// 且不再孤单地只有一个裸字符串。
const STATE_KEYS: &[&str] = &[context::STATE_KEY];

/// 当前用户消息块的截断标注（suffix 的 token 也计入预算）
const USER_MESSAGE_TRUNCATION_NOTE: &str =
    "\n…（当前消息已按小脑容量截断，完整背景见「节点输入要点」）";

/// 节点输入要点块的截断标注
const NODE_FACTS_TRUNCATION_NOTE: &str = "\n…（节点输入要点已按小脑容量截断）";

/// 强度决策所在的**节点现场**：由 `run_think_loop` 从自己的入参直接填，
/// 不再让小脑凭一条消息猜整个任务。
///
/// 全部是借用 / 标量：`select` 只在这一次决策期间读，不持有所有权、不改消息。
#[derive(Debug, Clone, Copy, Default)]
pub struct StrengthNodeContext<'a> {
    /// 传入该节点的消息序列（大脑实际看到的东西，含 System / Tool / 历史）
    pub messages: &'a [ChatMessage],
    /// 本节点可用工具数（任务面宽度的直接信号）
    pub tool_count: usize,
    /// 本次循环起始轮次（跨压缩累计）
    pub start_round: usize,
    /// 总轮次上限
    pub max_rounds: usize,
    /// 意图识别产出的动态偏移（默认建议值的组成部分，仅兜底时硬兑现）
    pub dynamic_offset: i32,
    /// 绝对强度覆盖（有值时本次判断结果不会下发，仅留痕）
    pub force_intensity: Option<u8>,
}

/// 节点位置 / 真实目标 / 预期产出（修「五阶段共用一条题面」的错配）
///
/// 旧判据写「请判断用户这条消息需要多深的思考强度」，但 summary / compact /
/// settle 根本没有「用户这条消息」—— 它们处理的是工作摘要、完整对话、待沉淀
/// 队列。写清目标，模型才知道这次强度要服务于什么。
fn node_goal(thought: ThinkingScene) -> (&'static str, &'static str) {
    match thought {
        ThinkingScene::Awaken => (
            "主循环执行节点（awaken）",
            "按入口目标推进任务：多轮推理、调用工具、处理中途注入的新输入，直到产出 Final 或触发收尾策略。任务复杂度方差最大。",
        ),
        ThinkingScene::IntentAnalyze => (
            "意图识别节点（intent-analyze）",
            "只理解、不执行业务动作，产出结构化意图结论；该结论会驱动后续阶段的动态强度偏移。本身是单轮轻量任务。",
        ),
        ThinkingScene::Summary => (
            "收尾总结节点（summary）",
            "把已完成的多轮工作整理成用户可见的最终答复或存档总结，重点是覆盖与忠实，不需要新的探索。",
        ),
        ThinkingScene::Compact => (
            "上下文压缩节点（compact）",
            "把接近容量上限的完整对话压成一条短期记忆，要求信息保全；输入本身就是整段历史，规模而非语义是主要负担。",
        ),
        ThinkingScene::Settle => (
            "沉淀节点（settle）",
            "无人值守的内循环：把待沉淀队列归纳成知识节点、关系与可复用技能，需要关联与抽象；系统默认建议更深，是否兑现由本次判断决定。",
        ),
    }
}

/// 场景**重要度**：把「为什么系统会给出这个默认偏移」直接告诉小脑
///
/// 2026-11-12：此前只有数值偏移，模型看到的是「基准已被加好」，无法判断这次
/// 到底该不该兑现；现在重要度与默认建议值一起进题面 + state（同源），并明确
/// 只是先验。Settle 是唯一有非零固定偏移的场景。
fn scene_importance(thought: ThinkingScene) -> &'static str {
    match thought {
        ThinkingScene::Awaken => {
            "高但方差最大：主循环要推进任务、多轮推理与调用工具，深度取决于本次任务复杂度；系统默认不额外加减"
        }
        ThinkingScene::IntentAnalyze => {
            "中偏低：只做单轮理解、不执行业务动作，结论会驱动后续阶段的动态偏移；系统默认不额外加减"
        }
        ThinkingScene::Summary => {
            "中：结论直接可见，重覆盖与忠实、不需要新的探索；系统默认不额外加减"
        }
        ThinkingScene::Compact => "中：输入规模是主要负担，要求信息保全；系统默认不额外加减",
        ThinkingScene::Settle => {
            "高：无人值守地归纳知识节点、关系与可复用技能，需要关联与抽象；系统默认建议 +30，请按本次队列规模与要点判断是否真需要"
        }
    }
}

/// 「当前用户消息」在各节点的真实语义（它常常根本不是用户原话）
fn user_message_semantics(thought: ThinkingScene) -> &'static str {
    match thought {
        ThinkingScene::Awaken => "本条是当前用户输入（含画像 / 历史 / 资源等拼装区块）",
        ThinkingScene::IntentAnalyze => "本条是意图分析 Prompt（整份拼装，不是用户原话）",
        ThinkingScene::Summary => "本条是收尾总结 Prompt（含工作摘要，不是用户原话）",
        ThinkingScene::Compact => "本条是压缩指令连同完整对话历史（不是用户原话）",
        ThinkingScene::Settle => "本条是沉淀 Prompt（含待沉淀记忆队列，不是用户原话）",
    }
}

/// 配置基准的**来源**：与 [`strength_base`] 同一条取值链，不另起一份判断
fn strength_base_source(brain: &Brain) -> &'static str {
    if brain.runtime_config.thinking_intensity.is_some() {
        "Agent 显式覆盖"
    } else if brain
        .model_provider()
        .and_then(|po| po.config().thinking_intensity)
        .is_some()
    {
        "Provider 配置"
    } else {
        "平台默认 50"
    }
}

/// 「当前节点」块：位置 + 目标 + 轮次进度
fn node_block(thought: ThinkingScene, node: &StrengthNodeContext<'_>) -> String {
    let (position, goal) = node_goal(thought);
    let phase = thought.as_str();
    let rounds = if node.max_rounds > 0 {
        format!(
            "轮次进度：本次循环从第 {} 轮开始 / 上限 {} 轮（跨压缩累计）",
            node.start_round, node.max_rounds
        )
    } else {
        "轮次进度：未配置上限".to_string()
    };
    format!("阶段：{phase}\n节点位置：{position}\n本节点目标：{goal}\n{rounds}")
}

/// **默认建议值 / 兜底值**：配置基准 + 场景固定偏移 + 意图动态偏移
///
/// 只出现在两处：① 题面与 state 里的「场景重要度默认建议」；② 小脑无结论时
/// 的下发值（`think_round::runner::resolve_call_options` 复用本函数，避免
/// 两处各算一份）。判断有结论时它**完全不参与**折算，禁止把它当折算锚点。
pub(crate) fn strength_fallback(
    base: ThinkingIntensity,
    fixed_offset: i32,
    dynamic_offset: i32,
) -> ThinkingIntensity {
    base.offset(fixed_offset + dynamic_offset)
}

/// 「强度原值」块：配置基准 + 场景重要度 + 默认建议值（先验，不是硬加成）
///
/// ⚠️ 可调空间必须按**裸 `base`** 报：折算落在 `base` 上，报 `fallback` 的
/// 剩余空间会让题面与裁决再次错位。
fn baseline_block(
    brain: &Brain,
    thought: ThinkingScene,
    base: ThinkingIntensity,
    node: &StrengthNodeContext<'_>,
) -> String {
    let fixed = thought.thinking_intensity_offset();
    let dynamic = node.dynamic_offset;
    let fallback = strength_fallback(base, fixed, dynamic);
    let mut lines = vec![
        format!(
            "配置基准（本次判断锚点）：{} 分，来源：{}",
            base.score(),
            strength_base_source(brain)
        ),
        format!("场景重要度：{}", scene_importance(thought)),
        format!(
            "场景固定偏移：{fixed:+}；意图动态偏移：{dynamic:+}（只是默认先验，可被本次判断覆盖）"
        ),
        format!(
            "系统默认建议值（仅兜底）：{} 分（= 基准 + 固定偏移 + 动态偏移，已钳制）",
            fallback.score()
        ),
        format!(
            "可调空间：向上 {} 分 / 向下 {} 分（按当前基准的剩余空间折算）",
            100 - base.score(),
            base.score()
        ),
        format!("本节点可用工具：{} 个", node.tool_count),
    ];
    if let Some(forced) = node.force_intensity {
        // 必须明说：runner 里 force_intensity 在能力开关内直接 return，
        // 本次折算结果根本不会进请求体，只是写进轨迹。
        lines.push(format!(
            "绝对强度覆盖：{forced} 分（本次判断结果不会下发，仅留痕）"
        ));
    }
    lines.join("\n")
}

/// 当前用户消息按小脑容量截断：34K token 的拼装 Prompt 不该把判据稀释掉
///
/// 容量未配（`None`）⇒ 不截，与 `render_within` / `node_message_facts` 的
/// 「未配不猜不裁」同口径；容量已配时按 `PENDING_REPLY_BP` 份额取预算。
fn truncated_user_message(brain: &Brain, text: &str) -> String {
    match context::snippet_budget(brain, context::share::PENDING_REPLY_BP) {
        Some(budget) => truncate_to_tokens(text, budget, USER_MESSAGE_TRUNCATION_NOTE),
        None => text.to_string(),
    }
}

/// 节点输入要点块自身也要按预算截
///
/// ⚠️ 只靠 `render_within` 不够：它是**整块砍**，一个过长的要点块会被从尾部
/// 连根砍掉，前两块再前置也救不回第三块。这里先把块正文裁到「正文池的一半」，
/// 给前两块（短）与后面的用户消息 / 最近对话都留出空间，于是 `keep >= 3`
/// 恒可达成。截的是要点块自己的尾部行（最近动作优先被截，规模与背景摘要在
/// 块首），不是整块消失。
fn node_facts_body(brain: &Brain, facts: &[String]) -> String {
    let body = facts.join("\n");
    match context::snippet_budget(brain, context::share::BODY_BP) {
        Some(pool) => truncate_to_tokens(&body, (pool / 2).max(1), NODE_FACTS_TRUNCATION_NOTE),
        None => body,
    }
}

/// 强度决策的**专用装配**：高价值块前置，按模型容量自截
///
/// 不复用 [`context::render_for`]：那五个通用决策点只要「有没有价值 / 要不要
/// 回」，一条用户消息够用；强度要判「这个节点这一次该多深」，得看节点目标、
/// 输入规模、背景与原值。两者块序不同，硬塞进通用装配会把新块排到最先被砍
/// 的尾部。
pub(crate) fn render_strength_context(
    brain: &Brain,
    thought: ThinkingScene,
    base: ThinkingIntensity,
    node: &StrengthNodeContext<'_>,
    user_text: &str,
) -> String {
    let mut bundle = context::ContextBundle::new();
    // ① 节点位置与目标：容量再紧也必须留下
    bundle.push("当前节点", node_block(thought, node));
    // ② 原值：相对判断的锚点
    bundle.push("强度原值", baseline_block(brain, thought, base, node));
    // ③ 传入节点的核心上下文要点（结构化提取，不塞整份 Prompt）
    let facts = context::node_message_facts(node.messages, context::dialogue_snippet_tokens(brain));
    bundle.push("节点输入要点", node_facts_body(brain, &facts));
    // ④ 用户消息：降级为输入之一，带语义标注 + 按容量截断
    bundle.push(
        "当前用户消息",
        format!(
            "（{}）\n{}",
            user_message_semantics(thought),
            truncated_user_message(brain, user_text)
        ),
    );
    // ⑤ 身份与最近对话殿后：容量不足时先从这里砍
    let bundle = bundle
        .with_identity(&brain.agent_id, &brain.agent_name)
        .with_recent_dialogue(
            &brain.memories,
            context::RECENT_DIALOGUE_LIMIT,
            context::dialogue_snippet_tokens(brain),
        );
    bundle.render_within(context::context_capacity(brain))
}

/// 一次强度选择的结果
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrengthChoice {
    /// 折算后的强度（0–100 分）：**最终下发值**，下游不得再叠加任何偏移
    pub strength: ThinkingIntensity,
    /// 小脑给的相对偏移力度（`[-MAX_DELTA, +MAX_DELTA]`，排障用）
    pub delta: i32,
    /// 本次的配置原值（Agent 覆盖 → Provider → DEFAULT）= 折算锚点，排障用
    pub base: u8,
    /// 默认建议值（配置原值 + 场景固定偏移 + 意图动态偏移），仅无结论时下发
    pub fallback: u8,
    /// 打分置信度（只作观测，不作门禁）
    pub confidence: f32,
}

/// 轨迹 `degraded` 值域：**每个值对应唯一触发条件**
///
/// 2026-11-10 修正：此前几种成因统一返回 `None`，`think_loop` 于是把
/// 「配额耗尽」也记成 `no_usable_answer`——统计上无法归因。
pub const DEGRADED_NO_USER_MESSAGE: &str = "no_user_message";
pub const DEGRADED_BUDGET: &str = "budget_exhausted";
pub const DEGRADED_EMPTY_REQUESTS: &str = "empty_requests";
pub const DEGRADED_NO_ANSWER: &str = "no_usable_answer";

/// 一次强度选择的完整产出：选了没有 + 耗了多久 + 为什么没选
///
/// 与 [`StrengthChoice`] 的分工：后者只表达「选到了什么」，本结构额外承载
/// **观测口径**——轨迹要回答「这次到底问没问、问出了什么」，
/// 把「没问」（配额 / 无用户消息）与「问了但没答案」记成同一个值就是失真。
#[derive(Debug, Clone)]
pub struct StrengthSelection {
    /// 选中项；`None` = 本次没有可用结论（原因见 `degraded`）
    pub choice: Option<StrengthChoice>,
    /// 轨迹口径的耗时；`None` = 没真正发出请求
    pub latency_ms: Option<u64>,
    /// 降级/未命中原因；`None` 且 `choice.is_some()` = 正常选中
    pub degraded: Option<&'static str>,
}

impl StrengthSelection {
    /// 零调用早退：没问，所以耗时恒 `None`
    fn not_called(degraded: &'static str) -> Self {
        Self {
            choice: None,
            latency_ms: None,
            degraded: Some(degraded),
        }
    }

    /// 调用过但没拿到可用结论：耗时取实测值
    fn unusable(latency_ms: Option<u64>, degraded: &'static str) -> Self {
        Self {
            choice: None,
            latency_ms,
            degraded: Some(degraded),
        }
    }
}

/// 配置基准的**唯一取值口**：Agent 覆盖 → Provider 基准 → DEFAULT(50)
///
/// 题面、原值块与 `runner::resolve_call_options` 共用这一处，禁止各写一遍取值链：
/// 两处漂移就会出现「题面说的基准」与「实际下发的基准」不是同一个数。
/// [`strength_base_source`] 只判定来源，取值仍走本函数。
pub(crate) fn strength_base(brain: &Brain) -> ThinkingIntensity {
    brain
        .runtime_config
        .thinking_intensity
        .or_else(|| {
            brain
                .model_provider()
                .and_then(|po| po.config().thinking_intensity)
        })
        .unwrap_or(ThinkingIntensity::DEFAULT)
}

/// 本场景的策略形态（按本次基准 + 默认建议值 + 场景重要度拼判据）
///
/// `min_score = 0`：本场景不用 `judge`（命中与否由「有没有结论」决定，
/// 低分是合法判断而不是未命中），阈值参数在这里无意义。
fn policy(base: ThinkingIntensity, fallback: ThinkingIntensity, importance: &str) -> ScorePolicy {
    ScorePolicy::new(
        SCENE,
        "思考强度选择",
        "相对当前配置基准，这次任务需要更深还是更浅的思考（偏移 -50~+50）",
        instructions(base, fallback, importance),
        LEVELS,
        0.0,
    )
    .with_upper(UPPER)
    .with_state_keys(STATE_KEYS)
}

/// 生产入口：小脑判定本次思考强度（`None` = 保持原逻辑）
pub async fn select(
    ctx: &RequestContext,
    dal: Arc<dyn BrainDal>,
    brain: &Brain,
    thought: ThinkingScene,
    last_user_message: Option<&str>,
    node: StrengthNodeContext<'_>,
) -> StrengthSelection {
    // 没有用户消息就没有可判断的输入：直接走原逻辑，零调用。
    let Some(user_text) = last_user_message else {
        return StrengthSelection::not_called(DEGRADED_NO_USER_MESSAGE);
    };

    // 阶段标签：同一个决策 scene，用 phase 区分是哪条链路的那一次
    let phase = thought.as_str();

    // 本 run 的调用配额（上限 = resolved 大脑轮次）：超限静默走原逻辑
    if !brain.cerebellum_quota.try_acquire() {
        log_info!(
            ctx,
            "cerebellum_quota",
            scene = SCENE,
            phase = phase,
            limit = brain.cerebellum_quota.limit(),
            used = brain.cerebellum_quota.used(),
            "cerebellum call budget exhausted, fall back to original logic"
        );
        return StrengthSelection::not_called(DEGRADED_BUDGET);
    }

    // 判断与折算都落在裸配置基准上；场景/意图偏移只作为「重要度先验 +
    // 默认建议值」进题面与 state，不参与有结论时的折算。
    // ⚠️ `strength_fallback` 是唯一取值口：题面 / state / 降级共用一份。
    let base = strength_base(brain);
    let fallback = strength_fallback(
        base,
        thought.thinking_intensity_offset(),
        node.dynamic_offset,
    );
    let policy = policy(base, fallback, scene_importance(thought));
    // 专用装配：节点位置 / 原值 / 输入要点前置，用户消息殿后。
    // 单出口仍是 [`context::STATE_KEY`]，DAL 与协议零改动。
    let rendered = render_strength_context(brain, thought, base, &node, user_text);
    let context_tokens = estimate_tokens(&rendered);
    let metrics = Metrics::new().with(context::STATE_KEY, rendered);

    // 阶段一：策略声明要问什么（纯函数；未命中就一次调用都不发）
    let requests = policy.prepare(&metrics);
    if requests.is_empty() {
        // 续-3：被丢弃的请求必须留痕。⚠️ 本分支在现配置下结构不可达
        // （`LEVELS` 是 11 项编译期常量，恒在协议 2~255 边界内）⇒ 本行日志
        // 只能由代码审查保证，不计入「已测」。
        log_info!(
            ctx,
            "thinking_strength",
            decision = "empty_requests",
            scene = SCENE,
            phase = phase,
            degraded_reason = DEGRADED_EMPTY_REQUESTS,
            "thinking strength asked nothing, keep original logic"
        );
        return StrengthSelection::not_called(DEGRADED_EMPTY_REQUESTS);
    }

    // 阶段二：唯一 async 点（门禁 / 超时 / 协议转换 / 降级全在 DAL）
    let outcome = dal.decide(ctx.clone(), brain, requests).await;
    let latency_ms = outcome.latency_ms;
    // 轨迹口径：`None` = 本次没真正发出请求（空请求门 / 三个零调用门）
    let traced_latency_ms = outcome.traced_latency_ms();

    if let Some(reason) = outcome.degraded {
        log_info!(
            ctx,
            "thinking_strength",
            decision = "fallback",
            degraded_reason = reason.as_str(),
            scene = SCENE,
            phase = phase,
            latency_ms = latency_ms,
            "thinking strength degraded to original logic"
        );
        return StrengthSelection::unusable(traced_latency_ms, reason.as_str());
    }

    // 阶段三：答案注入事实桶 → 服务端判断值按等级数归一，再乘业务上限得到
    // 0–100 的**力度分**；加权在服务端已完成，这里只做一次线性缩放。
    // 力度分减去中位 50 = 相对偏移，再按剩余空间折算回绝对强度。
    let metrics = metrics.with_decisions(Arc::new(outcome.answers));
    let Some(grade) = policy.scaled(&metrics) else {
        log_info!(
            ctx,
            "thinking_strength",
            decision = "no_answer",
            scene = SCENE,
            phase = phase,
            latency_ms = latency_ms,
            "thinking strength produced no usable answer"
        );
        return StrengthSelection::unusable(traced_latency_ms, DEGRADED_NO_ANSWER);
    };

    let force = grade.score.round().clamp(0.0, UPPER) as i32;
    let delta = force - MAX_DELTA;
    // 折算直接落在裸基准上 ⇒ `resolved` 就是最终下发值；场景/意图偏移作为
    // 先验已被模型看过，判断后不再叠加，也不拿 `fallback` 兜其结果。
    let resolved = base.offset_with_headroom(delta, MAX_DELTA);
    let choice = StrengthChoice {
        strength: resolved,
        delta,
        base: base.score(),
        fallback: fallback.score(),
        confidence: grade.confidence,
    };

    log_info!(
        ctx,
        "thinking_strength",
        decision = "select",
        score = resolved.score(),
        delta = delta,
        base = choice.base,
        fallback = choice.fallback,
        force = force,
        confidence = choice.confidence,
        levels = LEVELS.len(),
        upper = UPPER,
        scene = SCENE,
        phase = phase,
        context_tokens = context_tokens,
        node_messages = node.messages.len(),
        tool_count = node.tool_count,
        latency_ms = latency_ms,
        input_tokens = outcome.usage.input_tokens,
        output_tokens = outcome.usage.output_tokens,
        "thinking strength selected"
    );
    StrengthSelection {
        choice: Some(choice),
        latency_ms: traced_latency_ms,
        degraded: None,
    }
}

/// 单例 DAL 的生产封装
pub async fn select_with_default_dao(
    ctx: &RequestContext,
    brain: &Brain,
    thought: ThinkingScene,
    last_user_message: Option<&str>,
    node: StrengthNodeContext<'_>,
) -> StrengthSelection {
    select(ctx, dal(), brain, thought, last_user_message, node).await
}

#[cfg(test)]
#[path = "thinking_strength_tests.rs"]
mod tests;
