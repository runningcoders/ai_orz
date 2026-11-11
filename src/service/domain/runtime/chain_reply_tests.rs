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
    /// 每次 `decide` 收到的 `DecisionRequest.state` 原文（运行时反事实的唯一证据面）
    ///
    /// 只捕获 state 不改签名：本场景的 state 是**发给模型的东西**，断言必须落在
    /// 它上面，而不是落在 `Metrics` 中间态上（后者不经过 `prepare()` 的键裁剪）。
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
        requests: Vec<crate::pkg::policy::DecisionRequest>,
    ) -> DecisionOutcome {
        self.calls.fetch_add(1, Ordering::Relaxed);
        {
            let mut states = self.states.lock().expect("mock states lock");
            for request in &requests {
                states.push(request.state.clone());
            }
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
    let (verdict, calls, _) = run_gate_full(messages, facts, script, brain, run_id).await;
    (verdict, calls)
}

/// 同 [`run_gate`]，额外吐出每次 `decide` 收到的 state（运行时反事实用）
///
/// 拆成两个函数而不是改签名：`run_gate` 有 30+ 处调用，改签名会把「一次无关的
/// 机械改动」混进本笔的语义收敛里（回滚面变大）。
async fn run_gate_full(
    messages: &[ChainMessage],
    facts: ChainPressureFacts,
    script: Vec<Option<DecisionOutcome>>,
    brain: Option<&Brain>,
    run_id: &str,
) -> (Option<ChainReplyVerdict>, usize, Vec<serde_json::Value>) {
    run_gate_full_with_pending(messages, facts, script, brain, "", run_id).await
}

/// 带待发正文的同一入口（新增输入侧的用例走这里；旧用例零改动）
async fn run_gate_full_with_pending(
    messages: &[ChainMessage],
    facts: ChainPressureFacts,
    script: Vec<Option<DecisionOutcome>>,
    brain: Option<&Brain>,
    pending: &str,
    run_id: &str,
) -> (Option<ChainReplyVerdict>, usize, Vec<serde_json::Value>) {
    // 全局 Storage 单例：单跑本模块时 `test_ctx()` 依赖它已初始化
    crate::pkg::storage::test_support::init_for_test().await;
    // 轨迹落盘基址：`record_cerebellum` 未初始化时静默丢弃（无副作用设计）
    crate::pkg::think_trace::init(std::env::temp_dir().join("ai_orz_chain_reply_test"));
    let ctx = test_ctx();
    let mock = Arc::new(MockBrainDal::new(script));
    let got = judge_chain_reply(&ctx, mock.clone(), brain, messages, &facts, pending, run_id).await;
    (got, mock.calls(), mock.states())
}

fn take_traces(run_id: &str) -> Vec<ThinkTraceCerebellum> {
    crate::pkg::think_trace::take_cerebellum(run_id)
}

// ---------- 预算分配（容量即预算，无时延反推轴） ----------

/// 测试用默认正文预算：标准 4000 容量 ⇒ 正文池 3500（与改造前 `None` 口径同值）
fn test_body_budget() -> usize {
    body_token_budget(input_token_budget(Some(4_000)))
}

#[test]
fn input_budget_is_provider_capacity_without_a_latency_cap() {
    // 速率轴已删：输入预算恒等于 provider 容量，不再被任何绝对 token 数夹顶
    assert_eq!(input_token_budget(Some(200_000)), 200_000);
    assert_eq!(input_token_budget(Some(128_000)), 128_000);
    assert_eq!(input_token_budget(Some(4_000)), 4_000);
    assert_eq!(input_token_budget(Some(600)), 600);
    // 未配置容量 ⇒ 不裁（容量未配不猜，绝不拍一个默认上限）
    assert_eq!(input_token_budget(None), usize::MAX);
    // 同一场景换两个不同容量的 provider ⇒ 预算必须不同（固定上限的反事实；
    // R49 静态证据：`git show 7ec1d6fc:src/service/domain/runtime/chain_reply.rs`
    // 里 `LATENCY_INPUT_CAP = 4000` 与 `.min(LATENCY_INPUT_CAP)` 均在档）
    assert_ne!(
        input_token_budget(Some(200_000)),
        input_token_budget(Some(4_000)),
        "两个容量的 provider ⇒ 预算必须不同"
    );
    // 骨架按比例派生：@4000 ⇒ 500（与改造前逐位一致），正文 3500
    assert_eq!(context::frame_tokens(4_000), 500);
    assert_eq!(body_token_budget(4_000), 3_500);
    // 小容量由比例收敛（不再被固定 500 骨架打成 0）
    assert_eq!(body_token_budget(300), 263);
    assert_eq!(body_token_budget(0), 0, "输入为 0 时才归零");
    // 条数上限退为防环硬顶（与消费者侧回溯硬上限同口径）
    assert_eq!(MAX_CHAIN_MESSAGES, 32);
}

// ---------- 输入组装（Q3：保头 + 保尾 + 省略标注） ----------

#[test]
fn render_chain_keeps_head_and_tail_and_marks_omission() {
    let chain = sample_chain(32);
    // 显式给一个小预算：预算放到 3500 后 32 条短消息能全装下，
    // 那样就测不到「保头 + 保尾 + 省略标注」这一组行为了（用例意图是截断形态）。
    let input = render_chain(&chain, test_body_budget().min(120));
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
    assert!(matches!(verdict, ChainReplyVerdict::Close { .. }));
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
    assert!(matches!(verdict, ChainReplyVerdict::Close { .. }));
    assert_eq!(calls, 1, "一次 PickOne，不是逐候选各发一题");
    assert_eq!(brain.cerebellum_quota.used(), 1);
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_CLOSE);
    assert!(traces[0].applied, "收口改变了下游路由 ⇒ applied=true");
    assert!(traces[0].confidence.is_some());
}

#[tokio::test]
async fn continue_verdict_lets_the_chain_continue() {
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
    // 软墙：continue 是**有结论**的分支（调用方据此直接走 Peer）
    let verdict = got.expect("continue 必须产出结论，调用方才能跳过静态判定");
    assert!(matches!(verdict, ChainReplyVerdict::Continue { .. }));
    assert_eq!(calls, 1);
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_CONTINUE);
    assert!(
        !traces[0].applied,
        "硬墙之下判 continue：下游本来就是 Peer ⇒ applied=false"
    );
}

/// 硬墙之上判 close：静态判定本就收口 ⇒ 下游没变 ⇒ `applied=false`
///
/// 与 `close_verdict_closes_the_chain`（硬墙之下 close ⇒ true）成对，两条一起
/// 钉住「applied = 是否改变下游」，防它退化回「close 恒 true」。
#[tokio::test]
async fn close_above_hard_window_depth_is_not_applied() {
    let brain = brain_with_quota(10);
    let run_id = "chain-reply-close-above-hard";
    let facts = ChainPressureFacts {
        chain_depth: 10,
        window_depth: 10,
        fatigue: 60,
        soft_fatigue: 36,
        hard_fatigue: 100,
        hard_window_depth: 8,
    };
    let (got, calls) = run_gate(
        &sample_chain(10),
        facts,
        vec![Some(pick_for(OPTION_CLOSE, 0.95))],
        Some(&brain),
        run_id,
    )
    .await;
    assert!(
        matches!(got, Some(ChainReplyVerdict::Close { .. })),
        "该区间小脑判 close 依旧收口（结论不变），变的只是 applied 记账"
    );
    assert_eq!(calls, 1, "该区间仍然每次都问（触发已改「每次都问」）");
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_CLOSE);
    assert!(
        !traces[0].applied,
        "越过硬墙的 close 与静态 Escalate 同结论 ⇒ 下游没变 ⇒ applied=false"
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
        verdict.is_some_and(|v| matches!(v, ChainReplyVerdict::Close { .. })),
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

/// 容量为 0（`max_context_length = 1` ⇒ 1 × 60% = 0）⇒ 零请求、零配额、记 capacity_too_small。
///
/// 判据是结构性的「正文预算 == 0」，不再是「低于某个最小可用 token 数」：
/// 唯一会落到 0 的输入就是容量 0 本身。
#[tokio::test]
async fn zero_capacity_sends_no_request() {
    let brain = brain_with_capacity(1);
    assert_eq!(
        brain
            .cerebellum
            .as_ref()
            .and_then(|po| po.config().recommended_context_length_tokens()),
        Some(0)
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

/// 行为变化实证：容量 90（改造前会被固定 500 骨架打到 0 而永远降级）现在照常问。
///
/// 判据从「低于最小可用 token 数」改成「正文预算 == 0」后，capacity 90 ⇒ 正文 79 > 0
/// ⇒ 必须真发请求。这是「不做多余限制」的直接后果，用断言钉住免得被后续调参改回去。
#[tokio::test]
async fn small_capacity_still_asks_after_the_floor_was_removed() {
    let brain = brain_with_capacity(150);
    assert_eq!(
        brain
            .cerebellum
            .as_ref()
            .and_then(|po| po.config().recommended_context_length_tokens()),
        Some(90)
    );
    assert_eq!(body_token_budget(90), 79, "容量 90 ⇒ 正文 79 > 0 ⇒ 必须问");

    let (verdict, calls) = run_gate(
        &sample_chain(4),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CLOSE, 0.95))],
        Some(&brain),
        "run-capacity-ok",
    )
    .await;
    assert!(
        verdict.is_some_and(|v| matches!(v, ChainReplyVerdict::Close { .. })),
        "容量够时必须正常出结论"
    );
    assert_eq!(calls, 1);
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

// ---------- Q1 三断：接线层真问过 + 判的是 continue + 软墙放行（硬墙降兜底） ----------

#[tokio::test]
async fn cerebellum_continue_overrides_the_static_hard_wall() {
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
    // ③ 软墙（AMan 2026-11-10 定谳，覆盖此前「硬墙绝对护栏」的口径）：
    //    continue 是**有结论**的分支 ⇒ 调用方直接 Peer，静态硬墙此时不生效
    assert!(
        matches!(verdict, Some(ChainReplyVerdict::Continue { .. })),
        "continue 必须产出结论，否则调用方会落回静态 Escalate（硬墙又变成绝对护栏）"
    );
    // ④ 反事实：同一条 pressure 在**没有小脑结论**时仍然 Escalate（兜底不变）
    assert_eq!(
        judge_chain_reply_route(&pressure),
        AutoReplyRoute::CloseChainAsNotify {
            reason: "collaboration pressure exhausted"
        },
        "硬墙只降为兜底：小脑没结论时它必须逐字节保持原样"
    );
    // ⑤ applied 的语义：这一跳下游本是 CloseChainAsNotify，被软墙改回 Peer
    assert!(traces[0].applied, "越过硬墙的放行必须标 applied=true");
}

// ---------- 熔断：探针失败仍 open（前台断言形的另一半） ----------

/// 探针失败：闸必须保持开、失败计数继续累加、周期重新计数。
///
/// 只测「探针成功自愈」会漏掉最危险的形态——端点持续坏时，熔断被一次
/// 探针意外解除（等于每 5 条回复白等一个超时，比不做熔断更糟）。
#[tokio::test]
async fn failed_probe_keeps_the_circuit_open() {
    let brain = brain_with_quota(0);
    let agent_id = brain.agent_id.clone();
    let state = AgentRuntimeStateManager::global();

    // 3 次端点级失败 ⇒ 开闸
    for i in 0..3 {
        let _ = run_gate(
            &sample_chain(4),
            triggered_facts(),
            vec![Some(timeout_outcome())],
            Some(&brain),
            &format!("run-probe-fail-open-{i}"),
        )
        .await;
    }
    assert!(state.is_cerebellum_circuit_open(&agent_id));

    // 周期内 4 次跳过：一次都不许发
    for i in 0..4 {
        let (_, calls) = run_gate(
            &sample_chain(4),
            triggered_facts(),
            vec![],
            Some(&brain),
            &format!("run-probe-fail-skip-{i}"),
        )
        .await;
        assert_eq!(calls, 0, "探针周期未到，第 {} 次仍不许发请求", i + 1);
    }
    assert_eq!(
        state.cerebellum_skipped_since_probe(&agent_id),
        4,
        "4 次跳过之后才轮到探针（周期 5 = 4 跳过 + 1 探针）"
    );

    // 第 5 次 = 真探针，端点仍坏 ⇒ 保持开 + 失败计数继续累加
    let (verdict, calls) = run_gate(
        &sample_chain(4),
        triggered_facts(),
        vec![Some(timeout_outcome())],
        Some(&brain),
        "run-probe-fail-probe",
    )
    .await;
    assert!(verdict.is_none());
    assert_eq!(calls, 1, "到周期必须真探一次");
    assert_eq!(
        state.cerebellum_consecutive_failures(&agent_id),
        4,
        "探针失败必须继续累计（不得因放过一次就清零）"
    );
    assert!(
        state.is_cerebellum_circuit_open(&agent_id),
        "探针失败后闸必须保持开"
    );
    assert_eq!(
        state.cerebellum_skipped_since_probe(&agent_id),
        0,
        "探针后周期计数归零，从头再数"
    );

    // 探针失败后的下一次：重新回到跳过
    let (_, calls) = run_gate(
        &sample_chain(4),
        triggered_facts(),
        vec![],
        Some(&brain),
        "run-probe-fail-after",
    )
    .await;
    assert_eq!(calls, 0, "探针失败后必须回到跳过");
}

// ---------- 接线层护栏：生产入口必须真的转发到本门函数 ----------

/// 生产链路三层：`handle_agent_message` → `judge_chain_reply_by_cerebellum`
/// → `judge_chain_reply_with_default_dao` → `judge_chain_reply`。
///
/// `run_gate` 直接调的是**最内层**门函数（带 mock DAL），所以它证明不了上游
/// 两层真的把请求转过来了：把 `judge_chain_reply_by_cerebellum` 里的转发删掉、
/// 或让 `judge_chain_reply_with_default_dao` 直接 `return None`，本文件其余
/// 用例**全部照绿** —— 这是 Q1 假绿的第二次形态，只是往上游挪了一层。
///
/// 用源码级断言锁住转发（`include_str!` 编译期内嵌，改坏即红；不引入运行期 IO）。
#[test]
fn production_wiring_reaches_the_gate() {
    let consumer = include_str!("../../../consumer/message.rs");
    let domain = include_str!("chain_reply.rs");

    // ① 消费者层：恰好一处调用薄封装
    //    0 处 = 小脑整门在生产不可达（「每次都问」变成「从来不问」）；
    //    多处 = 接线分叉，两处口径迟早漂移。
    let call_sites = consumer
        .matches("judge_chain_reply_with_default_dao(")
        .count();
    assert_eq!(
        call_sites, 1,
        "接线层必须恰好有一处调用薄封装，实际 {call_sites} 处"
    );

    // ② 域层：薄封装必须只是转发（既不能缺 `dal()` 注入，也不能自己造逻辑）
    let with_dao = domain
        .split("pub(crate) async fn judge_chain_reply_with_default_dao")
        .nth(1)
        .expect("薄封装必须存在（删掉它 = 接线层各自装配 DAL）");
    let body = &with_dao[..with_dao.find("\n}").expect("薄封装必须有函数体")];
    assert!(
        body.contains("judge_chain_reply("),
        "薄封装必须转发到门函数，不得提前 return / 自造判断"
    );
    assert!(
        body.contains("dal()"),
        "薄封装必须注入单例 DAL，否则接线与测试各走一套"
    );
}

// ---------- 软墙 + 绝对天花板（AMan 2026-11-10 定谳） ----------

/// 硬墙之上判 continue ⇒ 放行（这是「硬墙降软墙」的直接断言点）
#[tokio::test]
async fn continue_above_hard_window_depth_lets_the_chain_go_on() {
    let brain = brain_with_quota(10);
    let run_id = "run-soft-wall-continue";
    // window_depth = hard(8) + 2
    let facts = ChainPressureFacts {
        chain_depth: 10,
        window_depth: 10,
        fatigue: 60,
        soft_fatigue: 36,
        hard_fatigue: 100,
        hard_window_depth: 8,
    };
    let (verdict, calls) = run_gate(
        &sample_chain(10),
        facts,
        vec![Some(pick_for(OPTION_CONTINUE, 0.95))],
        Some(&brain),
        run_id,
    )
    .await;
    assert!(
        matches!(verdict, Some(ChainReplyVerdict::Continue { .. })),
        "深度越过硬墙也必须以 continue 为准（软墙）"
    );
    assert_eq!(calls, 1, "必须真问过一次（不是绕过判定直接放行）");
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_CONTINUE);
    assert_eq!(traces[0].degraded, None, "该次必须是正常判定，不得是降级");
    assert!(traces[0].applied, "越过硬墙 ⇒ 改变了下游 ⇒ applied=true");
}

/// 同一深度、小脑没结论（超时）⇒ 落回静态硬墙（兜底仍在）
#[tokio::test]
async fn degraded_above_hard_window_depth_still_falls_back_to_the_static_wall() {
    use crate::consumer::message_route_policy::{AutoReplyRoute, judge_chain_reply_route};
    use crate::pkg::collaboration_pressure::{ChainHop, ChainPressure};

    let brain = brain_with_quota(10);
    let facts = ChainPressureFacts {
        chain_depth: 10,
        window_depth: 10,
        fatigue: 60,
        soft_fatigue: 36,
        hard_fatigue: 100,
        hard_window_depth: 8,
    };
    let (verdict, calls) = run_gate(
        &sample_chain(10),
        facts,
        vec![Some(timeout_outcome())],
        Some(&brain),
        "run-soft-wall-degraded",
    )
    .await;
    assert!(verdict.is_none(), "降级不得产出结论");
    assert_eq!(calls, 1);
    let traces = take_traces("run-soft-wall-degraded");
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded.as_deref(), Some("timeout"));

    // 调用方此时走静态判定 ⇒ 硬墙照旧收口（这是「小脑挂了也不会无限聊」的保险）
    let mut pressure = ChainPressure::default();
    let hops: Vec<ChainHop> = (0..10).map(|_| ChainHop::plain()).collect();
    pressure.observe_chain(hops);
    assert_eq!(
        judge_chain_reply_route(&pressure),
        AutoReplyRoute::CloseChainAsNotify {
            reason: "collaboration pressure exhausted"
        }
    );
}

/// 触到绝对天花板 ⇒ 强制收口，且**不问小脑**（零请求 / 零配额 / 留痕）
#[tokio::test]
async fn absolute_ceiling_forces_close_without_asking() {
    let brain = brain_with_quota(10);
    let run_id = "run-ceiling";
    let facts = ChainPressureFacts {
        chain_depth: ABSOLUTE_CEILING,
        window_depth: ABSOLUTE_CEILING,
        fatigue: 80,
        soft_fatigue: 36,
        hard_fatigue: 100,
        hard_window_depth: 8,
    };
    let (verdict, calls) = run_gate(
        &sample_chain(6),
        facts,
        // 剧本给 continue：若实现漏了天花板，这一跳会被放行 ⇒ 断言必红
        vec![Some(pick_for(OPTION_CONTINUE, 0.99))],
        Some(&brain),
        run_id,
    )
    .await;
    assert!(
        matches!(verdict, Some(ChainReplyVerdict::Close { confidence: None })),
        "天花板必须无条件收口，且不是小脑判断（confidence=None）"
    );
    assert_eq!(calls, 0, "结论已定 ⇒ 不该再花一次请求预算");
    assert_eq!(brain.cerebellum_quota.used(), 0, "也不该吃配额");
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1, "强制收口也必须留痕，不许零轨迹");
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded.as_deref(), Some("ceiling_forced"));
    assert!(
        !traces[0].applied,
        "天花板没问小脑、且下游本就收口 ⇒ applied=false（degraded + false 才是「未生效但留痕」）"
    );
    assert_eq!(traces[0].latency_ms, None, "没发生调用 ⇒ latency=None");
}

// ---------- 上下文两字段（AMan 硬要求：链深 + 疲劳必须写进上下文） ----------

/// 生产同一装配点的直调：`state_metrics` 是本场景**唯一** state 装配入口
///
/// 刻意不再保留一份「测试侧平行拼装」：上一版的 `metrics_with` 自己重拼 9 个键，
/// 所以「生产收敛成单键」这件事它**测不出来**（拼装逻辑改了两边不必同步）。
fn metrics_for(facts: &ChainPressureFacts) -> Metrics {
    state_metrics(
        &brain_with_quota(1),
        "[#1] agent-a 文本消息: 正文",
        "",
        facts,
    )
}

/// 渲染出的 prompt / state 里必须**同时**出现链深与疲劳值且非空
///
/// 防「塞进 state 却没渲染进 prompt」这类静默失效：只断言 metrics 里有值
/// 是不够的，必须断言 `prepare()` 产出的请求体里真的有。
#[test]
fn prompt_carries_window_depth_and_fatigue_as_auxiliary() {
    let facts = triggered_facts();
    let brain = brain_with_quota(1);
    let reqs = policy().prepare(&state_metrics(
        &brain,
        "[#1] agent-a 文本消息: 正文",
        "",
        &facts,
    ));
    assert_eq!(reqs.len(), 1, "一次 PickOne 一个请求");

    // 断言**不删只搬**：值仍在 state 里，只是从平铺键搬进了 `## 协作压力` 块正文
    let body = reqs[0].state[context::STATE_KEY]
        .as_str()
        .expect("state 必须是单键分块正文");
    assert!(
        body.contains(&format!("window_depth: {}", facts.window_depth)),
        "state 必须带当前链深度（实际取值，不是键存在）：{body:?}"
    );
    assert!(
        body.contains(&format!("fatigue: {}", facts.fatigue)),
        "state 必须带疲劳值（实际取值）：{body:?}"
    );

    // 判据侧必须写明「仅作辅助参考」，否则模型会拿跳数当硬判据
    assert!(INSTRUCTIONS.contains("window_depth"));
    assert!(INSTRUCTIONS.contains("fatigue"));
    assert!(INSTRUCTIONS.contains("仅作辅助参考"));
}

/// 场景级超时覆盖：chain_reply 声明 5000ms（严格高于全局缺省），不改全局缺省
#[test]
fn scene_declares_a_5s_timeout_override() {
    assert_eq!(SCENE_TIMEOUT_MS, 5_000, "AMan 定谳：本场景请求小脑 5 秒");
    let reqs = policy().prepare(&metrics_for(&triggered_facts()));
    assert_eq!(
        reqs[0].timeout_ms,
        Some(SCENE_TIMEOUT_MS),
        "超时必须由场景显式声明（否则落回全局缺省 3000ms，链内多拉上下文的预算被砍 2 秒）"
    );

    // 三钉字面断言：全局缺省 / 场景声明 / 两者差值，三个数字全部写死。
    // 不做「常量 vs 常量」的比较 —— 那样只证「两值不等」，证不了「缺省被顺手
    // 改成了别的值」（`ea06affb` 把缺省误落成 800 那次，正是靠字面断言在下一轮
    // 被抓出来）。缺省值已由 AMan 续令（`01a1c5c9`）改为 3000ms，原「统一 5 秒」
    // 作废；本场景声明仍是 5000ms 且必须**严格高于**缺省（差值为 0 说明 override
    // 被摘掉，负值说明缺省被抬过了场景声明）。
    assert_eq!(
        crate::service::dao::cerebellum::DEFAULT_CEREBELLUM_TIMEOUT_MS,
        3_000,
        "全局缺省必须是 3000ms（AMan 01a1c5c9 续令）：本场景 5 秒属场景级覆盖"
    );
    assert_eq!(
        SCENE_TIMEOUT_MS - crate::service::dao::cerebellum::DEFAULT_CEREBELLUM_TIMEOUT_MS,
        2_000,
        "场景覆盖必须严格高于全局缺省（差值 2000ms 字面成立，摘掉 override 时立刻红）"
    );
}

// ---------- 上下文单出口（笔：chain_reply 收敛）----------

/// 哨兵：正文必须**逐字**带上（R54 ②「新形态取值正确」的证据面）
const CHAIN_SENTINEL_HEAD: &str = "链头哨兵：这条链在修小脑上下文";
const CHAIN_SENTINEL_TAIL: &str = "尾部哨兵：刚发来的这一跳";

/// 指定**容量**（token）的 Brain：`recommended_context_length` 直给，
/// 避开 `max_context_length × 60%` 的回落，才能精确打在边界点上。
fn brain_with_capacity_tokens(capacity: u64) -> Brain {
    let mut brain = brain_with_quota(4);
    let mut po = mock_provider();
    po.config = serde_json::to_string(&crate::models::model_provider::ModelProviderConfig {
        recommended_context_length: Some(capacity as i32),
        ..Default::default()
    })
    .expect("provider config 必须可序列化");
    brain.cerebellum = Some(po);
    brain
}

/// 摘要预算随 provider 容量变化（同一场景换容量 ⇒ 换预算），不是固定整数
///
/// 改前必红：旧实现 `SNIPPET_CHARS = 160` 是常量，两个容量的取值必然相等。
#[test]
fn snippet_budget_follows_provider_capacity() {
    let small = brain_with_capacity_tokens(4_000);
    let large = brain_with_capacity_tokens(128_000);
    assert_eq!(
        context::snippet_budget(&small, context::share::SNIPPET_BP),
        Some(160),
        "@4000 ⇒ 4% = 160 token（与改造前 160 字符等值）"
    );
    assert_eq!(
        context::snippet_budget(&large, context::share::SNIPPET_BP),
        Some(5_120),
        "@128K ⇒ 4% = 5120 token（改造前恒 160）"
    );
    assert_ne!(
        context::snippet_budget(&small, context::share::SNIPPET_BP),
        context::snippet_budget(&large, context::share::SNIPPET_BP),
        "预算必须随容量变，否则又退化成硬编码绝对值"
    );
    assert_eq!(
        context::snippet_budget(&brain_with_quota(4), context::share::SNIPPET_BP),
        None,
        "容量未配置 ⇒ 不猜预算（不截断）"
    );
}

/// R54 成对断言：形态迁移必须同时断「旧形态已消失」与「新形态取值正确」
///
/// 只断后者时，**双出口共存**仍会全绿（改造前 9 个平铺键还在，新键也照样有值）。
#[tokio::test]
async fn request_state_is_single_rendered_context_block() {
    let brain = brain_with_quota(4);
    let facts = triggered_facts();
    let messages = vec![
        msg("agent-a", "文本消息", CHAIN_SENTINEL_HEAD),
        msg("agent-b", "文本消息", "收到"),
        msg("agent-a", "工具执行结果", CHAIN_SENTINEL_TAIL),
    ];
    let (verdict, calls, states) = run_gate_full(
        &messages,
        facts,
        vec![Some(pick_for(OPTION_CLOSE, 0.95))],
        Some(&brain),
        "run-single-exit",
    )
    .await;

    assert!(matches!(verdict, Some(ChainReplyVerdict::Close { .. })));
    assert_eq!(calls, 1, "应当问且只问一次");
    assert_eq!(states.len(), 1, "应当捕获一份 state");

    let state = &states[0];
    // ① 旧形态（9 键平铺）在该路径**已消失** —— 这是本笔的迁移方向证据
    for key in [
        "agent_id",
        "agent_name",
        "chain_depth",
        "window_depth",
        "fatigue",
        "soft_fatigue",
        "hard_fatigue",
        "hard_window_depth",
        "chain_messages",
    ] {
        assert!(
            state.get(key).is_none(),
            "旧平铺键 `{key}` 必须已从 state 消失（双出口共存不算收敛）：{state}"
        );
    }
    assert_eq!(
        state.as_object().map(|o| o.len()),
        Some(1),
        "state 必须恰一个键（单出口）：{state}"
    );

    // ② 新形态取值正确（实际取值，不是「非空」）
    let body = state[context::STATE_KEY]
        .as_str()
        .expect("state 必须是单键分块正文");
    assert!(
        body.contains(&format!("agent_id: {}", brain.agent_id)),
        "身份块必须带 agent_id 实际取值：{body:?}"
    );
    assert!(
        body.contains(&format!("agent_name: {}", brain.agent_name)),
        "身份块必须带 agent_name 实际取值：{body:?}"
    );
    assert!(body.contains("## 当前场景"), "{body:?}");
    assert!(body.contains(SCENE), "场景块必须写明 scene：{body:?}");
    assert!(body.contains("## 链内消息"), "{body:?}");
    assert!(
        body.contains(CHAIN_SENTINEL_HEAD),
        "链头正文必须逐字带上：{body:?}"
    );
    assert!(
        body.contains(CHAIN_SENTINEL_TAIL),
        "尾部正文必须逐字带上：{body:?}"
    );
    assert!(body.contains("## 协作压力"), "{body:?}");
    assert!(
        body.contains(&format!("chain_depth: {}", facts.chain_depth)),
        "{body:?}"
    );
    assert!(
        body.contains(&format!("window_depth: {}", facts.window_depth)),
        "{body:?}"
    );
    assert!(
        body.contains(&format!("fatigue: {}", facts.fatigue)),
        "{body:?}"
    );
    assert!(
        body.contains(&format!("soft_fatigue: {}", facts.soft_fatigue)),
        "{body:?}"
    );
    assert!(
        body.contains(&format!("hard_fatigue: {}", facts.hard_fatigue)),
        "{body:?}"
    );
    assert!(
        body.contains(&format!("hard_window_depth: {}", facts.hard_window_depth)),
        "{body:?}"
    );
}

/// 块顺序断言：`会话身份` → `当前场景` → `链内消息` → `协作压力`
///
/// 不是洁癖：组织器的截断是**从尾部整块砍**，顺序错了「先砍压力块还是先砍证据块」
/// 就变了 —— 顺序是截断语义的一部分。
#[test]
fn context_block_order_is_identity_scene_chain_pressure() {
    let brain = brain_with_quota(1);
    let rendered = state_bundle(&brain, CHAIN_SENTINEL_HEAD, "", &triggered_facts()).render();
    let at = |needle: &str| {
        rendered
            .find(needle)
            .unwrap_or_else(|| panic!("缺块 {needle}：{rendered:?}"))
    };
    let identity = at("## 会话身份");
    let scene = at("## 当前场景");
    let chain = at("## 链内消息");
    let pressure = at("## 协作压力");
    assert!(
        identity < scene && scene < chain && chain < pressure,
        "块序必须是 身份 < 场景 < 链内消息 < 协作压力（实际 {identity} / {scene} / {chain} / {pressure}）：{rendered:?}"
    );
}

/// 边界五点实测：capacity = 110 / 500 / 1088 / 1500 / 200_000
///
/// 改造前骨架预留是固定 500，capacity=500 时正文预算被打到 0（永远降级）；
/// 改按比例后同类小容量仍能挤出一份可判正文（438）；删掉「最小可判线」后
/// 五点（含 capacity=110 ⇒ 97）全部真发请求 —— 「不问」分支由零容量用例单独覆盖。
///
/// R53 同源自证：每个点先断言**实际走了哪条分支**（calls / states），再断结构——
/// 否则「每块都有非空 body」在「压根没构造请求」的点上也会绿（假绿）。
#[tokio::test]
async fn boundary_capacities_never_emit_a_title_without_body() {
    // 长链：4 条 × 480 字（CJK ≈ 1 token/字）⇒ ≈ 1920 token，足以打满
    // capacity=2500 的正文预算（2000），真正进入「预算裁尾」分支。
    let long = "链内正文".repeat(120);
    let mut messages = vec![msg("agent-a", "文本消息", CHAIN_SENTINEL_HEAD)];
    for i in 0..3 {
        messages.push(msg(
            if i % 2 == 0 { "agent-b" } else { "agent-a" },
            "文本消息",
            &long,
        ));
    }
    messages.push(msg("agent-b", "文本消息", CHAIN_SENTINEL_TAIL));

    for capacity in [110u64, 500, 1_088, 1_500, 200_000] {
        let brain = brain_with_capacity_tokens(capacity);
        let input_budget = input_token_budget(Some(capacity));
        let body_budget = body_token_budget(input_budget);
        // 容量即预算：200_000 ⇒ 骨架 25_000、正文池 175_000（不再被夹顶）
        let expected_body =
            [97usize, 438, 952, 1_313, 175_000][[110u64, 500, 1_088, 1_500, 200_000]
                .iter()
                .position(|c| *c == capacity)
                .unwrap()];
        assert_eq!(
            body_budget, expected_body,
            "capacity={capacity} 的正文预算必须落在实测值上"
        );
        let (_, calls, states) = run_gate_full(
            &messages,
            triggered_facts(),
            vec![Some(pick_for(OPTION_CLOSE, 0.95))],
            Some(&brain),
            &format!("run-boundary-{capacity}"),
        )
        .await;

        // 五个点全部走「真发请求」分支（判据已改为 `body_budget == 0`，
        // capacity 110 ⇒ 97 > 0）；「零容量不问」由 `zero_capacity_sends_no_request` 覆盖。

        assert_eq!(calls, 1, "capacity={capacity} 应当问一次");
        assert_eq!(states.len(), 1, "capacity={capacity} 应当捕获一份 state");
        let body = states[0][context::STATE_KEY]
            .as_str()
            .unwrap_or_else(|| panic!("capacity={capacity}：state 必须是单键字符串"));

        // 结构判据（与组织器既有 tail 判据同源）：每段首行 `## ` 且非空行 ≥ 2
        for part in body.split("\n\n") {
            assert!(
                !part.trim().is_empty(),
                "capacity={capacity} 出现空段落：{body:?}"
            );
            let first = part.lines().next().unwrap_or("");
            assert!(
                first.starts_with("## "),
                "capacity={capacity} 每块首行必须是二级标题：{part:?}"
            );
            let non_empty = part.lines().filter(|l| !l.trim().is_empty()).count();
            assert!(
                non_empty >= 2,
                "capacity={capacity} 每块至少「标题 + 正文」两行：{part:?}"
            );
        }
        assert!(
            body.contains("## 链内消息"),
            "capacity={capacity} 必须带链内消息块：{body:?}"
        );
        // 链头恒保留（定值）；尾部是否保留取决于预算 ⇒ 只断链头
        assert!(
            body.contains(CHAIN_SENTINEL_HEAD),
            "capacity={capacity} 链头必须保留：{body:?}"
        );

        // ★ 设计取舍的实证：同一份内容若交给组织器 `render_within(Some(capacity))`
        // 会怎样（本场景刻意不用它 —— 见 `state_bundle` doc）
        let bundle = state_bundle(
            &brain,
            &render_chain(&messages, body_budget).rendered,
            "",
            &triggered_facts(),
        );
        let actual_tokens = estimate_tokens(&bundle.render());
        // 组织器预算与场景自截共用同一比例表（`share::FRAME_BP` / `share::BODY_BP`）；
        // 两处基准现已一致（都吃 capacity），差别只在裁法：场景侧按 body 预算裁正文、组织器按块整砍。
        let organizer_budget = context::body_budget_tokens(capacity as usize);
        let would_be = bundle.render_within(Some(capacity));
        if actual_tokens > organizer_budget {
            assert!(
                would_be.contains("省略") || would_be.contains("已按模型容量截断"),
                "capacity={capacity}（state {actual_tokens} token > 组织器预算 {organizer_budget}）\
                 必须发生截断，否则本用例没证到冲突区：{would_be:?}"
            );
            // ★ 冲突区最坏形态：尾块整砍会把 `## 链内消息` 这块**主要证据**砍掉 ——
            // 这正是本场景不走组织器截断的理由（比不截断更坏）。
            assert!(
                !would_be.contains(CHAIN_SENTINEL_HEAD),
                "capacity={capacity}：组织器截断应把链内消息整块砍掉（否则冲突区论证不成立）：{would_be:?}"
            );
        } else {
            assert_eq!(
                would_be,
                bundle.render(),
                "capacity={capacity}（state {actual_tokens} token ≤ 组织器预算 {organizer_budget}）不应截断"
            );
            assert!(
                would_be.contains(CHAIN_SENTINEL_HEAD),
                "capacity={capacity}：不截断时链内消息必须在：{would_be:?}"
            );
        }
    }
}

// ══════ 三态（continue / close / silent）独立计数 ══════
//
// AMan 2026-11-11 定谳：「noreply 是让对方不回复，不是不回复对方；直接不回复是
// 一个独立的选项」⇒ ② close 与 ③ silent 是两个独立结论，各自独立计数，
// **不得压成同一个值**（压成同值就算没做）。

/// `silent` 是一个**独立**的第三个结论，不是 `close` 的别名
#[tokio::test]
async fn silent_verdict_is_a_distinct_third_state() {
    let brain = brain_with_quota(10);
    let run_id = "chain-reply-silent";
    let (got, calls) = run_gate(
        &sample_chain(6),
        triggered_facts(),
        vec![Some(pick_for(OPTION_SILENT, 0.95))],
        Some(&brain),
        run_id,
    )
    .await;
    let verdict = got.expect("silent 必须产出结论，调用方才能走 SilentClose（对端零发送）");
    assert!(matches!(verdict, ChainReplyVerdict::Silent { .. }));
    assert_eq!(calls, 1, "一次 PickOne，不是逐候选各发一题");
    assert_eq!(brain.cerebellum_quota.used(), 1);
    let traces = take_traces(run_id);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].decision, DECISION_SILENT);
    assert_ne!(
        traces[0].decision, DECISION_CLOSE,
        "三态不得压成同一个值：silent 与 close 是两个 decision 值"
    );
    assert!(
        traces[0].applied,
        "silent 恒改变下游（墙下静态本 Peer、墙上静态本发知会，都被零发送改变）"
    );
    assert!(traces[0].confidence.is_some());
}

/// R45：新增的 `silent` 计数**不是死的** —— 同一条门对三个选项分别产出三个不同取值
#[tokio::test]
async fn three_states_are_counted_separately() {
    let cases = [
        (OPTION_CONTINUE, DECISION_CONTINUE),
        (OPTION_CLOSE, DECISION_CLOSE),
        (OPTION_SILENT, DECISION_SILENT),
    ];
    for (option, want) in cases {
        let brain = brain_with_quota(10);
        let run_id = format!("chain-reply-3state-{option}");
        let (got, calls) = run_gate(
            &sample_chain(6),
            triggered_facts(),
            vec![Some(pick_for(option, 0.95))],
            Some(&brain),
            &run_id,
        )
        .await;
        assert!(got.is_some(), "有结论的选项必须产出 verdict：{option}");
        assert_eq!(calls, 1, "{option}: 一次请求");
        let traces = take_traces(&run_id);
        assert_eq!(traces.len(), 1, "{option}: 一条留痕");
        assert_eq!(
            traces[0].decision, want,
            "{option}: decision 必须是它自己那一值（不得恒同值 / 恒 0）"
        );
    }
}

/// 降级不变量对 `silent` 同样成立：低置信 ⇒ `None`（走原静态判定，绝不静默吞消息）
#[tokio::test]
async fn low_confidence_silent_falls_back_to_static() {
    let brain = brain_with_quota(10);
    let run_id = "chain-reply-silent-low-confidence";
    let (got, calls) = run_gate(
        &sample_chain(6),
        triggered_facts(),
        vec![Some(pick_for(OPTION_SILENT, 0.5))],
        Some(&brain),
        run_id,
    )
    .await;
    assert!(
        got.is_none(),
        "低置信的 silent 不得生效：必须落回静态判定（宁多发一条，不误静默）"
    );
    assert_eq!(calls, 1);
    let traces = take_traces(run_id);
    assert_eq!(traces[0].decision, DECISION_FALLBACK);
    assert_eq!(traces[0].degraded, Some(DEGRADED_NO_ANSWER.to_string()));
}

/// 选项表形态：三选项、`silent` 与 `close` 不同 id，且旧两值字符串零改动
#[test]
fn options_are_three_and_keep_old_ids() {
    assert_eq!(OPTIONS.len(), 3, "OPTIONS 必须是 3 项");
    let ids: Vec<&str> = OPTIONS.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, vec![OPTION_CONTINUE, OPTION_CLOSE, OPTION_SILENT]);
    assert_eq!(OPTION_CONTINUE, "continue", "旧值字符串零改动");
    assert_eq!(
        OPTION_CLOSE, "close",
        "旧值字符串零改动（② 语义与字符串不变）"
    );
    assert_ne!(OPTION_SILENT, OPTION_CLOSE, "③ 不得与 ② 同 id");
    // 判据文案必须把 ②/③ 的差别写清楚（模型唯一可判的差别）
    let silent_desc = OPTIONS
        .iter()
        .find(|(id, _)| *id == OPTION_SILENT)
        .map(|(_, d)| *d)
        .expect("silent 判据描述");
    assert!(silent_desc.contains("到此为止"), "{silent_desc}");
    let close_desc = OPTIONS
        .iter()
        .find(|(id, _)| *id == OPTION_CLOSE)
        .map(|(_, d)| *d)
        .expect("close 判据描述");
    assert!(
        !close_desc.contains("到此为止"),
        "close 描述零改动：{close_desc}"
    );
}

// ══════ 判定输入补充（AMan 2026-11-11）：最近对话 + 待发消息 ══════
//
// 原任务书的判据输入是「链深度 + 疲劳度 + 上下文」，`state_bundle` 此前**刻意**
// 不带 `Brain.memories`（登记为待裁项，理由见旧 doc）。AMan 2026-11-11 明确两点后
// 两条都转成正式输入：①「最近对话对这个判断很有用」；②「当前消息处理结果，也就是
// 这条默认准备发送的消息」也要能看到。两条都是**输入侧**增量 —— 三态定义、
// ②③ 独立性、计数口径一律不变。

use crate::models::memory::{Memory, MemoryPo, ShortTermMemoryIndexPo};
use common::enums::MemoryStatus;

/// 待发正文的哨兵：唯一色，避免与链内正文里的任何既有字符串撞
const PENDING_SENTINEL: &str = "本条默认准备发送：新增结论 X=42，对方还不知道。";

fn short_term(id: &str, summary: &str) -> Memory {
    Memory::new(MemoryPo::ShortTerm(ShortTermMemoryIndexPo {
        id: id.to_string(),
        agent_id: "agent-test".to_string(),
        task_id: None,
        role: "user".to_string(),
        summary: summary.to_string(),
        tags: "[]".to_string(),
        trace_ids: "[]".to_string(),
        status: MemoryStatus::Active,
        created_at: 0,
        updated_at: 0,
    }))
}

/// 待发正文逐字进 state（`## 待发消息` 取值断言，不是「键存在」断言）
#[tokio::test]
async fn pending_reply_reaches_the_model_verbatim() {
    let brain = brain_with_quota(10);
    let (_, calls, states) = run_gate_full_with_pending(
        &sample_chain(4),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CONTINUE, 0.95))],
        Some(&brain),
        PENDING_SENTINEL,
        "chain-reply-pending-verbatim",
    )
    .await;
    assert_eq!(calls, 1, "有待发正文不影响「问一次」");
    let body = states[0][context::STATE_KEY]
        .as_str()
        .expect("state 必须是单键分块正文");
    assert!(body.contains("## 待发消息"), "必须带待发消息块：{body:?}");
    assert!(
        body.contains(PENDING_SENTINEL),
        "待发正文必须逐字进 state：{body:?}"
    );
    assert!(
        body.contains("## 链内消息"),
        "与链内块共存，不是替换关系：{body:?}"
    );
}

/// 待发正文为空 ⇒ 整块跳过（不留裸标题）
#[tokio::test]
async fn empty_pending_reply_leaves_no_block() {
    let brain = brain_with_quota(10);
    let (_, _, states) = run_gate_full_with_pending(
        &sample_chain(4),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CONTINUE, 0.95))],
        Some(&brain),
        "",
        "chain-reply-pending-empty",
    )
    .await;
    let body = states[0][context::STATE_KEY]
        .as_str()
        .expect("state 必须是单键分块正文");
    assert!(
        !body.contains("## 待发消息"),
        "空待发正文不得留裸标题：{body:?}"
    );
}

/// 超长待发正文按子预算截断并显式标注（不许悄悄丢内容）
#[tokio::test]
async fn oversized_pending_reply_is_truncated_with_annotation() {
    // 必须给一个**有容量**的 provider：容量未配 ⇒ 不裁（容量即预算），
    // 那时待发正文不会被截断，本用例就测不到截断形态了。
    let brain = brain_with_capacity_tokens(4_000);
    let long = "待发正文".repeat(1_200); // 4800 字 ⇒ 远超 600 token
    let (_, _, states) = run_gate_full_with_pending(
        &sample_chain(4),
        triggered_facts(),
        vec![Some(pick_for(OPTION_CONTINUE, 0.95))],
        Some(&brain),
        &long,
        "chain-reply-pending-truncated",
    )
    .await;
    let body = states[0][context::STATE_KEY]
        .as_str()
        .expect("state 必须是单键分块正文");
    assert!(
        body.contains(PENDING_TRUNCATION_SUFFIX),
        "超长必须显式标注截断：{body:?}"
    );
    assert!(!body.contains(&long), "不许把全文塞进来：{body:?}");
    let block = body
        .split("\n\n")
        .find(|part| part.starts_with("## 待发消息"))
        .expect("待发消息块必须存在");
    let used = estimate_tokens(block.trim_start_matches("## 待发消息\n"));
    let pending_cap = context::bp_of(
        input_token_budget(Some(4_000)),
        context::share::PENDING_REPLY_BP,
    );
    assert!(
        used <= pending_cap,
        "待发正文实测 {used} token 超过子预算 {pending_cap}"
    );
}

/// 最近对话块跟 `Brain.memories` 走：空列表整块跳过，非空逐字进正文
#[test]
fn recent_dialogue_block_follows_brain_memories() {
    let mut brain = brain_with_quota(1);
    let facts = triggered_facts();
    let without = state_bundle(&brain, CHAIN_SENTINEL_HEAD, "", &facts).render();
    assert!(
        !without.contains("## 最近对话"),
        "空 memories ⇒ 整块跳过：{without:?}"
    );
    brain.memories = vec![
        short_term("st-d1", "第一条：链内最近对话甲"),
        short_term("st-d2", "第二条：链内最近对话乙"),
    ];
    let with = state_bundle(&brain, CHAIN_SENTINEL_HEAD, "", &facts).render();
    assert!(with.contains("## 最近对话"), "{with:?}");
    assert!(
        with.contains("第一条：链内最近对话甲"),
        "摘要必须逐字在：{with:?}"
    );
    assert!(with.contains("第二条：链内最近对话乙"), "{with:?}");
}

/// 块序（新增两块的位置是截断语义的一部分）
#[test]
fn context_block_order_includes_new_inputs() {
    let mut brain = brain_with_quota(1);
    brain.memories = vec![short_term("st-o1", "最近一句")];
    let rendered = state_bundle(
        &brain,
        CHAIN_SENTINEL_HEAD,
        PENDING_SENTINEL,
        &triggered_facts(),
    )
    .render();
    let at = |needle: &str| {
        rendered
            .find(needle)
            .unwrap_or_else(|| panic!("缺块 {needle}：{rendered:?}"))
    };
    let identity = at("## 会话身份");
    let scene = at("## 当前场景");
    let dialogue = at("## 最近对话");
    let chain = at("## 链内消息");
    let pending = at("## 待发消息");
    let pressure = at("## 协作压力");
    assert!(
        identity < scene
            && scene < dialogue
            && dialogue < chain
            && chain < pending
            && pending < pressure,
        "块序必须是 身份 < 场景 < 最近对话 < 链内消息 < 待发消息 < 协作压力\
         （实际 {identity}/{scene}/{dialogue}/{chain}/{pending}/{pressure}）：{rendered:?}"
    );
}

/// 待发正文的子预算是**同一池里的份额**，不是新增预算
#[test]
fn pending_budget_is_a_share_of_the_same_pool() {
    let input = input_token_budget(Some(4_000)); // 容量即预算
    let body = body_token_budget(input); // 4000 − 500 = 3500
    assert_eq!(body, 3500, "可用正文池不受本笔影响");
    assert_eq!(
        pending_token_budget(input, body),
        600,
        "标准容量由比例表绑定（4000 × 15% = 600）"
    );
    assert_eq!(
        pending_token_budget(400, 400),
        60,
        "小输入由比例绑定（400 × 15% = 60 < 400/4）"
    );
    // 极小正文池：由「正文池 / 4」收敛（不把链内证据挤掉）
    assert_eq!(
        pending_token_budget(input, 100),
        25,
        "正文池极小时按 1/4 收敛"
    );
    // 反事实（R49）：改前本函数只接 body 一个参数、上限是常量 600；
    // 本轮改为两参数比例派生后，同一 body 在不同输入预算下给出不同结果。
    assert_ne!(
        pending_token_budget(4_000, body),
        pending_token_budget(8_000, body),
        "待发预算必须随输入预算变化，不得是固定 600"
    );
}
