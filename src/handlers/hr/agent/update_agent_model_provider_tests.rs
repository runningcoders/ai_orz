//! update_agent_model_provider Handler 单元测试
//!
//! 覆盖口径：Agent 上下文可改模型绑定（与 `update_agent` 的关键差异）、
//! provider 存在 / 能力校验、其它字段不被触碰、`admin` 工具包归属护栏。

use super::*;
use crate::models::agent::AgentPo;
use crate::models::model_provider::{ModelProvider, ModelProviderPo};
use common::enums::{AgentKind, AgentStatus, CallerType, ModelCapability, ProviderType};
use sqlx::SqlitePool;

fn init_env(pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::init_service_for_test();
    crate::pkg::request_context_test_support::new_test_ctx("user-test", pool)
}

fn agent_caller_ctx(base: &RequestContext, agent_id: &str) -> RequestContext {
    base.to_builder()
        .caller_type(CallerType::Agent)
        .agent_id(agent_id)
        .build()
}

async fn insert_agent(ctx: &RequestContext, hint: &str) -> (String, AgentPo) {
    let mut po = AgentPo::new(
        format!("Agent-{hint}"),
        vec!["assistant".to_string()],
        format!("描述-{hint}"),
        vec!["chat".to_string()],
        format!("Soul-{hint}"),
        "provider-stub".to_string(),
        ctx.uid(),
    );
    po.id = format!(
        "agent-{hint}-{}",
        common::constants::utils::current_timestamp_ms()
    );
    po.status = AgentStatus::Onboarded;
    po.kind = AgentKind::Local;
    crate::service::dao::agent::dao()
        .insert(ctx.clone(), &po)
        .await
        .expect("insert agent");
    (po.id.clone(), po)
}

async fn insert_provider(ctx: &RequestContext, hint: &str, capability: ModelCapability) -> String {
    let mut po = ModelProviderPo::new(
        format!("provider-{hint}"),
        ProviderType::OpenAI,
        capability,
        "test-model".to_string(),
        "test-key".to_string(),
        None,
        None,
        "user-test".to_string(),
    );
    po.id = format!(
        "provider-{hint}-{}",
        common::constants::utils::current_timestamp_ms()
    );
    let id = po.id.clone();
    crate::service::domain::finance::domain()
        .model_provider_manage()
        .create_model_provider(ctx.clone(), &ModelProvider::from_po(po))
        .await
        .expect("insert provider");
    id
}

async fn stored_agent(ctx: &RequestContext, agent_id: &str) -> AgentPo {
    domain()
        .agent_manage()
        .get_agent(ctx.clone(), agent_id, Default::default())
        .await
        .expect("get agent")
        .expect("agent exists")
        .po
}

/// 核心判据：Agent 上下文**不再**静默忽略 `model_provider_id`。
/// 反事实：若本 handler 改成转发 `update_agent`，这条会红。
#[sqlx::test]
async fn agent_context_can_rebind_model_provider(pool: SqlitePool) {
    let ctx = init_env(pool);
    let (agent_id, _) = insert_agent(&ctx, "rebind").await;
    let provider_id = insert_provider(&ctx, "chat", ModelCapability::Agent).await;

    let resp = update_agent_model_provider(
        agent_caller_ctx(&ctx, &agent_id),
        UpdateAgentModelProviderRequest {
            id: agent_id.clone(),
            model_provider_id: provider_id.clone(),
        },
    )
    .await
    .expect("Agent 上下文必须能改模型绑定");

    assert_eq!(resp.id, agent_id);
    assert_eq!(resp.model_provider_id, provider_id);
    assert_eq!(
        stored_agent(&ctx, &agent_id).await.model_provider_id,
        provider_id
    );
}

/// 窄接口语义：除模型绑定外，身份 / 灵魂 / 运行态字段一律不碰。
#[sqlx::test]
async fn only_model_provider_id_changes(pool: SqlitePool) {
    let ctx = init_env(pool);
    let (agent_id, before) = insert_agent(&ctx, "narrow").await;
    let provider_id = insert_provider(&ctx, "narrow", ModelCapability::Agent).await;

    update_agent_model_provider(
        ctx.clone(),
        UpdateAgentModelProviderRequest {
            id: agent_id.clone(),
            model_provider_id: provider_id.clone(),
        },
    )
    .await
    .expect("User 上下文同样可用");

    let after = stored_agent(&ctx, &agent_id).await;
    assert_eq!(after.name, before.name);
    assert_eq!(after.role, before.role);
    assert_eq!(after.soul, before.soul);
    assert_eq!(after.description, before.description);
    assert_ne!(after.model_provider_id, before.model_provider_id);
}

#[sqlx::test]
async fn rejects_unknown_provider(pool: SqlitePool) {
    let ctx = init_env(pool);
    let (agent_id, _) = insert_agent(&ctx, "missing").await;

    let err = update_agent_model_provider(
        ctx,
        UpdateAgentModelProviderRequest {
            id: agent_id,
            model_provider_id: "provider-does-not-exist".to_string(),
        },
    )
    .await
    .expect_err("不存在的 provider 必须拒绝");
    assert!(err.to_string().contains("not found"), "{err}");
}

/// 后端补上前端 `BindModelModal` 之外的校验：工具链路没有 UI 兜底。
#[sqlx::test]
async fn rejects_embedding_provider(pool: SqlitePool) {
    let ctx = init_env(pool);
    let (agent_id, _) = insert_agent(&ctx, "embed").await;
    let embedding_id = insert_provider(&ctx, "embed", ModelCapability::Embedding).await;

    let err = update_agent_model_provider(
        ctx,
        UpdateAgentModelProviderRequest {
            id: agent_id,
            model_provider_id: embedding_id,
        },
    )
    .await
    .expect_err("Embedding provider 不能当对话模型绑定");
    assert!(err.to_string().contains("only Agent capability"), "{err}");
}

/// 授权边界的工具侧表达：只能挂 `admin`，不能同时挂出生可达包。
#[test]
fn tool_is_gated_by_admin_pack_only() {
    let registry = crate::pkg::tool_registry::get_registry();
    let factory = registry
        .get_builtin_factory("update_agent_model_provider")
        .expect("必须注册为内置工具");
    assert_eq!(
        factory.create_po().get_tags(),
        vec!["admin".to_string()],
        "挂到 neural / tool_management 会让所有 Agent 都能改模型绑定"
    );
}
