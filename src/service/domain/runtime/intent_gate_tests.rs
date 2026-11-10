//! intent_gate 单元测试（`#[path]` 注册，保住 `mod tests` 层级）
//!
//! 覆盖四件事，对应 P3-a 的硬要求：
//! ① **配额 / 零调用**：按**真实调用次数**（`AtomicUsize`）对账 ——
//!    `quota` 恰好剩 0 时一次请求都不发、直接 `None` 走原逻辑；
//! ② **零信息量输入零调用**：空白输入不占配额、不发请求，且轨迹恰一条
//!    `intent_gate/empty_input`（与模型判 `skip` 可区分）；
//! ③ **跳过 = 降级到现状**：`key_terms` 是用户原文整句，且不产出直答 / 澄清；
//! ④ **brain 缺失只允许降级**：返回 `None`、零请求、不 panic（不升级成错误）。

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
        unimplemented!("not needed by intent gate tests")
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &crate::models::model_provider::ModelProvider,
        _prompt: &str,
    ) -> Result<String> {
        unimplemented!("not needed by intent gate tests")
    }

    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &crate::models::brain::Brain,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[crate::models::cortex_types::ToolDescriptor],
        _options: &crate::models::cortex_types::ModelCallOptions,
    ) -> Result<crate::models::cortex_types::ThinkResult> {
        unimplemented!("not needed by intent gate tests")
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

/// 跑一次门：返回（结论, 真实调用次数）；`run_id` 同时是本 run 的轨迹键
async fn run_gate(
    text: &str,
    script: Vec<Option<DecisionOutcome>>,
    brain: Option<&Brain>,
    run_id: &str,
) -> (Option<IntentAnalysis>, usize) {
    // 全局 Storage 单例：单跑本模块时 `test_ctx()` 依赖它已初始化
    crate::pkg::storage::test_support::init_for_test().await;
    // 轨迹落盘基址：`record_cerebellum` 未初始化时静默丢弃（无副作用设计），
    // 断言轨迹条数前必须先注入（`OnceLock`，重复调用无副作用）
    crate::pkg::think_trace::init(std::env::temp_dir().join("ai_orz_intent_gate_test"));
    let ctx = test_ctx();
    let mock = Arc::new(MockBrainDal::new(script));
    let got = skip_if_pointless(&ctx, mock.clone(), brain, text, run_id).await;
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
    // 标签定谳：就叫 intent_gate，不造 gate.xxx 第二套命名
    assert_eq!(SCENE, "intent_gate");
    // 置信阈值与入口门同口径
    assert_eq!(MIN_CONFIDENCE, 0.9);
    // 两个选项 id 与轨迹决策值不撞车（empty_input 不是选项，只能来自本地判）
    assert_ne!(OPTION_SKIP, OPTION_ANALYZE);
    assert_ne!(DECISION_EMPTY_INPUT, DECISION_SKIP);
}

// ---------- ② 零信息量输入：零调用、零配额、恰一条轨迹 ----------

#[tokio::test]
async fn test_empty_input_zero_calls_and_exactly_one_trace() {
    let brain = brain_with_quota(5);
    let run_id = "intent-gate-empty";
    let (got, calls) = run_gate("  \n\t ", vec![], Some(&brain), run_id).await;

    // 「代码里写了检查」不算证据：断言真实调用次数与配额用量
    assert_eq!(calls, 0, "零信息量输入不得发起任何决策请求");
    assert_eq!(
        brain.cerebellum_quota.used(),
        0,
        "零信息量输入不消耗小脑配额"
    );

    let analysis = got.expect("空输入应本地短路，走跳过路径");
    assert!(analysis.key_terms.is_empty(), "空输入没有可用检索词");
    assert!(
        !analysis.has_direct_answer(),
        "跳过路径绝不产出业务结论（不得直答）"
    );
    assert!(
        analysis.need_clarification.is_empty(),
        "跳过路径绝不产出业务结论（不得追问）"
    );

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1, "空输入恰一条轨迹");
    assert_eq!(traces[0].scene, SCENE);
    assert_eq!(
        traces[0].decision, DECISION_EMPTY_INPUT,
        "本地判空必须与模型判低价值可区分（否则统计被污染）"
    );
    assert!(traces[0].applied, "本地判空真的改变了执行路径");
    assert!(traces[0].degraded.is_none(), "这是正常决策，不是降级");
}

// ---------- ③ 模型判跳过：等价现状（原文当检索词） ----------

#[tokio::test]
async fn test_model_skip_returns_user_text_as_key_terms() {
    let brain = brain_with_quota(5);
    let text = "好的，收到";
    let run_id = "intent-gate-skip";
    let (got, calls) = run_gate(
        text,
        vec![Some(pick_for(OPTION_SKIP, 0.95))],
        Some(&brain),
        run_id,
    )
    .await;

    assert_eq!(calls, 1, "判一次跳过 = 一次决策调用");
    assert_eq!(brain.cerebellum_quota.used(), 1, "配额与真实调用次数对账");

    let analysis = got.expect("小脑判跳过应短路");
    assert_eq!(
        analysis.key_terms,
        vec![text.to_string()],
        "K4-b：跳过路径检索词 = 用户原文整句（不拆词）"
    );
    assert!(!analysis.has_direct_answer());
    assert!(analysis.need_clarification.is_empty());

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_SKIP);
    assert_eq!(traces[0].confidence, Some(0.95));
    assert!(traces[0].applied);
}

// ---------- 放行：下游行为与无本门时一致 ----------

#[tokio::test]
async fn test_model_analyze_lets_original_path_run() {
    let brain = brain_with_quota(5);
    let run_id = "intent-gate-analyze";
    let (got, calls) = run_gate(
        "帮我分析一下这个方案的风险",
        vec![Some(pick_for(OPTION_ANALYZE, 0.97))],
        Some(&brain),
        run_id,
    )
    .await;

    assert_eq!(calls, 1);
    assert!(got.is_none(), "放行 = 调用方走原逻辑");

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1, "放行也留痕（跳过率的分母）");
    assert_eq!(traces[0].decision, DECISION_ANALYZE);
    assert!(!traces[0].applied, "放行不改变下游行为");
}

// ---------- 降级三态：低置信 / 配额耗尽 / DAL 降级 ----------

#[tokio::test]
async fn test_low_confidence_falls_back_without_skipping() {
    // 0.2 < MIN_CONFIDENCE(0.9)：判了但没结论 → 走原逻辑，不是「判为可跳过」
    let brain = brain_with_quota(3);
    let run_id = "intent-gate-lowconf";
    let (got, calls) = run_gate(
        "嗯",
        vec![Some(pick_for(OPTION_SKIP, 0.2))],
        Some(&brain),
        run_id,
    )
    .await;

    assert_eq!(calls, 1);
    assert!(got.is_none(), "低置信不得按跳过处理");

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
    let run_id = "intent-gate-budget";
    let (got, calls) = run_gate(
        "帮我分析一下这个方案的风险",
        vec![Some(pick_for(OPTION_SKIP, 0.99))],
        Some(&brain),
        run_id,
    )
    .await;

    assert_eq!(calls, 0, "配额耗尽：不得发起任何决策请求");
    assert!(got.is_none(), "超限静默走原逻辑 = 返回 None");
    assert_eq!(brain.cerebellum_quota.used(), 1, "超限不再累加用量");

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded.as_deref(), Some("budget_exhausted"));
}

#[tokio::test]
async fn test_dal_degraded_falls_back() {
    // 剧本为空 ⇒ mock 返回 degraded=CallError：门必须降级，不得当成跳过
    let brain = brain_with_quota(5);
    let run_id = "intent-gate-dalerr";
    let (got, calls) = run_gate("帮我分析一下这个方案的风险", vec![], Some(&brain), run_id).await;

    assert_eq!(calls, 1);
    assert!(got.is_none(), "DAL 降级 ⇒ 走原逻辑");

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert!(traces[0].degraded.is_some(), "降级原因必须留痕");
    assert!(!traces[0].applied);
}

// ---------- ④ brain 缺失：只允许降级（护栏用例） ----------

#[tokio::test]
async fn test_brain_absent_returns_none_without_error() {
    // 时序上 `analyze_input_intent_inner` 执行时 brain 必为 Some（装配在 awaken
    // 之前）；但本门仍按「缺失即降级」写死：小脑是可选增强，失败只允许降级，
    // 不得升级成错误或 panic（历史事故：可选增强被升级成致命错误，整轮思考被打断）。
    let run_id = "intent-gate-nobrain";
    let (got, calls) = run_gate("帮我分析一下这个方案的风险", vec![], None, run_id).await;

    assert!(got.is_none(), "无小脑 ⇒ 返回 None 走原逻辑，不阻断主流程");
    assert_eq!(calls, 0, "无小脑时一次请求都不发");
    assert!(
        take_traces(run_id).is_empty(),
        "未摸到小脑就没有决策可留痕（缺失不是决策）"
    );
}
