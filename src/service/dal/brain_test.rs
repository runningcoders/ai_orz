//! Brain DAL 单元测试
//! 测试 Brain DAL 的 wake_brain 和 test_connection 功能

use crate::models::{agent::AgentPo, memory::*, model_provider::*};
use crate::pkg::policy::{DecisionValue, QuestionSpec};
use crate::pkg::request_context::RequestContext;
use crate::service::dal::brain::{BrainDal, to_decision_value, to_protocol};
use crate::service::dao::cortex;
use common::api::cerebellum::{CerebellumAnswer, QuestionCriteria, QuestionType};
use common::enums::{AgentKind, ModelCapability, ProviderType};
use sqlx::SqlitePool;
use std::sync::Arc;

/// 初始化测试环境
async fn init_test_env(pool: SqlitePool) -> (Arc<dyn BrainDal + Send + Sync>, RequestContext) {
    cortex::init();
    crate::service::dao::tool_call::init();
    crate::service::dao::model_provider::init();
    crate::service::dal::brain::init();
    let tool_call_dao = crate::service::dao::tool_call::dao();
    let model_provider_dao = crate::service::dao::model_provider::dao();
    let http_client = crate::pkg::http::presets::outbound()
        .build()
        .expect("构建 Brain HTTP 客户端失败");
    let brain_dal = crate::service::dal::brain::new(tool_call_dao, model_provider_dao, http_client);
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);
    (brain_dal, ctx)
}

/// 创建测试 ModelProvider
fn create_test_provider() -> ModelProvider {
    let provider_po = ModelProviderPo::new(
        "OpenAI GPT-4o".to_string(),
        ProviderType::OpenAI,
        ModelCapability::Agent,
        "gpt-4o".to_string(),
        // 空 api_key：test_connection 期望返回错误，空 key 在 DAO 层被前置校验
        // 快速拒绝（ConfigInvalid），避免发起真实 HTTP 导致长时挂起。
        String::new(),
        Some("https://api.openai.com/v1".to_string()),
        Some("OpenAI GPT-4o Official".to_string()),
        "test".to_string(),
    );
    ModelProvider::from_po(provider_po)
}

/// 创建测试 Local AgentPo
fn create_test_local_agent(provider_id: &str) -> AgentPo {
    let mut po = AgentPo::new(
        "Test Agent".to_string(),
        vec!["assistant".to_string()],
        "Test description".to_string(),
        vec!["chat".to_string()],
        "Test soul".to_string(),
        provider_id.to_string(),
        "test-user".to_string(),
    );
    po.id = "test-agent".to_string();
    po.kind = AgentKind::Local;
    po
}

/// 测试 Brain DAL 创建 wake_brain 功能（Local agent）
#[sqlx::test]
async fn test_wake_brain_local(pool: SqlitePool) {
    let (brain_dal, ctx) = init_test_env(pool).await;

    // 先插入一个 ModelProvider
    let provider = create_test_provider();
    crate::service::dao::model_provider::dao()
        .insert(ctx.clone(), &provider.po)
        .await
        .unwrap();

    let agent = create_test_local_agent(&provider.po.id);

    let now = chrono::Utc::now().timestamp();
    let short_term_po = ShortTermMemoryIndexPo {
        id: "test-memory-1".to_string(),
        agent_id: "test-agent".to_string(),
        task_id: None,
        role: "system".to_string(),
        summary: "你是一个有用的AI助手".to_string(),
        tags: "[\"chat\", \"question\"]".to_string(),
        trace_ids: "[]".to_string(),
        status: common::enums::MemoryStatus::Active,
        created_at: now,
        updated_at: now,
    };
    let memory = Memory::new(MemoryPo::ShortTerm(short_term_po));
    let memories = vec![memory];

    let result = brain_dal.wake_brain(ctx.clone(), &agent, memories).await;

    assert!(result.is_ok());
    let brain = result.unwrap();
    assert!(brain.is_local());
    assert!(brain.model_provider().is_some());
    assert_eq!(brain.agent_id, "test-agent");
    assert_eq!(brain.agent_name, "Test Agent");
}

/// 测试 wake_brain 缺 model：Local Agent 未绑定 provider 时直接报错（不隐式兜底）
#[sqlx::test]
async fn test_wake_brain_local_missing_provider_errors(pool: SqlitePool) {
    let (brain_dal, ctx) = init_test_env(pool).await;

    // 不插入任何 ModelProvider：Agent 的 model_provider_id 指向不存在的 provider
    let agent = create_test_local_agent("non-existent-provider");

    let result = brain_dal.wake_brain(ctx.clone(), &agent, Vec::new()).await;

    // 必须明确报错（缺 model 配置），而不是静默回退到某个默认 provider
    let msg = match result {
        Ok(_) => {
            panic!("缺 model 配置时应报错，而不是成功 wake_brain")
        }
        Err(e) => e.to_string(),
    };
    assert!(
        msg.contains("缺少对话模型配置"),
        "错误信息应明确提示缺 model 配置，实际: {msg}"
    );
    assert!(
        msg.contains("model_provider_id"),
        "错误信息应包含绑定的 provider id 上下文，实际: {msg}"
    );
}

/// 测试 Brain DAL test_connection 功能
#[sqlx::test]
async fn test_test_connection(pool: SqlitePool) {
    let (brain_dal, ctx) = init_test_env(pool).await;

    let provider = create_test_provider();

    let result = brain_dal.test_connection(ctx, &provider, "Hello!").await;

    assert!(result.is_err());
}

// ==================== B2 小脑注入测试 ====================

/// 创建测试 Decision（小脑）ModelProvider
fn create_test_decision_provider() -> ModelProvider {
    let provider_po = ModelProviderPo::new(
        "Jev Cerebellum".to_string(),
        ProviderType::Jev,
        ModelCapability::Decision,
        "jev-latest".to_string(),
        "test-key".to_string(),
        None,
        Some("测试小脑模型".to_string()),
        "test".to_string(),
    );
    ModelProvider::from_po(provider_po)
}

/// 创建测试 Cli（外部）AgentPo
fn create_test_cli_agent() -> AgentPo {
    let mut po = AgentPo::new(
        "Test Cli Agent".to_string(),
        vec!["assistant".to_string()],
        "Test description".to_string(),
        vec!["chat".to_string()],
        "Test soul".to_string(),
        String::new(),
        "test-user".to_string(),
    );
    po.id = "test-cli-agent".to_string();
    po.kind = AgentKind::Cli;
    po
}

/// 有启用 Decision 记录 → Local brain 注入 Some 且 id 吻合
#[sqlx::test]
async fn test_wake_brain_local_injects_default_cerebellum(pool: SqlitePool) {
    let (brain_dal, ctx) = init_test_env(pool).await;

    let provider = create_test_provider();
    crate::service::dao::model_provider::dao()
        .insert(ctx.clone(), &provider.po)
        .await
        .unwrap();
    let cerebellum = create_test_decision_provider();
    crate::service::dao::model_provider::dao()
        .insert(ctx.clone(), &cerebellum.po)
        .await
        .unwrap();

    let agent = create_test_local_agent(&provider.po.id);
    let brain = brain_dal.wake_brain(ctx, &agent, Vec::new()).await.unwrap();

    let injected = brain
        .cerebellum
        .as_ref()
        .expect("存在启用 Decision 记录时应注入默认小脑");
    assert_eq!(injected.id, cerebellum.po.id);
}

/// 无任何 Decision 记录 → Local brain cerebellum=None（现状行为，逐字节一致）
#[sqlx::test]
async fn test_wake_brain_local_no_cerebellum_is_none(pool: SqlitePool) {
    let (brain_dal, ctx) = init_test_env(pool).await;

    let provider = create_test_provider();
    crate::service::dao::model_provider::dao()
        .insert(ctx.clone(), &provider.po)
        .await
        .unwrap();

    let agent = create_test_local_agent(&provider.po.id);
    let brain = brain_dal.wake_brain(ctx, &agent, Vec::new()).await.unwrap();

    assert!(
        brain.cerebellum.is_none(),
        "无启用 Decision 记录时应为 None（与现状一致）"
    );
}

/// Cli（外部）brain 恒不注入小脑（即便存在启用 Decision 记录）
#[sqlx::test]
async fn test_wake_brain_external_cerebellum_none(pool: SqlitePool) {
    let (brain_dal, ctx) = init_test_env(pool).await;

    let cerebellum = create_test_decision_provider();
    crate::service::dao::model_provider::dao()
        .insert(ctx.clone(), &cerebellum.po)
        .await
        .unwrap();

    let agent = create_test_cli_agent();
    let brain = brain_dal.wake_brain(ctx, &agent, Vec::new()).await.unwrap();

    assert!(brain.is_external());
    assert!(
        brain.cerebellum.is_none(),
        "Cli brain 不注入小脑（外部 Agent 小脑接入列后续可选，不在本期）"
    );
}

// ==================== decide：小脑决策出口的零调用降级护栏 ====================

fn decision_request() -> crate::pkg::policy::DecisionRequest {
    crate::pkg::policy::DecisionRequest {
        scene: "test",
        state: serde_json::json!({ "probe": 1 }),
        questions: std::collections::BTreeMap::from([(
            "q".to_string(),
            crate::pkg::policy::QuestionSpec::Presence {
                instructions: "test".to_string(),
            },
        )]),
    }
}

/// 空请求批次 ⇒ **正常无操作**，不是降级
///
/// 策略组本轮都没 prepare 时必须零开销返回，不能报告成「小脑失败」，
/// 否则普通轮次的日志会全是假降级。
#[sqlx::test]
async fn test_decide_empty_requests_is_not_degraded(pool: SqlitePool) {
    let (brain_dal, ctx) = init_test_env(pool).await;

    let brain = crate::models::brain::Brain::new_local(
        "agent-1".to_string(),
        "Agent".to_string(),
        Default::default(),
        create_test_provider().po,
        Vec::new(),
    );

    let outcome = brain_dal.decide(ctx, &brain, Vec::new()).await;

    assert!(outcome.degraded.is_none(), "空请求是正常无操作，不该算降级");
    assert!(outcome.answers.is_empty());
    assert_eq!(outcome.latency_ms, 0);
}

/// 小脑未装配（外部 Agent / 无启用 provider）⇒ NoProvider
#[sqlx::test]
async fn test_decide_without_cerebellum_degrades(pool: SqlitePool) {
    let (brain_dal, ctx) = init_test_env(pool).await;

    let brain = crate::models::brain::Brain::new_local(
        "agent-1".to_string(),
        "Agent".to_string(),
        Default::default(),
        create_test_provider().po,
        Vec::new(),
    );

    let outcome = brain_dal
        .decide(ctx, &brain, vec![decision_request()])
        .await;

    assert_eq!(
        outcome.degraded,
        Some(common::api::cerebellum::DegradedReason::NoProvider)
    );
    assert!(outcome.answers.is_empty());
}

/// 小脑 provider 已装配但 api_key 为空 ⇒ 必须在发请求前降级为 NoProvider
///
/// ⚠️ 这条护栏守的是「增强功能反向劫持主流程」——拿无效凭据打网络最轻是
/// 白等一个超时，最重是错误顺着 future 传播把整轮思考炸掉。两者都表现为
/// 「Agent 置忙后零进展」，现场很难定位。
#[sqlx::test]
async fn test_decide_degrades_on_empty_api_key(pool: SqlitePool) {
    let (brain_dal, ctx) = init_test_env(pool).await;

    let mut provider = create_test_decision_provider();
    provider.po.api_key = "  ".to_string();

    let mut brain = crate::models::brain::Brain::new_local(
        "agent-empty-key".to_string(),
        "Empty Key Agent".to_string(),
        Default::default(),
        provider.po.clone(),
        Vec::new(),
    );
    brain.cerebellum = Some(provider.po.clone());

    let outcome = brain_dal
        .decide(ctx, &brain, vec![decision_request()])
        .await;

    assert_eq!(
        outcome.degraded,
        Some(common::api::cerebellum::DegradedReason::NoProvider)
    );
    assert!(outcome.answers.is_empty());
}

// ==================== 题型双向映射契约（to_protocol / to_decision_value）====================

/// `Presence` **绝不允许**映射出 `criteria`
///
/// ⚠️ 这条守的是整批失败：protocol 里 noul 带 criteria 是非法请求，
/// 而 client 侧「任一题不合法 ⇒ 整批 ConfigInvalid」，多场景同批时
/// 一个写错的 Presence 会把同批其它场景一起打挂。
#[test]
fn test_to_protocol_presence_never_carries_criteria() {
    let req = crate::pkg::policy::DecisionRequest {
        scene: "test",
        state: serde_json::json!({ "k": "v" }),
        questions: std::collections::BTreeMap::from([(
            "q".to_string(),
            QuestionSpec::Presence {
                instructions: "是否相关".to_string(),
            },
        )]),
    };

    let (state, questions) = to_protocol(req);
    assert_eq!(state, serde_json::json!({ "k": "v" }), "state 必须原样透传");

    let q = questions.get("q").expect("题 id 必须保留");
    assert_eq!(q.question_type, QuestionType::Noul);
    assert!(q.criteria.is_none(), "noul 带 criteria 会让整批请求失败");
    assert!(
        q.validate().is_ok(),
        "映射产物必须直接通过协议校验，否则发送即失败"
    );
}

/// `Pick` / `Grade` 的题型与判据形态一一对应，且产物自带合法性
#[test]
fn test_to_protocol_pick_and_grade_shapes() {
    let req = crate::pkg::policy::DecisionRequest {
        scene: "test",
        state: serde_json::json!({}),
        questions: std::collections::BTreeMap::from([
            (
                "pick_q".to_string(),
                QuestionSpec::Pick {
                    instructions: "选一个".to_string(),
                    options: std::collections::BTreeMap::from([
                        ("a".to_string(), "选项 A".to_string()),
                        ("b".to_string(), "选项 B".to_string()),
                    ]),
                },
            ),
            (
                "grade_q".to_string(),
                QuestionSpec::Grade {
                    instructions: "打几级".to_string(),
                    levels: vec!["低".to_string(), "中".to_string(), "高".to_string()],
                },
            ),
        ]),
    };

    let (_, questions) = to_protocol(req);

    let pick = questions.get("pick_q").expect("pick 题必须保留");
    assert_eq!(pick.question_type, QuestionType::Choice);
    assert_eq!(
        pick.criteria,
        Some(QuestionCriteria::Choices(std::collections::BTreeMap::from(
            [
                ("a".to_string(), "选项 A".to_string()),
                ("b".to_string(), "选项 B".to_string()),
            ]
        ))),
        "选项键值必须原样搬运——选项 id 是策略侧认的标识，不能被改写"
    );
    assert!(pick.validate().is_ok());

    let grade = questions.get("grade_q").expect("grade 题必须保留");
    assert_eq!(grade.question_type, QuestionType::Score);
    assert_eq!(
        grade.criteria,
        Some(QuestionCriteria::Levels(vec![
            "低".to_string(),
            "中".to_string(),
            "高".to_string()
        ])),
        "等级顺序即语义（低→高），不可重排"
    );
    assert!(grade.validate().is_ok());
}

/// 协议答案 → 引擎结论：三型无损、置信度不丢
#[test]
fn test_to_decision_value_preserves_three_shapes() {
    let choice = CerebellumAnswer::Choice {
        choice: "cortex_needed".to_string(),
        confidence: 0.92,
        probabilities: std::collections::BTreeMap::from([
            ("cortex_needed".to_string(), 0.92),
            ("trivial_direct".to_string(), 0.08),
        ]),
    };
    match to_decision_value(&choice) {
        DecisionValue::Pick {
            chosen,
            confidence,
            weights,
        } => {
            assert_eq!(chosen, "cortex_needed");
            assert!((confidence - 0.92).abs() < 1e-6);
            // 概率分布保留：策略侧可能要看「第二名」，丢了就再也拿不回来
            assert_eq!(weights.len(), 2);
        }
        other => panic!("choice 必须映射为 Pick，实际 {:?}", other),
    }

    let score = CerebellumAnswer::Score {
        score: 3.4,
        probabilities: std::collections::BTreeMap::new(),
        legend: std::collections::BTreeMap::from([
            ("0".to_string(), "低".to_string()),
            ("1".to_string(), "高".to_string()),
        ]),
        confidence: 0.7,
    };
    match to_decision_value(&score) {
        DecisionValue::Grade { score, confidence } => {
            assert!((score - 3.4).abs() < 1e-6);
            assert!((confidence - 0.7).abs() < 1e-6);
        }
        other => panic!("score 必须映射为 Grade，实际 {:?}", other),
    }

    let noul = CerebellumAnswer::Noul { noul: 0.83 };
    match to_decision_value(&noul) {
        // noul 没有 confidence：绝不能在这里凭空补一个 1.0 或 0.0，
        // 那会让「0~1 概率」被误当成「带置信度的二值判断」
        DecisionValue::Presence { value } => assert!((value - 0.83).abs() < 1e-6),
        other => panic!("noul 必须映射为 Presence，实际 {:?}", other),
    }
}
