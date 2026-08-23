//! HR Domain Skill 管理单元测试

use super::{CreateSkillParams, HrDomain, SkillFileImport, UpdateSkillParams};
use crate::models::skill::{Skill, SkillPo};
use crate::pkg::RequestContext;
use common::enums::SkillStatus;
use common::enums::skill::SkillAuthorType;
use sqlx::SqlitePool;
use tempfile::TempDir;

fn new_ctx(user_id: &str, pool: sqlx::SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
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
    crate::service::dao::skill::init_vector();
    crate::service::dao::tool_call::init();
    crate::service::dao::model_provider::init();
    crate::service::dao::cortex::init();

    // 初始化所有 DAL
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
    crate::service::dal::model_provider::init();
    let skill_dal = crate::service::dal::skill::new(
        crate::service::dao::skill::new_skill_dao_with_base_path(base_path),
        crate::service::dao::skill::vector_dao(),
        crate::service::dao::cortex::dao(),
        crate::service::dao::model_provider::dao(),
    );

    let domain = super::new(
        crate::service::dal::agent::dal(),
        crate::service::dal::tool::dal(),
        skill_dal,
    );
    let ctx = new_ctx("admin", pool);
    (domain, ctx, temp_dir)
}

/// 创建测试 Skill
fn create_test_skill(name: &str) -> Skill {
    let skill_po = SkillPo::new(
        format!(
            "{}--{}",
            name.to_lowercase().replace(" ", "-"),
            uuid::Uuid::new_v4()
        ),
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
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    let found: Option<Skill> = domain
        .skill_manage()
        .get_skill(ctx, skill.id())
        .await
        .unwrap();
    assert_eq!(found.unwrap().name(), "TestSkill");
}

#[sqlx::test]
async fn test_update_skill(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool.clone());

    let skill = create_test_skill("Original");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    let mut updated = skill.clone();
    updated.po.name = "Updated".to_string();
    let params = UpdateSkillParams {
        skill: &updated,
        imports: Vec::new(),
        file_deletes: Vec::new(),
        remote_source: None,
    };

    domain
        .skill_manage()
        .update_skill(new_ctx("editor", pool), params)
        .await
        .unwrap();

    let found: Option<Skill> = domain
        .skill_manage()
        .get_skill(ctx, updated.id())
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
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    domain
        .skill_manage()
        .delete_skill(ctx.clone(), skill.id())
        .await
        .unwrap();
    let found: Option<Skill> = domain
        .skill_manage()
        .get_skill(ctx, skill.id())
        .await
        .unwrap();
    // 软删除，记录还在，状态变为 Expired
    assert!(found.is_some());
    assert_eq!(
        found.unwrap().po.status,
        common::enums::SkillStatus::Expired
    );
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
            .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
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
            .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
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
            .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
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
            .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
            .await
            .unwrap();
    }

    let query = crate::service::dao::skill::SkillQuery {
        status: Some(SkillStatus::Draft),
        category: Some("coding".to_string()),
        author_id: None,
        parent_skill_id: None,
        keyword: None,
        tags: None,
        ids: None,
        exclude_status: None,
        has_parent: None,
        pagination: Default::default(),
    };

    let skills = domain
        .skill_manage()
        .query_skills(ctx, query)
        .await
        .unwrap();
    assert_eq!(skills.items.len(), 1);
}

#[sqlx::test]
async fn test_update_skill_imports_attachment_file_content(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool.clone());

    let skill = create_test_skill("ImportAttachment");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    let params = UpdateSkillParams {
        skill: &skill,
        imports: vec![SkillFileImport {
            target_path: Some("references/guide.md".to_string()),
            source_abs_path: None,
            content_bytes: Some(b"# Guide".to_vec()),
            suggested_name: None,
        }],
        file_deletes: Vec::new(),
        remote_source: None,
    };

    domain
        .skill_manage()
        .update_skill(new_ctx("editor", pool), params)
        .await
        .unwrap();

    let found = domain
        .skill_manage()
        .get_skill(ctx, skill.id())
        .await
        .unwrap()
        .unwrap();
    let imported = found
        .files
        .iter()
        .find(|file| file.filename == "references/guide.md")
        .expect("imported file should be listed");
    assert_eq!(imported.content.as_deref(), Some("# Guide"));
}

#[sqlx::test]
async fn test_update_skill_rejects_unsafe_import_target_path(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool.clone());

    let skill = create_test_skill("UnsafeImport");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    for target_path in [
        "../escape.md",
        "/tmp/escape.md",
        "./guide.md",
        "references/",
        "references\\guide.md",
    ] {
        let params = UpdateSkillParams {
            skill: &skill,
            imports: vec![SkillFileImport {
                target_path: Some(target_path.to_string()),
                source_abs_path: None,
                content_bytes: Some(b"bad".to_vec()),
                suggested_name: None,
            }],
            file_deletes: Vec::new(),
            remote_source: None,
        };

        let result = domain
            .skill_manage()
            .update_skill(new_ctx("editor", pool.clone()), params)
            .await;

        assert!(result.is_err(), "{target_path} should be rejected");
    }
}
