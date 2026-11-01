//! tests 单元测试（拆分自 cerebellum_router.rs）
//!
//! 文件瘦身：原 596 行 → 298 行，测试体 299 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::models::model_provider::ModelProviderPo;
use crate::pkg::policy::{DecisionAnswers, DecisionRequest, DecisionValue, Metrics};
use crate::service::dal::brain::DecisionOutcome;
use async_trait::async_trait;
use common::api::cerebellum::CerebellumUsage;
use common::error::Result;
use std::collections::BTreeMap;

// ---------- mock BrainDal（实现全 trait 方法，仅 decide 生效） ----------

/// `outcome = None` 表示模拟调用失败（CallError）
struct MockBrainDal {
    outcome: Option<DecisionOutcome>,
}

#[async_trait]
impl BrainDal for MockBrainDal {
    async fn decide(
        &self,
        _ctx: RequestContext,
        _brain: &Brain,
        _requests: Vec<DecisionRequest>,
    ) -> DecisionOutcome {
        match &self.outcome {
            Some(outcome) => outcome.clone(),
            None => DecisionOutcome {
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

/// 成功结论：Pick(choice, confidence)
fn pick_outcome(choice: &str, confidence: f32) -> DecisionOutcome {
    DecisionOutcome {
        answers: DecisionAnswers::new().with(
            route_policy().primary_qid(),
            DecisionValue::Pick {
                chosen: choice.to_string(),
                confidence,
                weights: BTreeMap::new(),
            },
        ),
        degraded: None,
        latency_ms: 10,
        usage: CerebellumUsage {
            input_tokens: 100,
            output_tokens: 10,
        },
    }
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

async fn run_with(
    outcome: Option<DecisionOutcome>,
    enabled: bool,
    user_msg: Option<&str>,
) -> RouteDecision {
    // 全局 Storage 单例：`test_ctx()` 走 `RequestContext::new`，其 builder
    // 在未显式注入 storage 时回退到全局单例 ⇒ 必须先初始化，否则单跑
    // 本模块时 panic「Storage 尚未初始化」。全量跑时靠其它用例先初始化
    // 侥幸通过，属测试隔离缺陷（与业务无关）。
    crate::pkg::storage::test_support::init_for_test().await;
    let ctx = test_ctx();
    route(
        &ctx,
        Arc::new(MockBrainDal { outcome }),
        &mock_brain(None),
        enabled,
        user_msg,
    )
    .await
}

fn test_ctx() -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx_from_global("test-user")
}

// ---------- 翻译三分类 ----------

#[tokio::test]
async fn test_route_trivial_direct() {
    let d = run_with(
        Some(pick_outcome(CHOICE_TRIVIAL_DIRECT, 0.95)),
        true,
        Some("在吗"),
    )
    .await;
    assert_eq!(
        d,
        RouteDecision::Direct {
            content: TRIVIAL_DIRECT_TEMPLATE.to_string(),
            confidence: 0.95
        }
    );
}

#[tokio::test]
async fn test_route_cortex_needed_injects() {
    let d = run_with(
        Some(pick_outcome(CHOICE_CORTEX_NEEDED, 0.92)),
        true,
        Some("帮我查一下上个月的报销数据"),
    )
    .await;
    assert_eq!(
        d,
        RouteDecision::Inject {
            system_prompt: CORTEX_BOOST_TEMPLATE.to_string(),
            confidence: 0.92
        }
    );
}

#[tokio::test]
async fn test_route_low_confidence_skips() {
    let d = run_with(
        Some(pick_outcome(CHOICE_CORTEX_NEEDED, 0.5)),
        true,
        Some("随便聊聊"),
    )
    .await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::LowConfidence
        }
    );
}

// ---------- 降级路径 ----------

#[tokio::test]
async fn test_route_disabled_skips_without_call() {
    let d = run_with(None, false, Some("在吗")).await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::Disabled
        }
    );
}

#[tokio::test]
async fn test_route_no_user_message_skips() {
    let d = run_with(None, true, None).await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::NoUserMessage
        }
    );
}

#[tokio::test]
async fn test_route_call_error_skips() {
    let d = run_with(None, true, Some("在吗")).await;
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
        true,
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
            route_policy().primary_qid(),
            DecisionValue::Grade {
                score: 3.0,
                confidence: 0.99,
            },
        ),
        degraded: None,
        latency_ms: 5,
        usage: CerebellumUsage::default(),
    };
    let d = run_with(Some(outcome), true, Some("在吗")).await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::UnsupportedAnswer
        }
    );
}

// ---------- 纯函数翻译边界 ----------

/// 把一条结论装进事实桶（translate 只看桶，不看原始 answers）
fn metrics_with(value: DecisionValue) -> Metrics {
    let policy = route_policy();
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
    let d = translate(&route_policy(), &metrics);
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::UnsupportedAnswer
        }
    );
}

#[test]
fn test_translate_missing_answer_skips() {
    // 没问出来（None）必须走 Skip —— 绝不能默认成 Direct 或 Inject
    let d = translate(&route_policy(), &Metrics::new());
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::UnsupportedAnswer
        }
    );
}

#[test]
fn test_translate_low_confidence_skips() {
    // 有结论但置信不足：与「压根没答」要区分开（观测口径不同）
    let metrics = metrics_with(DecisionValue::Pick {
        chosen: CHOICE_TRIVIAL_DIRECT.to_string(),
        confidence: CEREBELLUM_ROUTE_CONFIDENCE_THRESHOLD - 0.1,
        weights: BTreeMap::new(),
    });
    assert_eq!(
        translate(&route_policy(), &metrics),
        RouteDecision::Skip {
            reason: DegradedReason::LowConfidence
        }
    );
}

#[test]
fn test_degraded_reason_str() {
    // 「没配小脑」与「被开关关掉」必须是两个口径——排查时才能区分配置缺失与人为关闭
    assert_eq!(DegradedReason::NoProvider.as_str(), "no_provider");
    assert_eq!(DegradedReason::Disabled.as_str(), "disabled");
    assert_eq!(DegradedReason::Timeout.as_str(), "timeout");
}
