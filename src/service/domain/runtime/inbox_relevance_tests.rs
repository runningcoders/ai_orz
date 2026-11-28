//! tests 单元测试（拆分自 inbox_relevance.rs）

use super::*;
use crate::models::brain::Brain;
use crate::pkg::RequestContext;
use crate::pkg::policy::decision::DecisionAnswers;
use crate::service::dal::brain::{BrainDal, DecisionOutcome};
use common::api::cerebellum::{CerebellumUsage, DegradedReason};
use std::sync::atomic::{AtomicUsize, Ordering};

fn anchor(content: &str) -> InboxAnchor {
    InboxAnchor {
        agent_id: "agent-1".to_string(),
        from_id: "sender-1".to_string(),
        from_role: common::enums::MessageRole::User,
        project_id: None,
        task_id: None,
        thread_id: None,
        message_type: common::enums::MessageType::Text,
        created_at: 1_000,
        id: "msg-anchor".to_string(),
        content: content.to_string(),
        organization_id: Some("org-1".to_string()),
    }
}

fn candidate(id: &str, content: &str) -> Message {
    Message::from_po(crate::models::message::MessagePo {
        id: id.to_string(),
        content: content.to_string(),
        ..Default::default()
    })
}

fn pick(weights: BTreeMap<String, f32>) -> DecisionOutcome {
    DecisionOutcome {
        answers: DecisionAnswers::new().with(
            QUESTION_ID,
            DecisionValue::Pick {
                chosen: String::new(),
                confidence: 1.0,
                weights,
            },
        ),
        degraded: None,
        latency_ms: 0,
        usage: CerebellumUsage::default(),
    }
}

/// 可替换 DAL：只关心 decide 的调用次数与返回，其余方法与本场景无关
struct ScriptedBrainDal {
    outcome: Option<DecisionOutcome>,
    calls: AtomicUsize,
}

impl ScriptedBrainDal {
    fn new(outcome: Option<DecisionOutcome>) -> Self {
        Self {
            outcome,
            calls: AtomicUsize::new(0),
        }
    }
    fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl BrainDal for ScriptedBrainDal {
    async fn decide(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        _requests: Vec<DecisionRequest>,
    ) -> DecisionOutcome {
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.outcome.clone().unwrap_or_else(|| DecisionOutcome {
            answers: DecisionAnswers::new(),
            degraded: Some(DegradedReason::CallError),
            latency_ms: 0,
            usage: CerebellumUsage::default(),
        })
    }
    async fn wake_brain(
        &self,
        _ctx: RequestContext,
        _agent: &crate::models::agent::AgentPo,
        _memories: Vec<crate::models::memory::Memory>,
    ) -> common::error::Result<Brain> {
        unimplemented!("not needed by inbox relevance tests")
    }
    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &crate::models::model_provider::ModelProvider,
        _prompt: &str,
    ) -> common::error::Result<String> {
        unimplemented!("not needed by inbox relevance tests")
    }
    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[crate::models::cortex_types::ToolDescriptor],
        _options: &crate::models::cortex_types::ModelCallOptions,
    ) -> common::error::Result<crate::models::cortex_types::ThinkResult> {
        unimplemented!("not needed by inbox relevance tests")
    }
    async fn embed_entity(
        &self,
        _ctx: RequestContext,
        _entity: &dyn crate::models::vector::Vectorizable,
    ) -> common::error::Result<Option<crate::models::vector::VectorIndexParams>> {
        Ok(None)
    }
    async fn embed_text_for_search(
        &self,
        _ctx: RequestContext,
        _text: &str,
    ) -> common::error::Result<Option<crate::models::vector::VectorIndexParams>> {
        Ok(None)
    }
}

fn mock_provider() -> crate::models::model_provider::ModelProviderPo {
    crate::models::model_provider::ModelProviderPo::new(
        "p".to_string(),
        common::enums::ProviderType::Jev,
        common::enums::ModelCapability::Decision,
        "m".to_string(),
        "k".to_string(),
        None,
        None,
        "t".to_string(),
    )
}

/// 有 cerebellum 的 Brain（走真实 decide 分支）
fn brain_with_cerebellum() -> Brain {
    let mut brain = Brain::new_local(
        "agent-1".to_string(),
        "小美".to_string(),
        Default::default(),
        mock_provider(),
        Vec::new(),
    );
    brain.cerebellum = Some(mock_provider());
    brain
}

#[test]
fn probability_threshold_is_inclusive() {
    assert!(is_relevant_probability(INBOX_RELEVANCE_MIN_PROBABILITY));
    assert!(!is_relevant_probability(
        INBOX_RELEVANCE_MIN_PROBABILITY - 0.01
    ));
    assert!(is_relevant_probability(1.0));
    assert!(!is_relevant_probability(0.0));
}

#[test]
fn cache_key_separates_anchor_candidate_and_situation() {
    let empty = situation_fingerprint(&InboxJudgeContext::default());
    assert_eq!(cache_key("a1", "m1", empty), format!("a1|m1|{empty}"));
    assert_ne!(cache_key("a1", "m1", empty), cache_key("a1", "m2", empty));
    assert_ne!(cache_key("a1", "m1", empty), cache_key("a2", "m1", empty));

    // 稳定现场变了 ⇒ 指纹变、缓存键变；只变「最近动作」⇒ 指纹不变
    let base = InboxJudgeContext {
        situation: vec![
            "【任务上下文】摘要：修登录页".to_string(),
            "最近动作：读代码".to_string(),
        ],
        ..Default::default()
    };
    let changed_action = InboxJudgeContext {
        situation: vec![
            "【任务上下文】摘要：修登录页".to_string(),
            "最近动作：写代码".to_string(),
            "消息序列：共 20 条".to_string(),
        ],
        ..Default::default()
    };
    let changed_task = InboxJudgeContext {
        situation: vec!["【任务上下文】摘要：修支付页".to_string()],
        ..Default::default()
    };
    assert_eq!(
        situation_fingerprint(&base),
        situation_fingerprint(&changed_action)
    );
    assert_ne!(
        situation_fingerprint(&base),
        situation_fingerprint(&changed_task)
    );
}

#[test]
fn verdict_cache_hits_and_invalidates() {
    let cache = verdict_cache();
    let key = "test-anchor|test-message|0".to_string();
    cache.invalidate(&key);
    assert_eq!(cache.get(&key), None);

    cache.insert(key.clone(), true);
    assert_eq!(cache.get(&key), Some(true));
    cache.insert(key.clone(), false);
    assert_eq!(cache.get(&key), Some(false));

    assert!(cache.invalidate(&key));
    assert_eq!(cache.get(&key), None);
}

#[test]
fn weights_filter_uses_threshold_and_ignores_sentinel() {
    let ids = BTreeSet::from(["m1".to_string(), "m2".to_string(), "m3".to_string()]);
    let weights = BTreeMap::from([
        ("m1".to_string(), 0.6),
        ("m2".to_string(), 0.45),
        ("m3".to_string(), 0.05),
        (NONE_OPTION_ID.to_string(), 0.5),
    ]);
    // m2 过绝对阈值但没过哨兵，被拒；m3 连噪声地板都没过
    let hit = related_ids_from_weights(&weights, &ids);
    assert_eq!(hit, BTreeSet::from(["m1".to_string()]));
}

#[test]
fn weights_filter_missing_weight_or_sentinel_fails_open() {
    // 权重缺失与哨兵缺失同为答案不完整：一律放行，口径统一
    let ids = BTreeSet::from(["m1".to_string(), "m2".to_string()]);
    let weights = BTreeMap::from([("m1".to_string(), 0.9)]);
    assert_eq!(related_ids_from_weights(&weights, &ids), ids);
}

#[test]
fn weights_filter_multiple_related_candidates_survive_dilution() {
    // 五个相关候选 + 哨兵，概率归一到 0.17，仍应全部放行
    let ids = BTreeSet::from(["m1", "m2", "m3", "m4", "m5"].map(str::to_string));
    let weights = BTreeMap::from([
        ("m1".to_string(), 0.17),
        ("m2".to_string(), 0.17),
        ("m3".to_string(), 0.17),
        ("m4".to_string(), 0.17),
        ("m5".to_string(), 0.17),
        (NONE_OPTION_ID.to_string(), 0.15),
    ]);
    assert_eq!(related_ids_from_weights(&weights, &ids), ids);
}

#[test]
fn single_candidate_is_a_multi_choice_with_sentinel() {
    let ids = BTreeSet::from(["m1".to_string()]);
    let related = BTreeMap::from([("m1".to_string(), 0.8), (NONE_OPTION_ID.to_string(), 0.2)]);
    assert_eq!(related_ids_from_weights(&related, &ids), ids);

    let unrelated = BTreeMap::from([("m1".to_string(), 0.2), (NONE_OPTION_ID.to_string(), 0.8)]);
    assert!(related_ids_from_weights(&unrelated, &ids).is_empty());
}

#[test]
fn snippet_truncates_by_char_not_byte() {
    let short = "登录页打不开";
    assert_eq!(snippet(short, 400), short);

    let long = "燃".repeat(500);
    let cut = snippet(&long, 10);
    assert_eq!(cut.chars().count(), 10 + "…（已截断）".chars().count());
    assert!(cut.ends_with("…（已截断）"));
}

#[test]
fn build_context_carries_identity_scene_and_situation() {
    let a = anchor("帮我看登录页");
    let judge = InboxJudgeContext {
        agent_id: "agent-1".to_string(),
        agent_name: "前台小美".to_string(),
        scene: "Awaken".to_string(),
        situation: vec![
            "【任务上下文】摘要：修复登录页打不开".to_string(),
            "最近动作（时间正序）：读取了登录页代码".to_string(),
        ],
    };
    let rendered = build_context(&a, &judge);

    assert!(rendered.contains("agent_name: 前台小美"));
    assert!(rendered.contains("## 当前场景\nAwaken"));
    assert!(rendered.contains("帮我看登录页"), "锚点正文须进入上下文");
    assert!(
        rendered.contains("修复登录页打不开"),
        "任务摘要须进入上下文"
    );
    assert!(
        rendered.contains("读取了登录页代码"),
        "最近动作须进入上下文"
    );
    assert!(rendered.contains("from_id"), "结构上下文须保留供同源佐证");
}

/// 空现场上下文不得产出空标题块
#[test]
fn build_context_skips_empty_situation_block() {
    let rendered = build_context(&anchor("继续"), &InboxJudgeContext::default());
    assert!(!rendered.contains("## 当前思考现场"));
    assert!(rendered.contains("继续"));
}

/// `tag` 生成唯一 anchor / 候选 ID：测试并行跑时缓存键互不干扰
async fn run_rank(
    tag: &str,
    outcome: Option<DecisionOutcome>,
    brain: Option<&Brain>,
) -> (BTreeSet<String>, usize, String) {
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = RequestContext::builder().build();
    let mut a = anchor("帮我看登录页");
    a.id = format!("anchor-{tag}");
    let mid = format!("m-{tag}");
    let candidates = vec![candidate(&mid, "顺带把接口也修了")];
    let judge = InboxJudgeContext::default();
    let key = cache_key(&a.id, &mid, situation_fingerprint(&judge));
    verdict_cache().invalidate(&key);

    let dal = ScriptedBrainDal::new(outcome);
    let hit = rank_related(&ctx, &a, &candidates, &judge, brain, &dal).await;
    let calls = dal.calls();
    (hit, calls, key)
}

/// 小脑缺席时整批放行，并写缓存（用户拍板的降级口径）
#[tokio::test]
async fn rank_related_without_brain_admits_and_caches_all() {
    let (hit, calls, key) = run_rank("no-brain", None, None).await;
    assert_eq!(hit, BTreeSet::from(["m-no-brain".to_string()]));
    assert_eq!(calls, 0, "无 Brain 时零调用");
    assert_eq!(verdict_cache().get(&key), Some(true), "降级放行须写缓存");
    verdict_cache().invalidate(&key);
}

/// 正常 Pick：按权重筛选并把正反两种结论都写缓存
#[tokio::test]
async fn rank_related_normal_pick_caches_both_verdicts() {
    let mid = "m-normal-pick".to_string();
    let (hit, calls, key) = run_rank(
        "normal-pick",
        Some(pick(BTreeMap::from([
            (mid.clone(), 0.8),
            (NONE_OPTION_ID.to_string(), 0.2),
        ]))),
        Some(&brain_with_cerebellum()),
    )
    .await;
    assert_eq!(hit, BTreeSet::from([mid]));
    assert_eq!(calls, 1);
    assert_eq!(verdict_cache().get(&key), Some(true));
    verdict_cache().invalidate(&key);
}

/// degrade / 无答案 / 权重缺失三类降级：一律放行并写缓存，调用只发一次
#[tokio::test]
async fn rank_related_degraded_outcomes_fail_open_and_cache() {
    let brain = brain_with_cerebellum();
    for (tag, outcome) in [
        (
            "degraded",
            DecisionOutcome {
                answers: DecisionAnswers::new(),
                degraded: Some(DegradedReason::CallError),
                latency_ms: 0,
                usage: CerebellumUsage::default(),
            },
        ),
        (
            "no-answer",
            DecisionOutcome {
                answers: DecisionAnswers::new(),
                degraded: None,
                latency_ms: 0,
                usage: CerebellumUsage::default(),
            },
        ),
        (
            "missing-weight",
            // 权重缺失：判据层 fail-open
            pick(BTreeMap::from([(NONE_OPTION_ID.to_string(), 0.9)])),
        ),
    ] {
        let (hit, calls, key) = run_rank(tag, Some(outcome), Some(&brain)).await;
        assert_eq!(hit, BTreeSet::from([format!("m-{tag}")]), "降级须放行");
        assert_eq!(calls, 1);
        assert_eq!(verdict_cache().get(&key), Some(true), "降级放行须写缓存");
        verdict_cache().invalidate(&key);
    }
}

/// 缓存命中不再调用小脑：相关与不相关都缓存
#[tokio::test]
async fn rank_related_cache_hit_skips_decide() {
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = RequestContext::builder().build();
    let mut a = anchor("帮我看登录页");
    a.id = "anchor-cache-hit".to_string();
    let mid = "m-cache-hit".to_string();
    let candidates = vec![candidate(&mid, "接口也修一下")];
    let judge = InboxJudgeContext::default();
    let key = cache_key(&a.id, &mid, situation_fingerprint(&judge));

    for cached in [true, false] {
        verdict_cache().insert(key.clone(), cached);
        let dal = ScriptedBrainDal::new(None);
        let brain = brain_with_cerebellum();
        let hit = rank_related(&ctx, &a, &candidates, &judge, Some(&brain), &dal).await;
        assert_eq!(dal.calls(), 0, "缓存命中不应调用小脑");
        assert_eq!(hit.contains(&mid), cached);
    }
    verdict_cache().invalidate(&key);
}

/// 配额耗尽：零调用降级并写缓存
#[tokio::test]
async fn rank_related_quota_exhausted_admits_and_caches() {
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = RequestContext::builder().build();
    let mut a = anchor("帮我看登录页");
    a.id = "anchor-quota".to_string();
    let mid = "m-quota".to_string();
    let candidates = vec![candidate(&mid, "接口也修一下")];
    let judge = InboxJudgeContext::default();
    let key = cache_key(&a.id, &mid, situation_fingerprint(&judge));
    verdict_cache().invalidate(&key);

    let mut brain = brain_with_cerebellum();
    // limit=1 且先占用一次 ⇒ 本轮 try_acquire 必然失败（0 表示不限，不能用）
    brain.cerebellum_quota = std::sync::Arc::new(crate::models::brain::CerebellumQuota::new(1));
    assert!(brain.cerebellum_quota.try_acquire());
    let dal = ScriptedBrainDal::new(None);
    let hit = rank_related(&ctx, &a, &candidates, &judge, Some(&brain), &dal).await;

    assert_eq!(hit, BTreeSet::from([mid]));
    assert_eq!(dal.calls(), 0, "配额耗尽时零调用");
    assert_eq!(verdict_cache().get(&key), Some(true));
    verdict_cache().invalidate(&key);
}
