//! HR Domain Agent 管理单元测试

use super::{HrDomain, domain};
use crate::models::agent::{Agent, AgentPo};
use crate::pkg::RequestContext;
use common::enums::AgentStatus;
use sqlx::SqlitePool;
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: sqlx::SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化 HR Domain 所有依赖
/// 初始化顺序：dao -> dal -> domain
fn init_test_env(pool: SqlitePool) -> (std::sync::Arc<dyn HrDomain>, RequestContext) {
    // 初始化所有 DAO
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
    crate::service::dao::skill::init();
    crate::service::dao::tool_call::init();
    crate::service::dao::model_provider::init();
    crate::service::dao::cortex::init();

    // 初始化所有 DAL
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
    crate::service::dal::skill::init();
    crate::service::dal::model_provider::init();

    // 初始化 HR Domain
    super::init();

    let domain = domain();
    let ctx = new_ctx("admin", pool);
    (domain, ctx)
}

/// 创建测试 Agent
fn create_test_agent(name: &str) -> Agent {
    let agent_po = AgentPo::new(
        name.to_string(),
        vec!["worker".to_string()],
        "A helpful agent".to_string(),
        vec!["coding".to_string()],
        "A helpful agent that can code".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    Agent::from_po(agent_po)
}

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let agent = create_test_agent("TestAgent");

    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    let found: Option<Agent> = domain
        .agent_manage()
        .get_agent(ctx, &agent.id(), Default::default())
        .await
        .unwrap();
    assert_eq!(found.unwrap().name(), "TestAgent");
}

#[sqlx::test]
async fn test_list_agents(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    for i in 0..3 {
        let agent = create_test_agent(&format!("Agent{}", i));
        domain
            .agent_manage()
            .create_agent(ctx.clone(), &agent)
            .await
            .unwrap();
    }

    let agents: Vec<Agent> = domain.agent_manage().list_agents(ctx).await.unwrap();
    assert_eq!(agents.len(), 3);
}

#[sqlx::test]
async fn test_update_agent(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let agent = create_test_agent("Original");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    let mut updated = agent.clone();
    updated.po.name = "Updated".to_string();
    domain
        .agent_manage()
        .update_agent(new_ctx("editor", pool), &updated)
        .await
        .unwrap();

    let found: Option<Agent> = domain
        .agent_manage()
        .get_agent(ctx, &updated.id(), Default::default())
        .await
        .unwrap();
    assert_eq!(found.unwrap().name(), "Updated");
}

#[sqlx::test]
async fn test_delete_agent(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let agent = create_test_agent("ToDelete");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    domain
        .agent_manage()
        .delete_agent(ctx.clone(), &agent)
        .await
        .unwrap();
    let found: Option<Agent> = domain
        .agent_manage()
        .get_agent(ctx, &agent.id(), Default::default())
        .await
        .unwrap();
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_transition_status_persists_valid_agent_lifecycle(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let mut agent = create_test_agent("LifecycleAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    domain
        .agent_manage()
        .transition_status(ctx.clone(), &mut agent, AgentStatus::PendingOnboard)
        .await
        .unwrap();

    assert_eq!(agent.po.status, AgentStatus::PendingOnboard);

    let found = domain
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await
        .unwrap()
        .expect("transitioned agent should be readable");

    assert_eq!(found.po.status, AgentStatus::PendingOnboard);
}

#[sqlx::test]
async fn test_transition_status_rejects_invalid_agent_lifecycle(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let mut agent = create_test_agent("InvalidLifecycleAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    let result = domain
        .agent_manage()
        .transition_status(ctx.clone(), &mut agent, AgentStatus::Onboarded)
        .await;

    assert!(result.is_err());
    assert_eq!(agent.po.status, AgentStatus::Interviewing);

    let found = domain
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await
        .unwrap()
        .expect("rejected transition should keep agent readable");

    assert_eq!(found.po.status, AgentStatus::Interviewing);
}

#[sqlx::test]
async fn test_onboard_installs_project_management_tag(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let mut agent = create_test_agent("OnboardAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // Interviewing → PendingOnboard → Onboarded
    domain
        .agent_manage()
        .transition_status(ctx.clone(), &mut agent, AgentStatus::PendingOnboard)
        .await
        .unwrap();
    domain
        .agent_manage()
        .transition_status(ctx.clone(), &mut agent, AgentStatus::Onboarded)
        .await
        .unwrap();

    // Verify installed_tags contains "project_management"
    assert!(agent.po.get_installed_tags().contains(&"project_management".to_string()));

    // Verify persisted
    let found = domain
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await
        .unwrap()
        .expect("onboarded agent should be readable");

    assert!(found.po.get_installed_tags().contains(&"project_management".to_string()));
}

#[sqlx::test]
async fn test_non_onboard_transition_does_not_install_tag(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let mut agent = create_test_agent("NonOnboardAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // Interviewing → PendingOnboard (NOT Onboarded)
    domain
        .agent_manage()
        .transition_status(ctx.clone(), &mut agent, AgentStatus::PendingOnboard)
        .await
        .unwrap();

    // Verify installed_tags is empty
    assert!(agent.po.get_installed_tags().is_empty());

    // Verify persisted
    let found = domain
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await
        .unwrap()
        .expect("agent should be readable");

    assert!(found.po.get_installed_tags().is_empty());
}

#[sqlx::test]
async fn test_install_tool_pack_installs_tag_idempotently(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let agent = create_test_agent("ToolPackAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // Install once
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "data_analysis")
        .await
        .unwrap();

    // Install same tag again (idempotent)
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "data_analysis")
        .await
        .unwrap();

    // Verify installed_tags contains exactly one "data_analysis"
    let installed = domain
        .agent_manage()
        .list_installed_tool_packs(ctx.clone(), agent.id())
        .await
        .unwrap();

    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0], "data_analysis");

    // Verify persisted
    let found = domain
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await
        .unwrap()
        .expect("agent should be readable");

    assert_eq!(found.po.get_installed_tags().len(), 1);
    assert_eq!(found.po.get_installed_tags()[0], "data_analysis");
}

#[sqlx::test]
async fn test_uninstall_tool_pack_removes_tag_idempotently(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let agent = create_test_agent("UninstallAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // Install two tags
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "data_analysis")
        .await
        .unwrap();
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "project_management")
        .await
        .unwrap();

    // Uninstall one
    domain
        .agent_manage()
        .uninstall_tool_pack(ctx.clone(), agent.id(), "data_analysis")
        .await
        .unwrap();

    let installed = domain
        .agent_manage()
        .list_installed_tool_packs(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0], "project_management");

    // Uninstall same tag again (idempotent - no error, no change)
    domain
        .agent_manage()
        .uninstall_tool_pack(ctx.clone(), agent.id(), "data_analysis")
        .await
        .unwrap();

    let installed = domain
        .agent_manage()
        .list_installed_tool_packs(ctx, agent.id())
        .await
        .unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0], "project_management");
}

#[sqlx::test]
async fn test_list_installed_tool_packs_returns_all_tags(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let agent = create_test_agent("ListPacksAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // Initially empty
    let installed = domain
        .agent_manage()
        .list_installed_tool_packs(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert!(installed.is_empty());

    // Install multiple tags
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "data_analysis")
        .await
        .unwrap();
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "project_management")
        .await
        .unwrap();
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "coding")
        .await
        .unwrap();

    let installed = domain
        .agent_manage()
        .list_installed_tool_packs(ctx, agent.id())
        .await
        .unwrap();
    assert_eq!(installed.len(), 3);
    assert!(installed.contains(&"data_analysis".to_string()));
    assert!(installed.contains(&"project_management".to_string()));
    assert!(installed.contains(&"coding".to_string()));
}

#[sqlx::test]
async fn test_install_tool_pack_returns_error_for_nonexistent_agent(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let result = domain
        .agent_manage()
        .install_tool_pack(ctx, "nonexistent-agent-id", "data_analysis")
        .await;

    assert!(result.is_err());
}
