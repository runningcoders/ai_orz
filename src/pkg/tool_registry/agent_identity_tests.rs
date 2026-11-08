//! agent_identity 单元测试（纯函数 + DB 派生 + 缓存 TTL）
//!
//! 用 `#[path]` 而非内联 `mod tests`：保住 `use super::*` 可见父模块私有 helper，
//! 同时规避 inline-test-lint 对内联测试体量的限制。

use super::*;

#[test]
fn derive_identity_builds_role_slug_and_email() {
    let identity = derive_identity(Some(r#"["tech_lead"]"#), "000000006aae6de3", 1)
        .expect("role 数组应派生出身份");
    assert_eq!(identity.name, "tech_lead");
    assert_eq!(identity.email, "tech_lead@agents.ai-orz.local");
}

#[test]
fn derive_identity_multi_agent_role_appends_short_agent_id() {
    // 同一 role 有第二人时，邮箱加 agent_id 前 8 位消歧（当前 role 唯一，分支预留）
    let identity = derive_identity(Some(r#"["backend_developer"]"#), "abcdef0123456789", 2)
        .expect("多 Agent 同 role 应仍可派生");
    assert_eq!(identity.name, "backend_developer");
    assert_eq!(
        identity.email,
        "backend_developer+abcdef01@agents.ai-orz.local"
    );
}

#[test]
fn derive_identity_tolerates_scalar_and_rejects_missing_role() {
    let scalar = derive_identity(Some("\"reception\""), "agent-1", 1).expect("裸字符串也应支持");
    assert_eq!(scalar.email, "reception@agents.ai-orz.local");

    // 无 role / 空数组 / 空串 / 非法 JSON → None（不凭空造值）
    assert!(derive_identity(None, "agent-1", 1).is_none());
    assert!(derive_identity(Some("[]"), "agent-1", 1).is_none());
    assert!(derive_identity(Some("  "), "agent-1", 1).is_none());
    assert!(derive_identity(Some("not-json"), "agent-1", 1).is_none());
}

#[test]
fn derive_identity_sanitizes_role_slug() {
    let spaced = derive_identity(Some(r#"["Tech Lead"]"#), "agent-1", 1).expect("应归一化");
    assert_eq!(spaced.name, "tech-lead", "空格折成 '-'，大写转小写");
    // 纯符号 role 归一后为空 → None
    assert!(derive_identity(Some(r#"["///"]"#), "agent-1", 1).is_none());
}

#[test]
fn cache_lookup_expires_after_ttl() {
    let agent_id = "unit-test-agent-cache-expiry";
    clear_cache();
    let stale = Instant::now() - (CACHE_TTL + Duration::from_secs(1));
    cache_store(
        agent_id,
        Some(AgentGitIdentity {
            name: "tech_lead".to_string(),
            email: "tech_lead@agents.ai-orz.local".to_string(),
        }),
        stale,
    );
    assert!(
        cache_lookup(agent_id).is_none(),
        "超过 TTL 应视为未命中（重新查库）"
    );
}

#[test]
fn cache_keeps_negative_hits() {
    let agent_id = "unit-test-agent-cache-negative";
    clear_cache();
    cache_store(agent_id, None, Instant::now());
    assert_eq!(
        cache_lookup(agent_id),
        Some(None),
        "负命中也要缓存（否则未知 agent 会反复打库）"
    );
}

async fn test_pool() -> SqlitePool {
    use sqlx::sqlite::SqlitePoolOptions;
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    sqlx::query("CREATE TABLE agents (id TEXT PRIMARY KEY, name TEXT, role TEXT, status INTEGER NOT NULL DEFAULT 0)")
        .execute(&pool)
        .await
        .expect("create agents");
    pool
}

#[tokio::test]
async fn resolve_reads_role_from_agents_table() {
    let pool = test_pool().await;
    sqlx::query("INSERT INTO agents (id, name, role, status) VALUES (?1, ?2, ?3, 3)")
        .bind("resolve-agent-single")
        .bind("技术负责人")
        .bind(r#"["tech_lead"]"#)
        .execute(&pool)
        .await
        .expect("insert agent");
    clear_cache();

    let identity = resolve(&pool, "resolve-agent-single")
        .await
        .expect("应派生成功");
    assert_eq!(identity.name, "tech_lead");
    assert_eq!(identity.email, "tech_lead@agents.ai-orz.local");
}

#[tokio::test]
async fn resolve_returns_none_for_unknown_or_disabled_agent() {
    let pool = test_pool().await;
    sqlx::query("INSERT INTO agents (id, name, role, status) VALUES (?1, ?2, ?3, 0)")
        .bind("resolve-agent-disabled")
        .bind("已离职")
        .bind(r#"["tech_lead"]"#)
        .execute(&pool)
        .await
        .expect("insert agent");
    clear_cache();

    assert!(resolve(&pool, "resolve-agent-missing").await.is_none());
    assert!(
        resolve(&pool, "resolve-agent-disabled").await.is_none(),
        "status=0 的 Agent 不应派生身份"
    );
}

#[tokio::test]
async fn resolve_disambiguates_role_shared_by_multiple_agents() {
    let pool = test_pool().await;
    for (id, status) in [("shared-agent-a", 3), ("shared-agent-b", 3)] {
        sqlx::query("INSERT INTO agents (id, name, role, status) VALUES (?1, ?2, ?3, ?4)")
            .bind(id)
            .bind(id)
            .bind(r#"["backend_developer"]"#)
            .bind(status)
            .execute(&pool)
            .await
            .expect("insert agent");
    }
    clear_cache();

    let identity = resolve(&pool, "shared-agent-a").await.expect("应派生成功");
    assert_eq!(
        identity.email,
        "backend_developer+shared-a@agents.ai-orz.local"
    );
}
