//! Skill DAO SQLite 单元测试

use common::enums::skill::SkillAuthorType;
use sqlx::SqlitePool;
use common::enums::SkillStatus;
use crate::error::AppError;
use crate::models::skill::SkillPo;
use crate::pkg::RequestContext;
use crate::service::dao::skill::{self, SkillDao};
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

/// 测试插入新技能并按 ID 查询
#[sqlx::test]
async fn test_insert_and_find_by_id(pool: SqlitePool) -> Result<(), AppError> {
    skill::init();
    let skill_dao = skill::dao();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Test Skill".to_string(),
        "A test skill for unit testing".to_string(),
        vec!["test".to_string(), "unit-test".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    let ctx = new_ctx("test-user", pool);
    let found = skill_dao.find_by_id(ctx, &skill_id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, skill_id);
    assert_eq!(found.name, "Test Skill");
    assert_eq!(found.status, SkillStatus::Draft);
    let tags = found.parse_tags();
    assert_eq!(tags.len(), 2);
    assert!(tags.contains(&"test".to_string()));

    Ok(())
}

/// 测试更新技能
#[sqlx::test]
async fn test_update(pool: SqlitePool) -> Result<(), AppError> {
    skill::init();
    let skill_dao = skill::dao();

    let skill_id = Uuid::now_v7().to_string();
    let mut skill = SkillPo::new(
        skill_id.clone(),
        "Test Update".to_string(),
        "Original description".to_string(),
        vec!["test".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    skill.description = "Updated description".to_string();
    skill.status = SkillStatus::Published;

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.update(ctx, &skill).await?;

    let ctx = new_ctx("test-user", pool);
    let found = skill_dao.find_by_id(ctx, &skill_id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.description, "Updated description");
    assert_eq!(found.status, SkillStatus::Published);

    Ok(())
}

/// 测试按状态列表查询
#[sqlx::test]
async fn test_list_by_status(pool: SqlitePool) -> Result<(), AppError> {
    skill::init();
    let skill_dao = skill::dao();

    let skill1_id = Uuid::now_v7().to_string();
    let mut skill1 = SkillPo::new(
        skill1_id.clone(),
        "List Test 1".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill1_id}"),
    );
    skill1.status = SkillStatus::Published;

    let skill2_id = Uuid::now_v7().to_string();
    let skill2 = SkillPo::new(
        skill2_id.clone(),
        "List Test 2".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill2_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx.clone(), &skill1).await?;
    skill_dao.insert(ctx, &skill2).await?;

    let ctx = new_ctx("test-user", pool.clone());
    let available = skill_dao.list_by_status(ctx, SkillStatus::Published).await?;
    assert!(available.iter().any(|s| s.id == skill1_id));
    assert!(!available.iter().any(|s| s.id == skill2_id));

    let ctx = new_ctx("test-user", pool);
    let pending = skill_dao.list_by_status(ctx, SkillStatus::Draft).await?;
    assert!(pending.iter().any(|s| s.id == skill2_id));

    Ok(())
}

/// 测试按分类列表查询
#[sqlx::test]
async fn test_list_by_category(pool: SqlitePool) -> Result<(), AppError> {
    skill::init();
    let skill_dao = skill::dao();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Category Test".to_string(),
        "".to_string(),
        vec![],
        "documentation".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    let ctx = new_ctx("test-user", pool);
    let result = skill_dao.list_by_category(ctx, "documentation").await?;
    assert!(result.iter().any(|s| s.id == skill_id));

    Ok(())
}

/// 测试关键词搜索
#[sqlx::test]
async fn test_search(pool: SqlitePool) -> Result<(), AppError> {
    skill::init();
    let skill_dao = skill::dao();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Search Test Skill".to_string(),
        "This is a skill for searching".to_string(),
        vec!["search".to_string()],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    let ctx = new_ctx("test-user", pool.clone());
    let result = skill_dao.search(ctx, "Search").await?;
    assert!(result.iter().any(|s| s.id == skill_id));

    let ctx = new_ctx("test-user", pool);
    let result = skill_dao.search(ctx, "searching").await?;
    assert!(result.iter().any(|s| s.id == skill_id));

    Ok(())
}

/// 测试软删除（标记为过期）
#[sqlx::test]
async fn test_delete_by_id(pool: SqlitePool) -> Result<(), AppError> {
    skill::init();
    let skill_dao = skill::dao();

    let skill_id = Uuid::now_v7().to_string();
    let skill = SkillPo::new(
        skill_id.clone(),
        "Delete Test".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "test-user".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &skill).await?;

    let ctx = new_ctx("test-user", pool.clone());
    let found_before = skill_dao.find_by_id(ctx, &skill_id).await?;
    assert!(found_before.is_some());
    assert_eq!(found_before.unwrap().status, SkillStatus::Draft);

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.delete_by_id(ctx, &skill_id).await?;

    let ctx = new_ctx("test-user", pool);
    let found_after = skill_dao.find_by_id(ctx, &skill_id).await?;
    assert!(found_after.is_some());
    assert_eq!(found_after.unwrap().status, SkillStatus::Expired);

    Ok(())
}

/// 测试按作者列表查询
#[sqlx::test]
async fn test_list_by_author(pool: SqlitePool) -> Result<(), AppError> {
    skill::init();
    let skill_dao = skill::dao();

    let skill1_id = Uuid::now_v7().to_string();
    let skill1 = SkillPo::new(
        skill1_id.clone(),
        "Author Test 1".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "alice".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill1_id}"),
    );

    let skill2_id = Uuid::now_v7().to_string();
    let skill2 = SkillPo::new(
        skill2_id.clone(),
        "Author Test 2".to_string(),
        "".to_string(),
        vec![],
        "testing".to_string(),
        "".to_string(),
        "bob".to_string(),
        SkillAuthorType::User,
        format!("skills/pending/{skill2_id}"),
    );

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx.clone(), &skill1).await?;
    skill_dao.insert(ctx, &skill2).await?;

    let ctx = new_ctx("test-user", pool);
    let alice_skills = skill_dao.list_by_author(ctx, "alice").await?;
    assert!(alice_skills.iter().any(|s| s.id == skill1_id));
    assert!(!alice_skills.iter().any(|s| s.id == skill2_id));

    Ok(())
}

/// 测试安装共享技能到 Agent（install_to_agent）
#[sqlx::test]
async fn test_install_to_agent(pool: SqlitePool) -> Result<(), AppError> {
    skill::init();
    let skill_dao = skill::dao();

    // 1. 创建一个已发布的共享技能（源技能）
    let source_id = Uuid::now_v7().to_string();
    let mut source_skill = SkillPo::new(
        source_id.clone(),
        "Shared Skill".to_string(),
        "A shared published skill that can be installed to agents".to_string(),
        vec!["shared".to_string(), "utility".to_string()],
        "tools".to_string(),
        "".to_string(), // parent_skill_id is empty for original shared skill
        "system".to_string(), // author is system (shared library)
        SkillAuthorType::User,
        format!("shared/{}", source_id),
    );
    // Publish it
    source_skill.status = SkillStatus::Published;

    let ctx = new_ctx("system", pool.clone());
    skill_dao.insert(ctx, &source_skill).await?;

    // 2. Install to agent "agent-123"
    let ctx = new_ctx("admin", pool.clone());
    let installed = skill_dao.install_to_agent(ctx, &source_skill, "agent-123").await?;

    // 3. Verify the installed copy
    // - Should have new generated id
    assert!(!installed.id.is_empty());
    assert_ne!(installed.id, source_id);

    // - Should copy all metadata
    assert_eq!(installed.name, source_skill.name);
    assert_eq!(installed.description, source_skill.description);
    assert_eq!(installed.parse_tags(), source_skill.parse_tags());
    assert_eq!(installed.category, source_skill.category);

    // - Should have correct attributes
    assert_eq!(installed.parent_skill_id, source_id.clone());
    assert_eq!(installed.author_id, "agent-123");
    assert_eq!(installed.status, SkillStatus::Draft); // default is Draft
    assert_eq!(installed.content_path, format!("agents/agent-123/skills/{}", installed.id));

    // 4. Verify it exists in database
    let ctx = new_ctx("test-user", pool);
    let found = skill_dao.find_by_id(ctx, &installed.id).await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.id, installed.id);
    assert_eq!(found.parent_skill_id, source_id);

    Ok(())
}

/// 测试安装非已发布技能应该返回错误
#[sqlx::test]
async fn test_install_non_published_fails(pool: SqlitePool) -> Result<(), AppError> {
    skill::init();
    let skill_dao = skill::dao();

    // Create a draft skill (not published)
    let source_id = Uuid::now_v7().to_string();
    let source_skill = SkillPo::new(
        source_id.clone(),
        "Draft Skill".to_string(),
        "This is still a draft".to_string(),
        vec![],
        "test".to_string(),
        "".to_string(),
        "author".to_string(),
        SkillAuthorType::User,
        format!("skills/{}", source_id),
    );
    // It's Draft by default, not Published

    let ctx = new_ctx("test-user", pool.clone());
    skill_dao.insert(ctx, &source_skill).await?;

    // Try to install - should fail
    let ctx = new_ctx("test-user", pool);
    let result = skill_dao.install_to_agent(ctx, &source_skill, "agent-123").await;

    // Should be error
    assert!(result.is_err());
    let err = result.err().unwrap();
    // Error should mention that only published can be installed
    let err_msg = err.to_string();
    assert!(err_msg.contains("Only published skills can be installed"));

    Ok(())
}
