//! Agent DAO SQLite 单元测试

use sqlx::SqlitePool;
use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use common::enums::AgentStatus;
use crate::service::dao::agent::AgentDaoTrait;
use uuid::Uuid;
use crate::service::dao::agent::sqlite::{dao, init};

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

#[sqlx::test]
async fn test_insert_and_find_by_id(pool: SqlitePool) {
    init();
    
    let ctx = RequestContext::new_simple("admin", pool);
    let agent_dao = dao();

    // ========== 测试: 插入并查询
    let agent_po = AgentPo::new(
        "TestAgent".to_string(),
        "worker".to_string(),
        "A helpful agent".to_string(),
        vec!["coding".to_string()],
        "A helpful agent that can code".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let result = agent_dao.insert(ctx.clone(), &agent_po).await;
    assert!(result.is_ok());

    // 验证插入成功（使用 DAO 接口查询，不是直接 SQL）
    let found = agent_dao.find_by_id(ctx, &agent_po.id).await.unwrap();
    assert!(found.is_some());
    let found_agent = found.unwrap();
    assert_eq!(found_agent.name,"TestAgent".to_string());
    assert_eq!(found_agent.created_by,"admin".to_string());
}

#[sqlx::test]
async fn test_find_all(pool: SqlitePool) {
    init();

    let ctx = new_ctx("admin", pool);
    let agent_dao = dao();

    // 插入两个 Agent（全部通过 DAO 接口插入）
    for i in 0..2 {
        let agent_po2 = AgentPo::new(
            format!("Agent{}", i),
            "worker".to_string(),
            "".to_string(),
            vec![],
            "".to_string(),
            format!("provider-{}", i),
            "admin".to_string(),
        );
        let _ = agent_dao.insert(ctx.clone(), &agent_po2).await;
    }

    let all = agent_dao.find_all(ctx).await.unwrap();
    assert_eq!(all.len(), 2); // 每个测试独立数据库，只有这里插入的2个
}

#[sqlx::test]
async fn test_update(pool: SqlitePool) {
    init();

    let ctx = new_ctx("admin", pool);
    let agent_dao = dao();

    let agent_po = AgentPo::new(
        "Original".to_string(),
        "worker".to_string(),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let _ = agent_dao.insert(ctx.clone(), &agent_po).await;

    let found = agent_dao.find_by_id(ctx.clone(), &agent_po.id.clone()).await.unwrap().unwrap();
    let mut updated = found;
    updated.name ="UpdatedAgent".to_string();
    updated.modified_by = "editor".to_string();
    let result = agent_dao.update(ctx.clone(), &updated).await;
    assert!(result.is_ok());
    let found_after_update = agent_dao.find_by_id(ctx.clone(), &updated.id).await.unwrap().unwrap();
    assert_eq!(found_after_update.name,"UpdatedAgent".to_string());
    assert_eq!(found_after_update.modified_by,"editor".to_string());
}

#[sqlx::test]
async fn test_soft_delete(pool: SqlitePool) {
    init();

    let ctx = new_ctx("admin", pool);
    let agent_dao = dao();

    let agent_po = AgentPo::new(
        "ToDelete".to_string(),
        "worker".to_string(),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let _ = agent_dao.insert(ctx.clone(), &agent_po).await;

    assert!(agent_dao.delete(ctx.clone(), &agent_po).await.is_ok());
    let found_after_delete = agent_dao.find_by_id(ctx.clone(), &agent_po.id).await.unwrap();
    assert!(found_after_delete.is_none());
}

#[sqlx::test]
async fn test_find_all_excludes_deleted(pool: SqlitePool) {
    init();

    let ctx = new_ctx("admin", pool);
    let agent_dao = dao();

    // 插入两个 Agent，删除一个
    let agent_po1 = AgentPo::new(
        "Normal".to_string(),
        "worker".to_string(),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let agent_po2 = AgentPo::new(
        "Deleted".to_string(),
        "worker".to_string(),
        "".to_string(),
        vec![],
        "".to_string(),
        "provider-id-2".to_string(),
        "admin".to_string(),
    );

    let _ = agent_dao.insert(ctx.clone(), &agent_po1).await;
    let _ = agent_dao.insert(ctx.clone(), &agent_po2).await;
    let _ = agent_dao.delete(ctx.clone(), &agent_po2).await;

    let result = agent_dao.find_all(ctx).await.unwrap();
    assert_eq!(result.len(), 1);
    let names: Vec<String> = result.iter().map(|a| a.name.clone()).collect();
    assert!(names.contains(&"Normal".to_string()));
    assert!(!names.contains(&"Deleted".to_string()));
}

#[sqlx::test]
async fn test_find_not_exists(pool: SqlitePool) {
    init();

    let ctx = new_ctx("admin", pool);
    let agent_dao = dao();

    let found_none = agent_dao.find_by_id(ctx, "not-exist-id").await.unwrap();
    assert!(found_none.is_none());
}
