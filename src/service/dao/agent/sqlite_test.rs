//! Agent DAO SQLite 单元测试

use crate::models::agent::AgentPo;
use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentDao;
use crate::service::dao::agent::sqlite::{dao, init};
use sqlx::SqlitePool;
use std::sync::Arc;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化测试环境
fn init_test_env() -> Arc<dyn AgentDao> {
    init();
    dao()
}

/// 创建测试 AgentPo
fn create_test_agent(name: &str, provider_id: &str, created_by: &str) -> AgentPo {
    AgentPo::new(
        name.to_string(),
        vec!["worker".to_string()],
        "".to_string(),
        vec![],
        "".to_string(),
        provider_id.to_string(),
        created_by.to_string(),
    )
}

/// 短词元（<3 字符）LIKE 兜底路径回归测试。
///
/// 此前 QueryBuilder 两段式拼接（push 含 `?` 文本 + push_bind 参数）产生悬空
/// `???` 占位符，SQLite 报 `near "?": syntax error`。
#[sqlx::test]
async fn test_search_agents_short_keyword_like_fallback(pool: SqlitePool) {
    let agent_dao = init_test_env();

    let agent = create_test_agent("小叶助手", "provider-id-1", "admin");
    agent_dao
        .insert(new_ctx("admin", pool.clone()), &agent)
        .await
        .unwrap();

    let results = agent_dao
        .search_agents(
            new_ctx("admin", pool),
            crate::service::dao::agent::AgentSearch {
                keyword: Some("小叶".to_string()),
                ..Default::default()
            },
        )
        .await;
    assert!(
        results.is_ok(),
        "短关键词 LIKE 兜底路径执行失败: {:?}",
        results.err()
    );
    let results = results.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0.name, "小叶助手");
}

#[sqlx::test]
async fn test_insert_and_find_by_id(pool: SqlitePool) {
    let agent_dao = init_test_env();

    // ========== 测试: 插入并查询
    let agent_po = create_test_agent("TestAgent", "provider-id-1", "admin");
    let result = agent_dao
        .insert(new_ctx("admin", pool.clone()), &agent_po)
        .await;
    assert!(result.is_ok());

    // 验证插入成功（使用 DAO 接口查询，不是直接 SQL）
    let found = agent_dao
        .find_by_id(new_ctx("admin", pool), &agent_po.id)
        .await
        .unwrap();
    assert!(found.is_some());
    let found_agent = found.unwrap();
    assert_eq!(found_agent.name, "TestAgent".to_string());
    assert_eq!(found_agent.created_by, "admin".to_string());
}

#[sqlx::test]
async fn test_find_all(pool: SqlitePool) {
    let agent_dao = init_test_env();

    // 插入两个 Agent（全部通过 DAO 接口插入）
    for i in 0..2 {
        let agent_po2 =
            create_test_agent(&format!("Agent{}", i), &format!("provider-{}", i), "admin");
        let _ = agent_dao
            .insert(new_ctx("admin", pool.clone()), &agent_po2)
            .await;
    }

    let all = agent_dao.find_all(new_ctx("admin", pool)).await.unwrap();
    assert_eq!(all.len(), 2); // 每个测试独立数据库，只有这里插入的2个
}

#[sqlx::test]
async fn test_update(pool: SqlitePool) {
    let agent_dao = init_test_env();

    let agent_po = create_test_agent("Original", "provider-id-1", "admin");
    let _ = agent_dao
        .insert(new_ctx("admin", pool.clone()), &agent_po)
        .await;

    let found = agent_dao
        .find_by_id(new_ctx("admin", pool.clone()), &agent_po.id.clone())
        .await
        .unwrap()
        .unwrap();
    let mut updated = found;
    updated.name = "UpdatedAgent".to_string();
    let result = agent_dao
        .update(new_ctx("editor", pool.clone()), &updated)
        .await;
    assert!(result.is_ok());
    let found_after_update = agent_dao
        .find_by_id(new_ctx("admin", pool), &updated.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found_after_update.name, "UpdatedAgent".to_string());
    assert_eq!(found_after_update.modified_by, "editor".to_string());
}

#[sqlx::test]
async fn test_soft_delete(pool: SqlitePool) {
    let agent_dao = init_test_env();

    let agent_po = create_test_agent("ToDelete", "provider-id-1", "admin");
    let _ = agent_dao
        .insert(new_ctx("admin", pool.clone()), &agent_po)
        .await;

    assert!(
        agent_dao
            .delete(new_ctx("admin", pool.clone()), &agent_po)
            .await
            .is_ok()
    );
    let found_after_delete = agent_dao
        .find_by_id(new_ctx("admin", pool), &agent_po.id)
        .await
        .unwrap();
    assert!(found_after_delete.is_none());
}

#[sqlx::test]
async fn test_find_all_excludes_deleted(pool: SqlitePool) {
    let agent_dao = init_test_env();

    // 插入两个 Agent，删除一个
    let agent_po1 = create_test_agent("Normal", "provider-id-1", "admin");
    let agent_po2 = create_test_agent("Deleted", "provider-id-2", "admin");

    let _ = agent_dao
        .insert(new_ctx("admin", pool.clone()), &agent_po1)
        .await;
    let _ = agent_dao
        .insert(new_ctx("admin", pool.clone()), &agent_po2)
        .await;
    let _ = agent_dao
        .delete(new_ctx("admin", pool.clone()), &agent_po2)
        .await;

    let result = agent_dao.find_all(new_ctx("admin", pool)).await.unwrap();
    assert_eq!(result.len(), 1);
    let names: Vec<String> = result.iter().map(|a| a.name.clone()).collect();
    assert!(names.contains(&"Normal".to_string()));
    assert!(!names.contains(&"Deleted".to_string()));
}

#[sqlx::test]
async fn test_find_not_exists(pool: SqlitePool) {
    let agent_dao = init_test_env();

    let found_none = agent_dao
        .find_by_id(new_ctx("admin", pool), "not-exist-id")
        .await
        .unwrap();
    assert!(found_none.is_none());
}
