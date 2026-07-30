//! Integration tests for preset skills & builtin tools import on system initialization.
//!
//! Covers:
//! - `initialize_system` imports 5 preset skills (4 neural + 1 project_management) to the shared library
//! - `initialize_system` syncs builtin tools to DB
//! - Preset skills' author_id is replaced with the actual owner user_id
//! - Preset skill files (skill.md) are written and contain expected content
//! - `apply_preset_skills` is idempotent (second bootstrap updates, not duplicates)

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ai_orz::service::domain::hr;
use sqlx::SqlitePool;

/// 查询所有技能（避免导入 common::enums::SkillStatus，用 query_skills 代替 list_by_status）
async fn query_all_skills(ctx: &ai_orz::pkg::RequestContext) -> Vec<ai_orz::models::skill::Skill> {
    hr::domain()
        .skill_manage()
        .query_skills(ctx.clone(), Default::default())
        .await
        .expect("查询技能失败")
        .items
}

/// After system initialization, 5 preset skills should exist in the shared library.
#[sqlx::test]
async fn test_initialize_system_imports_preset_skills(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let bs = crate::common::factories::bootstrap_system(&app).await;

    let skills = query_all_skills(&ctx).await;

    let skill_ids: Vec<&str> = skills.iter().map(|s| s.po.id.as_str()).collect();
    assert!(
        skill_ids.contains(&"TEMPLATE_TOOL_BASICS"),
        "缺少预置技能：工具管理"
    );
    assert!(
        skill_ids.contains(&"TEMPLATE_SKILL_BASICS"),
        "缺少预置技能：技能管理"
    );
    assert!(
        skill_ids.contains(&"TEMPLATE_MEMORY_COGNITION"),
        "缺少预置技能：记忆认知"
    );
    assert!(
        skill_ids.contains(&"TEMPLATE_COMMUNICATION"),
        "缺少预置技能：协作沟通"
    );
    assert!(
        skill_ids.contains(&"TEMPLATE_PROJECT_MANAGEMENT"),
        "缺少预置技能：项目管理"
    );

    // 验证 author_id 被替换为实际 owner（B 方案）
    let tool_skill = skills
        .iter()
        .find(|s| s.po.id == "TEMPLATE_TOOL_BASICS")
        .expect("工具管理技能不存在");
    assert_eq!(
        tool_skill.po.author_id, bs.user_id,
        "预置技能 author_id 应替换为实际 owner id"
    );

    // 验证神经技能 tags 包含 neural
    assert!(
        tool_skill
            .po
            .parse_tags()
            .contains(&"neural".to_string()),
        "神经技能必须包含 neural tag"
    );

    // 验证项目管理技能不含 neural（按需加载）
    let pm_skill = skills
        .iter()
        .find(|s| s.po.id == "TEMPLATE_PROJECT_MANAGEMENT")
        .expect("项目管理技能不存在");
    assert!(
        !pm_skill
            .po
            .parse_tags()
            .contains(&"neural".to_string()),
        "项目管理技能不应包含 neural tag"
    );
    assert!(
        pm_skill
            .po
            .parse_tags()
            .contains(&"project_management".to_string()),
        "项目管理技能应包含 project_management tag"
    );
}

/// After system initialization, builtin tools should be synced to DB.
#[sqlx::test]
async fn test_initialize_system_syncs_builtin_tools(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let _bs = crate::common::factories::bootstrap_system(&app).await;

    // 通过 domain 层查询工具列表
    let tools = ai_orz::service::domain::finance::domain()
        .tool_provider_manage()
        .list_tools(ctx.clone())
        .await
        .expect("查询工具失败");

    assert!(
        !tools.is_empty(),
        "内置工具未同步到 DB — initialize_system 应调用 sync_builtin_tools"
    );
}

/// Preset skill files (skill.md) should be written and contain expected content.
#[sqlx::test]
async fn test_preset_skill_files_written(pool: SqlitePool) {
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let bs = crate::common::factories::bootstrap_system(&app).await;

    // list_skill_files 检查 author_id 权限，必须用 bootstrap 用户 ID 创建 ctx
    let ctx =
        ai_orz::pkg::RequestContext::from_storage(&bs.user_id, ai_orz::pkg::storage::get().clone());

    // 验证工具基础的 skill.md 文件
    let files = hr::domain()
        .skill_manage()
        .list_skill_files(ctx.clone(), "TEMPLATE_TOOL_BASICS")
        .await
        .expect("查询技能文件失败")
        .unwrap_or_default();

    assert!(!files.is_empty(), "工具管理应有 skill.md 文件");

    let skill_md = files
        .iter()
        .find(|f| f.filename == "skill.md")
        .expect("缺少 skill.md 文件");

    let content = skill_md
        .content
        .as_ref()
        .expect("skill.md 内容为空")
        .clone();

    assert!(
        content.contains("# 工具管理"),
        "skill.md 内容应包含标题"
    );
    assert!(
        content.contains("request_tool_call"),
        "skill.md 应提及 request_tool_call 工具"
    );
    assert!(
        content.contains("send_tool_call_message"),
        "skill.md 应提及 send_tool_call_message 工具"
    );

    // 验证记忆认知
    let memory_files = hr::domain()
        .skill_manage()
        .list_skill_files(ctx.clone(), "TEMPLATE_MEMORY_COGNITION")
        .await
        .expect("查询记忆认知文件失败")
        .unwrap_or_default();
    let memory_md = memory_files
        .iter()
        .find(|f| f.filename == "skill.md")
        .expect("记忆认知缺少 skill.md");
    assert!(
        memory_md
            .content
            .as_ref()
            .unwrap()
            .contains("# 记忆认知"),
        "记忆认知内容不正确"
    );

    // 验证协作沟通
    let comm_files = hr::domain()
        .skill_manage()
        .list_skill_files(ctx.clone(), "TEMPLATE_COMMUNICATION")
        .await
        .expect("查询协作沟通文件失败")
        .unwrap_or_default();
    let comm_md = comm_files
        .iter()
        .find(|f| f.filename == "skill.md")
        .expect("协作沟通缺少 skill.md");
    assert!(
        comm_md
            .content
            .as_ref()
            .unwrap()
            .contains("# 协作沟通"),
        "协作沟通内容不正确"
    );
}

/// apply_preset_skills should be idempotent: second bootstrap updates, not duplicates.
#[sqlx::test]
async fn test_preset_skills_idempotent(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    // 第一次 bootstrap — 创建预置技能
    let _bs1 = crate::common::factories::bootstrap_system(&app).await;

    let skills_after_first = query_all_skills(&ctx).await;
    let count_after_first = skills_after_first
        .iter()
        .filter(|s| s.po.id.starts_with("TEMPLATE_"))
        .count();
    assert_eq!(count_after_first, 5, "第一次初始化后应有 5 个预置技能");

    // 第二次 bootstrap — 应更新而非重复创建
    let bs2 = crate::common::factories::bootstrap_system(&app).await;

    let skills_after_second = query_all_skills(&ctx).await;
    let count_after_second = skills_after_second
        .iter()
        .filter(|s| s.po.id.starts_with("TEMPLATE_"))
        .count();
    assert_eq!(
        count_after_second, 5,
        "第二次初始化后仍应只有 5 个预置技能（idempotent）"
    );

    // author_id 应更新为第二个 owner
    let tool_skill = skills_after_second
        .iter()
        .find(|s| s.po.id == "TEMPLATE_TOOL_BASICS")
        .expect("工具管理技能不存在");
    assert_eq!(
        tool_skill.po.author_id, bs2.user_id,
        "第二次初始化后 author_id 应更新为新的 owner id"
    );
}
