//! Agent DAL 单元测试

use crate::service::dal::agent::{dal, init, AgentDal, AgentDalTrait};
use crate::models::agent::{Agent, AgentPo};
use crate::pkg::RequestContext;
use std::sync::Arc;
use sqlx::SqlitePool;
use uuid::Uuid;

#[sqlx::test]
async fn test_create_and_find_by_id(pool:SqlitePool) {
    init();
    let dal = dal();
    let ctx = RequestContext::new_simple("admin", pool);

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

    dal.create(ctx.clone(), &agent).await.unwrap();
    let found: Option<Agent> = dal.find_by_id(ctx, &agent.id()).await.unwrap();

    assert_eq!(found.as_ref().unwrap().name(), "TestAgent");
    assert_eq!(found.unwrap().po.created_by, "admin".to_string());
}

#[sqlx::test]
async fn test_find_all(pool:SqlitePool) {
    init();
    let dal = dal();
    let ctx = RequestContext::new_simple("admin", pool);

    for i in 0..3 {
        let agent_po = AgentPo::new(
            format!("Agent{}", i),
            "worker".to_string(),
            "".to_string(),
            vec![],
            "".to_string(),
            format!("provider-{}", i),
            "admin".to_string(),
        );
        let agent = Agent::from_po(agent_po);
        dal.create(ctx.clone(), &agent).await.unwrap();
    }

    let all: Vec<Agent> = dal.find_all(ctx).await.unwrap();
    assert_eq!(all.len(), 3);
}

#[sqlx::test]
async fn test_update(pool:SqlitePool) {
    init();
    let dal = dal();
    let ctx = RequestContext::new_simple("admin", pool.clone());

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
    dal.create(ctx.clone(), &agent).await.unwrap();

    let mut updated = agent.clone();
    updated.po.name = "Updated".to_string();
    dal.update(RequestContext::new_simple("editor", pool), &updated).await.unwrap();

    let found: Option<Agent> = dal.find_by_id(ctx, &updated.id()).await.unwrap();
    assert_eq!(found.as_ref().unwrap().name(), "Updated");
    assert_eq!(found.unwrap().po.modified_by, "editor".to_string());
}

#[sqlx::test]
async fn test_delete(pool:SqlitePool) {
    init();
    let dal = dal();
    let ctx = RequestContext::new_simple("admin", pool);

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
    dal.create(ctx.clone(), &agent).await.unwrap();

    dal.delete(ctx.clone(), &agent).await.unwrap();
    let found: Option<Agent> = dal.find_by_id(ctx, &agent.id()).await.unwrap();
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_find_all_excludes_deleted(pool:SqlitePool) {
    init();
    let dal = dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let agent1_po = AgentPo::new("Normal".to_string(), "w".to_string(), "".to_string(), vec![], "".to_string(), "provider-id-1".to_string(), "admin".to_string());
    let agent2_po = AgentPo::new("Deleted".to_string(), "w".to_string(), "".to_string(), vec![], "".to_string(), "provider-id-2".to_string(), "admin".to_string());

    let agent1 = Agent::from_po(agent1_po);
    let agent2 = Agent::from_po(agent2_po);

    dal.create(ctx.clone(), &agent1).await.unwrap();
    dal.create(ctx.clone(), &agent2).await.unwrap();
    dal.delete(ctx.clone(), &agent2).await.unwrap();

    let all: Vec<Agent> = dal.find_all(ctx).await.unwrap();
    assert_eq!(all.len(), 1);
    let names: Vec<String> = all.iter().map(|a| a.name().to_string()).collect();
    assert!(names.contains(&"Normal".to_string()));
    assert!(!names.contains(&"Deleted".to_string()));
}

#[sqlx::test]
async fn test_find_not_exists(pool:SqlitePool) {
    init();
    let dal = dal();
    let ctx = RequestContext::new_simple("admin", pool);

    let found: Option<Agent> = dal.find_by_id(ctx, "not-exists").await.unwrap();
    assert!(found.is_none());
}
