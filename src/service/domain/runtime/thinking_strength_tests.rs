//! thinking_strength 单元测试（`#[path]` 注册，保住 `mod tests` 层级）
//!
//! 覆盖四件事：
//! ① **scene 可区分**：`intent / awaken / summary / compact / settle` 五个阶段的
//!    决策 scene 名两两不同（修 Q9 报的「`thinking_strength` 三合一」缺口）；
//! ② **配额覆盖面**：intent 强度同样过 `CerebellumQuota`，断言按**真实调用次数**
//!    对账 —— `quota` 恰好剩 0 时一次请求都不发、直接 `None` 走原逻辑；
//! ③ 正常路径读回分数，且 `used` 与实际调用次数一致；
//! ④ 连续量按 `score` 题的协议原值**一次线性缩放**到 0–100：既不被档位
//!    离散化，也不做客户端加权 / 单档置信门禁。

use super::*;
use crate::models::brain::CerebellumQuota;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::pkg::policy::{DecisionAnswers, DecisionPolicy, DecisionRequest, DecisionValue};
use crate::service::dal::brain::{BrainDal, DecisionOutcome};
use async_trait::async_trait;
use common::api::cerebellum::{CerebellumUsage, DegradedReason};
use common::enums::ThinkingScene;
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
    let policy = policy();
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
    // 全局 Storage 单例：单跑本模块时 `test_ctx()` 依赖它已初始化
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = test_ctx();
    let mock = Arc::new(MockBrainDal::new(script));
    let got = select(&ctx, mock.clone(), brain, thought, user_msg).await;
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
/// 且 **phase 仍进正文「当前场景」块**（原 `scene` 键放的就是 phase，收敛不得丢维度）。
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
    assert!(rendered.contains("## 当前场景"), "必须有场景块：{rendered}");
    assert!(
        rendered.contains(phase),
        "phase 必须进正文场景块（维度信息不得丢）：期望 {phase}，实际 {rendered}"
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
        "## 当前场景\nawaken\n\n## 当前用户消息\n护栏",
    );
    let requests = super::policy().prepare(&metrics);
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
    let policy = policy();
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
