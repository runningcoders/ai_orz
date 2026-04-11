//! ModelProvider DAO 单元测试
//!
//! 单元测试使用独立数据库，测试隔离性好

use crate::models::model_provider::{ModelProviderPo};
use crate::pkg::storage::Storage;
use common::enums::{ProviderType, ModelProviderStatus};
use crate::service::dao::model_provider::{ModelProviderDao, ModelProviderDaoTrait};
use crate::pkg::RequestContext;

#[tokio::test]
async fn test_insert_and_find_model_provider() {
    // 创建临时数据库用于测试，完全隔离
    let db_path = "/tmp/ai_orz_test_model_provider_dao.db";
    let _ = std::fs::remove_file(&db_path);
    let storage = Storage::new(&db_path).await.expect("Failed to create storage");
    let pool = storage.pool();

    let ctx = RequestContext::new(Some("test".to_string()), None);
    let dao = ModelProviderDao::new(pool.clone());

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
    let result = dao.insert(ctx.clone(), &provider_po).await;
    assert!(result.is_ok());

    // 测试查询
    let found = dao.find_by_id(ctx.clone(), provider_po.id.as_ref().expect("id exists")).await.expect("Query failed");
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.name, Some("OpenAI GPT-4o".to_string()));
    assert_eq!(found.provider_type, ProviderType::OpenAI);
    assert_eq!(found.model_name, Some("gpt-4o".to_string()));
    assert_eq!(found.api_key, Some("test-key".to_string()));
    assert_eq!(found.description, Some("OpenAI GPT-4o 模型".to_string()));
    assert_eq!(found.status, ModelProviderStatus::Normal);
}

#[tokio::test]
async fn test_find_by_id_not_exists() {
    // 创建临时数据库用于测试
    let db_path = "/tmp/ai_orz_test_model_provider_not_exists.db";
    let _ = std::fs::remove_file(&db_path);
    let storage = Storage::new(&db_path).await.expect("Failed to create storage");
    let pool = storage.pool();

    let ctx = RequestContext::new(Some("test".to_string()), None);
    let dao = ModelProviderDao::new(pool.clone());

    // 查询不存在的 ID 应该返回 Ok(None)
    let found = dao.find_by_id(ctx.clone(), "not-exists-id").await.expect("Query failed");
    assert!(found.is_none());
}

#[tokio::test]
async fn test_find_all_model_provider() {
    // 创建临时数据库用于测试
    let db_path = "/tmp/ai_orz_test_model_provider_find_all.db";
    let _ = std::fs::remove_file(&db_path);
    let storage = Storage::new(&db_path).await.expect("Failed to create storage");
    let pool = storage.pool();

    let ctx = RequestContext::new(Some("test".to_string()), None);
    let dao = ModelProviderDao::new(pool.clone());

    // 查询空表应该返回空 Vec，不报错
    let all = dao.find_all(ctx.clone()).await.expect("Query all failed");
    assert!(all.is_empty());
}
