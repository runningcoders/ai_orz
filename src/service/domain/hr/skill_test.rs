//! HR Domain Skill 管理单元测试

use super::{CreateSkillParams, HrDomain, SkillFileImport, UpdateSkillParams};
use crate::models::skill::{Skill, SkillPo};
use crate::pkg::RequestContext;
use crate::service::dao::skill::SkillQuery;
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

    // 初始化所有 DAO（memory 最先：本体 DAL 组合依赖 MemoryDao 单例）
    crate::service::dao::memory::init();
    crate::service::dao::ontology::init();
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
    crate::service::dao::skill::init_vector();
    crate::service::dao::tool_call::init();
    crate::service::dao::model_provider::init();
    crate::service::dao::cortex::init();
    // 组织（HR 入职要读组织级配置 OrganizationConfig.agent_onboard）
    crate::service::dao::organization::init();
    crate::service::dao::organization_link::init();
    crate::service::dao::organization_pairing::init();
    crate::service::dao::federation_contract::init();

    // 初始化所有 DAL
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
    crate::service::dal::model_provider::init();
    crate::service::dal::organization::init();
    crate::service::dal::ontology::init();
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
        std::sync::Arc::new(crate::service::dal::agent::AgentRuntimeDalImpl),
        crate::service::dal::organization::dal(),
        crate::service::dal::ontology::dal(),
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
async fn test_query_skills_agent_visibility(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool.clone());

    // 四类数据：自己的 Agent 副本 / 别人的 Agent 副本 / 全局 Published / 用户私有 Draft
    let mut own_copy = create_test_skill("OwnCopy");
    own_copy.po.author_id = "agent-a".to_string();
    own_copy.po.author_type = SkillAuthorType::Agent;
    own_copy.po.status = SkillStatus::Draft;

    let mut other_copy = create_test_skill("OtherCopy");
    other_copy.po.author_id = "agent-b".to_string();
    other_copy.po.author_type = SkillAuthorType::Agent;
    other_copy.po.status = SkillStatus::Draft;

    let mut published = create_test_skill("GlobalPublished");
    published.po.status = SkillStatus::Published;

    let mut user_draft = create_test_skill("UserDraft");
    user_draft.po.status = SkillStatus::Draft;

    for skill in [&own_copy, &other_copy, &published, &user_draft] {
        domain
            .skill_manage()
            .create_skill(ctx.clone(), CreateSkillParams::from_skill(skill))
            .await
            .unwrap();
    }

    // Agent 可见性（handler 从 ctx.agent_id() 解析后显式传入）：
    // 只能看到自己的副本 + 全局 Published，看不到别人的副本和用户草稿
    let page = domain
        .skill_manage()
        .query_skills(
            ctx.clone(),
            SkillQuery {
                visible_to_agent_id: Some("agent-a".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let mut names: Vec<String> = page.items.iter().map(|s| s.name().to_string()).collect();
    names.sort();
    assert_eq!(
        names,
        vec!["GlobalPublished".to_string(), "OwnCopy".to_string()]
    );

    // 不带可见性参数（user 场景）：不受收紧影响（管理页仍可见全部）
    let page = domain
        .skill_manage()
        .query_skills(ctx, SkillQuery::default())
        .await
        .unwrap();
    assert_eq!(page.items.len(), 4);
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
        author_type: None,
        parent_skill_id: None,
        keyword: None,
        tags: None,
        ids: None,
        exclude_status: None,
        has_parent: None,
        visible_to_agent_id: None,
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
    // 注意：必须用本测试自己的 temp_dir，而非全局 config::get()——init_test_env 会
    // 重置进程级全局 config（set_var + config::init()），并发跑模块时它可能指向
    // 其他测试的 TempDir（已被 drop 删除），导致写入丢失、list 为空（测试隔离陷阱）
    let base = _temp_dir.path().to_path_buf();
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

/// Agent 上下文权限收紧回归测试（ensure_skill_access ⓪ 分支）：
/// - 写操作（update）：仅限 author_id == 自己的记录，Admin bypass / 创建者身份均不适用
/// - 读操作（list_files / get_file_content）：自己的记录 + Published 共享技能放行
/// - 关闭越权面：同创建者兄弟 Agent 副本、宿主用户的 Draft 源技能
#[sqlx::test]
async fn test_skill_access_agent_context_restriction(
    pool: SqlitePool,
) -> common::error::Result<()> {
    use common::constants::utils::current_timestamp_ms;
    use common::enums::{AgentKind, AgentStatus};

    let (domain, ctx_admin, _temp_dir) = init_test_env(pool.clone());

    // ====== 1. 派生身份：alice / bob 两个 Member + 各自的 Agent + Agent 上下文 ctx ======
    let ctx_alice = ctx_admin
        .to_builder()
        .user_id("user_alice".to_string())
        .user_role(2) // Member
        .build();
    let ctx_agent_alice = ctx_alice.to_builder().agent_id("agent_alice_owned").build();

    let now_ms = current_timestamp_ms();
    let insert_agent_sql = r#"
        INSERT INTO agents
            (id, name, role, description, soul, capabilities, runtime_config,
             model_provider_id, status, kind, created_by, modified_by, created_at, updated_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
    "#;
    for (agent_id, created_by) in [
        ("agent_alice_owned", "user_alice"),
        ("agent_bob_owned", "user_bob"),
    ] {
        sqlx::query(insert_agent_sql)
            .bind(agent_id)
            .bind(agent_id)
            .bind("[]")
            .bind("")
            .bind("")
            .bind("[]")
            .bind("{}")
            .bind("prov_placeholder")
            .bind(AgentStatus::Onboarded.to_i32())
            .bind(AgentKind::Local.to_i32())
            .bind(created_by)
            .bind("admin")
            .bind(now_ms)
            .bind(now_ms)
            .execute(&pool)
            .await
            .unwrap();
    }

    // ====== 2. 插入技能：alice 的 Agent 副本 / bob 的 Agent 副本 / alice 的 Draft 源 / bob 的 Published 源 ======
    // 用嵌套 async fn（生命周期显式统一）而非 async 闭包：闭包返回的 future 会携带
    // 参数引用与局部变量借用的推导冲突，async fn 内直接 await 则无此问题。
    async fn mk_skill(
        pool: &SqlitePool,
        now_ms: i64,
        id: &str,
        author_id: &str,
        author_type: i32,
        status: i32,
        parent: &str,
    ) -> Result<(), sqlx::Error> {
        let insert_skill_sql = r#"
        INSERT INTO skills
            (id, name, description, tags, category, parent_skill_id,
             author_id, author_type, modifier_id, status, created_at, updated_at, content_path)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
    "#;
        let content_path = if author_type == SkillAuthorType::Agent.to_i32() {
            format!("agents/{author_id}/skills/{id}")
        } else {
            format!("skills/{id}")
        };
        sqlx::query(insert_skill_sql)
            .bind(id)
            .bind(id)
            .bind("")
            .bind("[]")
            .bind("")
            .bind(parent)
            .bind(author_id)
            .bind(author_type)
            .bind(author_id)
            .bind(status)
            .bind(now_ms)
            .bind(now_ms)
            .bind(&content_path)
            .execute(pool)
            .await
            .map(|_| ())
    }
    mk_skill(
        &pool,
        now_ms,
        "alice_agent_copy",
        "agent_alice_owned",
        SkillAuthorType::Agent.to_i32(),
        SkillStatus::Draft.to_i32(),
        "src_placeholder",
    )
    .await
    .unwrap();
    mk_skill(
        &pool,
        now_ms,
        "bob_agent_copy",
        "agent_bob_owned",
        SkillAuthorType::Agent.to_i32(),
        SkillStatus::Draft.to_i32(),
        "src_placeholder",
    )
    .await
    .unwrap();
    mk_skill(
        &pool,
        now_ms,
        "alice_draft_source",
        "user_alice",
        SkillAuthorType::User.to_i32(),
        SkillStatus::Draft.to_i32(),
        "",
    )
    .await
    .unwrap();
    mk_skill(
        &pool,
        now_ms,
        "bob_published_skill",
        "user_bob",
        SkillAuthorType::User.to_i32(),
        SkillStatus::Published.to_i32(),
        "",
    )
    .await
    .unwrap();

    // ====== 3. 写操作：只能动自己的记录 ======
    // 3a. alice 的 Agent 更新自己的副本 → 放行
    let mut own = domain
        .skill_manage()
        .get_skill(ctx_agent_alice.clone(), "alice_agent_copy")
        .await?
        .expect("自己的副本应存在");
    own.po.description = "evolved by agent".to_string();
    domain
        .skill_manage()
        .update_skill(
            ctx_agent_alice.clone(),
            UpdateSkillParams {
                skill: &own,
                imports: vec![],
                file_deletes: vec![],
                remote_source: None,
            },
        )
        .await
        .expect("Agent 更新自己的技能副本应放行");

    // 3b. 借宿主身份改写宿主用户的 Draft 源技能 → 拒绝（原逻辑条件②会放行）
    let mut host_source = domain
        .skill_manage()
        .get_skill(ctx_agent_alice.clone(), "alice_draft_source")
        .await?
        .expect("宿主用户的源技能应存在");
    host_source.po.description = "hijacked".to_string();
    let err = domain
        .skill_manage()
        .update_skill(
            ctx_agent_alice.clone(),
            UpdateSkillParams {
                skill: &host_source,
                imports: vec![],
                file_deletes: vec![],
                remote_source: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("Forbidden"),
        "Agent 改写宿主用户技能应 Forbidden，实际: {err:?}"
    );

    // 3c. 改写同创建者兄弟 Agent 的副本 → 拒绝（原逻辑条件③会放行）
    let mut sibling = domain
        .skill_manage()
        .get_skill(ctx_agent_alice.clone(), "bob_agent_copy")
        .await?
        .expect("兄弟 Agent 副本应存在");
    sibling.po.description = "hijacked".to_string();
    let err = domain
        .skill_manage()
        .update_skill(
            ctx_agent_alice.clone(),
            UpdateSkillParams {
                skill: &sibling,
                imports: vec![],
                file_deletes: vec![],
                remote_source: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("Forbidden"),
        "Agent 改写兄弟 Agent 副本应 Forbidden，实际: {err:?}"
    );

    // 3d. 即使宿主是 SuperAdmin，Agent 上下文也不享受 Admin bypass → 拒绝
    let ctx_agent_admin_host = ctx_admin
        .to_builder()
        .user_role(0) // SuperAdmin
        .agent_id("agent_alice_owned")
        .build();
    host_source.po.description = "hijacked by admin host agent".to_string();
    let err = domain
        .skill_manage()
        .update_skill(
            ctx_agent_admin_host,
            UpdateSkillParams {
                skill: &host_source,
                imports: vec![],
                file_deletes: vec![],
                remote_source: None,
            },
        )
        .await
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("Forbidden"),
        "Admin 宿主的 Agent 上下文也不应绕过自身记录限定，实际: {err:?}"
    );

    // ====== 4. 读操作：自己的记录 + Published 共享技能放行，他人 Draft 拒绝 ======
    // 4a. 读 Published 共享技能（作者是 bob，与 alice 的 Agent 无关）→ 放行（隐藏技能按需读）
    let published = domain
        .skill_manage()
        .list_skill_files(ctx_agent_alice.clone(), "bob_published_skill")
        .await;
    assert!(
        published.is_ok(),
        "Agent 读 Published 共享技能应放行（search_skill 暴露面一致），实际: {:?}",
        published.err()
    );

    // 4b. 读宿主用户的 Draft 源技能 → 拒绝
    let err = domain
        .skill_manage()
        .list_skill_files(ctx_agent_alice.clone(), "alice_draft_source")
        .await
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("Forbidden"),
        "Agent 读宿主用户 Draft 技能应 Forbidden，实际: {err:?}"
    );

    // 4c. 用户上下文（无 agent_id）不受收紧影响：alice 仍能读自己的 Draft 源
    let own_source_files = domain
        .skill_manage()
        .list_skill_files(ctx_alice.clone(), "alice_draft_source")
        .await;
    assert!(
        own_source_files.is_ok(),
        "用户上下文读自己的技能不应受 Agent 收紧影响，实际: {:?}",
        own_source_files.err()
    );

    Ok(())
}

// ==================== N2 发布回写自身技能用例 ====================

/// 构造 Agent 自有根技能（Draft，author=指定 Agent，parent 为空）
fn create_agent_root_skill(agent_id: &str, name: &str) -> Skill {
    let slug = name.to_lowercase().replace(" ", "-");
    let skill_po = SkillPo::new(
        format!("{}--{}", slug, uuid::Uuid::new_v4()),
        name.to_string(),
        "Agent owned draft skill".to_string(),
        vec!["search".to_string()],
        "testing".to_string(),
        String::new(), // 根技能：parent 为空
        agent_id.to_string(),
        SkillAuthorType::Agent,
        format!("agents/{}/skills/{}", agent_id, slug),
    );
    Skill::from_po(skill_po)
}

/// N1 验收要点：Agent 上下文创建草稿 → 用户上下文发布 →
/// 该 Agent 名下恰好 1 条 Draft 副本（修复 A 谓词 parent 非空放行形态），
/// 且副本携带源文件、源技能保持 Published。
#[sqlx::test]
async fn test_publish_agent_root_skill_creates_writeback_copy(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool);

    let skill = create_agent_root_skill("agent-1", "DoubaoSearch");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    // 用户上下文发布：Draft → Published（Admin bypass），附带 skill.md 内容
    let mut published = skill.clone();
    published.po.status = SkillStatus::Published;
    let imports = vec![SkillFileImport {
        target_path: Some("skill.md".to_string()),
        source_abs_path: None,
        content_bytes: Some("# Doubao Search\n正文".as_bytes().to_vec()),
        suggested_name: None,
    }];
    let ctx_admin = ctx
        .to_builder()
        .user_id("admin".to_string())
        .user_role(1)
        .build();
    domain
        .skill_manage()
        .update_skill(
            ctx_admin.clone(),
            UpdateSkillParams {
                skill: &published,
                imports,
                file_deletes: vec![],
                remote_source: None,
            },
        )
        .await
        .unwrap();

    // 源技能保持 Published
    let src = domain
        .skill_manage()
        .get_skill(ctx.clone(), skill.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        src.po.status,
        SkillStatus::Published,
        "源技能应保持 Published"
    );

    // 该 Agent 名下恰好 1 条回写副本：Draft / parent=源 id / author=Agent / 文件齐全
    let copies = domain
        .skill_manage()
        .query_skills(
            ctx.clone(),
            SkillQuery {
                author_id: Some("agent-1".to_string()),
                parent_skill_id: Some(skill.id().to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(copies.items.len(), 1, "发布后应恰好回写 1 条副本");
    let copy = &copies.items[0];
    assert_eq!(copy.po.status, SkillStatus::Draft, "回写副本应为 Draft");
    assert_eq!(copy.po.parent_skill_id, skill.id(), "副本 parent 应指向源");
    assert_eq!(copy.po.author_id, "agent-1");
    assert!(matches!(copy.po.author_type, SkillAuthorType::Agent));
    assert!(
        copy.files.iter().any(|f| f.filename == "skill.md"),
        "回写副本应携带拷贝的 skill.md 文件"
    );
}

/// 重复发布幂等：发布 → 翻回 Draft → 再次发布，第二次仍命中回写路径，
/// 但幂等查重应跳过（不产生第二条副本）——N5 补偿重放语义依赖此处锁定。
#[sqlx::test]
async fn test_publish_writeback_is_idempotent(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool);
    let ctx_admin = ctx
        .to_builder()
        .user_id("admin".to_string())
        .user_role(1)
        .build();

    let skill = create_agent_root_skill("agent-1", "RepeatSearch");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    let copies_query = || async {
        domain
            .skill_manage()
            .query_skills(
                ctx.clone(),
                SkillQuery {
                    author_id: Some("agent-1".to_string()),
                    parent_skill_id: Some(skill.id().to_string()),
                    ..Default::default()
                },
            )
            .await
            .unwrap()
    };

    // 第一次发布
    let mut published = skill.clone();
    published.po.status = SkillStatus::Published;
    domain
        .skill_manage()
        .update_skill(
            ctx_admin.clone(),
            UpdateSkillParams {
                skill: &published,
                imports: vec![],
                file_deletes: vec![],
                remote_source: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        copies_query().await.items.len(),
        1,
        "首次发布应回写 1 条副本"
    );

    // 翻回 Draft 再发布：第二次仍命中回写路径，幂等查重跳过
    let mut back_to_draft = skill.clone();
    back_to_draft.po.status = SkillStatus::Draft;
    domain
        .skill_manage()
        .update_skill(
            ctx_admin.clone(),
            UpdateSkillParams {
                skill: &back_to_draft,
                imports: vec![],
                file_deletes: vec![],
                remote_source: None,
            },
        )
        .await
        .unwrap();
    let mut published_again = skill.clone();
    published_again.po.status = SkillStatus::Published;
    domain
        .skill_manage()
        .update_skill(
            ctx_admin,
            UpdateSkillParams {
                skill: &published_again,
                imports: vec![],
                file_deletes: vec![],
                remote_source: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        copies_query().await.items.len(),
        1,
        "重复发布不得产生第二条副本（幂等查重）"
    );
}

/// 负向语义：User 技能发布不回写；安装副本（parent 非空）状态变更不回写。
#[sqlx::test]
async fn test_publish_no_writeback_for_user_skill_and_installed_copy(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env(pool);
    let ctx_admin = ctx
        .to_builder()
        .user_id("admin".to_string())
        .user_role(1)
        .build();

    // ③ User 技能发布 → 不回写
    let user_skill = create_test_skill("UserOwned");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&user_skill))
        .await
        .unwrap();
    let mut published = user_skill.clone();
    published.po.status = SkillStatus::Published;
    domain
        .skill_manage()
        .update_skill(
            ctx_admin.clone(),
            UpdateSkillParams {
                skill: &published,
                imports: vec![],
                file_deletes: vec![],
                remote_source: None,
            },
        )
        .await
        .unwrap();
    let copies = domain
        .skill_manage()
        .query_skills(
            ctx.clone(),
            SkillQuery {
                author_id: Some("admin".to_string()),
                parent_skill_id: Some(user_skill.id().to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(copies.items.is_empty(), "User 技能发布不应产生回写副本");

    // ④ 安装副本形态（parent 非空，author=Agent）状态变更 → 不回写
    let installed_copy = Skill::from_po(SkillPo::new(
        format!("installed-copy--{}", uuid::Uuid::new_v4()),
        "Installed Search".to_string(),
        "installed copy".to_string(),
        vec!["search".to_string()],
        "testing".to_string(),
        user_skill.id().to_string(), // parent 非空：安装副本
        "agent-9".to_string(),
        SkillAuthorType::Agent,
        "agents/agent-9/skills/installed-copy".to_string(),
    ));
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&installed_copy))
        .await
        .unwrap();
    let mut copy_published = installed_copy.clone();
    copy_published.po.status = SkillStatus::Published;
    domain
        .skill_manage()
        .update_skill(
            ctx_admin,
            UpdateSkillParams {
                skill: &copy_published,
                imports: vec![],
                file_deletes: vec![],
                remote_source: None,
            },
        )
        .await
        .unwrap();
    let nested = domain
        .skill_manage()
        .query_skills(
            ctx,
            SkillQuery {
                author_id: Some("agent-9".to_string()),
                parent_skill_id: Some(installed_copy.id().to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(nested.items.is_empty(), "安装副本状态变更不应触发回写");
}
