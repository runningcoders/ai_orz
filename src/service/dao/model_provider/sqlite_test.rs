//! ModelProvider DAO 单元测试
//!
//! 单元测试使用独立数据库，测试隔离性好

use crate::models::model_provider::ModelProviderPo;
use common::enums::{ProviderType, ModelProviderStatus};
use crate::service::dao::model_provider::{self, ModelProviderDao};
use crate::pkg::RequestContext;
use uuid::Uuid;
use sqlx::SqlitePool;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

#[sqlx::test]
async fn test_insert_and_find_model_provider(pool: SqlitePool) {
    // sqlx::test 自动创建空数据库并运行迁移
    crate::service::dao::model_provider::init();
    let dao = model_provider::dao();

    // 创建测试对象
    let provider_po = ModelProviderPo::new(
        "OpenAI GPT-4o".to_string(),
        ProviderType::OpenAI,
        "gpt-4o".to_string(),
        "test-key".to_string(),
        None,
        Some("OpenAI GPT-4o 模型".to_string()),
        "test".to_string(),
    );

    // 测试插入
    let result = dao.insert(new_ctx("test", pool.clone()), &provider_po).await;
    assert!(result.is_ok());

    // 测试查询
    let found = dao.find_by_id(new_ctx("test", pool), provider_po.id.as_str()).await.expect("Query failed");
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, "OpenAI GPT-4o".to_string());
    assert_eq!(found.provider_type, ProviderType::OpenAI);
    assert_eq!(found.model_name, "gpt-4o".to_string());
    assert_eq!(found.api_key, "test-key".to_string());
    assert_eq!(found.description, Some("OpenAI GPT-4o 模型".to_string()));
    assert_eq!(found.status, ModelProviderStatus::Normal);
}

#[sqlx::test]
async fn test_find_by_id_not_exists(pool: SqlitePool) {
    // sqlx::test 自动创建空数据库并运行迁移
    crate::service::dao::model_provider::init();
    let dao = model_provider::dao();

    // 查询不存在的 ID 应该返回 Ok(None)
    let found = dao.find_by_id(new_ctx("test", pool), "not-exists-id").await.expect("Query failed");
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_find_all_model_provider(pool: SqlitePool) {
    // sqlx::test 自动创建空数据库并运行迁移
    crate::service::dao::model_provider::init();
    let dao = model_provider::dao();

    // 查询空表应该返回空 Vec，不报错
    let all = dao.find_all(new_ctx("test", pool)).await.expect("Query all failed");
    assert!(all.is_empty());
}


#[sqlx::test]
async fn test_query(pool: sqlx::SqlitePool) {
    crate::service::dao::model_provider::init();
    let dao = crate::service::dao::model_provider::dao();
    let ctx = crate::pkg::RequestContext::new_simple("admin", pool);
    
    use common::enums::ModelProviderStatus;
    use crate::service::dao::model_provider::ModelProviderQuery;
    
    // 测试空查询
    let query = ModelProviderQuery::default();
    let result = dao.query(ctx, query).await;
    println!("Query result: {:?}", result);
    assert!(result.is_ok());
}
