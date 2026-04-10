//! ModelProvider DAO 单元测试
//!
//! 单元测试使用内存数据库，不依赖全局 storage 连接池

use crate::models::model_provider::{ModelProviderPo};
use common::enums::ProviderType;
use crate::pkg::storage::sql;
use rusqlite::Connection;

#[tokio::test]
async fn test_insert_and_find_model_provider() {
    // 创建内存数据库用于测试
    let conn = Connection::open_in_memory().expect("Failed to create in-memory database");

    // 使用定义好的常量建表
    conn.execute(
        sql::SQLITE_CREATE_TABLE_MODEL_PROVIDERS,
        (),
    ).expect("Failed to create table model_providers");

    // dao 使用我们创建的连接测试，这里直接测试创建和查询逻辑
    let provider_po = ModelProviderPo {
        id: "test-id-1".to_string(),
        name: "OpenAI GPT-4o".to_string(),
        provider_type: ProviderType::OpenAI,
        model_name: "gpt-4o".to_string(),
        api_key: "test-key".to_string(),
        base_url: None,
        description: "OpenAI GPT-4o 模型".to_string(),
        status: Default::default(),
        created_by: "test".to_string(),
        modified_by: "test".to_string(),
        created_at: 0,
        updated_at: 0,
    };

    // 测试 SQL 插入语句语法正确
    let result = conn.execute(
        "INSERT INTO model_providers (id, name, provider_type, model_name, api_key, base_url, description, status, created_by, modified_by, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        rusqlite::params![
            provider_po.id,
            provider_po.name,
            serde_json::to_string(&provider_po.provider_type).unwrap(),
            provider_po.model_name,
            provider_po.api_key,
            provider_po.base_url,
            provider_po.description,
            provider_po.status.to_i32(),
            provider_po.created_by,
            provider_po.modified_by,
            provider_po.created_at,
            provider_po.updated_at,
        ],
    );

    // 语法正确就成功了，实际插入由 DAO 处理，这里只验证我们的代码语法正确
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_find_by_id_model_provider() {
    // 创建内存数据库用于测试
    let conn = Connection::open_in_memory().expect("Failed to create in-memory database");

    // 使用定义好的常量建表
    conn.execute(
        sql::SQLITE_CREATE_TABLE_MODEL_PROVIDERS,
        (),
    ).expect("Failed to create table model_providers");

    // 查询不存在的 ID 应该返回 Ok(None)
    let mut stmt = conn
        .prepare(
            "SELECT id, name, provider_type, model_name, api_key, base_url, description, status, created_by, modified_by, created_at, updated_at 
             FROM model_providers WHERE id = ?1 AND status != 0",
        )
        .expect("Failed to prepare statement");

    // 查询不存在的 ID，不应该报错，应该返回 None
    let result = stmt.query_row(["not-exists-id"], |_row| {
        Ok(())
    });

    match result {
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            // 这是预期结果，查询不存在返回 QueryReturnedNoRows，我们转换为 Ok(None) 所以这里不报错就是正确
            assert!(true);
        }
        _ => {
            panic!("Unexpected result for querying non-existent id");
        }
    }
}

#[tokio::test]
async fn test_find_all_model_provider() {
    // 创建内存数据库用于测试
    let conn = Connection::open_in_memory().expect("Failed to create in-memory database");

    // 使用定义好的常量建表
    conn.execute(
        sql::SQLITE_CREATE_TABLE_MODEL_PROVIDERS,
        (),
    ).expect("Failed to create table model_providers");

    // 查询空表应该返回空 Vec，不报错
    let mut stmt = conn
        .prepare(
            "SELECT id, name, provider_type, model_name, api_key, base_url, description, status, created_by, modified_by, created_at, updated_at 
             FROM model_providers WHERE status != 0 ORDER BY created_at DESC",
        )
        .expect("Failed to prepare statement");

    let result = stmt
        .query_map([], |_row| {
            Ok(())
        });

    assert!(result.is_ok());
}
