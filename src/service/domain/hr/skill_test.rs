//! HR Domain Skill 管理单元测试

use super::{HrDomain, domain, UpdateSkillParams};
use crate::models::skill::{Skill, SkillPo};
use crate::pkg::RequestContext;
use common::enums::SkillStatus;
use common::enums::skill::SkillAuthorType;
use sqlx::SqlitePool;
use tempfile::TempDir;

fn new_ctx(user_id: &str, pool: sqlx::SqlitePool) -> RequestContext {
    RequestContext::new_simple(user_id, pool)
}

/// 初始化 HR Domain 所有依赖
/// 初始化顺序：config -> dao -> dal -> domain
fn init_test_env(pool: SqlitePool) -> (std::sync::Arc<dyn HrDomain>, RequestContext, TempDir) {
    // 初始化 config 使用临时目录
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_path_buf();
    
    // 设置测试用的配置
    unsafe {
        std::env::set_var("AI_ORZ_BASE_PATH", base_path.to_str().unwrap());
    }
    
    // 初始化 config
    crate::config::init().unwrap();

    // 初始化所有 DAO
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
    crate::service::dao::skill::init();
    crate::service::dao::tool_call::init();
    crate::service::dao::model_provider::init();
    crate::service::dao::cortex::init();

    // 初始化所有 DAL
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
    crate::service::dal::skill::init();
    crate::service::dal::model_provider::init();

    // 初始化 HR Domain
    super::init();

    let domain = domain();
    let ctx = new_ctx("admin", pool);
    (domain, ctx, temp_dir)
}

/// 创建测试 Skill
fn create_test_skill(name: &str) -> Skill {
    let skill_po = SkillPo::new(
        format!("{}--{}", name.to_lowercase().replace(" ", "-"), uuid::Uuid::new_v4()),
        name.to_string(),
        "A test skill".to_string(),
        vec!["test".to_string()],
        "coding".to_string(),
        String::new(),
        "admin".to_string(),
        SkillAuthorType::User,
        format!("skills/{}", name.to_lowercase().replace(" ", "-")),
    );
    Skill::from_po(skill_po)
}

#[sqlx::test]
async fn test_create_and_get_by_id(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool);

    let skill = create_test_skill("TestSkill");

    domain
        .skill_manage()
        .create_skill(ctx.clone(), &skill)
        .await
        .unwrap();

    let found: Option<Skill> = domain
        .skill_manage()
        .get_skill(ctx, &skill.id())
        .await
        .unwrap();
    assert_eq!(found.unwrap().name(), "TestSkill");
}

#[sqlx::test]
async fn test_get_skill_po_only(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool);

    let skill = create_test_skill("TestPoSkill");

    domain
        .skill_manage()
        .create_skill(ctx.clone(), &skill)
        .await
        .unwrap();

    let found_po: Option<SkillPo> = domain
        .skill_manage()
        .get_skill_po(ctx, &skill.id())
        .await
        .unwrap();
    assert_eq!(found_po.unwrap().name, "TestPoSkill");
}

#[sqlx::test]
async fn test_update_skill(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool.clone());

    let skill = create_test_skill("Original");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), &skill)
        .await
        .unwrap();

    let mut updated = skill.clone();
    updated.po.name = "Updated".to_string();
    let params = UpdateSkillParams {
        skill: &updated,
        file_writes: Vec::new(),
        file_deletes: Vec::new(),
    };

    domain
        .skill_manage()
        .update_skill(new_ctx("editor", pool), params)
        .await
        .unwrap();

    let found: Option<Skill> = domain
        .skill_manage()
        .get_skill(ctx, &updated.id())
        .await
        .unwrap();
    assert_eq!(found.unwrap().name(), "Updated");
}

#[sqlx::test]
async fn test_delete_skill(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool.clone());

    let skill = create_test_skill("ToDelete");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), &skill)
        .await
        .unwrap();

    domain
        .skill_manage()
        .delete_skill(ctx.clone(), &skill.id())
        .await
        .unwrap();
    let found: Option<Skill> = domain
        .skill_manage()
        .get_skill(ctx, &skill.id())
        .await
        .unwrap();
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_list_by_status(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool.clone());

    for i in 0..3 {
        let mut skill = create_test_skill(&format!("Skill{}", i));
        skill.po.status = if i == 0 {
            SkillStatus::Published
        } else {
            SkillStatus::Draft
        };
        domain
            .skill_manage()
            .create_skill(ctx.clone(), &skill)
            .await
            .unwrap();
    }

    let published: Vec<Skill> = domain
        .skill_manage()
        .list_by_status(ctx, SkillStatus::Published)
        .await
        .unwrap();
    assert_eq!(published.len(), 1);
}

#[sqlx::test]
async fn test_list_by_category(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool.clone());

    for i in 0..3 {
        let mut skill = create_test_skill(&format!("Skill{}", i));
        skill.po.category = if i < 2 {
            "coding".to_string()
        } else {
            "writing".to_string()
        };
        domain
            .skill_manage()
            .create_skill(ctx.clone(), &skill)
            .await
            .unwrap();
    }

    let coding_skills: Vec<Skill> = domain
        .skill_manage()
        .list_by_category(ctx, "coding")
        .await
        .unwrap();
    assert_eq!(coding_skills.len(), 2);
}

#[sqlx::test]
async fn test_list_by_author(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool.clone());

    for i in 0..3 {
        let mut skill = create_test_skill(&format!("Skill{}", i));
        skill.po.author_id = if i < 2 {
            "admin".to_string()
        } else {
            "user".to_string()
        };
        domain
            .skill_manage()
            .create_skill(ctx.clone(), &skill)
            .await
            .unwrap();
    }

    let admin_skills: Vec<Skill> = domain
        .skill_manage()
        .list_by_author(ctx, "admin")
        .await
        .unwrap();
    assert_eq!(admin_skills.len(), 2);
}

#[sqlx::test]
async fn test_query_skills(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool.clone());

    for i in 0..3 {
        let mut skill = create_test_skill(&format!("Skill {}", i));
        skill.po.status = if i == 0 {
            SkillStatus::Published
        } else {
            SkillStatus::Draft
        };
        skill.po.category = if i < 2 {
            "coding".to_string()
        } else {
            "writing".to_string()
        };
        domain
            .skill_manage()
            .create_skill(ctx.clone(), &skill)
            .await
            .unwrap();
    }

    let query = crate::service::dao::skill::SkillQuery {
        status: Some(SkillStatus::Draft),
        category: Some("coding".to_string()),
        author_id: None,
        keyword: None,
        ids: None,
        exclude_status: None,
        limit: None,
    };

    let skills: Vec<Skill> = domain
        .skill_manage()
        .query_skills(ctx, query)
        .await
        .unwrap();
    assert_eq!(skills.len(), 1);
}
