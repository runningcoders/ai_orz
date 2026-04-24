//! Skill DAO SQLite 单元测试

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
