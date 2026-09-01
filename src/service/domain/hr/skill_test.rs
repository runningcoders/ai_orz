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

    // 以 Admin 身份执行（update_skill 新增了资源级权限校验：作者 / Agent 创建者 / 管理员任一放行）
    let ctx_editor_admin = ctx
        .to_builder()
        .user_id("editor".to_string())
        .user_role(1) // Admin
        .build();
    domain
        .skill_manage()
        .update_skill(ctx_editor_admin, params)
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

    // 以 Admin 身份执行（update_skill 新增了资源级权限校验）
    let ctx_editor_admin = ctx
        .to_builder()
        .user_id("editor".to_string())
        .user_role(1) // Admin
        .build();
    domain
        .skill_manage()
        .update_skill(ctx_editor_admin, params)
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

        // 以 Admin 身份执行，保证错误来自路径校验而非新增的资源级权限拦截
        let ctx_editor_admin = ctx
            .to_builder()
            .user_id("editor".to_string())
            .user_role(1) // Admin
            .build();
        let result = domain
            .skill_manage()
            .update_skill(ctx_editor_admin, params)
            .await;

        assert!(result.is_err(), "{target_path} should be rejected");
    }
}

/// Task 6 — Agent 创建者应自动拥有其名下 Agent 的技能访问权限。
///
/// 覆盖矩阵：
/// A. alice（Agent 创建者，Member）list_files ✅
/// B. alice 读 skill.md ✅
/// C. bob（路人，Member）list_files ❌ → 403 Forbidden
/// D. admin（管理员）list_files ✅（admin bypass）
#[sqlx::test]
async fn test_skill_access_allows_agent_creator(pool: SqlitePool) -> common::error::Result<()> {
    use common::constants::utils::current_timestamp_ms;
    use common::enums::{AgentKind, AgentStatus};
    use std::io::Write;

    let (domain, ctx_admin, _temp_dir) = init_test_env(pool.clone());

    // ====== 1. 给 ctx_admin 显式补上 SuperAdmin 角色（new_test_ctx 不自动赋 role）
    //         并派生 alice / bob 两个 Member 身份（继承同一 storage 池）======
    let ctx_admin = ctx_admin
        .to_builder()
        .user_role(0) // SuperAdmin
        .build();
    let ctx_alice = ctx_admin
        .to_builder()
        .user_id("user_alice".to_string())
        .user_role(2) // Member
        .build();
    let ctx_bob = ctx_admin
        .to_builder()
        .user_id("user_bob".to_string())
        .user_role(2) // Member
        .build();

    // ====== 2. 插入 Agent A（创建者 = alice）======
    let agent_id = "agent_alice_owned";
    let now_ms = current_timestamp_ms();
    let insert_agent_sql = r#"
        INSERT INTO agents
            (id, name, role, description, soul, capabilities, runtime_config,
             model_provider_id, status, kind, created_by, modified_by, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
    "#;
    sqlx::query(insert_agent_sql)
        .bind(agent_id)
        .bind("Alice Agent")
        .bind("[]")
        .bind("Agent owned by alice")
        .bind("")
        .bind("[]")
        .bind("{}")
        .bind("prov_placeholder")
        .bind(AgentStatus::Onboarded.to_i32())
        .bind(AgentKind::Local.to_i32())
        .bind("user_alice") // ← 关键：创建者 = alice
        .bind("admin")
        .bind(now_ms)
        .bind(now_ms)
        .execute(&pool)
        .await
        .unwrap();

    // ====== 3. 插入 Agent 副本 Skill（author = Agent A，author_type = Agent）======
    //     通过 raw SQL 直接写 skills 表（绕过全局 DAO 单例 + 强制写指定 author）
    let copy_id = "agent_copy_owned_by_alice_agent";
    let copy_content_path = format!("agents/{agent_id}/skills/{copy_id}");
    let now_ms = current_timestamp_ms();
    let insert_skill_sql = r#"
        INSERT INTO skills
            (id, name, description, tags, category, parent_skill_id,
             author_id, author_type, modifier_id, status, created_at, updated_at, content_path)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
    "#;
    sqlx::query(insert_skill_sql)
        .bind(copy_id)
        .bind("Agent Copy Skill")
        .bind("Agent owned skill copy")
        .bind("[]")
        .bind("")
        .bind("src_parent_placeholder")
        .bind(agent_id) // author_id = Agent A 的 ID
        .bind(SkillAuthorType::Agent.to_i32())
        .bind(agent_id) // modifier_id
        .bind(SkillStatus::Draft.to_i32())
        .bind(now_ms)
        .bind(now_ms)
        .bind(&copy_content_path)
        .execute(&pool)
        .await
        .unwrap();

    // ====== 4. 给副本写主文件 skill.md（list_files / read_file 依赖真实 FS 文件）======
    let base = crate::config::get().base_data_path();
    let copy_dir = crate::pkg::paths::agent_skill_dir(&base, agent_id, copy_id);
    std::fs::create_dir_all(&copy_dir).unwrap();
    let main = copy_dir.join("skill.md");
    std::fs::File::create(&main)
        .unwrap()
        .write_all(b"# agent skill copy content\n")
        .unwrap();

    let files_alice = domain
        .skill_manage()
        .list_skill_files(ctx_alice.clone(), copy_id)
        .await?
        .expect("alice 是 Agent 创建者，应能 list 到文件列表");
    assert!(
        !files_alice.is_empty(),
        "至少有 skill.md 主文件，实际数量: {}",
        files_alice.len()
    );

    // ====== 断言 B：alice 能读 skill.md 内容 ======
    let content_alice = domain
        .skill_manage()
        .get_skill_file_content(ctx_alice.clone(), copy_id, "skill.md")
        .await?
        .expect("alice 应能读到 skill.md 内容");
    assert!(content_alice.contains("agent skill copy"));

    // ====== 断言 C：bob（路人）list_files → 权限错误 ======
    let err_bob = domain
        .skill_manage()
        .list_skill_files(ctx_bob.clone(), copy_id)
        .await;
    assert!(
        err_bob.is_err(),
        "bob 非创建者/管理员，list_files 应报错，实际: {:?}",
        err_bob
    );
    let msg_bob = format!("{:?}", err_bob.unwrap_err());
    assert!(
        msg_bob.contains("权限")
            || msg_bob.contains("Forbidden")
            || msg_bob.contains("forbidden")
            || msg_bob.contains("无权"),
        "错误信息应说明是权限问题，实际: {}",
        msg_bob
    );

    // ====== 断言 D：admin（管理员）能 list_files（角色 bypass）======
    let files_admin = domain
        .skill_manage()
        .list_skill_files(ctx_admin.clone(), copy_id)
        .await?
        .expect("admin 角色应能访问任意技能文件");
    assert!(!files_admin.is_empty());

    Ok(())
}
