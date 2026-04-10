//! HR Domain Agent 管理单元测试

use crate::models::agent::{Agent, AgentPo};
use common::constants::RequestContext;
use super::{HrDomain, HrDomainImpl};
use std::sync::Arc;

fn new_ctx(user_id: &str) -> RequestContext {
    RequestContext::new(Some(user_id.to_string()), None)
}

fn setup_test_domain() -> Arc<dyn HrDomain> {
    // 使用 dal 测试中的基础设施，dal 已经复用 dao 的基础设施
    let dal = crate::service::dal::agent_test::setup_test_dal();
    Arc::new(HrDomainImpl::new(dal))
}

/// 测试所有 HR Domain Agent 管理功能
/// 
/// 由于 storage 使用全局 OnceLock 只能初始化一次，
/// 所以所有测试放在一个函数中顺序执行。
#[test]
fn test_all_hr_domain_agent_functions() {
    let domain = setup_test_domain();
    let ctx = new_ctx("admin");

    // ========== 测试 1: 创建 Agent
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

    domain.agent_manage().create_agent(ctx.clone(), &agent).unwrap();

    // ========== 测试 2: 获取 Agent
    let found = domain.agent_manage().get_agent(ctx.clone(), &agent.id()).unwrap().unwrap();
    assert_eq!(found.name(), "TestAgent");

    // ========== 测试 3: 列表 Agent
    for i in 0..2 {
        let agent_po = AgentPo::new(
            format!("Agent{}", i),
            Some("worker".to_string()),
            "".to_string(),
            vec![],
            "".to_string(),
            "provider-id-1".to_string(),
            "admin".to_string(),
        );
        let agent = Agent::from_po(agent_po);
        domain.agent_manage().create_agent(ctx.clone(), &agent).unwrap();
    }

    let agents = domain.agent_manage().list_agents(ctx.clone()).unwrap();
    assert_eq!(agents.len(), 3);

    // ========== 测试 4: 更新 Agent
    let mut updated = agent.clone();
    updated.po.name = "Updated".to_string();
    domain.agent_manage().update_agent(new_ctx("editor"), &updated).unwrap();

    let found = domain.agent_manage().get_agent(ctx.clone(), &updated.id()).unwrap().unwrap();
    assert_eq!(found.name(), "Updated");

    // ========== 测试 5: 删除 Agent
    domain.agent_manage().delete_agent(ctx.clone(), &updated).unwrap();
    assert!(domain.agent_manage().get_agent(ctx.clone(), &updated.id()).unwrap().is_none());
}
