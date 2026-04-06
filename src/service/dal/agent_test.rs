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
        "provider-id-1".to_string(),
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
            format!("provider-{}", i),
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
        "provider-id-1".to_string(),
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
        "provider-id-1".to_string(),
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

    let agent1_po = AgentPo::new("Normal".to_string(), "w".to_string(), vec![], "".to_string(), "provider-id-1".to_string(), "admin".to_string());
    let agent2_po = AgentPo::new("Deleted".to_string(), "w".to_string(), vec![], "".to_string(), "provider-id-2".to_string(), "admin".to_string());

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

#[test]
fn test_wake_brain_without_provider_not_exists() {
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    // Create agent with non-existent model provider id
    let agent_po = AgentPo::new(
        "TestAgent".to_string(),
        "worker".to_string(),
        vec!["coding".to_string()],
        "A helpful agent".to_string(),
        "non-existent-provider-id".to_string(),
        "admin".to_string(),
    );
    let mut agent = Agent::from_po(agent_po);

    // Wake brain without passing model provider (should query from db and fail)
    let result = dal.wake_brain(ctx, &mut agent, None);
    assert!(result.is_err());
    // Should be NotFound error
    assert!(result.unwrap_err().is_not_found());
}

#[test]
fn test_wake_brain_with_passed_provider() {
    // This test verifies that:
    // 1. When passing a model provider, it updates the model_provider_id in po
    // 2. It successfully creates the cortex and stores it in agent.brain
    let dal = setup_test_dal();
    let ctx = new_ctx("admin");

    // Create agent
    let agent_po = AgentPo::new(
        "TestAgent".to_string(),
        "worker".to_string(),
        vec!["coding".to_string()],
        "A helpful agent".to_string(),
        "old-provider-id".to_string(),
        "admin".to_string(),
    );
    let mut agent = Agent::from_po(agent_po);
    dal.create(ctx.clone(), &agent).unwrap();

    // Create a mock model provider (we just need the structure)
    use crate::models::model_provider::{ModelProvider, ModelProviderPo};
    use crate::pkg::constants::ProviderType;
    let mp_po = ModelProviderPo::new(
        "OpenAI".to_string(),
        "gpt-4o-mini".to_string(),
        ProviderType::OpenAI,
        "https://api.openai.com/v1".to_string(),
        "sk-test-key".to_string(),
        "admin".to_string(),
    );
    let mp = ModelProvider::from_po(mp_po);

    // Wake brain with the passed model provider
    // Note: BrainDao will not actually be able to connect since it's a test with fake key
    // but it should still successfully update model_provider_id and store the brain (the actual connection error happens inside brain_dao)
    let result = dal.wake_brain(ctx.clone(), &mut agent, Some(&mp));

    // The model_provider_id should be updated
    assert_eq!(agent.po.model_provider_id, mp.po.id);

    // The brain field should be Some
    assert!(agent.brain.is_some());

    // If the database was updated, we should see the new model_provider_id when we query back
    let found = dal.find_by_id(ctx.clone(), &agent.id()).unwrap().unwrap();
    assert_eq!(found.po.model_provider_id, mp.po.id);
}
