use super::*;

async fn pool_with_schema() -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite should connect");
    sqlx::query(
        "CREATE TABLE agents (id TEXT PRIMARY KEY, name TEXT NOT NULL, role TEXT, \
         capabilities TEXT, status INTEGER NOT NULL DEFAULT 0)",
    )
    .execute(&pool)
    .await
    .expect("agents table");
    sqlx::query(
        "CREATE TABLE users (id TEXT PRIMARY KEY, username TEXT NOT NULL, \
         display_name TEXT NOT NULL, email TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .expect("users table");
    pool
}

async fn insert_agent(pool: &SqlitePool, id: &str, name: &str, role: &str, caps: &str) {
    sqlx::query(
        "INSERT INTO agents (id, name, role, capabilities, status) VALUES (?1, ?2, ?3, ?4, 3)",
    )
    .bind(id)
    .bind(name)
    .bind(role)
    .bind(caps)
    .execute(pool)
    .await
    .expect("insert agent");
}

#[tokio::test]
async fn agent_dataset_derives_all_identity_fields() {
    let pool = pool_with_schema().await;
    insert_agent(
        &pool,
        "agent-derive",
        "技术负责人",
        "[\"tech_lead\"]",
        "[\"rust\", \"architecture\"]",
    )
    .await;

    let dataset = agent_dataset(&pool, "agent-derive")
        .await
        .expect("dataset should hit");
    assert_eq!(dataset.id, "agent-derive");
    assert_eq!(dataset.name, "技术负责人");
    assert_eq!(dataset.role, "tech_lead");
    assert_eq!(dataset.roles, vec!["tech_lead".to_string()]);
    assert_eq!(dataset.git_name, "tech_lead");
    assert_eq!(dataset.email, "tech_lead@agents.ai-orz.local");
    assert_eq!(
        dataset.tags,
        vec!["rust".to_string(), "architecture".to_string()]
    );

    // 白名单字段提取（注入需求消费口径）
    assert_eq!(
        dataset.field_value("email").as_deref(),
        Some("tech_lead@agents.ai-orz.local")
    );
    assert_eq!(dataset.field_value("roles").as_deref(), Some("tech_lead"));
    assert_eq!(
        dataset.field_value("tags").as_deref(),
        Some("rust,architecture")
    );
    // 越界字段 = 派生失败（不造值）
    assert_eq!(dataset.field_value("display_name"), None);
}

#[tokio::test]
async fn agent_dataset_email_carries_short_code_when_role_shared() {
    let pool = pool_with_schema().await;
    insert_agent(
        &pool,
        "agent-shared-1",
        "前端一",
        "[\"frontend_developer\"]",
        "[]",
    )
    .await;
    insert_agent(
        &pool,
        "agent-shared-2",
        "前端二",
        "[\"frontend_developer\"]",
        "[]",
    )
    .await;

    let dataset = agent_dataset(&pool, "agent-shared-1")
        .await
        .expect("dataset should hit");
    assert_eq!(dataset.git_name, "frontend_developer");
    assert!(dataset.email.starts_with("frontend_developer+"));
    assert!(dataset.email.contains("agent-sh"));
    assert!(dataset.email.ends_with("@agents.ai-orz.local"));
}

#[tokio::test]
async fn agent_dataset_missing_agent_or_empty_id_is_none() {
    let pool = pool_with_schema().await;
    assert!(agent_dataset(&pool, "agent-absent").await.is_none());
    assert!(agent_dataset(&pool, "   ").await.is_none());
}

#[tokio::test]
async fn agent_dataset_cache_serves_stale_until_cleared() {
    let pool = pool_with_schema().await;
    insert_agent(
        &pool,
        "agent-cache",
        "旧名",
        "[\"backend_developer\"]",
        "[]",
    )
    .await;
    assert_eq!(
        agent_dataset(&pool, "agent-cache").await.unwrap().name,
        "旧名"
    );

    sqlx::query("UPDATE agents SET name = '新名' WHERE id = 'agent-cache'")
        .execute(&pool)
        .await
        .unwrap();
    // TTL 内命中缓存（仍为旧值）
    assert_eq!(
        agent_dataset(&pool, "agent-cache").await.unwrap().name,
        "旧名"
    );
    clear_cache();
    assert_eq!(
        agent_dataset(&pool, "agent-cache").await.unwrap().name,
        "新名"
    );
}

#[tokio::test]
async fn user_dataset_reads_users_row_and_field_whitelist() {
    let pool = pool_with_schema().await;
    sqlx::query(
        "INSERT INTO users (id, username, display_name, email) \
         VALUES ('u-1', 'wangting', 'AMan', 'aman@example.com')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let dataset = user_dataset(&pool, "u-1", None)
        .await
        .expect("dataset should hit");
    assert_eq!(dataset.field_value("username").as_deref(), Some("wangting"));
    assert_eq!(dataset.field_value("display_name").as_deref(), Some("AMan"));
    assert_eq!(
        dataset.field_value("email").as_deref(),
        Some("aman@example.com")
    );
    assert_eq!(dataset.field_value("role"), None); // 越界字段
}

#[tokio::test]
async fn user_dataset_falls_back_to_ctx_username_without_db_row() {
    let pool = pool_with_schema().await;
    let dataset = user_dataset(&pool, "u-fallback", Some("wangting"))
        .await
        .expect("ctx username fallback should hit");
    assert_eq!(dataset.field_value("username").as_deref(), Some("wangting"));
    // 兜底档案没有 email → field 派生失败（不造值）
    assert_eq!(dataset.field_value("email"), None);

    assert!(user_dataset(&pool, "u-no-ctx", None).await.is_none());
}
