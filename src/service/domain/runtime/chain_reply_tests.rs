//! chain_reply 单元测试（`#[path]` 注册，保住 `mod tests` 层级）
//!
//! 覆盖 P2-A 的硬要求：
//! ① **触发阈值两条件并列**（Q2）：`fatigue >= soft` 或 `window_depth >= hard-1`
//!    任一命中都要问；两者都不命中才 `not_triggered`（零调用）；
//! ② **输入侧双阈取小 + 省略标注**（Q3）：超预算时保链头 + 最近 N 条，
//!    且 prompt 里必须出现「中间 N 条已省略（共 M 条）」——
//!    不标注，小脑会把「看不到」当「没有信息」而误封后半段被截掉的链；
//! ③ **配额 / 零调用**：按真实调用次数（`AtomicUsize`）对账，quota 恰剩 0 时
//!    一次请求都不发；
//! ④ **收口 vs 放行反事实成对**：`close` → `Some(close=true)`（调用方走
//!    `CloseChainAsNotify`），`continue` → `None`（调用方走原静态阈值 → Peer）；
//! ⑤ **降级不变量**：degraded / 低置信 / 幻觉选项 / brain 缺失一律 `None`。

use super::*;
use crate::models::brain::CerebellumQuota;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::RequestContext;
use crate::pkg::policy::{DecisionAnswers, DecisionPolicy, DecisionValue, PickOnePolicy};
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
        _requests: Vec<crate::pkg::policy::DecisionRequest>,
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
    ) -> Result<Brain> {
        unimplemented!("not needed by chain_reply tests")
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &crate::models::model_provider::ModelProvider,
        _prompt: &str,
    ) -> Result<String> {
        unimplemented!("not needed by chain_reply tests")
    }

    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[crate::models::cortex_types::ToolDescriptor],
        _options: &crate::models::cortex_types::ModelCallOptions,
    ) -> Result<crate::models::cortex_types::ThinkResult> {
        unimplemented!("not needed by chain_reply tests")
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
    let policy: PickOnePolicy = policy();
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
            input_tokens: 320,
            output_tokens: 2,
        },
    }
}

fn test_ctx() -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx_from_global("test-user")
}

/// 默认压力事实：疲劳度 48（>= soft 36）⇒ 触发
fn triggered_facts() -> ChainPressureFacts {
    ChainPressureFacts {
        chain_depth: 4,
        window_depth: 4,
        fatigue: 48,
        soft_fatigue: 36,
        hard_fatigue: 100,
        hard_window_depth: 8,
    }
}

fn msg(from_id: &str, type_label: &'static str, content: &str) -> ChainMessage {
    ChainMessage {
        from_id: from_id.to_string(),
        type_label,
        content: content.to_string(),
        created_at: 0,
    }
}

/// 一条链：链头 + 若干往返回复
fn sample_chain(n: usize) -> Vec<ChainMessage> {
    (0..n)
        .map(|i| {
            msg(
                if i % 2 == 0 { "agent-a" } else { "agent-b" },
                "文本消息",
                &format!("第 {i} 跳的正文"),
            )
        })
        .collect()
}

/// 跑一次门：返回（结论, 真实调用次数）；`run_id` 同时是本 run 的轨迹键
async fn run_gate(
    messages: &[ChainMessage],
    facts: ChainPressureFacts,
    script: Vec<Option<DecisionOutcome>>,
    brain: Option<&Brain>,
    run_id: &str,
) -> (Option<ChainReplyVerdict>, usize) {
    // 全局 Storage 单例：单跑本模块时 `test_ctx()` 依赖它已初始化
    crate::pkg::storage::test_support::init_for_test().await;
    // 轨迹落盘基址：`record_cerebellum` 未初始化时静默丢弃（无副作用设计）
    crate::pkg::think_trace::init(std::env::temp_dir().join("ai_orz_chain_reply_test"));
    let ctx = test_ctx();
    let mock = Arc::new(MockBrainDal::new(script));
    let got = judge_chain_reply(&ctx, mock.clone(), brain, messages, &facts, run_id).await;
    (got, mock.calls())
}

fn take_traces(run_id: &str) -> Vec<ThinkTraceCerebellum> {
    crate::pkg::think_trace::take_cerebellum(run_id)
}

// ---------- 预算分配（Q3：800 是总输入上界，不是正文预算） ----------

#[test]
fn total_input_budget_is_800_and_body_gets_the_remainder() {
    // Q3 定谳的 800 必须字面落在代码里（不靠注释自述）
    assert_eq!(INPUT_TOKEN_BUDGET, 800);
    assert_eq!(FRAME_OVERHEAD_RESERVE, 400);
    assert_eq!(body_token_budget(), 400);
    // 条数上限与消费者侧回溯硬上限同口径
    assert_eq!(MAX_CHAIN_MESSAGES, 32);
}

// ---------- 触发阈值（Q2：两条件并列） ----------

#[test]
fn should_ask_triggers_on_fatigue_alone() {
    let facts = ChainPressureFacts {
        chain_depth: 3,
        window_depth: 3,
        fatigue: 36,
        soft_fatigue: 36,
        hard_fatigue: 100,
        hard_window_depth: 8,
    };
    assert!(facts.should_ask(), "fatigue 恰达软阈值必须触发");
}

#[test]
fn should_ask_triggers_on_window_depth_alone() {
    // 回血型链：工具结果把疲劳度扣回去（这里 0），窗口仍连续增长
    let facts = ChainPressureFacts {
        chain_depth: 8,
        window_depth: 7,
        fatigue: 0,
        soft_fatigue: 36,
        hard_fatigue: 100,
        hard_window_depth: 8,
    };
    assert!(
        facts.should_ask(),
        "回血型链 fatigue 到不了软阈值，窗口 >= hard-1 必须单独触发，\
否则这些链会一次小脑都没问过就被 Escalate 封掉"
    );
}

#[test]
fn should_ask_is_false_only_below_both_thresholds() {
    let facts = ChainPressureFacts {
        chain_depth: 2,
        window_depth: 6,
        fatigue: 35,
        soft_fatigue: 36,
        hard_fatigue: 100,
        hard_window_depth: 8,
    };
    assert!(!facts.should_ask(), "两条都不达阈值才不触发");
}

// ---------- 输入组装（Q3：保头 + 保尾 + 省略标注） ----------

#[test]
fn render_chain_keeps_head_and_tail_and_marks_omission() {
    let chain = sample_chain(32);
    let input = render_chain(&chain, body_token_budget());
    assert!(input.included >= 2, "至少保住链头 + 最近一条");
    assert!(
        input.omitted > 0,
        "32 条短消息 + 省略标注必然有中间条被省略"
    );
    assert_eq!(input.included + input.omitted, chain.len());
    // 链头必须是第一条：丢了链头，模型只看到尾部确认语，必然误判 close
    assert!(
        input
            .rendered
            .starts_with("[#1] agent-a 文本消息: 第 0 跳的正文")
    );
    // 省略标注必须**显式**写出两个数字（Q3 硬要求）
    assert!(input.rendered.contains("已省略"), "省略必须可见");
    assert!(
        input
            .rendered
            .contains(&format!("中间 {} 条已省略", input.omitted)),
        "省略条数必须与事实一致：{}",
        input.rendered
    );
    assert!(
        input
            .rendered
            .contains(&format!("共 {} 条链内消息", chain.len())),
        "总数必须写明，模型才能判断「看不到」不是「没有」"
    );
}

#[test]
fn render_chain_within_budget_has_no_omission_mark() {
    let chain = sample_chain(3);
    let input = render_chain(&chain, body_token_budget());
    assert_eq!(input.included, 3);
    assert_eq!(input.omitted, 0);
    assert!(
        !input.rendered.contains("已省略"),
        "没截断就不许出现省略标注"
    );
}

#[test]
fn render_chain_empty_yields_zero_included() {
    let input = render_chain(&[], body_token_budget());
    assert_eq!(input.included, 0);
    assert!(input.rendered.is_empty());
}

#[test]
fn render_chain_truncates_oversized_head_instead_of_dropping_it() {
    // 链头单条远超预算：必须截断保留，而不是整条丢掉（丢头 = 判据失去语境）
    let long = "长".repeat(4000);
    let chain = vec![
        msg("agent-a", "文本消息", &long),
        msg("agent-b", "文本消息", "尾"),
    ];
    let input = render_chain(&chain, body_token_budget());
    assert!(input.included >= 1, "链头必须保住（哪怕截断）");
    assert!(input.rendered.contains("正文已截断"), "截断必须可见");
    // 截断后仍在预算内（估算口径与 compaction 同一份）
    assert!(
        estimate_tokens(&input.rendered) <= body_token_budget(),
        "渲染结果不得超过正文预算：{} > {}",
        estimate_tokens(&input.rendered),
        body_token_budget()
    );
}

#[test]
fn rendered_chain_fits_body_budget_for_every_size() {
    // 双阈取小的直接证据：任意链长都不得越过正文预算
    for n in 1..=MAX_CHAIN_MESSAGES {
        let chain = sample_chain(n);
        let input = render_chain(&chain, body_token_budget());
        assert!(
            estimate_tokens(&input.rendered) <= body_token_budget(),
            "n={n} 超出正文预算：{}",
            estimate_tokens(&input.rendered)
        );
    }
}

// ---------- 未触发 / 空输入：零调用 ----------

#[tokio::test]
async fn not_triggered_sends_no_request() {
    let facts = ChainPressureFacts {
        chain_depth: 1,
        window_depth: 1,
        fatigue: 12,
        soft_fatigue: 36,
        hard_fatigue: 100,
        hard_window_depth: 8,
    };
    let brain = brain_with_quota(10);
    let run_id = "chain-reply-not-triggered";
    let (got, calls) = run_gate(
        &sample_chain(2),
        facts,
        vec![Some(pick_for(OPTION_CLOSE, 0.99))],
        Some(&brain),
        run_id,
    )
    .await;
    assert!(got.is_none(), "未触发不得产出收口结论");
    assert_eq!(calls, 0, "未触发必须零请求");
    assert_eq!(brain.cerebellum_quota.used(), 0, "未触发不得占配额");
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1, "记账条目也要占轨迹名额（1 条）");
    assert_eq!(traces[0].scene, SCENE);
    assert_eq!(traces[0].decision, DECISION_NOT_TRIGGERED);
    assert!(!traces[0].applied);
}

#[tokio::test]
async fn empty_chain_sends_no_request() {
    let brain = brain_with_quota(10);
    let run_id = "chain-reply-empty";
    let (got, calls) = run_gate(
        &[],
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.99))],
        Some(&brain),
        run_id,
    )
    .await;
    assert!(got.is_none());
    assert_eq!(calls, 0, "拉不到链内消息 → 不调小脑");
    assert_eq!(brain.cerebellum_quota.used(), 0);
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_EMPTY_INPUT);
    assert!(!traces[0].applied);
}

// ---------- 收口 vs 放行：反事实成对 ----------

#[tokio::test]
async fn close_verdict_closes_the_chain() {
    let brain = brain_with_quota(10);
    let run_id = "chain-reply-close";
    let (got, calls) = run_gate(
        &sample_chain(6),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.95))],
        Some(&brain),
        run_id,
    )
    .await;
    let verdict = got.expect("close 必须产出结论，调用方才能走 CloseChainAsNotify");
    assert!(verdict.close);
    assert_eq!(calls, 1, "一次 PickOne，不是逐候选各发一题");
    assert_eq!(brain.cerebellum_quota.used(), 1);
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_CLOSE);
    assert!(traces[0].applied, "收口改变了下游路由 ⇒ applied=true");
    assert!(traces[0].confidence.is_some());
}

#[tokio::test]
async fn continue_verdict_keeps_original_path() {
    let brain = brain_with_quota(10);
    let run_id = "chain-reply-continue";
    let (got, calls) = run_gate(
        &sample_chain(6),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CONTINUE, 0.95))],
        Some(&brain),
        run_id,
    )
    .await;
    assert!(got.is_none(), "continue ⇒ 不覆盖路由，原静态阈值继续适用");
    assert_eq!(calls, 1);
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_CONTINUE);
    assert!(
        !traces[0].applied,
        "放行不改下游 ⇒ applied=false（跳过率才有意义）"
    );
}

// ---------- 降级：一律落回原阈值 ----------

#[tokio::test]
async fn degraded_outcome_falls_back_to_static_threshold() {
    let brain = brain_with_quota(10);
    let run_id = "chain-reply-degraded";
    let degraded = DecisionOutcome {
        answers: DecisionAnswers::new(),
        degraded: Some(DegradedReason::Timeout),
        latency_ms: 1000,
        usage: CerebellumUsage::default(),
    };
    let (got, calls) = run_gate(
        &sample_chain(6),
        triggered_facts(),
        vec![Some(degraded)],
        Some(&brain),
        run_id,
    )
    .await;
    assert!(got.is_none(), "超时/失败必须落回原阈值，不产出收口结论");
    assert_eq!(calls, 1);
    let traces = take_traces(run_id);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded.as_deref(), Some("timeout"));
    assert!(!traces[0].applied);
}

#[tokio::test]
async fn low_confidence_falls_back() {
    let brain = brain_with_quota(10);
    let run_id = "chain-reply-low-conf";
    let (got, calls) = run_gate(
        &sample_chain(6),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.5))],
        Some(&brain),
        run_id,
    )
    .await;
    assert!(got.is_none(), "置信度低于 {} 不得收口", MIN_CONFIDENCE);
    assert_eq!(calls, 1);
    let traces = take_traces(run_id);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded.as_deref(), Some("no_answer"));
}

#[tokio::test]
async fn hallucinated_option_is_not_trusted() {
    let brain = brain_with_quota(10);
    let run_id = "chain-reply-hallucination";
    let (got, _) = run_gate(
        &sample_chain(6),
        triggered_facts(),
        vec![Some(pick_for("stop_everything", 0.99))],
        Some(&brain),
        run_id,
    )
    .await;
    assert!(got.is_none(), "不在选项集内的答案视为没结论");
    let traces = take_traces(run_id);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
}

#[tokio::test]
async fn missing_brain_degrades_without_call() {
    let run_id = "chain-reply-no-brain";
    let (got, calls) = run_gate(
        &sample_chain(6),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.99))],
        None,
        run_id,
    )
    .await;
    assert!(got.is_none(), "未装配小脑只允许降级");
    assert_eq!(calls, 0);
    // 未装配不进入决策 ⇒ 不写轨迹（与 intent_gate 同口径）
    assert!(take_traces(run_id).is_empty());
}

#[tokio::test]
async fn quota_exhausted_sends_no_request_and_keeps_used() {
    // 配额恰剩 0：必须「一次请求都不发」而不是「发了再降级」
    let brain = brain_with_quota(1);
    assert!(brain.cerebellum_quota.try_acquire(), "先用掉唯一一次");
    assert_eq!(brain.cerebellum_quota.used(), 1);

    let run_id = "chain-reply-quota";
    let (got, calls) = run_gate(
        &sample_chain(6),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.99))],
        Some(&brain),
        run_id,
    )
    .await;
    assert!(got.is_none());
    assert_eq!(calls, 0, "配额耗尽必须零请求");
    assert_eq!(brain.cerebellum_quota.used(), 1, "used 不得因降级而变动");
    let traces = take_traces(run_id);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded.as_deref(), Some("budget_exhausted"));
}

// ---------- 判据文本的两条不可退让点 ----------

#[test]
fn instructions_forbid_treating_omission_as_absence() {
    // 本场景唯一的假阴性形态：把「看不到」当「没有新信息」
    assert!(INSTRUCTIONS.contains("中间 N 条已省略"));
    assert!(INSTRUCTIONS.contains("不得把「看不到」当成「没有新信息」"));
    assert!(INSTRUCTIONS.contains("证据不足"));
}

#[test]
fn instructions_default_to_continue() {
    // 只写黑名单侧：误收口代价高于多回一轮
    assert!(INSTRUCTIONS.contains("判不准就选 continue"));
    assert_eq!(MIN_CONFIDENCE, 0.9);
}
