//! HR Domain Agent 管理单元测试

use super::{HrDomain, HrDomainImpl};
use crate::models::agent::{Agent, AgentPo};
use crate::pkg::RequestContext;
use std::sync::Arc;
use uuid::Uuid;

fn new_ctx(user_id: &str) -> RequestContext {
    RequestContext::new(Some(user_id.to_string()), None)
}

fn setup_test_domain() -> Arc<dyn HrDomain> {
    // 使用 dal 测试中的基础设施，dal 已经复用 dao 的基础设施
    let dal = crate::service::dal::agent_test::setup_test_dal();
    Arc::new(HrDomainImpl::new(dal))
}

#[test]
fn test_create_and_find_by_id() {
    let domain = setup_test_domain();
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "TestAgent".to_string(),
        Some("worker".to_string()),
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
        .unwrap();

    let found = domain
        .agent_manage()
        .get_agent(ctx, &agent.id())
        .unwrap()
        .unwrap();
    assert_eq!(found.name(), "TestAgent");
}

#[test]
fn test_list_agents() {
    let domain = setup_test_domain();
    let ctx = new_ctx("admin");

    for i in 0..3 {
        let agent_po = AgentPo::new(
            format!("Agent{}", i),
            Some("worker".to_string()),
            None,
            vec![],
            "".to_string(),
            "provider-id-1".to_string(),
            "admin".to_string(),
        );
        let agent = Agent::from_po(agent_po);
        domain
            .agent_manage()
            .create_agent(ctx.clone(), &agent)
            .unwrap();
    }

    let agents = domain.agent_manage().list_agents(ctx).unwrap();
    assert_eq!(agents.len(), 3);
}

#[test]
fn test_update_agent() {
    let domain = setup_test_domain();
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "Original".to_string(),
        Some("worker".to_string()),
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
        .unwrap();

    let mut updated = agent.clone();
    updated.po.name = "Updated".to_string();
    domain
        .agent_manage()
        .update_agent(new_ctx("editor"), &updated)
        .unwrap();

    let found = domain
        .agent_manage()
        .get_agent(ctx, &updated.id())
        .unwrap()
        .unwrap();
    assert_eq!(found.name(), "Updated");
}

#[test]
fn test_delete_agent() {
    let domain = setup_test_domain();
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "ToDelete".to_string(),
        Some("worker".to_string()),
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
        .unwrap();

    domain
        .agent_manage()
        .delete_agent(ctx.clone(), &agent)
        .unwrap();
    assert!(domain
        .agent_manage()
        .get_agent(ctx, &agent.id())
        .unwrap()
        .is_none());
}
