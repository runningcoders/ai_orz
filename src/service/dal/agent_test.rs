//! Agent DAL 单元测试

use super::*;
use crate::models::agent::{Agent, AgentPo};
use crate::pkg::storage;
use crate::service::dao::agent;

fn new_ctx(user_id: &str) -> crate::pkg::RequestContext {
    crate::pkg::RequestContext::new(Some(user_id.to_string()), None)
}

fn setup_test_dal() -> Arc<dyn AgentDalTrait> {
    // 初始化内存数据库并创建表
    let conn = storage::test_conn();
    conn.execute_batch(crate::pkg::sql::SQLITE_CREATE_TABLE_AGENTS)
        .unwrap();

    // 使用 DAO 的初始化方法，不需要直接操作静态变量
    agent::init();

    Arc::new(AgentDal::new(agent::dao()))
}

#[test]
fn test_create_and_find_by_id() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "TestAgent".to_string(),
        "worker".to_string(),
        vec!["coding".to_string()],
        "A helpful agent".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po);

    dal.create(ctx.clone(), &agent).unwrap();
    let found = dal.find_by_id(ctx, &agent.id()).unwrap().unwrap();

    assert_eq!(found.name(), "TestAgent");
    assert_eq!(found.po.created_by, "admin");
}

#[test]
fn test_find_all() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    for i in 0..3 {
        let agent_po = AgentPo::new(
            format!("Agent{}", i),
            "worker".to_string(),
            vec![],
            "".to_string(),
            "admin".to_string(),
        );
        let agent = Agent::from_po(agent_po);
        dal.create(ctx.clone(), &agent).unwrap();
    }

    let all = dal.find_all(ctx).unwrap();
    assert_eq!(all.len(), 3);
}

#[test]
fn test_update() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "Original".to_string(),
        "worker".to_string(),
        vec![],
        "".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po);
    dal.create(ctx.clone(), &agent).unwrap();

    let mut updated = agent.clone();
    updated.po.name = "Updated".to_string();
    dal.update(new_ctx("editor"), &updated).unwrap();

    let found = dal.find_by_id(ctx, &updated.id()).unwrap().unwrap();
    assert_eq!(found.name(), "Updated");
    assert_eq!(found.po.modified_by, "editor");
}

#[test]
fn test_delete() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    let agent_po = AgentPo::new(
        "ToDelete".to_string(),
        "worker".to_string(),
        vec![],
        "".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(agent_po);
    dal.create(ctx.clone(), &agent).unwrap();

    dal.delete(ctx.clone(), &agent).unwrap();
    assert!(dal.find_by_id(ctx, &agent.id()).unwrap().is_none());
}

#[test]
fn test_find_all_excludes_deleted() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    let agent1_po = AgentPo::new("Normal".to_string(), "w".to_string(), vec![], "".to_string(), "admin".to_string());
    let agent2_po = AgentPo::new("Deleted".to_string(), "w".to_string(), vec![], "".to_string(), "admin".to_string());

    let agent1 = Agent::from_po(agent1_po);
    let agent2 = Agent::from_po(agent2_po);

    dal.create(ctx.clone(), &agent1).unwrap();
    dal.create(ctx.clone(), &agent2).unwrap();
    dal.delete(ctx.clone(), &agent2).unwrap();

    let all = dal.find_all(ctx).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name(), "Normal");
}

#[test]
fn test_find_not_exists() {
    let dal = setup_test_dal();
    let ctx = new_ctx("user1");

    assert!(dal.find_by_id(ctx, "not-exists").unwrap().is_none());
}
