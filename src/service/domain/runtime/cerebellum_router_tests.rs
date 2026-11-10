//! cerebellum_router 单元测试（`#[path]` 注册，保住 `mod tests` 层级）
//!
//! 覆盖两个决策点：入口门三分类（Direct / Proceed / Skip）+ 候选池（选中 / 回退）
//! + 两者共享同一本 per-run 配额账（**新增决策点漏检查即在此失败**）。

use super::*;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::policy::{DecisionAnswers, DecisionRequest, DecisionValue, Metrics};
use crate::service::dal::brain::DecisionOutcome;
use async_trait::async_trait;
use common::api::cerebellum::CerebellumUsage;
use common::error::Result;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

// ---------- mock BrainDal（实现全 trait 方法，仅 decide 生效） ----------

/// 按剧本依次作答：`None` / 剧本耗尽 = 该次调用失败（CallError）
///
/// `calls` 是配额断言的唯一依据：配额上限 = N 时，真实调用次数**不得**超过 N。
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
            // 剧本耗尽也算调用失败：调用次数超出预期时，断言会直接炸出来
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
        unimplemented!("not needed by cerebellum router tests")
    }

    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &crate::models::model_provider::ModelProvider,
        _prompt: &str,
    ) -> Result<String> {
        unimplemented!("not needed by cerebellum router tests")
    }

    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &crate::models::brain::Brain,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[crate::models::cortex_types::ToolDescriptor],
        _options: &crate::models::cortex_types::ModelCallOptions,
    ) -> Result<crate::models::cortex_types::ThinkResult> {
        unimplemented!("not needed by cerebellum router tests")
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

/// 测试用 Brain（cerebellum 缺省 None；需要时由调用方注入）
fn mock_brain(cerebellum: Option<ModelProviderPo>) -> Brain {
    let mut brain = Brain::new_local(
        "agent-1".to_string(),
        "Test Agent".to_string(),
        Default::default(),
        mock_provider(),
        vec![],
    );
    brain.cerebellum = cerebellum;
    brain
}

/// 带 per-run 配额的 Brain（与生产装配口径一致：配额挂在 Brain 上）
fn brain_with_quota(limit: usize) -> Brain {
    let mut brain = mock_brain(None);
    brain.cerebellum_quota = Arc::new(crate::models::brain::CerebellumQuota::new(limit));
    brain
}

/// 用指定策略的 qid 装一条 Pick 结论
fn pick_for(policy: &PickOnePolicy, choice: &str, confidence: f32) -> DecisionOutcome {
    DecisionOutcome {
        answers: DecisionAnswers::new().with(
            policy.primary_qid(),
            DecisionValue::Pick {
                chosen: choice.to_string(),
                confidence,
                weights: BTreeMap::new(),
            },
        ),
        degraded: None,
        latency_ms: 8,
        usage: CerebellumUsage {
            input_tokens: 100,
            output_tokens: 10,
        },
    }
}

/// 入口门结论
fn gate_pick(choice: &str, confidence: f32) -> DecisionOutcome {
    pick_for(&entry_gate_policy(), choice, confidence)
}

/// 入口池结论（`id` 必须是 `pool_N`）
fn pool_pick(id: &str, confidence: f32) -> DecisionOutcome {
    pick_for(&pool_policy(), id, confidence)
}

/// 降级结论（DAL 侧判定的失败原因）
fn degraded_outcome(reason: DegradedReason) -> DecisionOutcome {
    DecisionOutcome {
        answers: DecisionAnswers::new(),
        degraded: Some(reason),
        latency_ms: 5,
        usage: CerebellumUsage::default(),
    }
}

async fn run_with(outcome: Option<DecisionOutcome>, user_msg: Option<&str>) -> RouteDecision {
    run_with_brain(outcome, user_msg, mock_brain(None)).await
}

async fn run_with_brain(
    outcome: Option<DecisionOutcome>,
    user_msg: Option<&str>,
    brain: Brain,
) -> RouteDecision {
    run_script(vec![outcome], user_msg, brain).await.0
}

/// 按剧本走完整 `route`（门 → 池），返回结论与**真实决策调用次数**
async fn run_script(
    script: Vec<Option<DecisionOutcome>>,
    user_msg: Option<&str>,
    brain: Brain,
) -> (RouteDecision, usize) {
    let (outcome, calls) = run_script_full(script, user_msg, brain).await;
    (outcome.decision, calls)
}

/// 同上，但保留完整产出：**门与池两条轨迹各自的耗时**
async fn run_script_full(
    script: Vec<Option<DecisionOutcome>>,
    user_msg: Option<&str>,
    brain: Brain,
) -> (RouteOutcome, usize) {
    // 全局 Storage 单例：`test_ctx()` 走 `RequestContext::new`，其 builder
    // 在未显式注入 storage 时回退到全局单例 ⇒ 必须先初始化，否则单跑
    // 本模块时 panic「Storage 尚未初始化」。
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = test_ctx();
    let mock = Arc::new(MockBrainDal::new(script));
    let outcome = route(&ctx, mock.clone(), &brain, user_msg).await;
    (outcome, mock.calls())
}

fn test_ctx() -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx_from_global("test-user")
}

// ---------- 入口门三分类 ----------

#[tokio::test]
async fn test_entry_gate_trivial_then_pool_picked() {
    // 门判无价值 → 串第二次决策选中 pool_1：文案取自池，id 记 pool_1
    let (d, calls) = run_script(
        vec![
            Some(gate_pick(CHOICE_TRIVIAL_DIRECT, 0.95)),
            Some(pool_pick("pool_1", 0.93)),
        ],
        Some("在吗"),
        mock_brain(None),
    )
    .await;
    assert_eq!(calls, 2, "门 + 池 = 两次串行决策");
    assert_eq!(
        d,
        RouteDecision::Direct {
            content: ENTRY_POOL[1].to_string(),
            confidence: 0.95,
            pool: PoolReply::Picked {
                id: "pool_1".to_string(),
                confidence: 0.93,
            },
        }
    );
}

#[tokio::test]
async fn test_route_cortex_needed_proceeds() {
    // 有价值 = 零干预：不再有 System 注入（K3 删 Inject）；池**一次都不调**
    let (d, calls) = run_script(
        vec![Some(gate_pick(CHOICE_CORTEX_NEEDED, 0.92))],
        Some("帮我查一下上个月的报销数据"),
        mock_brain(None),
    )
    .await;
    assert_eq!(calls, 1, "判为有价值时只走门，不问池");
    assert_eq!(d, RouteDecision::Proceed { confidence: 0.92 });
}

#[tokio::test]
async fn test_route_entry_gate_uses_independent_threshold() {
    // 0.89 在旧阈值 0.85 之上、新入口门阈值 0.9 之下：必须判低置信 → Skip。
    // 这条锁死「入口门用自己的 0.9，不复用旧 0.85」。
    let (d, calls) = run_script(
        vec![Some(gate_pick(CHOICE_TRIVIAL_DIRECT, 0.89))],
        Some("在吗"),
        mock_brain(None),
    )
    .await;
    assert_eq!(calls, 1, "低置信不进池");
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::LowConfidence
        }
    );
}

#[tokio::test]
async fn test_route_low_confidence_skips() {
    let d = run_with(Some(gate_pick(CHOICE_CORTEX_NEEDED, 0.5)), Some("随便聊聊")).await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::LowConfidence
        }
    );
}

// ---------- 降级路径 ----------

#[tokio::test]
async fn test_route_no_user_message_skips() {
    let d = run_with(None, None).await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::NoUserMessage
        }
    );
}

#[tokio::test]
async fn test_route_call_error_skips() {
    let d = run_with(None, Some("在吗")).await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::CallError
        }
    );
}

#[tokio::test]
async fn test_route_timeout_skips() {
    // 超时判定已下沉到 DAL（单一超时预算），router 只把 degraded 翻成 Skip
    let d = run_with(
        Some(degraded_outcome(DegradedReason::Timeout)),
        Some("在吗"),
    )
    .await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::Timeout
        }
    );
}

#[tokio::test]
async fn test_route_unsupported_answer_skips() {
    // 非 Pick 结论（如 Grade）→ UnsupportedAnswer
    let outcome = DecisionOutcome {
        answers: DecisionAnswers::new().with(
            entry_gate_policy().primary_qid(),
            DecisionValue::Grade {
                score: 3.0,
                confidence: 0.99,
            },
        ),
        degraded: None,
        latency_ms: 5,
        usage: CerebellumUsage::default(),
    };
    let d = run_with(Some(outcome), Some("在吗")).await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::UnsupportedAnswer
        }
    );
}

// ---------- 入口池回退路径（唯一例外：门已放行直回，必须给出一句） ----------

#[tokio::test]
async fn test_entry_pool_low_confidence_falls_back_to_most_neutral() {
    let (d, _) = run_script(
        vec![
            Some(gate_pick(CHOICE_TRIVIAL_DIRECT, 0.95)),
            Some(pool_pick("pool_1", ENTRY_POOL_CONFIDENCE_THRESHOLD - 0.2)),
        ],
        Some("在吗"),
        mock_brain(None),
    )
    .await;
    assert_eq!(
        d,
        RouteDecision::Direct {
            content: ENTRY_POOL[0].to_string(),
            confidence: 0.95,
            pool: PoolReply::Fallback {
                reason: DegradedReason::LowConfidence
            },
        }
    );
}

#[tokio::test]
async fn test_entry_pool_unknown_id_falls_back() {
    // 模型编造池外 id（pool_9）：不得拿它去查文案，按「没有结论」回退
    let (d, _) = run_script(
        vec![
            Some(gate_pick(CHOICE_TRIVIAL_DIRECT, 0.99)),
            Some(pool_pick("pool_9", 0.99)),
        ],
        Some("在吗"),
        mock_brain(None),
    )
    .await;
    assert_eq!(
        d,
        RouteDecision::Direct {
            content: ENTRY_POOL[0].to_string(),
            confidence: 0.99,
            pool: PoolReply::Fallback {
                reason: DegradedReason::UnsupportedAnswer
            },
        }
    );
}

#[tokio::test]
async fn test_entry_pool_degraded_falls_back() {
    let (d, _) = run_script(
        vec![
            Some(gate_pick(CHOICE_TRIVIAL_DIRECT, 0.99)),
            Some(degraded_outcome(DegradedReason::Timeout)),
        ],
        Some("在吗"),
        mock_brain(None),
    )
    .await;
    assert_eq!(
        d,
        RouteDecision::Direct {
            content: ENTRY_POOL[0].to_string(),
            confidence: 0.99,
            pool: PoolReply::Fallback {
                reason: DegradedReason::Timeout
            },
        }
    );
}

// ---------- 配额：两个决策点共享同一本 per-run 账 ----------

#[tokio::test]
async fn test_entry_decisions_share_one_quota_budget() {
    // 配额 2 = 门一次 + 池一次：两次都真发请求，额度用尽
    let brain = brain_with_quota(2);
    let probe = brain.clone();
    let (d, calls) = run_script(
        vec![
            Some(gate_pick(CHOICE_TRIVIAL_DIRECT, 0.99)),
            Some(pool_pick("pool_2", 0.97)),
        ],
        Some("在吗"),
        brain,
    )
    .await;
    assert_eq!(
        calls, 2,
        "两个决策点各发一次：新增决策点漏过配额检查会在这里露出来"
    );
    assert_eq!(probe.cerebellum_quota.used(), 2);
    assert_eq!(
        d,
        RouteDecision::Direct {
            content: ENTRY_POOL[2].to_string(),
            confidence: 0.99,
            pool: PoolReply::Picked {
                id: "pool_2".to_string(),
                confidence: 0.97,
            },
        }
    );
}

#[tokio::test]
async fn test_entry_pool_quota_exhausted_falls_back_without_call() {
    // 配额 1：门吃掉唯一额度 ⇒ 池**一次请求都不发**，直接回退最中性一条。
    // 剧本里故意放一条会选中 pool_2 的结论——若池真发了请求，结果就会是 Picked。
    let brain = brain_with_quota(1);
    let probe = brain.clone();
    let (d, calls) = run_script(
        vec![
            Some(gate_pick(CHOICE_TRIVIAL_DIRECT, 0.99)),
            Some(pool_pick("pool_2", 0.99)),
        ],
        Some("在吗"),
        brain,
    )
    .await;
    assert_eq!(calls, 1, "额度已耗尽：池不得再发请求");
    assert_eq!(probe.cerebellum_quota.used(), 1);
    assert_eq!(
        d,
        RouteDecision::Direct {
            content: ENTRY_POOL[0].to_string(),
            confidence: 0.99,
            pool: PoolReply::Fallback {
                reason: DegradedReason::BudgetExhausted
            },
        }
    );
}

#[tokio::test]
async fn test_route_quota_exhausted_skips_without_call() {
    // 配额 1 次且已被占满：门必须直接降级，**一次请求都不发**。
    // 这里故意给一个会判 Direct 的结论——若真发了请求，结果就会是 Direct。
    let brain = brain_with_quota(1);
    assert!(brain.cerebellum_quota.try_acquire());
    let d = run_with_brain(
        Some(gate_pick(CHOICE_TRIVIAL_DIRECT, 0.99)),
        Some("在吗"),
        brain,
    )
    .await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::BudgetExhausted
        }
    );
}

// ---------- 池结构与纯函数翻译边界 ----------

#[test]
fn test_entry_pool_ids_align_with_texts() {
    assert_eq!(
        ENTRY_POOL_IDS.len(),
        ENTRY_POOL.len(),
        "id 与文案必须一一对应"
    );
    for (i, id) in ENTRY_POOL_IDS.iter().enumerate() {
        assert_eq!(*id, format!("pool_{i}"), "池 id 必须是 pool_N 且与下标一致");
    }
    assert!((3..=5).contains(&ENTRY_POOL.len()), "池容量 3~5 条（K8）");
}

/// 把一条结论装进事实桶（translate 只看桶，不看原始 answers）
fn metrics_with(value: DecisionValue) -> Metrics {
    let policy = entry_gate_policy();
    Metrics::new().with_decisions(Arc::new(
        DecisionAnswers::new().with(policy.primary_qid(), value),
    ))
}

#[test]
fn test_translate_unknown_choice_skips() {
    let metrics = metrics_with(DecisionValue::Pick {
        chosen: "unknown_choice".to_string(),
        confidence: 0.99,
        weights: BTreeMap::new(),
    });
    assert_eq!(
        translate_gate(&entry_gate_policy(), &metrics),
        GateVerdict::Skip {
            reason: DegradedReason::UnsupportedAnswer
        }
    );
}

#[test]
fn test_translate_missing_answer_skips() {
    // 没问出来（None）必须走 Skip —— 绝不能默认成 Direct 或 Proceed
    assert_eq!(
        translate_gate(&entry_gate_policy(), &Metrics::new()),
        GateVerdict::Skip {
            reason: DegradedReason::UnsupportedAnswer
        }
    );
}

#[test]
fn test_translate_low_confidence_skips() {
    // 有结论但置信不足：与「压根没答」要区分开（观测口径不同）
    let metrics = metrics_with(DecisionValue::Pick {
        chosen: CHOICE_TRIVIAL_DIRECT.to_string(),
        confidence: ENTRY_GATE_CONFIDENCE_THRESHOLD - 0.1,
        weights: BTreeMap::new(),
    });
    assert_eq!(
        translate_gate(&entry_gate_policy(), &metrics),
        GateVerdict::Skip {
            reason: DegradedReason::LowConfidence
        }
    );
}

#[test]
fn test_degraded_reason_str() {
    // 稳定字符串口径（日志/前端展示）。`Disabled` 已无生产方（小脑开关整体取消），
    // 保留变体只为解释历史日志。
    assert_eq!(DegradedReason::NoProvider.as_str(), "no_provider");
    assert_eq!(DegradedReason::Disabled.as_str(), "disabled");
    assert_eq!(DegradedReason::Timeout.as_str(), "timeout");
}

// ---------- 轨迹耗时：门 / 池两条独立轨迹，各记各的 ----------

#[tokio::test]
async fn test_gate_and_pool_latency_are_traced_separately() {
    // 剧本自报 8ms（`pick_for`）：两次调用各自 8ms，两列都必须取到实测值
    let (outcome, calls) = run_script_full(
        vec![
            Some(gate_pick(CHOICE_TRIVIAL_DIRECT, 0.95)),
            Some(pool_pick("pool_1", 0.93)),
        ],
        Some("在吗"),
        mock_brain(None),
    )
    .await;
    assert_eq!(calls, 2, "门 + 池 = 两次串行决策");
    assert_eq!(
        outcome.gate_latency_ms,
        Some(8),
        "门侧耗时必须是实测值（改造前轨迹固定 None 会红）"
    );
    assert_eq!(
        outcome.pool_latency_ms,
        Some(8),
        "池侧耗时必须是实测值，且与门侧分开记"
    );
}

#[tokio::test]
async fn test_pool_latency_is_none_when_gate_proceeds() {
    // 门判「有价值」→ 池**一次都不调**：池耗时必须是 None，不是 Some(0)
    let (outcome, calls) = run_script_full(
        vec![Some(gate_pick(CHOICE_CORTEX_NEEDED, 0.92))],
        Some("帮我查一下上个月的报销数据"),
        mock_brain(None),
    )
    .await;
    assert_eq!(calls, 1);
    assert_eq!(outcome.gate_latency_ms, Some(8));
    assert_eq!(
        outcome.pool_latency_ms, None,
        "没走池 ⇒ None（记成 0ms 会污染池侧的 p95）"
    );
}

#[tokio::test]
async fn test_zero_call_early_return_has_no_latency_at_all() {
    // 无用户消息：本地早退，零调用 ⇒ 两列都 None
    let (outcome, calls) = run_script_full(vec![], None, mock_brain(None)).await;
    assert_eq!(calls, 0);
    assert_eq!(outcome.gate_latency_ms, None);
    assert_eq!(outcome.pool_latency_ms, None);
}

#[tokio::test]
async fn test_no_provider_degrade_is_not_counted_as_a_call() {
    // NoProvider 是**零调用门**：即便剧本自报 5ms，轨迹也必须是 None
    // —— 这条锁死「靠变体判，而不是靠 latency>0 判」
    let (outcome, calls) = run_script_full(
        vec![Some(degraded_outcome(DegradedReason::NoProvider))],
        Some("在吗"),
        mock_brain(None),
    )
    .await;
    assert_eq!(calls, 1);
    assert_eq!(
        outcome.gate_latency_ms, None,
        "NoProvider = 没真正问 ⇒ 轨迹耗时必须 None"
    );
}

#[tokio::test]
async fn test_timeout_degrade_keeps_measured_latency() {
    // 超时是**真实调用**：付了时延，轨迹要记实测值（这里剧本自报 5ms）
    let (outcome, _) = run_script_full(
        vec![Some(degraded_outcome(DegradedReason::Timeout))],
        Some("在吗"),
        mock_brain(None),
    )
    .await;
    assert_eq!(
        outcome.gate_latency_ms,
        Some(5),
        "超时也要记耗时（否则 p95 只能看到成功样本）"
    );
}

/// 运行时反事实（笔④-2 笔 B）：entry_gate 请求体收敛为单一 `context` 分块正文。
///
/// 改造前必红（R49）：改造前 `ENTRY_STATE_KEYS` 是三键白名单，`collect_state`
/// 产出 `{agent_id, agent_name, last_user_message}`，无 `context` 键。
#[tokio::test]
async fn test_request_state_is_single_rendered_context_block() {
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = test_ctx();
    let brain = mock_brain(Some(mock_provider()));
    let mock = Arc::new(MockBrainDal::new(vec![None]));
    let sentinel = "运行时反事实哨兵-entry";
    let _ = route(&ctx, mock.clone(), &brain, Some(sentinel)).await;

    let states = mock.states();
    assert_eq!(states.len(), 1, "首轮应恰发一条请求：{states:?}");
    let state = &states[0];
    assert!(
        state.get("agent_id").is_none(),
        "平铺键必须已消失（单出口不变量）：{state}"
    );
    let rendered = state["context"]
        .as_str()
        .expect("context 必须是渲染后的字符串正文");
    assert!(rendered.contains("## 会话身份"), "必须有身份块：{rendered}");
    assert!(
        rendered.contains("agent_id: agent-1"),
        "身份必须带实际取值（不是「非空」）：{rendered}"
    );
    assert!(
        rendered.contains(sentinel),
        "用户消息必须逐字进正文：{rendered}"
    );
}
