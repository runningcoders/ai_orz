//! Handler: PUT /api/v1/agents/{id} - Update agent information

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{AgentRuntimeConfigInfo, UpdateAgentRequest, UpdateAgentResponse};
use common::error::Result;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::enrich_ctx;

/// Update the metadata and configuration of an existing AI agent.
///
/// Note for agent callers: an agent may only update itself (params.id must match
/// ctx.agent_id, otherwise an error is returned) and only the self-edit fields are
/// applied (description, capabilities, soul). Identity / routing fields (name, roles,
/// model_provider_id, runtime_config) are silently ignored when called from an agent
/// context; to change those please ask a human administrator. Human callers (no
/// agent_id) may update any supported field without restrictions.
#[register_handler_tool(
    id = "update_agent",
    name = "Update Agent Config",
    description = "Update an existing agent's fields (name, roles, description, capabilities, soul, model_provider_id, runtime_config); only provided fields change, except runtime_config which is replaced as a whole object. Returns the updated agent. Fails with NotFound if the agent does not exist. For an agent caller: self-update only (params.id must match the calling agent_id), identity / routing fields (name, roles, model_provider_id, runtime_config) are ignored — only description, capabilities, soul are applied.",
    params = "common::api::UpdateAgentRequest",
    neural
)]
#[generate_http_handler]
pub async fn update_agent(
    ctx: RequestContext,
    params: UpdateAgentRequest,
) -> Result<UpdateAgentResponse> {
    fn current_timestamp() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    // Agent 上下文：只允许更新自己；基础设施字段（name/roles/model_provider_id/
    // runtime_config）静默忽略，仅保留 description/capabilities/soul 可改
    let is_agent_ctx = ctx.agent_id().is_some();
    if is_agent_ctx {
        let caller = ctx.agent_id().expect("is_agent_ctx guard above");
        if caller != &params.id {
            return Err(common::error::Error::bad_request(format!(
                "Agent can only update itself: params.id {} does not match calling agent {}",
                params.id, caller
            )));
        }
    }

    let mut agent = domain()
        .agent_manage()
        .get_agent(ctx.clone(), &params.id, Default::default())
        .await?
        .ok_or_else(|| common::error::Error::not_found(format!("Agent {} not found", params.id)))?;

    let ctx = enrich_ctx!(&ctx, &agent);

    // Update fields
    //
    // Identity / routing fields (name, roles, model_provider_id, runtime_config) are
    // reserved to human administrators when a neural agent self-edit is in progress.
    if !is_agent_ctx {
        if let Some(name) = params.name {
            agent.po.name = name;
        }
        if let Some(roles) = params.roles {
            agent.po.role = serde_json::to_string(&roles).unwrap_or_else(|_| "[]".to_string());
        }
        if let Some(model_provider_id) = params.model_provider_id {
            agent.po.model_provider_id = model_provider_id;
        }
        // 更新运行时配置（整体替换：前端传入完整的 runtime_config 对象）
        if let Some(rc_info) = params.runtime_config {
            let mut rc = agent.po.get_runtime_config();
            rc.max_thinking_rounds = rc_info.max_thinking_rounds;
            rc.intent_analyze_max_rounds = rc_info.intent_analyze_max_rounds;
            rc.summary_max_rounds = rc_info.summary_max_rounds;
            rc.think_timeout_secs = rc_info.think_timeout_secs;
            agent.po.set_runtime_config(&rc);
        }
    }
    if let Some(description) = params.description {
        agent.po.description = description;
    }
    if let Some(capabilities) = params.capabilities {
        agent.po.capabilities =
            serde_json::to_string(&capabilities).unwrap_or_else(|_| "[]".to_string());
    }
    if let Some(soul) = params.soul {
        agent.po.soul = soul;
    }

    // Update modified_by and updated_at
    agent.po.modified_by = ctx.uid();
    agent.po.updated_at = current_timestamp();

    domain().agent_manage().update_agent(ctx, &agent).await?;

    let roles: Vec<String> = agent.po.get_roles();
    let capabilities: Vec<String> = agent.po.get_capabilities();

    // 构造运行时配置信息（思考轮次 / 超时等用户可调参数）
    let runtime_config = {
        let rc = agent.po.get_runtime_config();
        Some(AgentRuntimeConfigInfo {
            max_thinking_rounds: rc.max_thinking_rounds,
            intent_analyze_max_rounds: rc.intent_analyze_max_rounds,
            summary_max_rounds: rc.summary_max_rounds,
            think_timeout_secs: rc.think_timeout_secs,
        })
    };

    Ok(UpdateAgentResponse {
        id: agent.id().to_string(),
        name: agent.name().to_string(),
        roles,
        description: if agent.po.description.is_empty() {
            None
        } else {
            Some(agent.po.description.clone())
        },
        capabilities: if capabilities.is_empty() {
            None
        } else {
            Some(capabilities)
        },
        soul: if agent.po.soul.is_empty() {
            None
        } else {
            Some(agent.po.soul.clone())
        },
        kind: agent.po.kind.to_string(),
        model_provider_id: agent.po.model_provider_id.clone(),
        runtime_config,
        updated_at: agent.po.updated_at,
    })
}

#[cfg(test)]
mod tests {
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
    }

    /// Agent 自改：description / capabilities / soul 生效；基础设施字段静默忽略
    #[sqlx::test]
    async fn test_agent_self_update_allows_identity_fields_but_ignores_them(
        pool: sqlx::SqlitePool,
    ) {
        let ctx = init_env(pool);
        let agent_id = insert_agent(&ctx, "self").await;
        let original_name = format!("Agent-self");

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
}
