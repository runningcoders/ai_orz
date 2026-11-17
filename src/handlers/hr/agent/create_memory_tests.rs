//! create_memory 的 ID 生成回归测试。

use super::*;
use common::api::CreateMemoryParams;

fn init_env(pool: sqlx::SqlitePool) -> RequestContext {
    let _ = crate::config::init();
    let base_path = crate::config::get().base_data_path();
    crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);
    crate::service::dao::init_all();
    crate::service::dal::init_all();
    crate::service::domain::runtime::init();
    crate::pkg::request_context_test_support::new_test_ctx("test-user", pool)
}

fn assert_id(id: &str, prefix: common::id::IdPrefix) {
    let parsed =
        common::id::EntityId::parse(id).unwrap_or_else(|e| panic!("ID 解析失败: {id} ({e})"));
    assert_eq!(parsed.prefix(), &prefix, "ID 前缀错误: {id}");
    assert_eq!(parsed.uuid().get_version_num(), 7, "应使用 UUIDv7: {id}");
    assert!(!id.contains('-'), "ID 不能带横杠: {id}");
}

/// 短期记忆与知识节点都必须使用带前缀的 UUIDv7，且同秒同内容不能撞主键。
#[sqlx::test]
async fn create_memory_uses_uuid_v7_ids(pool: sqlx::SqlitePool) {
    let ctx = init_env(pool);

    let params = CreateMemoryParams {
        memory_type: "short_term".to_string(),
        content: "同秒同摘要".to_string(),
        summary: Some("同秒同摘要".to_string()),
        tags: None,
        task_id: None,
    };
    let first = create_memory(ctx.clone(), params.clone()).await.unwrap();
    let second = create_memory(ctx.clone(), params).await.unwrap();
    assert_id(&first.memory_id, common::id::IdPrefix::ShortTerm);
    assert_id(&second.memory_id, common::id::IdPrefix::ShortTerm);
    assert_ne!(
        first.memory_id, second.memory_id,
        "同秒同摘要也必须拿到不同 ID"
    );

    let node = create_memory(
        ctx,
        CreateMemoryParams {
            memory_type: "knowledge_node".to_string(),
            content: "知识节点内容".to_string(),
            summary: Some("知识节点摘要".to_string()),
            tags: None,
            task_id: None,
        },
    )
    .await
    .unwrap();
    assert_id(&node.memory_id, common::id::IdPrefix::KnowledgeNode);
}
