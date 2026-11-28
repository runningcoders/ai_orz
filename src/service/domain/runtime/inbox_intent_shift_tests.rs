//! tests 单元测试（拆分自 inbox_intent_shift.rs）

use super::*;
use crate::models::brain::Brain;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::policy::decision::DecisionAnswers;
use crate::service::dal::brain::DecisionOutcome;
use common::api::cerebellum::{CerebellumUsage, DegradedReason};
use common::enums::{ModelCapability, ProviderType};
use std::sync::atomic::{AtomicUsize, Ordering};

fn judge() -> InboxJudgeContext {
    InboxJudgeContext {
        agent_id: "agent-1".to_string(),
        agent_name: "小美".to_string(),
        scene: "Awaken".to_string(),
        situation: vec![
            "【任务上下文】摘要：修登录页".to_string(),
            "【输入理解结果】意图：修复登录失败问题".to_string(),
        ],
    }
}

fn batch() -> Vec<InjectedMessage> {
    vec![InjectedMessage {
        id: "m1".to_string(),
        created_at: 1_000,
        content: "另外把注册页的校验也一起改了".to_string(),
        attachment_ids: Vec::new(),
    }]
}

fn mock_provider() -> ModelProviderPo {
    ModelProviderPo::new(
        "p".to_string(),
        ProviderType::Jev,
        ModelCapability::Decision,
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

/// 可替换 DAL：只关心 decide 的调用次数与返回
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
        unimplemented!("not needed by intent shift tests")
    }
    async fn test_connection(
        &self,
        _ctx: RequestContext,
        _provider: &crate::models::model_provider::ModelProvider,
        _prompt: &str,
    ) -> common::error::Result<String> {
        unimplemented!("not needed by intent shift tests")
    }
    async fn think(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        _messages: &[crate::models::cortex_types::ChatMessage],
        _tools: &[crate::models::cortex_types::ToolDescriptor],
        _options: &crate::models::cortex_types::ModelCallOptions,
    ) -> common::error::Result<crate::models::cortex_types::ThinkResult> {
        unimplemented!("not needed by intent shift tests")
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

fn pick_chosen(chosen: &str) -> DecisionOutcome {
    DecisionOutcome {
        answers: DecisionAnswers::new().with(
            QUESTION_ID,
            DecisionValue::Pick {
                chosen: chosen.to_string(),
                confidence: 1.0,
                weights: BTreeMap::new(),
            },
        ),
        degraded: None,
        latency_ms: 0,
        usage: CerebellumUsage::default(),
    }
}

#[tokio::test]
async fn reinterpret_when_brain_picks_reinterpret() {
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = RequestContext::new_system();
    let brain = brain_with_cerebellum();
    let dal = ScriptedBrainDal::new(Some(pick_chosen(OPTION_REINTERPRET)));

    let shift = decide(&ctx, &judge(), &batch(), Some(&brain), &dal).await;
    assert_eq!(shift, IntentShift::Reinterpret);
    assert!(shift.needs_reinterpret());
    assert_eq!(dal.calls(), 1, "有小脑即判断一次");
}

#[tokio::test]
async fn supplement_when_brain_picks_supplement() {
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = RequestContext::new_system();
    let brain = brain_with_cerebellum();
    let dal = ScriptedBrainDal::new(Some(pick_chosen(OPTION_SUPPLEMENT)));

    let shift = decide(&ctx, &judge(), &batch(), Some(&brain), &dal).await;
    assert_eq!(shift, IntentShift::Supplement);
    assert!(!shift.needs_reinterpret());
    assert_eq!(dal.calls(), 1);
}

#[tokio::test]
async fn no_brain_returns_supplement_without_call() {
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = RequestContext::new_system();
    let dal = ScriptedBrainDal::new(Some(pick_chosen(OPTION_REINTERPRET)));

    let shift = decide(&ctx, &judge(), &batch(), None, &dal).await;
    assert_eq!(shift, IntentShift::Supplement);
    assert_eq!(dal.calls(), 0, "无 Brain 时零调用降级");
}

#[tokio::test]
async fn degraded_returns_supplement() {
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = RequestContext::new_system();
    let brain = brain_with_cerebellum();
    let dal = ScriptedBrainDal::new(None); // 缺省 outcome = degraded

    let shift = decide(&ctx, &judge(), &batch(), Some(&brain), &dal).await;
    assert_eq!(shift, IntentShift::Supplement);
    assert_eq!(dal.calls(), 1);
}

#[tokio::test]
async fn no_answer_returns_supplement() {
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = RequestContext::new_system();
    let brain = brain_with_cerebellum();
    // 有应答、非降级，但 answers 里没有本题 ⇒ supplement
    let outcome = DecisionOutcome {
        answers: DecisionAnswers::new(),
        degraded: None,
        latency_ms: 0,
        usage: CerebellumUsage::default(),
    };
    let dal = ScriptedBrainDal::new(Some(outcome));

    let shift = decide(&ctx, &judge(), &batch(), Some(&brain), &dal).await;
    assert_eq!(shift, IntentShift::Supplement);
}

#[tokio::test]
async fn quota_exhausted_returns_supplement_without_call() {
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = RequestContext::new_system();
    let mut brain = brain_with_cerebellum();
    // limit=1 且先占用一次 ⇒ try_acquire 必失败（0 表示不限，不能用）
    brain.cerebellum_quota = std::sync::Arc::new(crate::models::brain::CerebellumQuota::new(1));
    assert!(brain.cerebellum_quota.try_acquire());
    let dal = ScriptedBrainDal::new(Some(pick_chosen(OPTION_REINTERPRET)));

    let shift = decide(&ctx, &judge(), &batch(), Some(&brain), &dal).await;
    assert_eq!(shift, IntentShift::Supplement);
    assert_eq!(dal.calls(), 0, "配额耗尽零调用");
}

#[tokio::test]
async fn unknown_chosen_defaults_to_supplement() {
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = RequestContext::new_system();
    let brain = brain_with_cerebellum();
    // 模型回了一个不认识的选项 id，不能当作 reinterpret
    let dal = ScriptedBrainDal::new(Some(pick_chosen("something_else")));

    let shift = decide(&ctx, &judge(), &batch(), Some(&brain), &dal).await;
    assert_eq!(shift, IntentShift::Supplement);
}

#[test]
fn state_includes_judge_situation_and_new_inputs() {
    let state = build_state(&judge(), &batch());
    assert!(state.contains("修复登录失败问题"), "现场需带当前意图");
    assert!(state.contains("注册页的校验"), "现场需带本批新输入");
}
