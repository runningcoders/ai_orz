//! update_agent Handler 单元测试（配置更新 / 运行态字段 / 权限校验）
//!
//! 拆分自 `update_agent.rs` 尾部 tests 模块（文件瘦身，341 → 144 行）。
//! 用 `#[path]` 保持 `mod tests` 层级，测试里 `use super::*` 仍可见私有项。
//!
//! 覆盖口径：AgentRuntimeConfig 字段增量更新语义、身份与调用主体校验失败路径。

use super::*;
use crate::models::agent::AgentPo;
use common::enums::{AgentKind, AgentStatus};

/// 拉起单例并注入 sqlx 内存连接池。与 settle_memory 等 agent handler 测试一致，
/// 但额外调用 service::init() 以保证 HR Domain 单例就绪（本 handler 先碰 domain() 再查）。
fn init_env(pool: sqlx::SqlitePool) -> RequestContext {
    let _ = crate::config::init();
    let base_path = crate::config::get().base_data_path();
    crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);

    crate::service::init();

    crate::pkg::request_context_test_support::new_test_ctx("user-test", pool)
}

async fn insert_agent(ctx: &RequestContext, id_hint: &str) -> String {
    let mut po = AgentPo::new(
        format!("Agent-{id_hint}"),
        vec!["assistant".to_string()],
        format!("描述-{id_hint}"),
        vec!["chat".to_string()],
        format!("Soul-{id_hint} 初始设定"),
        "provider-stub".to_string(),
        ctx.uid(),
    );
    po.id = format!(
        "agent-{id_hint}-{}",
        common::constants::utils::current_timestamp_ms()
    );
    // 保证状态为 Onboarded，模拟真实可编辑 Agent
    po.status = AgentStatus::Onboarded;
    po.kind = AgentKind::Local;
    // 先落 DAO：handler 后续通过 domain().get_agent 再查 DAO 返回
    crate::service::dao::agent::dao()
        .insert(ctx.clone(), &po)
        .await
        .expect("DAO insert agent");
    po.id
}

fn build_rc_info(rounds: usize) -> AgentRuntimeConfigInfo {
    AgentRuntimeConfigInfo {
        max_thinking_depth: 42,
        max_thinking_rounds: rounds,
        intent_analyze_max_rounds: 3,
        summary_max_rounds: 3,
        think_timeout_secs: 120,
    }
}

/// 真人会话：允许更新任意字段（name/roles/soul/runtime_config 都生效）
#[sqlx::test]
async fn test_human_update_all_fields(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let agent_id = insert_agent(&ctx, "human-all").await;

    let resp = update_agent(
        ctx.clone(),
        UpdateAgentRequest {
            id: agent_id.clone(),
            name: Some("新名字".to_string()),
            roles: Some(vec!["reception".to_string()]),
            description: Some("新描述".to_string()),
            capabilities: Some(vec!["chat".to_string(), "task".to_string()]),
            soul: Some("新灵魂设定".to_string()),
            model_provider_id: Some("provider-new".to_string()),
            runtime_config: Some(build_rc_info(8)),
        },
    )
    .await
    .expect("human update succeeds");

    assert_eq!(resp.name, "新名字");
    assert_eq!(resp.roles, vec!["reception".to_string()]);
    assert_eq!(resp.description.as_deref(), Some("新描述"));
    assert_eq!(
        resp.capabilities.as_deref(),
        Some(vec!["chat".to_string(), "task".to_string()].as_slice())
    );
    assert_eq!(resp.soul.as_deref(), Some("新灵魂设定"));
    assert_eq!(resp.model_provider_id, "provider-new");
    assert_eq!(resp.runtime_config.as_ref().unwrap().max_thinking_rounds, 8);
    assert_eq!(resp.runtime_config.as_ref().unwrap().max_thinking_depth, 42);
}

/// Agent 自改：description / capabilities / soul 生效；基础设施字段静默忽略
#[sqlx::test]
async fn test_agent_self_update_allows_identity_fields_but_ignores_them(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let agent_id = insert_agent(&ctx, "self").await;
    let original_name = "Agent-self".to_string();

    let mut agent_ctx = ctx.clone();
    agent_ctx.agent_id = Some(agent_id.clone());

    let resp = update_agent(
        agent_ctx,
        UpdateAgentRequest {
            id: agent_id.clone(),
            name: Some("试图改名字".to_string()),
            roles: Some(vec!["hacker".to_string()]),
            description: Some("自改描述".to_string()),
            capabilities: Some(vec!["chat".to_string(), "knowledge".to_string()]),
            soul: Some("自改灵魂设定".to_string()),
            model_provider_id: Some("provider-evil".to_string()),
            runtime_config: Some(build_rc_info(99)),
        },
    )
    .await
    .expect("self-update succeeds");

    // 允许改的字段：成功更新
    assert_eq!(resp.description.as_deref(), Some("自改描述"));
    assert_eq!(
        resp.capabilities.as_deref(),
        Some(vec!["chat".to_string(), "knowledge".to_string()].as_slice())
    );
    assert_eq!(resp.soul.as_deref(), Some("自改灵魂设定"));

    // 基础设施字段：保持原值（name/roles/model_provider_id/runtime_config）
    assert_eq!(resp.name, original_name);
    assert_eq!(resp.roles, vec!["assistant".to_string()]);
    assert_eq!(resp.model_provider_id, "provider-stub");
    // 默认 runtime_config：max_thinking_rounds 为 0（语义 = 使用系统配置）
    assert_eq!(resp.runtime_config.as_ref().unwrap().max_thinking_rounds, 0);
    // 身份字段被忽略：未被 DTO 覆盖的 max_thinking_depth 保持默认值
    assert_eq!(
        resp.runtime_config.as_ref().unwrap().max_thinking_depth,
        common::api::DEFAULT_MAX_THINKING_DEPTH
    );
}

/// Agent 跨改其他 Agent：直接报错
#[sqlx::test]
async fn test_agent_cross_update_returns_error(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let _agent_a = insert_agent(&ctx, "cross-a").await;
    let agent_b = insert_agent(&ctx, "cross-b").await;

    let mut agent_ctx = ctx;
    agent_ctx.agent_id = Some("agent-cross-a-0000000000000".to_string()); // 不是 B

    let err = update_agent(
        agent_ctx,
        UpdateAgentRequest {
            id: agent_b.clone(),
            name: None,
            roles: None,
            description: None,
            capabilities: None,
            soul: Some("我来改你的灵魂".to_string()),
            model_provider_id: None,
            runtime_config: None,
        },
    )
    .await
    .expect_err("cross-agent update must fail");

    assert!(
        err.to_string().contains("can only update itself"),
        "unexpected err: {err}"
    );
}

/// Agent 自改自己的 ID 与上下文匹配（只传 soul，确保成功路径）
#[sqlx::test]
async fn test_agent_self_update_soul_only_success(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);
    let agent_id = insert_agent(&ctx, "soul").await;
    let mut agent_ctx = ctx.clone();
    agent_ctx.agent_id = Some(agent_id.clone());

    let resp = update_agent(
        agent_ctx,
        UpdateAgentRequest {
            id: agent_id.clone(),
            name: None,
            roles: None,
            description: None,
            capabilities: None,
            soul: Some("学完后沉淀的新风格：回复简洁、中文优先".to_string()),
            model_provider_id: None,
            runtime_config: None,
        },
    )
    .await
    .expect("self soul update");

    assert_eq!(
        resp.soul.as_deref(),
        Some("学完后沉淀的新风格：回复简洁、中文优先")
    );
}
