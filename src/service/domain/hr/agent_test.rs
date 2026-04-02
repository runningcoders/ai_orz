//! HR Domain Agent 管理单元测试

use super::*;
use crate::models::agent::{Agent, AgentPo};
use crate::pkg::storage;

fn new_ctx(user_id: &str) -> crate::pkg::RequestContext {
    crate::pkg::RequestContext::new(Some(user_id.to_string()), None)
}

fn setup_test_domain() -> Arc<dyn HrDomain> {
    // 使用 dal 测试中的基础设施，dal 已经复用 dao 的基础设施
    let dal = crate::service::dal::agent_test::setup_test_dal();
    Arc::new(agent::HrDomainImpl::new(dal))
}

#[tokio::test]
async fn test_create_agent() {
    let domain = setup_test_domain();
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "TestAgent".to_string(),
        "worker".to_string(),
        vec!["coding".to_string()],
        "model_provider_id".to_string(),
        "A helpful agent".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po);

    domain.agent_manage().create(ctx, &agent).unwrap();
}

#[tokio::test]
async fn test_get_agent() {
    let domain = setup_test_domain();
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "TestAgent".to_string(),
        "worker".to_string(),
        vec!["coding".to_string()],
        "model_provider_id".to_string(),
        "A helpful agent".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po.clone());
    domain.agent_manage().create(ctx.clone(), &agent).unwrap();

    let found = domain.agent_manage().get(ctx, &agent_po.id).unwrap().unwrap();
    assert_eq!(found.name(), "TestAgent");
}

#[tokio::test]
async fn test_list_agents() {
    let domain = setup_test_domain();
    let ctx = new_ctx("admin");

    for i in 0..3 {
        let agent_po = AgentPo::new(
            format!("Agent{}", i),
            "worker".to_string(),
            vec![],
            "model_provider_id".to_string(),
            "".to_string(),
            "admin".to_string(),
        );
        let agent = Agent::from_po(agent_po);
        domain.agent_manage().create(ctx.clone(), &agent).unwrap();
    }

    let agents = domain.agent_manage().list(ctx).unwrap();
    assert_eq!(agents.len(), 3);
}

#[tokio::test]
async fn test_update_agent() {
    let domain = setup_test_domain();
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "Original".to_string(),
        "worker".to_string(),
        vec![],
        "model_provider_id".to_string(),
        "".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po);
    domain.agent_manage().create(ctx.clone(), &agent).unwrap();

    let mut updated = agent.clone();
    updated.po.name = "Updated".to_string();
    domain.agent_manage().update(new_ctx("editor"), &updated).unwrap();

    let found = domain.agent_manage().get(ctx, &updated.id()).unwrap().unwrap();
    assert_eq!(found.name(), "Updated");
}

#[tokio::test]
async fn test_delete_agent() {
    let domain = setup_test_domain();
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "ToDelete".to_string(),
        "worker".to_string(),
        vec![],
        "model_provider_id".to_string(),
        "".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po);
    domain.agent_manage().create(ctx.clone(), &agent).unwrap();

    domain.agent_manage().delete(ctx.clone(), &agent).unwrap();
    assert!(domain.agent_manage().get(ctx, &agent.id()).unwrap().is_none());
}
