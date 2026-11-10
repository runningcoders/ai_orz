//! summary_gate 单元测试（`#[path]` 注册，保住 `mod tests` 层级）
//!
//! 覆盖四件事，对应 P3-c 的硬要求：
//! ① **唯一翻译点**：`apply` 的判决 → 「是否走 TurnArchive 压缩」，反事实成对
//!    （`gate=None` → 压缩 1 次；`gate=skip` → 压缩 0 次，改走轻量落档 1 次）；
//! ② **优先级**：阈值策略判「不该压」时一次都不问（零调用 / 零配额 / 零轨迹）；
//! ③ **配额与零调用**：`calls` 与 `quota.used()` 对账 ——「没发请求」要被数出来；
//! ④ **不适用 ≠ 降级**：未装配 provider 时零请求、零轨迹、不落任何 `DECISION_*`；
//!    brain / 小脑 provider 缺失只允许降级，返回 `None`、不 panic。

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
        unimplemented!("not needed by summary gate tests")
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &crate::models::model_provider::ModelProvider,
        _prompt: &str,
    ) -> Result<String> {
        unimplemented!("not needed by summary gate tests")
    }

    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &crate::models::brain::Brain,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[crate::models::cortex_types::ToolDescriptor],
        _options: &crate::models::cortex_types::ModelCallOptions,
    ) -> Result<crate::models::cortex_types::ThinkResult> {
        unimplemented!("not needed by summary gate tests")
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

/// 未装配小脑 provider 的 Brain：本门必须按「不适用」处理（零请求 / 零轨迹）
fn brain_without_cerebellum() -> Brain {
    Brain::new_local(
        "agent-1".to_string(),
        "Test Agent".to_string(),
        Default::default(),
        mock_provider(),
        vec![],
    )
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
            input_tokens: 48,
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
    would_compress: bool,
) -> (Option<SkipSummary>, usize) {
    // 全局 Storage 单例：单跑本模块时 `test_ctx()` 依赖它已初始化
    crate::pkg::storage::test_support::init_for_test().await;
    // 轨迹落盘基址：`record_cerebellum` 未初始化时静默丢弃（无副作用设计），
    // 断言轨迹条数前必须先注入（`OnceLock`，重复调用无副作用）
    crate::pkg::think_trace::init(std::env::temp_dir().join("ai_orz_summary_gate_test"));
    let ctx = test_ctx();
    let mock = Arc::new(MockBrainDal::new(script));
    let got = maybe_skip(&ctx, mock.clone(), brain, text, run_id, would_compress).await;
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
    // 标签定谳：就叫 summary_gate，不造 gate.xxx 第二套命名
    assert_eq!(SCENE, "summary_gate");
    // 置信阈值与入口门 / 唤醒门同口径
    assert_eq!(MIN_CONFIDENCE, 0.9);
    // 两个选项 id 与轨迹决策值成对且不撞车
    assert_ne!(OPTION_SKIP, OPTION_SUMMARIZE);
    assert_eq!(DECISION_SKIP, OPTION_SKIP);
    assert_eq!(DECISION_SUMMARIZE, OPTION_SUMMARIZE);
    assert_ne!(DECISION_FALLBACK, DECISION_SKIP);
}

// ---------- ① 唯一翻译点：反事实成对 ----------

#[test]
fn test_apply_skip_turns_compression_off() {
    // 反事实成对：**同一输入**（would_compress = true）下
    // gate=None → 仍压缩（TurnArchive 1 次）；gate=skip → 不压缩（0 次）。
    assert!(apply(None, true), "gate=None ⇒ 逐字节等价，照旧压缩");
    assert!(!apply(Some(SkipSummary), true), "gate=skip ⇒ 不压缩");
}

#[test]
fn test_apply_never_turns_no_compression_into_compression() {
    // 只能做减法：阈值策略判「不该压」时，任何判决都不得翻成「压」
    assert!(!apply(None, false));
    assert!(!apply(Some(SkipSummary), false));
}

/// 分流判据仿真：与 `ConsolidateNode::run` 的 `if !should_compress {} else if exit` 同形
///
/// ⚠️ 诚实边界：这是**形态仿真**，不是端到端计数（端到端要能驱动阈值策略命中 +
/// 数 `ThinkBackend::compact` 调用，需给集成夹具加可注入的压缩策略，另立项）。
/// 它锁的是「`apply` 的两个出口接到分流点上分别产生几次 TurnArchive」这件事，
/// 生产分流点的静态可读要求见 `nodes.rs` 的 `let should_compress = summary_gate::apply(..)`。
fn turn_archive_calls(
    gate: Option<SkipSummary>,
    would_compress: bool,
    exit_is_max_rounds: bool,
) -> usize {
    let should_compress = apply(gate, would_compress);
    if !should_compress {
        0
    } else if !exit_is_max_rounds {
        1
    } else {
        0
    }
}

/// 分流判据仿真：跳过/不压时走的轻量落档次数（`persist_turn_memory` 唯一出口）
fn quick_store_calls(gate: Option<SkipSummary>, would_compress: bool) -> usize {
    if apply(gate, would_compress) { 0 } else { 1 }
}

#[test]
fn test_turn_archive_branch_counterfactual_pair() {
    // 成对断言：不许只锁「变小了」
    assert_eq!(
        turn_archive_calls(None, true, false),
        1,
        "gate=None ⇒ TurnArchive 压缩恰 1 次"
    );
    assert_eq!(
        turn_archive_calls(Some(SkipSummary), true, false),
        0,
        "gate=skip ⇒ TurnArchive 压缩恰 0 次（不压缩、不重复压）"
    );
    // 同一对输入下轻量落档正好相反：跳过不是「不写记忆」，是换出口
    assert_eq!(quick_store_calls(None, true), 0);
    assert_eq!(quick_store_calls(Some(SkipSummary), true), 1);
    // max_rounds 路径已在 CompactNode 压过：本门不得再加一次 TurnArchive
    assert_eq!(turn_archive_calls(None, true, true), 0);
}

// ---------- ② 优先级：本来不压 ⇒ 一次都不问 ----------

#[tokio::test]
async fn test_not_would_compress_asks_nothing() {
    let brain = brain_with_quota(5);
    let run_id = "summary-gate-no-compress";
    // 剧本里备了一条 skip 结论：若门被误开，calls 会是 1 且产生轨迹
    let (got, calls) = run_gate(
        "好的，收到",
        vec![Some(pick_for(OPTION_SKIP, 0.99))],
        Some(&brain),
        run_id,
        false,
    )
    .await;

    assert!(got.is_none(), "阈值策略判不压 ⇒ 本门不得改判成跳过");
    assert_eq!(calls, 0, "本来不压 ⇒ 一次请求都不发");
    assert_eq!(brain.cerebellum_quota.used(), 0, "零请求不得消耗配额");
    assert!(
        take_traces(run_id).is_empty(),
        "不问门 ⇒ 零轨迹（否则跳过率分母被灌水）"
    );
}

// ---------- ③ 配额与零调用 ----------

#[tokio::test]
async fn test_budget_exhausted_sends_no_request() {
    // ⚠️ `CerebellumQuota::new(0)` 是「不限」而不是「耗尽」，不能拿它造耗尽场景：
    // 用 limit=1 并先真实占用一次，模拟「本 run 的小脑额度已被更早的决策点吃掉」。
    let brain = brain_with_quota(1);
    assert!(
        brain.cerebellum_quota.try_acquire(),
        "夹具前提：额度 1 的配额能成功占用一次"
    );
    let run_id = "summary-gate-budget";
    let used_before = brain.cerebellum_quota.used();
    assert_eq!(used_before, 1, "夹具前提：额度已吃满");
    let (got, calls) = run_gate(
        "查一下那个任务的进度",
        vec![Some(pick_for(OPTION_SKIP, 0.99))],
        Some(&brain),
        run_id,
        true,
    )
    .await;

    assert!(got.is_none());
    assert_eq!(calls, 0, "配额恰剩 0 ⇒ 没发请求（断言这件事本身）");
    assert_eq!(
        brain.cerebellum_quota.used(),
        used_before,
        "没发请求 ⇒ used 不变"
    );

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1, "配额耗尽要留一条 fallback 轨迹");
    assert_eq!(traces[0].scene, SCENE);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded.as_deref(), Some("budget_exhausted"));
    assert!(!traces[0].applied, "降级不得声称改变了下游行为");
}

// ---------- ④ 不适用 / 降级 ----------

#[tokio::test]
async fn test_brain_absent_is_degraded_without_request() {
    let run_id = "summary-gate-no-brain";
    let (got, calls) = run_gate(
        "好的",
        vec![Some(pick_for(OPTION_SKIP, 0.99))],
        None,
        run_id,
        true,
    )
    .await;
    assert!(got.is_none());
    assert_eq!(calls, 0, "brain 缺失 ⇒ 零请求");
}

#[tokio::test]
async fn test_cerebellum_provider_absent_is_not_applicable_not_degraded() {
    let brain = brain_without_cerebellum();
    assert!(brain.cerebellum.is_none(), "夹具前提：provider 未装配");
    let run_id = "summary-gate-no-provider";
    let used_before = brain.cerebellum_quota.used();
    let (got, calls) = run_gate(
        "好的",
        vec![Some(pick_for(OPTION_SKIP, 0.99))],
        Some(&brain),
        run_id,
        true,
    )
    .await;

    assert!(got.is_none());
    assert_eq!(calls, 0, "未装配 provider ⇒ 一次请求都不发");
    assert_eq!(brain.cerebellum_quota.used(), used_before);
    assert!(
        take_traces(run_id).is_empty(),
        "「不适用」不是一次决策：零轨迹、零 DECISION_*（不得进 degraded 分母）"
    );
}

#[tokio::test]
async fn test_call_error_degrades_to_original_logic() {
    let brain = brain_with_quota(5);
    let run_id = "summary-gate-call-error";
    let (got, calls) = run_gate("查一下配置", vec![None], Some(&brain), run_id, true).await;

    assert!(got.is_none(), "调用失败 ⇒ 走原逻辑（照旧压缩）");
    assert_eq!(calls, 1, "失败也算一次真实调用");
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(
        traces[0].degraded.as_deref(),
        Some(DegradedReason::CallError.as_str())
    );
    assert!(!traces[0].applied);
}

#[tokio::test]
async fn test_low_confidence_is_no_answer() {
    let brain = brain_with_quota(5);
    let run_id = "summary-gate-low-confidence";
    // 0.5 < MIN_CONFIDENCE(0.9)：低于阈值按「没结论」处理，不得当成 skip
    let (got, calls) = run_gate(
        "有点复杂的一轮",
        vec![Some(pick_for(OPTION_SKIP, 0.5))],
        Some(&brain),
        run_id,
        true,
    )
    .await;

    assert!(got.is_none());
    assert_eq!(calls, 1);
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded.as_deref(), Some("no_answer"));
    assert!(traces[0].confidence.is_none());
}

#[tokio::test]
async fn test_hallucinated_option_is_no_answer() {
    let brain = brain_with_quota(5);
    let run_id = "summary-gate-hallucination";
    let (got, calls) = run_gate(
        "随便一轮",
        vec![Some(pick_for("keep_the_summary_please", 0.99))],
        Some(&brain),
        run_id,
        true,
    )
    .await;

    assert!(got.is_none(), "不在选项表里的结论不得被采信");
    assert_eq!(calls, 1);
    let traces = take_traces(run_id);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded.as_deref(), Some("no_answer"));
}

// ---------- ⑤ 两个正常出口 ----------

#[tokio::test]
async fn test_skip_decision_changes_downstream() {
    let brain = brain_with_quota(5);
    let run_id = "summary-gate-skip";
    let (got, calls) = run_gate(
        "在吗",
        vec![Some(pick_for(OPTION_SKIP, 0.95))],
        Some(&brain),
        run_id,
        true,
    )
    .await;

    assert_eq!(calls, 1, "问门恰一次");
    assert_eq!(brain.cerebellum_quota.used(), 1, "配额与真实调用次数对账");
    assert!(got.is_some(), "判跳过 ⇒ 返回判决，调用方据此改走轻量落档");

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].scene, SCENE);
    assert_eq!(traces[0].decision, DECISION_SKIP);
    assert_eq!(traces[0].confidence, Some(0.95));
    assert!(traces[0].applied, "跳过真的改变了下游行为");
    assert!(traces[0].degraded.is_none(), "这是正常决策，不是降级");
}

#[tokio::test]
async fn test_summarize_decision_keeps_original_path() {
    let brain = brain_with_quota(5);
    let run_id = "summary-gate-summarize";
    let (got, calls) = run_gate(
        "帮我把这个模块的 clippy 全清一遍再跑测试",
        vec![Some(pick_for(OPTION_SUMMARIZE, 0.95))],
        Some(&brain),
        run_id,
        true,
    )
    .await;

    assert_eq!(calls, 1);
    assert!(got.is_none(), "判照旧总结 ⇒ 返回 None，调用方走原逻辑");

    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_SUMMARIZE);
    assert!(
        !traces[0].applied,
        "照旧总结时下游行为与无本门时一致，applied=false"
    );
}

// ---------- 轨迹耗时：调用过必带实测值 ----------

#[tokio::test]
async fn test_trace_latency_measured_when_called() {
    let brain = brain_with_quota(5);
    let run_id = "summary-gate-latency";
    let (got, calls) = run_gate(
        "帮我总结一下这一轮",
        vec![Some(pick_for(OPTION_SKIP, 0.95))],
        Some(&brain),
        run_id,
        true,
    )
    .await;
    assert_eq!(calls, 1);
    assert!(got.is_some(), "判跳过应真的改变下游行为");
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(
        traces[0].latency_ms,
        Some(3),
        "发生了调用 ⇒ 轨迹必须带实测耗时（改造前固定 None 会红）"
    );
}
