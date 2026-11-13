//! thinking_strength 单元测试（`#[path]` 注册，保住 `mod tests` 层级）
//!
//! 覆盖六件事：
//! ① **scene 可区分**：`intent / awaken / summary / compact / settle` 五个阶段的
//!    决策 scene 名两两不同（修 Q9 报的「`thinking_strength` 三合一」缺口）；
//! ② **配额覆盖面**：intent 强度同样过 `CerebellumQuota`，断言按**真实调用次数**
//!    对账 —— `quota` 恰好剩 0 时一次请求都不发、直接 `None` 走原逻辑；
//! ③ 正常路径读回分数，且 `used` 与实际调用次数一致；
//! ④ 连续量按 `score` 题的协议原值**一次线性缩放**到 0–100 的力度分，
//!    既不被档位离散化，也不做客户端加权 / 单档置信门禁；
//! ⑤ **相对偏移**（2026-11-12）：力度分减中位得 `delta`，按**裸配置基准**的
//!    剩余空间折算回最终下发值。基准 50 时与旧断言逐位等价；基准 100 时正向
//!    不动、反向兑现，钉死「配置不再被小脑绝对分覆盖」。场景/意图偏移改作
//!    「重要度先验 + 默认建议值」，判断有结论时完全以它为准、仅无结论时才
//!    兜底硬兑现（⑤-b 组护栏）。

use super::*;
use crate::models::brain::CerebellumQuota;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::pkg::policy::{DecisionAnswers, DecisionPolicy, DecisionRequest, DecisionValue};
use crate::service::dal::brain::{BrainDal, DecisionOutcome};
use async_trait::async_trait;
use common::api::cerebellum::{CerebellumUsage, DegradedReason};
use common::enums::{ThinkingIntensity, ThinkingScene};
use common::error::Result;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

// ---------- mock BrainDal（实现全 trait 方法，仅 decide 生效） ----------

/// 按剧本依次作答；`calls` 是配额断言的唯一依据
///
/// 配额上限 = N 时，真实调用次数**不得**超过 N。剧本耗尽也记一次调用（并返回
/// `CallError`），所以「多打了一次」不会被静默吞掉。
struct MockBrainDal {
    script: Mutex<VecDeque<Option<DecisionOutcome>>>,
    calls: AtomicUsize,
    /// 每次 decide 收到的 `DecisionRequest.state`（运行时反事实的唯一捕获面）
    states: Mutex<Vec<serde_json::Value>>,
}

impl MockBrainDal {
    fn new(script: Vec<Option<DecisionOutcome>>) -> Self {
        Self {
            script: Mutex::new(script.into()),
            calls: AtomicUsize::new(0),
            states: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    /// 逐条取出捕获到的 state（顺序 = 请求发出顺序）
    fn states(&self) -> Vec<serde_json::Value> {
        self.states.lock().expect("mock states lock").clone()
    }
}

#[async_trait]
impl BrainDal for MockBrainDal {
    async fn decide(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        requests: Vec<DecisionRequest>,
    ) -> DecisionOutcome {
        self.calls.fetch_add(1, Ordering::Relaxed);
        for request in &requests {
            self.states
                .lock()
                .expect("mock states lock")
                .push(request.state.clone());
        }
        match self.script.lock().expect("mock script lock").pop_front() {
            Some(Some(outcome)) => outcome,
            _ => DecisionOutcome {
                answers: DecisionAnswers::new(),
                degraded: Some(DegradedReason::CallError),
                latency_ms: 0,
                usage: CerebellumUsage::default(),
            },
        }
    }

    async fn wake_brain(
        &self,
        _ctx: RequestContext,
        _agent: &crate::models::agent::AgentPo,
        _memories: Vec<crate::models::memory::Memory>,
    ) -> Result<crate::models::brain::Brain> {
        unimplemented!("not needed by thinking strength tests")
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &crate::models::model_provider::ModelProvider,
        _prompt: &str,
    ) -> Result<String> {
        unimplemented!("not needed by thinking strength tests")
    }

    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &crate::models::brain::Brain,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[crate::models::cortex_types::ToolDescriptor],
        _options: &crate::models::cortex_types::ModelCallOptions,
    ) -> Result<crate::models::cortex_types::ThinkResult> {
        unimplemented!("not needed by thinking strength tests")
    }

    async fn embed_entity(
        &self,
        _ctx: RequestContext,
        _entity: &dyn crate::models::vector::Vectorizable,
    ) -> Result<Option<crate::models::vector::VectorIndexParams>> {
        Ok(None)
    }

    async fn embed_text_for_search(
        &self,
        _ctx: RequestContext,
        _text: &str,
    ) -> Result<Option<crate::models::vector::VectorIndexParams>> {
        Ok(None)
    }
}

// ---------- 测试夹具 ----------

fn mock_provider() -> ModelProviderPo {
    ModelProviderPo::new(
        "Jev Cerebellum".to_string(),
        common::enums::ProviderType::Jev,
        common::enums::ModelCapability::Decision,
        "jev-latest".to_string(),
        "test-key".to_string(),
        None,
        None,
        "test".to_string(),
    )
}

/// 带 per-run 配额的 Brain（与生产装配口径一致：配额挂在 Brain 上）
fn brain_with_quota(limit: usize) -> Brain {
    let mut brain = Brain::new_local(
        "agent-1".to_string(),
        "Test Agent".to_string(),
        Default::default(),
        mock_provider(),
        vec![],
    );
    brain.cerebellum_quota = Arc::new(CerebellumQuota::new(limit));
    brain
}

/// 用生产同一份策略（`SCENE` 恒为 `thinking_strength`，不按阶段拆 scene）
/// 装一条 `score` 题结论。
///
/// `raw` 是**协议原值**：0-based 等级序号的加权期望，11 级题值域 0~10。
/// 经 `score_to_unit(raw, 11) × 100` 缩放后即 0–100 分（raw 7.0 → 70 分）。
/// 阶段只体现在 `state.scene` 与轨迹标签上，答案的 qid 与阶段无关。
fn grade_for(raw: f32, confidence: f32) -> DecisionOutcome {
    let policy = policy(
        ThinkingIntensity::DEFAULT,
        ThinkingIntensity::DEFAULT,
        scene_importance(ThinkingScene::Awaken),
    );
    DecisionOutcome {
        answers: DecisionAnswers::new().with(
            policy.primary_qid(),
            DecisionValue::Grade {
                score: raw,
                confidence,
            },
        ),
        degraded: None,
        latency_ms: 4,
        usage: CerebellumUsage {
            input_tokens: 40,
            output_tokens: 4,
        },
    }
}

fn test_ctx() -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx_from_global("test-user")
}

async fn run_script(
    script: Vec<Option<DecisionOutcome>>,
    brain: &Brain,
    thought: ThinkingScene,
    user_msg: Option<&str>,
) -> (Option<StrengthChoice>, usize) {
    let (selection, calls) = run_full(script, brain, thought, user_msg).await;
    (selection.choice, calls)
}

/// 同上，但保留完整产出：**选没选到 + 耗时 + 为什么没选到**
async fn run_full(
    script: Vec<Option<DecisionOutcome>>,
    brain: &Brain,
    thought: ThinkingScene,
    user_msg: Option<&str>,
) -> (StrengthSelection, usize) {
    // 旧用例只验分数换算 / 配额 / 降级：给空节点现场，不改原有断言口径。
    // 新增的上下文结构由本文件末尾的专用护栏覆盖。
    run_full_with_node(
        script,
        brain,
        thought,
        user_msg,
        StrengthNodeContext::default(),
    )
    .await
}

/// 可指定节点现场的变体：默认建议护栏要能传入 `Settle` / `dynamic_offset`
async fn run_full_with_node(
    script: Vec<Option<DecisionOutcome>>,
    brain: &Brain,
    thought: ThinkingScene,
    user_msg: Option<&str>,
    node: StrengthNodeContext<'_>,
) -> (StrengthSelection, usize) {
    // 全局 Storage 单例：单跑本模块时 `test_ctx()` 依赖它已初始化
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = test_ctx();
    let mock = Arc::new(MockBrainDal::new(script));
    let got = select(&ctx, mock.clone(), brain, thought, user_msg, node).await;
    (got, mock.calls())
}

// ---------- ① scene 可区分 ----------

#[test]
fn test_trace_scene_distinguishes_intent_awaken_summary() {
    assert_eq!(
        trace_scene(ThinkingScene::Awaken),
        "thinking_strength.awaken"
    );
    assert_eq!(
        trace_scene(ThinkingScene::IntentAnalyze),
        "thinking_strength.intent"
    );

    // 五个阶段的 scene 名必须两两不同：否则看板读不出是哪一次决策
    let all: Vec<&str> = [
        ThinkingScene::Awaken,
        ThinkingScene::IntentAnalyze,
        ThinkingScene::Summary,
        ThinkingScene::Compact,
        ThinkingScene::Settle,
    ]
    .into_iter()
    .map(trace_scene)
    .collect();
    let mut dedup = all.clone();
    dedup.sort_unstable();
    dedup.dedup();
    assert_eq!(
        dedup.len(),
        5,
        "ThoughtScene 的决策 scene 名不得重名: {all:?}"
    );
}

// ---------- ③ 正常路径 ----------

#[tokio::test]
async fn test_intent_strength_selected_from_cerebellum() {
    let brain = brain_with_quota(5);
    let (got, calls) = run_script(
        vec![Some(grade_for(7.0, 0.82))],
        &brain,
        ThinkingScene::IntentAnalyze,
        Some("帮我分析一下这个方案的风险"),
    )
    .await;
    assert_eq!(calls, 1, "选一次强度 = 一次决策调用");
    assert_eq!(brain.cerebellum_quota.used(), 1, "配额与真实调用次数对账");
    let choice = got.expect("应选出分数");
    assert_eq!(choice.strength.score(), 70, "7.0 / 10 × 100 = 70");
    assert!((choice.confidence - 0.82).abs() < f32::EPSILON);
}

#[tokio::test]
async fn test_low_confidence_is_still_a_valid_continuous_judgement() {
    // 反向护栏：连续量不做单档置信门禁。此前 0.2 < MIN_CONFIDENCE(0.5) 会
    // 判「没结论」→ fallback，而 choice 题的概率本就摊薄，导致全量降级。
    let brain = brain_with_quota(3);
    let (sel, calls) = run_full(
        vec![Some(grade_for(9.0, 0.2))],
        &brain,
        ThinkingScene::IntentAnalyze,
        Some("在吗"),
    )
    .await;
    assert_eq!(calls, 1);
    assert_eq!(sel.degraded, None, "低置信不是降级，只是观测值偏低");
    let choice = sel.choice.expect("低置信也要读出分数");
    assert_eq!(choice.strength.score(), 90);
    assert!((choice.confidence - 0.2).abs() < 1e-6);
}

// ---------- ② 配额覆盖面（按调用次数断言） ----------

#[tokio::test]
async fn test_intent_strength_budget_exhausted_sends_no_request() {
    // quota=1 且额度已被前面的决策点吃掉（如入口门）→ intent 强度一次请求都不发
    let brain = brain_with_quota(1);
    assert!(
        brain.cerebellum_quota.try_acquire(),
        "先吃掉唯一额度，模拟同 run 内前面的决策点"
    );
    let (got, calls) = run_script(
        vec![Some(grade_for(9.0, 0.99))],
        &brain,
        ThinkingScene::IntentAnalyze,
        Some("帮我重构这块代码"),
    )
    .await;
    assert_eq!(calls, 0, "配额耗尽：不得发起任何决策请求");
    assert!(got.is_none(), "超限静默走原逻辑 = 返回 None");
    assert_eq!(brain.cerebellum_quota.used(), 1, "超限不再累加用量");
}

#[tokio::test]
async fn test_intent_strength_without_user_message_sends_no_request() {
    // 没有用户消息就没有可判断的输入：零调用（也不占配额）
    let brain = brain_with_quota(5);
    let (got, calls) = run_script(vec![], &brain, ThinkingScene::IntentAnalyze, None).await;
    assert_eq!(calls, 0);
    assert!(got.is_none());
    assert_eq!(brain.cerebellum_quota.used(), 0);
}

// ---------- ④ 值域分列：degraded 按**真实成因**落值 + 耗时口径 ----------
//
// 改造前：几种成因统一返回 `None`，`think_loop` 一律记 `no_usable_answer`，
// 于是「配额耗尽」在统计里被算成「答案不可用」。下面按值逐条钉死。

fn degraded_outcome(reason: DegradedReason, latency_ms: u64) -> DecisionOutcome {
    DecisionOutcome {
        answers: DecisionAnswers::new(),
        degraded: Some(reason),
        latency_ms,
        usage: CerebellumUsage::default(),
    }
}

#[tokio::test]
async fn test_no_user_message_is_zero_call_and_latency_none() {
    let brain = brain_with_quota(5);
    let (sel, calls) = run_full(vec![], &brain, ThinkingScene::Awaken, None).await;
    assert_eq!(calls, 0, "没有用户消息 = 零调用");
    assert_eq!(sel.degraded, Some(DEGRADED_NO_USER_MESSAGE));
    assert_eq!(sel.latency_ms, None, "零调用 ⇒ 轨迹耗时为 None");
    assert!(sel.choice.is_none());
}

#[tokio::test]
async fn test_budget_exhausted_is_not_folded_into_no_answer() {
    let brain = brain_with_quota(1);
    assert!(brain.cerebellum_quota.try_acquire(), "先吃掉唯一额度");
    let (sel, calls) = run_full(
        vec![Some(grade_for(9.0, 0.99))],
        &brain,
        ThinkingScene::Awaken,
        Some("帮我重构这块代码"),
    )
    .await;
    assert_eq!(calls, 0, "配额耗尽：一次请求都不发");
    assert_eq!(
        sel.degraded,
        Some(DEGRADED_BUDGET),
        "配额耗尽必须落 budget_exhausted，不能折叠成 no_usable_answer"
    );
    assert_eq!(sel.latency_ms, None);
}

#[tokio::test]
async fn test_no_provider_keeps_no_latency_despite_elapsed_ms() {
    // 零调用门返回的是 elapsed_ms()（这里刻意给 3ms）：若代码直接透传，
    // 断言会红 —— 这条锁死「靠变体判而非靠数值」
    let brain = brain_with_quota(5);
    let (sel, calls) = run_full(
        vec![Some(degraded_outcome(DegradedReason::NoProvider, 3))],
        &brain,
        ThinkingScene::Awaken,
        Some("在吗"),
    )
    .await;
    assert_eq!(calls, 1);
    assert_eq!(sel.degraded, Some("no_provider"));
    assert_eq!(
        sel.latency_ms, None,
        "NoProvider 是零调用门 ⇒ 轨迹耗时 None"
    );
}

#[tokio::test]
async fn test_timeout_and_call_error_keep_measured_latency() {
    let brain = brain_with_quota(5);
    let (sel, _) = run_full(
        vec![Some(degraded_outcome(DegradedReason::Timeout, 7))],
        &brain,
        ThinkingScene::Awaken,
        Some("在吗"),
    )
    .await;
    assert_eq!(sel.degraded, Some("timeout"));
    assert_eq!(sel.latency_ms, Some(7), "超时真实付了时延");

    let brain = brain_with_quota(5);
    let (sel, _) = run_full(
        vec![Some(degraded_outcome(DegradedReason::CallError, 7))],
        &brain,
        ThinkingScene::Awaken,
        Some("在吗"),
    )
    .await;
    assert_eq!(sel.degraded, Some("call_error"));
    assert_eq!(sel.latency_ms, Some(7), "调用失败同样付了时延");
}

#[tokio::test]
async fn test_empty_answers_is_no_usable_answer_with_latency() {
    // 调用成功但答案缺失（形态取不到结论）⇒ no_usable_answer，且耗时是实测值
    let brain = brain_with_quota(5);
    let (sel, calls) = run_full(
        vec![Some(DecisionOutcome {
            answers: DecisionAnswers::new(),
            degraded: None,
            latency_ms: 7,
            usage: CerebellumUsage::default(),
        })],
        &brain,
        ThinkingScene::Awaken,
        Some("在吗"),
    )
    .await;
    assert_eq!(calls, 1);
    assert_eq!(sel.degraded, Some(DEGRADED_NO_ANSWER));
    assert_eq!(sel.latency_ms, Some(7), "问了就问到了，耗时不能丢");
    assert!(sel.choice.is_none());
}

#[tokio::test]
async fn test_selected_strength_carries_latency_and_no_degraded() {
    let brain = brain_with_quota(5);
    let (sel, calls) = run_full(
        vec![Some(grade_for(7.0, 0.82))],
        &brain,
        ThinkingScene::IntentAnalyze,
        Some("帮我分析一下这个方案的风险"),
    )
    .await;
    assert_eq!(calls, 1);
    assert_eq!(sel.degraded, None, "选到了就不是降级");
    assert_eq!(
        sel.latency_ms,
        Some(4),
        "轨迹必须带实测耗时（剧本自报 4ms）"
    );
    assert_eq!(sel.choice.expect("应选出分数").strength.score(), 70);
}

/// 运行时反事实（笔④-2 笔 B）：强度选择的请求体收敛为单一 `context` 分块正文，
/// 且 **phase 仍进正文「当前节点」块**（原 `scene` 键放的就是 phase，收敛不得丢维度）。
#[tokio::test]
async fn test_request_state_is_single_rendered_context_block() {
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = test_ctx();
    let brain = brain_with_quota(5);
    let mock = Arc::new(MockBrainDal::new(vec![None]));
    let sentinel = "运行时反事实哨兵-strength";
    let phase = ThinkingScene::Awaken.as_str();
    let _ = select(
        &ctx,
        mock.clone(),
        &brain,
        ThinkingScene::Awaken,
        Some(sentinel),
        StrengthNodeContext::default(),
    )
    .await;

    let states = mock.states();
    assert_eq!(states.len(), 1, "应恰发一条请求：{states:?}");
    let state = &states[0];
    assert!(
        state.get("scene").is_none() && state.get("last_user_message").is_none(),
        "平铺键必须已消失（单出口不变量）：{state}"
    );
    let rendered = state["context"]
        .as_str()
        .expect("context 必须是渲染后的字符串正文");
    assert!(rendered.contains("## 当前节点"), "必须有节点块：{rendered}");
    assert!(
        rendered.contains(phase),
        "phase 必须进正文节点块（维度信息不得丢）：期望 {phase}，实际 {rendered}"
    );
    assert!(
        rendered.contains("## 强度原值") && rendered.contains("## 节点输入要点"),
        "原值与输入要点必须与节点块同源进 body：{rendered}"
    );
    assert!(
        rendered.contains("agent_id: agent-1"),
        "身份必须带实际取值：{rendered}"
    );
    assert!(
        rendered.contains(sentinel),
        "用户消息必须逐字进正文：{rendered}"
    );
}

/// 回归护栏：`state` 收敛成单键后，`ScorePolicy::prepare` 仍**恰好**产出 1 条请求。
#[test]
fn test_single_context_key_still_prepares_exactly_one_request() {
    let metrics = Metrics::new().with(
        crate::service::domain::runtime::cerebellum_context::STATE_KEY,
        "## 当前节点\nawaken\n\n## 当前用户消息\n护栏",
    );
    let requests = super::policy(
        ThinkingIntensity::DEFAULT,
        ThinkingIntensity::DEFAULT,
        scene_importance(ThinkingScene::Awaken),
    )
    .prepare(&metrics);
    assert_eq!(requests.len(), 1, "强度选择只应产出一条请求");
    let state = &requests[0].state;
    assert!(
        state
            .get(crate::service::domain::runtime::cerebellum_context::STATE_KEY)
            .is_some(),
        "单键必须原样进入 state：{state}"
    );
}

// ---------- ⑤ 连续量：服务端判断值一次缩放，不加权 / 不离散 / 不做门禁 ----------
//
// 生产反事实：2026-11-10~11-11 日志里 thinking_strength 共 79 次降级、0 次
// select，全部落在 `no_usable_answer`。根因是把连续强度套成 11 档 choice，
// 又用 top-1 confidence ≥ 0.5 当门禁（实测真实端点 top-1 只有 0.18）。
// 改走 `score` 题后：服务端已加权，客户端只做 `raw / (levels-1) × 100`。

/// 真实端点样本（2026-11-11 实测，11 级语义题）：
/// `score=6.95, confidence=0.79`，概率分布由服务端合成，客户端不再加权。
#[tokio::test]
async fn test_real_endpoint_score_scales_to_continuous_strength() {
    let brain = brain_with_quota(5);
    let (sel, calls) = run_full(
        vec![Some(grade_for(6.95, 0.79))],
        &brain,
        ThinkingScene::Awaken,
        Some("帮我把这个 Rust 模块里的降级逻辑排查一下并修复"),
    )
    .await;
    assert_eq!(calls, 1);
    assert_eq!(sel.degraded, None, "有结论就不该降级");
    let choice = sel.choice.expect("应读出缩放后的分数");
    // 6.95 / 10 × 100 = 69.5 → 取整 70
    assert_eq!(choice.strength.score(), 70);
    assert!((choice.confidence - 0.79).abs() < 1e-6);
}

#[tokio::test]
async fn test_strength_is_not_discretized_to_level_boundaries() {
    // 旧 11 档 choice 只能产出 0/10/…/100；`score` 题的 6.42 落在两级之间，
    // 缩放后是 64 分（不是 60 或 70）—— 这条钉死「输出不再被档位离散化」。
    let brain = brain_with_quota(5);
    let (sel, _) = run_full(
        vec![Some(grade_for(6.42, 0.7))],
        &brain,
        ThinkingScene::Awaken,
        Some("帮我看看这段逻辑"),
    )
    .await;
    let score = sel.choice.expect("应读出分数").strength.score();
    assert_eq!(score, 64);
    assert_ne!(score % 10, 0, "连续量不必落在 10 的倍数上");
}

#[tokio::test]
async fn test_full_scale_reaches_business_ceiling() {
    // 分母必须是「本轮最大等级序号 10」，不是协议的等级数上限 255：
    // 写死 /255 时满分只会读出 3.9 分。
    let brain = brain_with_quota(5);
    let (sel, _) = run_full(
        vec![Some(grade_for(10.0, 0.99))],
        &brain,
        ThinkingScene::Awaken,
        Some("穷尽式排查"),
    )
    .await;
    assert_eq!(sel.choice.expect("满分").strength.score(), 100);
}

#[tokio::test]
async fn test_low_confidence_and_non_finite_score_are_not_silent_zero() {
    // confidence 只作观测；但协议原值非法（NaN）必须当「没结论」，
    // 不得 unwrap_or_default 压成 0 分（0 分是明确判断，不是未知）。
    let brain = brain_with_quota(5);
    let (sel, _) = run_full(
        vec![Some(grade_for(f32::NAN, 0.9))],
        &brain,
        ThinkingScene::Awaken,
        Some("在吗"),
    )
    .await;
    assert_eq!(sel.degraded, Some(DEGRADED_NO_ANSWER));
    assert!(sel.choice.is_none());
}

#[tokio::test]
async fn test_old_choice_answer_shape_is_not_silently_accepted() {
    // 题型契约：强度现在是 `score` 题。若上游仍回 choice 答案，必须记
    // no_usable_answer 走原逻辑，绝不能静默当成 0 分。
    let policy = policy(
        ThinkingIntensity::DEFAULT,
        ThinkingIntensity::DEFAULT,
        scene_importance(ThinkingScene::Awaken),
    );
    let old_shape = DecisionOutcome {
        answers: DecisionAnswers::new().with(
            policy.primary_qid(),
            DecisionValue::Pick {
                chosen: "70".to_string(),
                confidence: 0.99,
                weights: Default::default(),
            },
        ),
        degraded: None,
        latency_ms: 4,
        usage: CerebellumUsage::default(),
    };
    let brain = brain_with_quota(5);
    let (sel, _) = run_full(
        vec![Some(old_shape)],
        &brain,
        ThinkingScene::Awaken,
        Some("在吗"),
    )
    .await;
    assert_eq!(sel.degraded, Some(DEGRADED_NO_ANSWER));
    assert!(sel.choice.is_none());
}

// ---------- ⑥ 相对偏移（2026-11-12）：配置基准不再被绝对分覆盖 ----------

/// 显式配置基准的 Brain（agent 覆盖优先于 provider，见 `strength_base`）
fn brain_with_base(base: u8, limit: usize) -> Brain {
    let mut brain = brain_with_quota(limit);
    brain.runtime_config.thinking_intensity = Some(ThinkingIntensity::new(base));
    brain
}

/// 量程与业务上限对齐：力度分 0..100 减中位 50 ⇒ `delta` -50..+50
#[test]
fn delta_range_matches_upper() {
    assert_eq!(MAX_DELTA as f32, UPPER / 2.0);
    assert_eq!(LEVELS.len(), 11, "锚点数仍是 11（协议边界内）");
}

/// 题面必须带**裸基准 / 默认建议值 / 场景重要度**，钉死上限并说明不再叠加偏移
#[test]
fn instructions_declare_current_base_and_the_ceiling() {
    let at_max = instructions(
        ThinkingIntensity::MAX,
        ThinkingIntensity::MAX,
        scene_importance(ThinkingScene::Awaken),
    );
    assert!(at_max.contains("当前基准强度 = 100 分"), "{at_max}");
    assert!(at_max.contains("100 是最大值"), "{at_max}");
    assert!(
        at_max.contains("剩余空间"),
        "必须说明偏移会被折算：{at_max}"
    );
    assert!(
        at_max.contains("不会再叠加场景或意图偏移"),
        "题面必须与裁决口径一致：{at_max}"
    );
    assert!(at_max.contains("场景重要度："), "{at_max}");
    assert!(at_max.contains("默认建议值 = 100 分"), "{at_max}");
    assert!(
        at_max.contains("不是必须兑现") && at_max.contains("以你的判断为准"),
        "必须把默认建议定位成可覆盖的先验：{at_max}"
    );
    assert!(
        at_max.contains("只有本次没给出可用结论时"),
        "必须写明默认偏移仅兜底生效：{at_max}"
    );

    let at_thirty = instructions(
        ThinkingIntensity::new(30),
        ThinkingIntensity::new(60),
        scene_importance(ThinkingScene::Settle),
    );
    assert!(at_thirty.contains("当前基准强度 = 30 分"), "{at_thirty}");
    assert!(at_thirty.contains("默认建议值 = 60 分"), "{at_thirty}");
    assert!(!at_thirty.contains("当前基准强度 = 100 分"));
}

/// 反事实（本次事故的核心）：base=100 时小脑再怎么判「更深」也不能改变基准，
/// 而「更浅」正常生效。旧链路里 `grade_for(7.0)` 会把显式配的 100 直接换成 70。
#[tokio::test]
async fn configured_ceiling_resists_upward_delta_but_accepts_downward() {
    // force=100 → delta=+50；剩余上升空间 0 ⇒ 折算后仍是 100
    let brain = brain_with_base(100, 5);
    let (up, _) = run_script(
        vec![Some(grade_for(10.0, 0.9))],
        &brain,
        ThinkingScene::Awaken,
        Some("跨模块排查"),
    )
    .await;
    let up = up.expect("应产出结论");
    assert_eq!(up.base, 100);
    assert_eq!(up.delta, 50);
    assert_eq!(up.strength.score(), 100, "100 是最大值，向上无法再动");

    // force=0 → delta=-50；反向空间 100，正常兑现一个量程（-50 分）
    let brain = brain_with_base(100, 5);
    let (down, _) = run_script(
        vec![Some(grade_for(0.0, 0.9))],
        &brain,
        ThinkingScene::Awaken,
        Some("在吗"),
    )
    .await;
    let down = down.expect("应产出结论");
    assert_eq!(down.delta, -50);
    assert_eq!(down.strength.score(), 50, "一个量程 = 50 分");
}

/// 中位基准下 1:1 兑现（旧断言之所以逐位不变的根据）
#[tokio::test]
async fn mid_base_applies_delta_one_to_one() {
    let brain = brain_with_base(50, 5);
    let (got, _) = run_script(
        vec![Some(grade_for(7.0, 0.82))],
        &brain,
        ThinkingScene::IntentAnalyze,
        Some("帮我分析一下这个方案的风险"),
    )
    .await;
    let choice = got.expect("应产出结论");
    assert_eq!(choice.base, 50);
    assert_eq!(choice.delta, 20);
    assert_eq!(choice.strength.score(), 70);
}

/// 「基础越大向上越难」：同一力度 +20，中位兑现 +20，靠近上限只兑现 +8
#[tokio::test]
async fn same_delta_moves_less_when_near_the_ceiling() {
    let brain = brain_with_base(80, 5);
    let (got, _) = run_script(
        vec![Some(grade_for(7.0, 0.85))],
        &brain,
        ThinkingScene::Awaken,
        Some("复杂任务"),
    )
    .await;
    let choice = got.expect("应产出结论");
    assert_eq!(choice.delta, 20);
    assert_eq!(
        choice.strength.score(),
        88,
        "剩余空间 20/50 ⇒ +20 的力度只兑现 +8"
    );
}

/// 基准取值链：显式配置赢，未配回落 DEFAULT(50)
#[test]
fn strength_base_follows_config_then_default() {
    let fallback = brain_with_quota(1);
    assert_eq!(strength_base(&fallback), ThinkingIntensity::DEFAULT);
    let configured = brain_with_base(100, 1);
    assert_eq!(strength_base(&configured), ThinkingIntensity::MAX);
}

// ---------- ⑤-b 先验 / 兜底分离（2026-11-12 续：场景偏移不再硬并入锚点） ----------

/// 默认建议值 = 配置基准 + 场景固定偏移 + 意图动态偏移，钳制到 0–100
#[test]
fn strength_fallback_includes_scene_and_intent_offsets() {
    assert_eq!(
        strength_fallback(ThinkingIntensity::new(60), 30, 0).score(),
        90
    );
    assert_eq!(
        strength_fallback(ThinkingIntensity::new(60), 0, -20).score(),
        40
    );
    assert_eq!(
        strength_fallback(ThinkingIntensity::new(100), 30, 20).score(),
        100,
        "仍由 ThinkingIntensity::offset 饱和钳制"
    );
}

/// 反事实：折算落在裸 base 上，默认建议值不参与有结论时的计算
///
/// 旧口径（偏移硬并入锚点 90）算得 94；现在 base=60、向上剩余 40/50 ⇒ +20
/// 兑现 +16 ⇒ 76。小脑判「更深」也必须能低于默认建议值 90。
#[test]
fn fallback_does_not_fold_into_the_judgment_anchor() {
    let base = ThinkingIntensity::new(60);
    let fallback = strength_fallback(base, 30, 0);
    assert_eq!(fallback.score(), 90);

    let resolved = base.offset_with_headroom(20, MAX_DELTA);
    assert_eq!(
        resolved.score(),
        76,
        "按裸基准向上剩余 40/50 ⇒ +20 兑现 +16"
    );
    assert!(
        resolved.score() < fallback.score(),
        "有结论时完全以小脑为准，默认建议 90 不得托底"
    );
    let legacy = fallback.offset_with_headroom(20, MAX_DELTA);
    assert_eq!(legacy.score(), 94, "旧口径：锚点 90 只剩 10/50 空间");
    assert_ne!(resolved, legacy);
}

/// 题面的基准必须是裸配置原值；默认建议值与场景重要度单列，不冒充锚点
#[test]
fn instructions_use_raw_base_and_report_fallback_as_advice() {
    let base = ThinkingIntensity::new(60);
    let fallback = strength_fallback(base, 30, 0);
    let text = instructions(base, fallback, scene_importance(ThinkingScene::Settle));
    assert!(text.contains("当前基准强度 = 60 分"), "{text}");
    assert!(!text.contains("当前基准强度 = 90 分"), "{text}");
    assert!(text.contains("默认建议值 = 90 分"), "{text}");
    assert!(text.contains("场景重要度：高"), "{text}");
    assert!(text.contains("不是必须兑现"), "{text}");
}

/// 端到端：Settle 场景下 `select` 按裸基准折算，默认建议值只写进轨迹供排障
#[tokio::test]
async fn select_folds_delta_on_raw_base_and_reports_fallback() {
    let brain = brain_with_base(60, 5);
    let node = StrengthNodeContext {
        dynamic_offset: 0,
        ..Default::default()
    };
    let (selection, _) = run_full_with_node(
        vec![Some(grade_for(7.0, 0.85))], // force=70 ⇒ delta=+20
        &brain,
        ThinkingScene::Settle,
        Some("复杂任务"),
        node,
    )
    .await;
    let choice = selection.choice.expect("应产出结论");
    assert_eq!(choice.base, 60);
    assert_eq!(choice.fallback, 90, "Settle +30 只作为默认建议值留痕");
    assert_eq!(choice.delta, 20);
    assert_eq!(
        choice.strength.score(),
        76,
        "按裸基准 60 向上剩余 40/50 折算 ⇒ +16"
    );
    assert!(
        choice.strength.score() < choice.fallback,
        "小脑结论优先于默认建议值"
    );
}

/// 核心诉求：小脑判「本次不需要那么深」时，结果必须能显著低于默认建议值
#[tokio::test]
async fn shallow_judgment_beats_the_default_advice() {
    let brain = brain_with_base(60, 5);
    let node = StrengthNodeContext {
        dynamic_offset: 20, // 意图也建议更深，仍不得压过小脑结论
        ..Default::default()
    };
    let (selection, _) = run_full_with_node(
        vec![Some(grade_for(2.0, 0.9))], // force=20 ⇒ delta=-30
        &brain,
        ThinkingScene::Settle,
        Some("简单确认"),
        node,
    )
    .await;
    let choice = selection.choice.expect("应产出结论");
    assert_eq!(choice.fallback, 100, "Settle +30 与动态 +20 的默认建议顶格");
    assert_eq!(choice.delta, -30);
    assert_eq!(choice.strength.score(), 30, "向下剩余 60/50 ⇒ -30 全兑现");
    assert!(
        choice.strength.score() < choice.base,
        "判断出不需要那么高时必须以它为准"
    );
}

/// 场景重要度要让五个阶段说清「为什么有这个默认偏移」，且 Settle 单独标重要
#[test]
fn scene_importance_explains_the_default_offset() {
    let all = [
        ThinkingScene::Awaken,
        ThinkingScene::IntentAnalyze,
        ThinkingScene::Summary,
        ThinkingScene::Compact,
        ThinkingScene::Settle,
    ]
    .into_iter()
    .map(scene_importance)
    .collect::<Vec<_>>();
    let mut dedup = all.clone();
    dedup.sort_unstable();
    dedup.dedup();
    assert_eq!(
        dedup.len(),
        5,
        "五个阶段的重要度说明不得复用同一句：{all:?}"
    );
    assert!(
        all[4].contains("高") && all[4].contains("+30"),
        "{}",
        all[4]
    );
    for text in &all {
        assert!(text.contains("系统默认"), "{text}");
    }
}

// ---------- ⑥ 节点上下文（2026-11-12 续：不再只凭用户消息判断） ----------
//
// ⚠️ 本组按 AMan 要求「先不着急测试」只落地护栏代码、不运行；编译通过由
// `cargo check --lib --tests` 保证。正式跑全量时这些用例必须一起绿。

/// 指定小脑容量的 Brain（容量截断唯一的取值入口）
fn brain_with_capacity(capacity: i32) -> Brain {
    let mut brain = brain_with_quota(4);
    let mut po = mock_provider();
    po.config = serde_json::to_string(&crate::models::model_provider::ModelProviderConfig {
        recommended_context_length: Some(capacity),
        ..Default::default()
    })
    .expect("provider config 必须可序列化");
    brain.cerebellum = Some(po);
    brain
}

/// 一个「传入节点」的典型消息序列：System 带背景区块、Assistant 带工具调用、
/// Tool 结果、最后是带图消息（验证 base64 不外泄）。
fn sample_node_messages() -> Vec<ChatMessage> {
    vec![
        ChatMessage::system(
            "【Agent 人设】你是技术负责人\n【任务上下文】修复 DAL 层一个编译错误\n【项目上下文】ai_orz\n【历史对话】（略）",
        ),
        ChatMessage::user("请继续"),
        ChatMessage::Assistant {
            content: Some("我先看下代码".to_string()),
            tool_calls: Some(vec![crate::models::cortex_types::ToolCallRequest {
                id: "call-1".to_string(),
                name: "fs_read".to_string(),
                arguments: serde_json::json!({ "path": "src/a.rs" }),
            }]),
        },
        ChatMessage::tool("call-1", "fn main() {}"),
        ChatMessage::UserMultimodal {
            text: "带图补充".to_string(),
            images: vec![crate::models::cortex_types::ImagePart {
                mime_type: "image/png".to_string(),
                data_base64: "__SECRET_BASE64__".to_string(),
            }],
        },
    ]
}

/// 块序即「保留优先级」：高价值块必须在用户消息之前
///
/// `render_within` 从**尾部**砍块。把新块追加到末尾 ⇒ 容量一紧就先被砍，
/// 等于白改。这条断言把顺序钉死。
#[test]
fn strength_context_puts_high_value_blocks_before_user_message() {
    let brain = brain_with_quota(1);
    let node = StrengthNodeContext::default();
    let rendered = render_strength_context(
        &brain,
        ThinkingScene::Compact,
        ThinkingIntensity::DEFAULT,
        &node,
        "压缩指令",
    );
    let at = |title: &str| {
        rendered
            .find(title)
            .unwrap_or_else(|| panic!("缺块 {title}：{rendered}"))
    };
    assert!(
        at("## 当前节点") < at("## 强度原值")
            && at("## 强度原值") < at("## 节点输入要点")
            && at("## 节点输入要点") < at("## 当前用户消息"),
        "节点 / 原值 / 输入要点必须依次前置：{rendered}"
    );
    // 阶段目标按节点分叉，且明确说清这条 User 不是用户原话
    assert!(rendered.contains("把接近容量上限的完整对话"), "{rendered}");
    assert!(rendered.contains("不是用户原话"), "{rendered}");
    // 原值块：来源 + 配置基准（判断锚点）+ 默认建议值（仅兜底）
    assert!(rendered.contains("平台默认 50"), "{rendered}");
    assert!(
        rendered.contains("配置基准（本次判断锚点）：50 分"),
        "{rendered}"
    );
    assert!(
        rendered.contains("系统默认建议值（仅兜底）：50 分"),
        "{rendered}"
    );
}

/// 五个节点的真实目标两两不同（旧题面五阶段共用「用户这条消息」）
#[test]
fn node_goal_differs_across_all_five_stages() {
    let all = [
        ThinkingScene::Awaken,
        ThinkingScene::IntentAnalyze,
        ThinkingScene::Summary,
        ThinkingScene::Compact,
        ThinkingScene::Settle,
    ]
    .into_iter()
    .map(|s| node_goal(s).1)
    .collect::<Vec<_>>();
    let mut dedup = all.clone();
    dedup.sort_unstable();
    dedup.dedup();
    assert_eq!(dedup.len(), 5, "五个阶段的目标不得复用同一句：{all:?}");
    for goal in &all {
        assert!(!goal.contains("用户这条消息"), "不得退回旧题面：{goal}");
    }
}

/// 原值块必须区分「判断锚点（裸基准）」与「默认建议值（仅兜底）」
///
/// 把 Settle +30 报成判断锚点会让题面与裁决错位：折算其实落在 50 分上。
#[test]
fn baseline_block_separates_judgment_base_from_default_advice() {
    let brain = brain_with_base(50, 1);
    let node = StrengthNodeContext {
        tool_count: 12,
        dynamic_offset: 0,
        ..Default::default()
    };
    let body = baseline_block(
        &brain,
        ThinkingScene::Settle,
        ThinkingIntensity::new(50),
        &node,
    );
    assert!(body.contains("来源：Agent 显式覆盖"), "{body}");
    assert!(body.contains("场景固定偏移：+30"), "{body}");
    assert!(body.contains("配置基准（本次判断锚点）：50 分"), "{body}");
    assert!(body.contains("系统默认建议值（仅兜底）：80 分"), "{body}");
    assert!(body.contains("场景重要度：高"), "{body}");
    assert!(body.contains("可调空间：向上 50 分 / 向下 50 分"), "{body}");
    assert!(body.contains("本节点可用工具：12 个"), "{body}");
    // state 报的默认建议必须与兜底取值口逐位同源
    assert_eq!(
        strength_fallback(ThinkingIntensity::new(50), 30, node.dynamic_offset).score(),
        80
    );
}

/// 从节点消息序列提取的要点：规模 / 背景区块 / 任务摘要 / 工具调用 / 最近动作
/// 且**不得**把多模态 base64 带进小脑 state
#[test]
fn node_message_facts_extract_structure_without_secrets() {
    let messages = sample_node_messages();
    let facts =
        crate::service::domain::runtime::cerebellum_context::node_message_facts(&messages, None)
            .join("\n");
    assert!(facts.contains("共 5 条"), "{facts}");
    assert!(facts.contains("大脑已注入的背景区块"), "{facts}");
    assert!(facts.contains("任务上下文"), "{facts}");
    assert!(facts.contains("修复 DAL 层一个编译错误"), "{facts}");
    assert!(
        facts.contains("已发起工具调用 1 次（fs_read×1）"),
        "{facts}"
    );
    assert!(facts.contains("最近动作（时间正序）"), "{facts}");
    assert!(
        !facts.contains("__SECRET_BASE64__"),
        "base64 不得外泄：{facts}"
    );
}

/// 反事实（本次要修的核心缺口）：34K 级拼装 Prompt + 紧容量下，前三块仍全部在，
/// 且超长用户消息被截断 —— 旧装配会把整份 Prompt 塞进去、把判据稀释掉。
#[test]
fn tight_capacity_keeps_node_baseline_and_facts_and_truncates_long_prompt() {
    let brain = brain_with_capacity(2_000);
    let messages = sample_node_messages();
    let node = StrengthNodeContext {
        messages: &messages,
        tool_count: 20,
        start_round: 3,
        max_rounds: 20,
        ..Default::default()
    };
    let long_prompt = "请排查并修复这个问题。".repeat(3_000);
    let rendered = render_strength_context(
        &brain,
        ThinkingScene::Awaken,
        ThinkingIntensity::MAX,
        &node,
        &long_prompt,
    );
    for title in ["## 当前节点", "## 强度原值", "## 节点输入要点"] {
        assert!(
            rendered.contains(title),
            "容量紧张时仍须保住 {title}：{rendered}"
        );
    }
    assert!(rendered.contains("修复 DAL 层一个编译错误"), "{rendered}");
    assert!(
        !rendered.contains(&long_prompt),
        "超长拼装 Prompt 不得整段进 state"
    );
    assert!(
        rendered.contains(USER_MESSAGE_TRUNCATION_NOTE) || !rendered.contains("## 当前用户消息"),
        "要么截断带标注、要么整块被砍，不能原样保留：{rendered}"
    );
}

/// 题面必须把「节点目标 + 输入要点」立为主要依据，否则模型会退回只看用户消息
#[test]
fn instructions_prioritize_node_goal_and_input_facts() {
    let text = instructions(
        ThinkingIntensity::new(70),
        ThinkingIntensity::new(70),
        scene_importance(ThinkingScene::Awaken),
    );
    assert!(text.contains("当前节点"), "{text}");
    assert!(text.contains("节点输入要点"), "{text}");
    assert!(text.contains("强度原值"), "{text}");
    assert!(text.contains("不要只看用户消息"), "{text}");
}
