//! Agent DAL 单元测试

use crate::service::dal::agent::{dal, init};
use crate::service::dao::agent::init as agent_dao_init;
use crate::models::agent::{Agent, AgentPo};
use crate::pkg::RequestContext;
use std::sync::Arc;
use sqlx::SqlitePool;
use uuid::Uuid;

/// 初始化测试环境
async fn init_test_env(pool: SqlitePool) -> (Arc<dyn crate::service::dal::agent::AgentDal + Send + Sync>, RequestContext) {
    agent_dao_init();
    init();
    let dal = dal();
    let ctx = RequestContext::new_simple("admin", pool);
    (dal, ctx)
}

/// 创建测试 Agent
fn create_test_agent(name: &str, provider_id: &str) -> Agent {
    let agent_po = AgentPo::new(
        name.to_string(),
        vec!["worker".to_string()],
        "".to_string(),
        vec![],
        "".to_string(),
        provider_id.to_string(),
        "admin".to_string(),
    );
    Agent::from_po(agent_po)
}

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let agent = create_test_agent("TestAgent", "provider-id-1");
    dal.create(ctx.clone(), &agent).await.unwrap();

    let found: Option<Agent> = dal.find_by_id(ctx, &agent.id()).await.unwrap();
    assert_eq!(found.as_ref().unwrap().name(), "TestAgent");
    assert_eq!(found.unwrap().po.created_by, "admin".to_string());
}

#[sqlx::test]
async fn test_find_all(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    for i in 0..3 {
        let agent = create_test_agent(&format!("Agent{}", i), &format!("provider-{}", i));
        dal.create(ctx.clone(), &agent).await.unwrap();
    }

    let all: Vec<Agent> = dal.find_all(ctx).await.unwrap();
    assert_eq!(all.len(), 3);
}

#[sqlx::test]
async fn test_update(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool.clone()).await;

    let agent = create_test_agent("Original", "provider-id-1");
    dal.create(ctx.clone(), &agent).await.unwrap();

    let mut updated = agent.clone();
    updated.po.name = "Updated".to_string();
    dal.update(RequestContext::new_simple("editor", pool), &updated).await.unwrap();

    let found: Option<Agent> = dal.find_by_id(ctx, &updated.id()).await.unwrap();
    assert_eq!(found.as_ref().unwrap().name(), "Updated");
    assert_eq!(found.unwrap().po.modified_by, "editor".to_string());
}

#[sqlx::test]
async fn test_delete(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let agent = create_test_agent("ToDelete", "provider-id-1");
    dal.create(ctx.clone(), &agent).await.unwrap();

    dal.delete(ctx.clone(), &agent).await.unwrap();
    let found: Option<Agent> = dal.find_by_id(ctx, &agent.id()).await.unwrap();
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_find_all_excludes_deleted(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let agent1 = create_test_agent("Normal", "provider-id-1");
    let agent2 = create_test_agent("Deleted", "provider-id-2");

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
async fn test_find_not_exists(pool: SqlitePool) {
    let (dal, ctx) = init_test_env(pool).await;

    let found: Option<Agent> = dal.find_by_id(ctx, "not-exists").await.unwrap();
    assert!(found.is_none());
}
