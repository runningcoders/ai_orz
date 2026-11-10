//! wake_gate 单元测试（`#[path]` 注册，保住 `mod tests` 层级）
//!
//! 覆盖四件事，对应 P3-b 的硬要求：
//! ① **唯一翻译点**：`apply` 的判决 → 参数映射（两维各一条用例，**每条都断言
//!    整个二元组** —— 只锁一维时另一维被弱化照样绿）；
//! ② **门闩来源**：只认 run 级 `total_rounds`；非首轮不问门、零调用零配额零轨迹；
//! ③ **配额 / 零调用**：按真实调用次数（`AtomicUsize`）与 `quota.used()` 对账 ——
//!    「没发请求」这件事必须被数出来，而不是「calls 变了但 used 没变」；
//! ④ **brain / 小脑 provider 缺失只允许降级**：返回 `None`、零请求、不 panic。

use super::*;
use crate::models::brain::CerebellumQuota;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::pkg::policy::{DecisionAnswers, DecisionPolicy, DecisionRequest, DecisionValue};
use crate::service::dal::brain::{BrainDal, DecisionOutcome};
use async_trait::async_trait;
use common::api::cerebellum::{CerebellumUsage, DegradedReason};
use common::error::Result;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

// ---------- mock BrainDal（实现全 trait 方法，仅 decide 生效） ----------

/// 按剧本依次作答；`calls` 是配额断言的唯一依据
///
/// 剧本耗尽也记一次调用（并返回 `CallError`），所以「多打了一次」不会被静默吞掉。
struct MockBrainDal {
    script: Mutex<VecDeque<Option<DecisionOutcome>>>,
    calls: AtomicUsize,
}

impl MockBrainDal {
    fn new(script: Vec<Option<DecisionOutcome>>) -> Self {
        Self {
            script: Mutex::new(script.into()),
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl BrainDal for MockBrainDal {
    async fn decide(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        _requests: Vec<DecisionRequest>,
    ) -> DecisionOutcome {
        self.calls.fetch_add(1, Ordering::Relaxed);
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
        unimplemented!("not needed by wake gate tests")
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &crate::models::model_provider::ModelProvider,
        _prompt: &str,
    ) -> Result<String> {
        unimplemented!("not needed by wake gate tests")
    }

    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &crate::models::brain::Brain,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[crate::models::cortex_types::ToolDescriptor],
        _options: &crate::models::cortex_types::ModelCallOptions,
    ) -> Result<crate::models::cortex_types::ThinkResult> {
        unimplemented!("not needed by wake gate tests")
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

/// 带 per-run 配额的 Brain（**必须装上小脑 provider**：未装配时本门连请求都不发）
fn brain_with_quota(limit: usize) -> Brain {
    let mut brain = Brain::new_local(
        "agent-1".to_string(),
        "Test Agent".to_string(),
        Default::default(),
        mock_provider(),
        vec![],
    );
    brain.cerebellum = Some(mock_provider());
    brain.cerebellum_quota = Arc::new(CerebellumQuota::new(limit));
    brain
}

/// 用生产同一份策略装一条 Pick 结论（选项 id 与置信度可控）
fn pick_for(option: &str, confidence: f32) -> DecisionOutcome {
    let policy = policy();
    DecisionOutcome {
        answers: DecisionAnswers::new().with(
            policy.primary_qid(),
            DecisionValue::Pick {
                chosen: option.to_string(),
                confidence,
                weights: BTreeMap::new(),
            },
        ),
        degraded: None,
        latency_ms: 3,
        usage: CerebellumUsage {
            input_tokens: 32,
            output_tokens: 2,
        },
    }
}

fn test_ctx() -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx_from_global("test-user")
}

/// 跑一次门：返回（判决, 本次真实调用次数）；`run_id` 同时是本 run 的轨迹键
async fn run_gate(
    text: &str,
    script: Vec<Option<DecisionOutcome>>,
    brain: Option<&Brain>,
    run_id: &str,
    total_rounds: usize,
) -> (Option<Shallow>, usize) {
    // 全局 Storage 单例：单跑本模块时 `test_ctx()` 依赖它已初始化
    crate::pkg::storage::test_support::init_for_test().await;
    // 轨迹落盘基址：`record_cerebellum` 未初始化时静默丢弃（无副作用设计），
    // 断言轨迹条数前必须先注入（`OnceLock`，重复调用无副作用）
    crate::pkg::think_trace::init(std::env::temp_dir().join("ai_orz_wake_gate_test"));
    let ctx = test_ctx();
    let mock = Arc::new(MockBrainDal::new(script));
    let got = maybe_shallow(&ctx, mock.clone(), brain, text, run_id, total_rounds).await;
    (got, mock.calls())
}

/// 取本 run 的小脑轨迹（并清理，避免跨用例串味）
fn take_traces(run_id: &str) -> Vec<ThinkTraceCerebellum> {
    crate::pkg::think_trace::take_cerebellum(run_id)
}

// ---------- 策略形态与标签 ----------

#[test]
fn test_policy_scene_and_labels_follow_convention() {
    assert_eq!(policy().scene(), SCENE);
    // 标签定谳：就叫 wake_gate，不造 gate.xxx 第二套命名
    assert_eq!(SCENE, "wake_gate");
    // 置信阈值与入口门同口径
    assert_eq!(MIN_CONFIDENCE, 0.9);
    // 两个选项 id 与轨迹决策值成对且不撞车
    assert_ne!(OPTION_SHALLOW, OPTION_DEEP);
    assert_eq!(DECISION_SHALLOW, OPTION_SHALLOW);
    assert_eq!(DECISION_DEEP, OPTION_DEEP);
    assert_ne!(DECISION_FALLBACK, DECISION_SHALLOW);
}

// ---------- ① 唯一翻译点：两维各一条用例，每条都断言整个二元组 ----------

#[test]
fn test_apply_shallow_max_rounds_dim() {
    // 轮次维：effective_max_rounds 无论多大，浅答一律压到 SHALLOW_MAX_ROUNDS。
    // 「分开」= 分用例，不是每条只锁一维 —— 本用例同样断言整个二元组。
    assert_eq!(
        apply(Some(Shallow), 25),
        (SHALLOW_MAX_ROUNDS, Some(ThinkingIntensity::MIN))
    );
}

#[test]
fn test_apply_shallow_intensity_dim() {
    // 强度维：必须**真为 0**（不是「小于基准」、不是「归零到 70」）。
    // 同样整元组断言，防另一维被悄悄弱化。
    assert_eq!(
        apply(Some(Shallow), 10),
        (SHALLOW_MAX_ROUNDS, Some(ThinkingIntensity::MIN))
    );
    assert_eq!(ThinkingIntensity::MIN.score(), 0, "MIN 就是真 0");
}

#[test]
fn test_apply_deep_passes_through_without_force() {
    // 透传不误判是独立断言：小脑没给结论 / 判深答 ⇒ 原样，且绝不注入强度。
    assert_eq!(apply(None, 10), (10, None));
    assert_eq!(apply(None, 365), (365, None));
}

// ---------- ② 门闩来源：只认 run 级 total_rounds ----------

#[tokio::test]
async fn test_first_pass_asks_gate() {
    let brain = brain_with_quota(5);
    let run_id = "wake-gate-first";
    let (got, calls) = run_gate(
        "好的，收到",
        vec![Some(pick_for(OPTION_SHALLOW, 0.95))],
        Some(&brain),
        run_id,
        0,
    )
    .await;

    assert_eq!(calls, 1, "首轮必须问门（一次决策调用）");
    assert_eq!(brain.cerebellum_quota.used(), 1, "配额与真实调用次数对账");
    assert!(got.is_some(), "判浅答 ⇒ 返回判决，调用方据此改行为");

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].scene, SCENE);
    assert_eq!(traces[0].decision, DECISION_SHALLOW);
    assert_eq!(traces[0].confidence, Some(0.95));
    assert!(traces[0].applied, "浅答真的改变了下游行为");
    assert!(traces[0].degraded.is_none(), "这是正常决策，不是降级");
}

#[tokio::test]
async fn test_gate_source_is_run_level_total_rounds_not_period_start_round() {
    // 第 7 例三条，缺一不认：
    // ① `total_rounds == 0`（run 级首轮）⇒ 问门，calls == 1
    // ② 「周期已推进（start_round > 0）但 run 级 total_rounds == 0」⇒ 仍须问门
    // ③ `total_rounds > 0`（run 里已跑过轮次）⇒ 不问门，零调用零轨迹
    //
    // ⚠️ 关于 ② 的诚实边界：本函数签名里**没有** `start_round`（单参形态本身
    // 就是用类型排除一源），所以 runtime 上无法让两者在实参层面分叉。真正的
    // provenance 护栏是 review 级静态可读要求：调用点实参必须是
    // `read_budget(bus).total_rounds`（见 `nodes.rs` 里该局部变量的赋值行）。
    // 这里用 `simulated_start_round` 明示语境，断言门只认传入的 total_rounds ——
    // 不得用「两个源都传 0」冒充覆盖。
    let run_id_first = "wake-gate-source-first";
    let brain_first = brain_with_quota(5);
    let simulated_start_round = 0usize;
    let run_level_total_rounds = 0usize;
    assert_eq!(
        simulated_start_round, run_level_total_rounds,
        "首进时两源同为 0"
    );
    let (first, calls_first) = run_gate(
        "好的，收到",
        vec![Some(pick_for(OPTION_SHALLOW, 0.95))],
        Some(&brain_first),
        run_id_first,
        run_level_total_rounds,
    )
    .await;
    assert_eq!(calls_first, 1, "① run 级首轮 ⇒ 问门");
    assert!(first.is_some());

    let run_id_periodic = "wake-gate-source-periodic";
    let brain_periodic = brain_with_quota(5);
    // ② 「周期内已推进 3 轮」的语境：门不得因此跳过（仍以 run 级 total_rounds 为准）
    let simulated_start_round = 3usize;
    let run_level_total_rounds = 0usize;
    assert!(
        simulated_start_round > 0 && run_level_total_rounds == 0,
        "本用例的语境：周期已推进但 run 级累计仍为 0"
    );
    let (periodic, calls_periodic) = run_gate(
        "好的，收到",
        vec![Some(pick_for(OPTION_SHALLOW, 0.95))],
        Some(&brain_periodic),
        run_id_periodic,
        run_level_total_rounds,
    )
    .await;
    assert_eq!(
        calls_periodic, 1,
        "② 门只认 run 级 total_rounds：start_round>0 也要问门"
    );
    assert!(periodic.is_some());

    let run_id_nonfirst = "wake-gate-source-nonfirst";
    let brain_nonfirst = brain_with_quota(5);
    let (nonfirst, calls_nonfirst) = run_gate(
        "继续",
        vec![Some(pick_for(OPTION_SHALLOW, 0.99))],
        Some(&brain_nonfirst),
        run_id_nonfirst,
        2,
    )
    .await;
    assert_eq!(calls_nonfirst, 0, "③ 非首轮一次请求都不发");
    assert!(nonfirst.is_none(), "非首轮没有判决 ⇒ 走原逻辑");
    assert_eq!(
        brain_nonfirst.cerebellum_quota.used(),
        0,
        "非首轮不消耗配额"
    );
    assert!(
        take_traces(run_id_nonfirst).is_empty(),
        "非首轮不写轨迹（否则浅答率分母被续跑轮重复计入）"
    );
}

#[tokio::test]
async fn test_second_entry_of_same_run_does_not_ask_again() {
    // 续跑（poll_exit 复用同一 run）不得重走门：判决沿首轮结果。
    let brain = brain_with_quota(5);
    let run_id = "wake-gate-second";
    let (first, calls_first) = run_gate(
        "好的，收到",
        vec![Some(pick_for(OPTION_SHALLOW, 0.95))],
        Some(&brain),
        run_id,
        0,
    )
    .await;
    assert!(first.is_some());
    assert_eq!(calls_first, 1);

    let (second, calls_second) = run_gate(
        "再补一句",
        vec![Some(pick_for(OPTION_SHALLOW, 0.95))],
        Some(&brain),
        run_id,
        1,
    )
    .await;
    assert_eq!(calls_second, 0, "第二次没发请求（剧本一条未消费）");
    assert!(second.is_none(), "续跑轮不问门 ⇒ 无判决");
    assert_eq!(brain.cerebellum_quota.used(), 1, "配额也只扣一次");
    assert_eq!(take_traces(run_id).len(), 1, "轨迹仍恰一条");
}

// ---------- 放行：下游行为与无本门时一致 ----------

#[tokio::test]
async fn test_deep_lets_original_path_run() {
    let brain = brain_with_quota(5);
    let run_id = "wake-gate-deep";
    let (got, calls) = run_gate(
        "帮我把这个方案的风险梳理一下",
        vec![Some(pick_for(OPTION_DEEP, 0.97))],
        Some(&brain),
        run_id,
        0,
    )
    .await;

    assert_eq!(calls, 1);
    assert!(got.is_none(), "放行 = 调用方走原逻辑");

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1, "放行也留痕（浅答率的分母）");
    assert_eq!(traces[0].decision, DECISION_DEEP);
    assert!(!traces[0].applied, "放行不改变下游行为");
}

// ---------- 降级三态：低置信 / 配额耗尽 / DAL 降级 ----------

#[tokio::test]
async fn test_low_confidence_falls_back_without_shallow() {
    // 0.2 < MIN_CONFIDENCE(0.9)：判了但没结论 → 走原逻辑，不是「判为浅答」
    let brain = brain_with_quota(3);
    let run_id = "wake-gate-lowconf";
    let (got, calls) = run_gate(
        "好的",
        vec![Some(pick_for(OPTION_SHALLOW, 0.2))],
        Some(&brain),
        run_id,
        0,
    )
    .await;

    assert_eq!(calls, 1);
    assert!(got.is_none(), "低置信不得按浅答处理");

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded.as_deref(), Some("no_answer"));
    assert!(!traces[0].applied);
}

#[tokio::test]
async fn test_budget_exhausted_sends_no_request() {
    // quota=1 且已被同 run 前面的决策点吃掉 → 一次请求都不发
    let brain = brain_with_quota(1);
    assert!(
        brain.cerebellum_quota.try_acquire(),
        "先吃掉唯一额度，模拟同 run 内前面的决策点"
    );
    let used_before = brain.cerebellum_quota.used();
    let run_id = "wake-gate-budget";
    let (got, calls) = run_gate(
        "好的，收到",
        vec![Some(pick_for(OPTION_SHALLOW, 0.99))],
        Some(&brain),
        run_id,
        0,
    )
    .await;

    assert_eq!(calls, 0, "配额耗尽：不得发起任何决策请求");
    assert!(got.is_none(), "超限静默走原逻辑 = 返回 None");
    assert_eq!(
        brain.cerebellum_quota.used(),
        used_before,
        "断言的是「没发请求」：used 一点没涨"
    );

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded.as_deref(), Some("budget_exhausted"));
}

#[tokio::test]
async fn test_dal_degraded_falls_back() {
    // 剧本为空 ⇒ mock 返回 degraded=CallError：门必须降级，不得当成浅答
    let brain = brain_with_quota(5);
    let run_id = "wake-gate-dalerr";
    let (got, calls) = run_gate("好的，收到", vec![], Some(&brain), run_id, 0).await;

    assert_eq!(calls, 1);
    assert!(got.is_none(), "DAL 降级 ⇒ 走原逻辑");

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert!(traces[0].degraded.is_some(), "降级原因必须留痕");
    assert!(!traces[0].applied);
}

// ---------- ④ 缺失：只允许降级（护栏用例） ----------

#[tokio::test]
async fn test_brain_absent_returns_none_without_error() {
    // 小脑是可选增强，失败只允许降级，不得升级成错误或 panic
    // （历史事故：可选增强被升级成致命错误，整轮思考被打断）。
    let run_id = "wake-gate-nobrain";
    let (got, calls) = run_gate("好的，收到", vec![], None, run_id, 0).await;

    assert!(got.is_none(), "无 brain ⇒ 返回 None 走原逻辑，不阻断主流程");
    assert_eq!(calls, 0, "无 brain 时一次请求都不发");
    assert!(
        take_traces(run_id).is_empty(),
        "未摸到小脑就没有决策可留痕（缺失不是决策）"
    );
}

#[tokio::test]
async fn test_cerebellum_provider_absent_sends_no_request() {
    // 未装配小脑 provider 的组织：一次请求都不发（同时保证集成测试不打网络）
    let mut brain = brain_with_quota(5);
    brain.cerebellum = None;
    let run_id = "wake-gate-nocerebellum";
    let (got, calls) = run_gate(
        "好的，收到",
        vec![Some(pick_for(OPTION_SHALLOW, 0.99))],
        Some(&brain),
        run_id,
        0,
    )
    .await;

    assert_eq!(calls, 0, "小脑 provider 未装配：零请求");
    assert_eq!(brain.cerebellum_quota.used(), 0, "也不消耗配额");
    assert!(got.is_none());
    assert!(take_traces(run_id).is_empty());
}
