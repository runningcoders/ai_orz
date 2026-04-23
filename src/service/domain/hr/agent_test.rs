//! HR Domain Agent 管理单元测试

use super::{HrDomain, domain};
use crate::models::agent::{Agent, AgentPo};
use crate::pkg::RequestContext;
use uuid::Uuid;
use sqlx::SqlitePool;

fn new_ctx(user_id: &str, pool: sqlx::SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

#[sqlx::test]
async fn test_create_and_find_by_id(pool: sqlx::SqlitePool) {
    // 初始化依赖：dao -> dal -> domain (和线上初始化顺序一致)
    crate::service::dao::agent::init();
    crate::service::dal::agent::init();
    super::init();
    let domain = domain();
    let ctx = new_ctx("admin", pool);

    let agent_po = AgentPo::new(
        "TestAgent".to_string(),
        "worker".to_string(),
        "A helpful agent".to_string(),
        vec!["coding".to_string()],
        "A helpful agent that can code".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po);

    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    let found: Option<Agent> = domain
        .agent_manage()
        .get_agent(ctx, &agent.id())
        .await
        .unwrap();
    assert_eq!(found.unwrap().name(), "TestAgent");
}

#[sqlx::test]
async fn test_list_agents(pool: sqlx::SqlitePool) {
    // 初始化依赖：dao -> dal -> domain (和线上初始化顺序一致)
    crate::service::dao::agent::init();
    crate::service::dal::agent::init();
    super::init();
    let domain = domain();

    for i in 0..3 {
        let ctx = new_ctx("admin", pool.clone());
        let agent_po = AgentPo::new(
            format!("Agent{}", i),
            "worker".to_string(),
            "".to_string(),
            vec![],
            "".to_string(),
            "provider-id-1".to_string(),
            "admin".to_string(),
        );
        let agent = Agent::from_po(agent_po);
        domain
            .agent_manage()
            .create_agent(ctx.clone(), &agent)
            .await
            .unwrap();
    }

    let ctx = new_ctx("admin", pool);
    let agents: Vec<Agent> = domain.agent_manage().list_agents(ctx).await.unwrap();
    assert_eq!(agents.len(), 3);
}

#[sqlx::test]
async fn test_update_agent(pool: sqlx::SqlitePool) {
    // 初始化依赖：dao -> dal -> domain (和线上初始化顺序一致)
    crate::service::dao::agent::init();
    crate::service::dal::agent::init();
    super::init();
    let domain = domain();
    let ctx = new_ctx("admin", pool.clone());

    let agent_po = AgentPo::new(
        "Original".to_string(),
        "worker".to_string(),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po);
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
        .get_agent(ctx, &updated.id())
        .await
        .unwrap();
    assert_eq!(found.unwrap().name(), "Updated");
}

#[sqlx::test]
async fn test_delete_agent(pool: sqlx::SqlitePool) {
    // 初始化依赖：dao -> dal -> domain (和线上初始化顺序一致)
    crate::service::dao::agent::init();
    crate::service::dal::agent::init();
    super::init();
    let domain = domain();
    let ctx = new_ctx("admin", pool.clone());

    let agent_po = AgentPo::new(
        "ToDelete".to_string(),
        "worker".to_string(),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po);
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
        .get_agent(ctx, &agent.id())
        .await
        .unwrap();
    assert!(found.is_none());
}
