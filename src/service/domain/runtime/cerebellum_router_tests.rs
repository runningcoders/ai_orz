//! tests 单元测试（拆分自 cerebellum_router.rs）
//!
//! 文件瘦身：原 596 行 → 298 行，测试体 299 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::models::cerebellum_types::CerebellumUsage;
use async_trait::async_trait;
use common::error::{Result, err};
use std::time::Duration as StdDuration;

// ---------- mock BrainDal（实现全 trait 方法，仅 think_fast 生效） ----------

struct MockBrainDal {
    result: Option<Result<ThinkFastResult>>,
    delay_ms: u64,
}

#[async_trait]
impl BrainDal for MockBrainDal {
    async fn think_fast(
        &self,
        _ctx: RequestContext,
        _provider: &ModelProviderPo,
        _state: serde_json::Value,
        _questions: BTreeMap<String, CerebellumQuestion>,
    ) -> Result<ThinkFastResult> {
        if self.delay_ms > 0 {
            tokio::time::sleep(StdDuration::from_millis(self.delay_ms)).await;
        }
        match &self.result {
            Some(r) => r.clone(),
            None => Err(err!(Internal, "mock error")),
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

fn choice_result(choice: &str, confidence: f32) -> ThinkFastResult {
    let mut answers = BTreeMap::new();
    answers.insert(
        ROUTE_QUESTION_ID.to_string(),
        CerebellumAnswer::Choice {
            choice: choice.to_string(),
            confidence,
            probabilities: BTreeMap::new(),
        },
    );
    ThinkFastResult {
        answers,
        usage: CerebellumUsage {
            input_tokens: 100,
            output_tokens: 10,
        },
    }
}

async fn run_with(
    result: Option<Result<ThinkFastResult>>,
    delay_ms: u64,
    enabled: bool,
    user_msg: Option<&str>,
) -> RouteDecision {
    let ctx = test_ctx();
    route(
        &ctx,
        Arc::new(MockBrainDal { result, delay_ms }),
        &mock_provider(),
        enabled,
        "agent-1",
        "Test Agent",
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
        Some(Ok(choice_result(CHOICE_TRIVIAL_DIRECT, 0.95))),
        0,
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
        Some(Ok(choice_result(CHOICE_CORTEX_NEEDED, 0.92))),
        0,
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
        Some(Ok(choice_result(CHOICE_CORTEX_NEEDED, 0.5))),
        0,
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
    let d = run_with(None, 0, false, Some("在吗")).await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::NoCerebellum
        }
    );
}

#[tokio::test]
async fn test_route_no_user_message_skips() {
    let d = run_with(None, 0, true, None).await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::NoUserMessage
        }
    );
}

#[tokio::test]
async fn test_route_call_error_skips() {
    let d = run_with(None, 0, true, Some("在吗")).await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::CallError
        }
    );
}

#[tokio::test]
async fn test_route_timeout_skips() {
    // 850ms > 800ms 预算 → Timeout 降级
    let d = run_with(
        Some(Ok(choice_result(CHOICE_CORTEX_NEEDED, 0.95))),
        850,
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
    // 非 choice 答案
    let mut answers = BTreeMap::new();
    answers.insert(
        ROUTE_QUESTION_ID.to_string(),
        CerebellumAnswer::Score {
            score: 3.0,
            probabilities: BTreeMap::new(),
            legend: vec![],
            confidence: 0.99,
        },
    );
    let d = run_with(
        Some(Ok(ThinkFastResult {
            answers,
            usage: CerebellumUsage::default(),
        })),
        0,
        true,
        Some("在吗"),
    )
    .await;
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::UnsupportedAnswer
        }
    );
}

// ---------- 纯函数翻译边界 ----------

#[test]
fn test_translate_unknown_choice_skips() {
    let d = translate(&choice_result("unknown_choice", 0.99), 10);
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::UnsupportedAnswer
        }
    );
}

#[test]
fn test_translate_missing_answer_skips() {
    let d = translate(&ThinkFastResult::default(), 10);
    assert_eq!(
        d,
        RouteDecision::Skip {
            reason: DegradedReason::UnsupportedAnswer
        }
    );
}

#[test]
fn test_degraded_reason_str() {
    assert_eq!(DegradedReason::NoCerebellum.as_str(), "no_cerebellum");
    assert_eq!(DegradedReason::Timeout.as_str(), "timeout");
}
