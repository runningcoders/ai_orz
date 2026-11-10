//! chain_reply 单元测试（`#[path]` 注册，保住 `mod tests` 层级）
//!
//! 覆盖 P2-A 的硬要求：
//! ① **每次都问**（Q2 改判）：静态未命中即问一次，**没有**门槛；同时
//!    「未启用小脑模型 ⇒ 零开销」（不渲染 / 不占配额 / 不发请求）；
//! ② **输入侧 min(容量, 时延) + 省略标注**（Q3 改判）：超预算时保链头 + 最近 N 条，
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

/// 每个 Brain 一个全局唯一的 agent_id
///
/// 熔断状态挂在进程级单例 `AgentRuntimeStateManager` 上（键 = agent_id）：
/// 若所有用例共用 `agent-1`，上一例残留的熔断态会让「calls==0」**假绿** ——
/// 与 Q1 那次抓到的假绿同形，只是换了个地方长出来。
static TEST_AGENT_SEQ: AtomicUsize = AtomicUsize::new(0);

fn unique_agent_id() -> String {
    format!(
        "agent-chain-reply-{}",
        TEST_AGENT_SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// 带 per-run 配额的 Brain（与生产装配口径一致：配额挂在 Brain 上）
fn brain_with_quota(limit: usize) -> Brain {
    let mut brain = Brain::new_local(
        unique_agent_id(),
        "Test Agent".to_string(),
        Default::default(),
        mock_provider(),
        vec![],
    );
    brain.cerebellum_quota = Arc::new(CerebellumQuota::new(limit));
    // 生产装配口径：未启用小脑的 Agent 根本走不到门内（零开销），因此
    // 「能进门的 Brain」必然带小脑 provider。缺这一行所有正向用例都会在
    // 「未启用小脑模型」这道门提前返回。
    brain.cerebellum = Some(mock_provider());
    brain
}

/// 未启用小脑模型的 Brain（零开销用例专用）
fn brain_without_cerebellum(limit: usize) -> Brain {
    let mut brain = brain_with_quota(limit);
    brain.cerebellum = None;
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

// ---------- 预算分配（Q3 改判：min(容量上限, 时延上限)） ----------

/// 测试用默认正文预算：无容量配置 ⇒ 时延上限绑定
fn test_body_budget() -> usize {
    body_token_budget(input_token_budget(None))
}

#[test]
fn latency_cap_is_800_and_body_gets_the_remainder() {
    // 时延反推上限必须字面落在代码里（不靠注释自述）
    assert_eq!(LATENCY_INPUT_CAP, 800);
    assert_eq!(FRAME_OVERHEAD_RESERVE, 450);
    assert_eq!(body_token_budget(800), 350);
    // 条数上限退为防环硬顶（与消费者侧回溯硬上限同口径）
    assert_eq!(MAX_CHAIN_MESSAGES, 32);
}

#[test]
fn input_budget_is_min_of_capacity_and_latency_cap() {
    // 标准 provider（128K / 200K）：容量远大于时延上限 ⇒ 时延侧绑定
    assert_eq!(input_token_budget(Some(200_000)), 800);
    assert_eq!(input_token_budget(Some(128_000)), 800);
    // 小容量 provider：容量侧绑定（不能按 800 硬拉）
    assert_eq!(input_token_budget(Some(600)), 600);
    // 未配置容量：保守退化到时延上限
    assert_eq!(input_token_budget(None), 800);
    // 容量小于骨架预留：正文预算归零而不是下溢
    assert_eq!(body_token_budget(300), 0);
}

// ---------- 输入组装（Q3：保头 + 保尾 + 省略标注） ----------

#[test]
fn render_chain_keeps_head_and_tail_and_marks_omission() {
    let chain = sample_chain(32);
    let input = render_chain(&chain, test_body_budget());
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
    let input = render_chain(&chain, test_body_budget());
    assert_eq!(input.included, 3);
    assert_eq!(input.omitted, 0);
    assert!(
        !input.rendered.contains("已省略"),
        "没截断就不许出现省略标注"
    );
}

#[test]
fn render_chain_empty_yields_zero_included() {
    let input = render_chain(&[], test_body_budget());
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
    let input = render_chain(&chain, test_body_budget());
    assert!(input.included >= 1, "链头必须保住（哪怕截断）");
    assert!(input.rendered.contains("正文已截断"), "截断必须可见");
    // 截断后仍在预算内（估算口径与 compaction 同一份）
    assert!(
        estimate_tokens(&input.rendered) <= test_body_budget(),
        "渲染结果不得超过正文预算：{} > {}",
        estimate_tokens(&input.rendered),
        test_body_budget()
    );
}

#[test]
fn rendered_chain_fits_body_budget_for_every_size() {
    // 双阈取小的直接证据：任意链长都不得越过正文预算
    for n in 1..=MAX_CHAIN_MESSAGES {
        let chain = sample_chain(n);
        let input = render_chain(&chain, test_body_budget());
        assert!(
            estimate_tokens(&input.rendered) <= test_body_budget(),
            "n={n} 超出正文预算：{}",
            estimate_tokens(&input.rendered)
        );
    }
}

// ---------- 未触发 / 空输入：零调用 ----------

#[tokio::test]
async fn below_every_threshold_still_asks_once() {
    // Q2 改判的直接证据：压力远低于软阈值（fatigue=12 / window=1）也**必须**
    // 问一次——「有可能从第二句开始就是废话」，挂门槛就会漏掉这些短链。
    let facts = ChainPressureFacts {
        chain_depth: 1,
        window_depth: 1,
        fatigue: 12,
        soft_fatigue: 36,
        hard_fatigue: 100,
        hard_window_depth: 8,
    };
    let brain = brain_with_quota(10);
    let run_id = "chain-reply-always-ask";
    let (got, calls) = run_gate(
        &sample_chain(2),
        facts,
        vec![Some(pick_for(OPTION_CLOSE, 0.99))],
        Some(&brain),
        run_id,
    )
    .await;
    assert_eq!(calls, 1, "门槛取消后，低压力链也必须问一次");
    assert_eq!(brain.cerebellum_quota.used(), 1, "问了就占一次配额");
    let verdict = got.expect("判 close 必须产出结论");
    assert!(verdict.close);
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_CLOSE);
}

#[tokio::test]
async fn missing_cerebellum_model_has_zero_cost() {
    // 「每次都问」之后最危险的成本形态：没配小脑的 Agent 每条跨 Agent 回复
    // 都吃一次「渲染 + 请求 + 等待」。这道门保证零开销（且不占配额）。
    let brain = brain_without_cerebellum(10);
    let run_id = "chain-reply-no-cerebellum";
    let (got, calls) = run_gate(
        &sample_chain(6),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.99))],
        Some(&brain),
        run_id,
    )
    .await;
    assert!(got.is_none(), "未启用小脑模型只允许降级");
    assert_eq!(calls, 0, "未启用小脑模型必须零请求");
    assert_eq!(brain.cerebellum_quota.used(), 0, "零开销必须包含不占配额");
    assert!(take_traces(run_id).is_empty(), "不进入决策就不写轨迹");
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

// ---------- 熔断（circuit_open）：run 级、独立值域、下一 run 自动恢复 ----------

/// 端点级失败（超时）
fn timeout_outcome() -> DecisionOutcome {
    DecisionOutcome {
        answers: DecisionAnswers::new(),
        degraded: Some(DegradedReason::Timeout),
        latency_ms: 1000,
        usage: CerebellumUsage::default(),
    }
}

/// 指定 provider 容量（`max_context_length`）的 Brain
///
/// 容量经 `recommended_context_length_tokens()` 走「max × 60%」回落，
/// 故传 500 ⇒ 容量 300（低于 450 骨架）；传 1000 ⇒ 容量 600（够用）。
fn brain_with_capacity(max_context_length: i32) -> Brain {
    let mut brain = brain_with_quota(0);
    let mut po = mock_provider();
    po.config = serde_json::to_string(&crate::models::model_provider::ModelProviderConfig {
        max_context_length: Some(max_context_length),
        ..Default::default()
    })
    .expect("provider config 必须可序列化");
    brain.cerebellum = Some(po);
    brain
}

/// 连续 3 次端点级失败 ⇒ 熔断；开闸后零调用零配额；到周期放真探针并自愈。
///
/// 载体 = `AgentRuntimeStateManager`（键 = agent_id）。Brain 每条入站消息重建，
/// 挂在 Brain 上的计数在生产永不达阈 —— 这里刻意**跨多次 `run_gate` 复用同一个
/// agent_id** 来复现生产语义（同一个 Agent 的连续多条入站消息）。
#[tokio::test]
async fn circuit_opens_after_three_endpoint_failures_then_probes_and_heals() {
    let brain = brain_with_quota(0);
    let agent_id = brain.agent_id.clone();
    let state = AgentRuntimeStateManager::global();

    for i in 0..3 {
        let (verdict, calls) = run_gate(
            &sample_chain(4),
            triggered_facts(),
            vec![Some(timeout_outcome())],
            Some(&brain),
            &format!("run-circuit-{i}"),
        )
        .await;
        assert!(verdict.is_none(), "降级不得产出结论");
        assert_eq!(calls, 1, "第 {} 次必须真发一次请求", i + 1);
        assert_eq!(
            state.cerebellum_consecutive_failures(&agent_id),
            i as u32 + 1,
            "连续失败计数必须逐次递增，且跨消息累计（不随 Brain 重建丢失）"
        );
    }
    assert!(
        state.is_cerebellum_circuit_open(&agent_id),
        "连续 3 次端点级失败必须熔断"
    );

    // ① 开闸后第 1 次：零请求、零配额、记独立值 circuit_open
    let (verdict, calls) = run_gate(
        &sample_chain(4),
        triggered_facts(),
        vec![],
        Some(&brain),
        "run-circuit-open",
    )
    .await;
    assert!(verdict.is_none());
    assert_eq!(calls, 0, "熔断后一次请求都不许发");
    assert_eq!(brain.cerebellum_quota.used(), 3, "熔断分支不得白吃配额");
    let traces = take_traces("run-circuit-open");
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(
        traces[0].degraded.as_deref(),
        Some("circuit_open"),
        "必须记独立值 circuit_open，不得并入 timeout"
    );
    assert_ne!(
        traces[0].degraded.as_deref(),
        Some("timeout"),
        "熔断 ≠ 超时"
    );
    assert_eq!(traces[0].latency_ms, None, "没发生调用不得编造时延");

    // ② 周期未到的补充尝试：继续零请求
    for i in 0..3 {
        let (verdict, calls) = run_gate(
            &sample_chain(4),
            triggered_facts(),
            vec![],
            Some(&brain),
            &format!("run-circuit-skip-{i}"),
        )
        .await;
        assert!(verdict.is_none());
        assert_eq!(
            calls,
            0,
            "探针周期未到，第 {} 次补充尝试仍不许发请求",
            i + 1
        );
    }

    // ③ 第 5 次尝试 = 真探针（周期 5：4 次跳过 + 1 次探针）
    let (verdict, calls) = run_gate(
        &sample_chain(4),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.95))],
        Some(&brain),
        "run-circuit-probe",
    )
    .await;
    assert_eq!(calls, 1, "到探针周期必须真发一次请求");
    assert!(
        verdict.is_some_and(|v| v.close),
        "探针成功应正常产出结论（自愈生效）"
    );
    assert_eq!(
        state.cerebellum_consecutive_failures(&agent_id),
        0,
        "探针成功必须清零"
    );
    assert!(
        !state.is_cerebellum_circuit_open(&agent_id),
        "清零后闸必须关闭"
    );

    // ④ 自愈后下一次：正常问、无降级
    let (verdict, calls) = run_gate(
        &sample_chain(4),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.95))],
        Some(&brain),
        "run-circuit-healed",
    )
    .await;
    assert_eq!(calls, 1, "自愈后首条必须重新尝试");
    assert!(verdict.is_some());
    let traces = take_traces("run-circuit-healed");
    assert_eq!(traces[0].degraded, None, "自愈后不得再记降级");
}

/// 端点恢复（有应答）即清零：低置信不是失败。
#[tokio::test]
async fn any_answer_resets_the_consecutive_failure_counter() {
    let brain = brain_with_quota(0);
    let agent_id = brain.agent_id.clone();
    let state = AgentRuntimeStateManager::global();

    let _ = run_gate(
        &sample_chain(4),
        triggered_facts(),
        vec![Some(timeout_outcome())],
        Some(&brain),
        "run-reset-a",
    )
    .await;
    let _ = run_gate(
        &sample_chain(4),
        triggered_facts(),
        vec![Some(timeout_outcome())],
        Some(&brain),
        "run-reset-b",
    )
    .await;
    assert_eq!(state.cerebellum_consecutive_failures(&agent_id), 2);

    // 低置信（有应答、无可用结论）→ 端点健康，清零
    let (verdict, _) = run_gate(
        &sample_chain(4),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.1))],
        Some(&brain),
        "run-reset-c",
    )
    .await;
    assert!(verdict.is_none());
    assert_eq!(
        state.cerebellum_consecutive_failures(&agent_id),
        0,
        "有应答即清零：熔断判的是端点可用性，不是判断质量"
    );
}

// ---------- 正文预算下限：容量不足就不问，而不是发一个只剩骨架的请求 ----------

/// 容量（500 × 60% = 300）连 450 骨架都装不下 ⇒ 零请求、零配额、记 capacity_too_small。
#[tokio::test]
async fn capacity_below_judge_skeleton_sends_no_request() {
    let brain = brain_with_capacity(500);
    assert_eq!(
        brain
            .cerebellum
            .as_ref()
            .and_then(|po| po.config().recommended_context_length_tokens()),
        Some(300)
    );

    let (verdict, calls) = run_gate(
        &sample_chain(4),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.99))],
        Some(&brain),
        "run-capacity-small",
    )
    .await;
    assert!(verdict.is_none());
    assert_eq!(calls, 0, "容量不足必须零请求");
    assert_eq!(brain.cerebellum_quota.used(), 0, "也不得占配额");
    let traces = take_traces("run-capacity-small");
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded.as_deref(), Some("capacity_too_small"));
    assert_eq!(traces[0].latency_ms, None);
}

/// 反事实：容量刚好够（1000 × 60% = 600 ⇒ 正文 150 ≥ 100）必须照常问。
#[tokio::test]
async fn capacity_above_min_body_budget_still_asks() {
    let brain = brain_with_capacity(1000);
    assert_eq!(body_token_budget(input_token_budget(Some(600))), 150);

    let (verdict, calls) = run_gate(
        &sample_chain(4),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.95))],
        Some(&brain),
        "run-capacity-ok",
    )
    .await;
    assert!(verdict.is_some_and(|v| v.close), "容量够时必须正常出结论");
    assert_eq!(calls, 1);
}

// ---------- 等价断言（容量 ≥ 时延反推量 ⇒ 预算 == 时延反推量） ----------

#[test]
fn capacity_at_or_above_latency_cap_binds_the_latency_side() {
    for capacity in [
        LATENCY_INPUT_CAP as u64,
        LATENCY_INPUT_CAP as u64 + 1,
        128_000,
        200_000,
        1_000_000,
    ] {
        assert_eq!(
            input_token_budget(Some(capacity)),
            LATENCY_INPUT_CAP,
            "容量 {capacity} ≥ 时延反推量 ⇒ 预算必须恰等于时延反推量"
        );
        assert_eq!(
            body_token_budget(input_token_budget(Some(capacity))),
            LATENCY_INPUT_CAP - FRAME_OVERHEAD_RESERVE,
            "容量 {capacity} 下正文预算必须恰为 时延上限 − 骨架预留"
        );
    }
    // 边界另一侧：容量比时延上限小 1 ⇒ 容量绑定（逐 token 相等）
    assert_eq!(
        input_token_budget(Some(LATENCY_INPUT_CAP as u64 - 1)),
        LATENCY_INPUT_CAP - 1
    );
}

// ---------- 时延回收点：applied=false / degraded 也要记 ----------

#[tokio::test]
async fn latency_lands_in_trace_for_continue_close_and_degraded() {
    // 放行（applied=false）
    let brain = brain_with_quota(0);
    let _ = run_gate(
        &sample_chain(3),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CONTINUE, 0.95))],
        Some(&brain),
        "run-lat-continue",
    )
    .await;
    let t = take_traces("run-lat-continue");
    assert_eq!(t[0].decision, DECISION_CONTINUE);
    assert!(!t[0].applied);
    assert_eq!(t[0].latency_ms, Some(3), "放行也必须记时延（p95 分母）");

    // 收口（applied=true）
    let brain = brain_with_quota(0);
    let _ = run_gate(
        &sample_chain(3),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.95))],
        Some(&brain),
        "run-lat-close",
    )
    .await;
    let t = take_traces("run-lat-close");
    assert_eq!(t[0].decision, DECISION_CLOSE);
    assert!(t[0].applied);
    assert_eq!(t[0].latency_ms, Some(3));

    // 降级（超时：真实时延 1000ms）
    let brain = brain_with_quota(0);
    let _ = run_gate(
        &sample_chain(3),
        triggered_facts(),
        vec![Some(timeout_outcome())],
        Some(&brain),
        "run-lat-degraded",
    )
    .await;
    let t = take_traces("run-lat-degraded");
    assert_eq!(t[0].decision, DECISION_FALLBACK);
    assert_eq!(t[0].degraded.as_deref(), Some("timeout"));
    assert_eq!(
        t[0].latency_ms,
        Some(1000),
        "降级样本必须记时延 —— 漏掉会把 p95 系统性低估"
    );
}

// ---------- Q1 三断：接线层真问过 + 判的是 continue + 最终仍走静态硬墙 ----------

#[tokio::test]
async fn cerebellum_is_asked_once_and_continue_still_hits_the_static_hard_wall() {
    use crate::consumer::message_route_policy::{AutoReplyRoute, judge_chain_reply_route};
    use crate::pkg::collaboration_pressure::{ChainHop, ChainPressure};

    // 窗口撞满 hard_window_depth（缺省 8）⇒ 静态判定必然 Escalate
    let mut pressure = ChainPressure::default();
    let hops: Vec<ChainHop> = (0..8).map(|_| ChainHop::plain()).collect();
    pressure.observe_chain(hops);
    assert_eq!(
        judge_chain_reply_route(&pressure),
        AutoReplyRoute::CloseChainAsNotify {
            reason: "collaboration pressure exhausted"
        }
    );

    // 小脑被判「继续」（高置信）—— 接线层必须真问过一次
    let brain = brain_with_quota(0);
    let facts = ChainPressureFacts {
        chain_depth: 8,
        window_depth: 8,
        fatigue: 60,
        soft_fatigue: 36,
        hard_fatigue: 100,
        hard_window_depth: 8,
    };
    let (verdict, calls) = run_gate(
        &sample_chain(8),
        facts,
        vec![Some(pick_for(OPTION_CONTINUE, 0.95))],
        Some(&brain),
        "run-q1-wiring",
    )
    .await;

    // ① 真问过（不是单元级空转：把 should_ask / quota / 未配任一处改成「不问」这里会红）
    assert_eq!(calls, 1, "接线层必须真问过一次小脑");
    // ② 判的是 continue
    let traces = take_traces("run-q1-wiring");
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_CONTINUE);
    assert_eq!(traces[0].latency_ms, Some(3));
    // ② 必须断「小脑真答了」：调了但超时/报错同样落回静态路由，
    //    只断最终路由是假绿（测的是降级路径，不是 continue 与 Escalate 的冲突）
    assert_eq!(traces[0].degraded, None, "② 该次必须是正常判定，不得是降级");
    // ③ continue 不覆盖 ⇒ 最终路由仍是静态硬墙
    assert!(verdict.is_none(), "continue 不得产出覆盖结论");
    assert_eq!(
        judge_chain_reply_route(&pressure),
        AutoReplyRoute::CloseChainAsNotify {
            reason: "collaboration pressure exhausted"
        },
        "小脑 continue 之下，硬墙收口必须逐字节保持原样"
    );
}
