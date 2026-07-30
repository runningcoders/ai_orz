//! Integration tests for preset skills & builtin tools import on system initialization.
//!
//! Covers:
//! - `initialize_system` imports 3 preset neural skills to the shared library
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

/// After system initialization, 3 preset neural skills should exist in the shared library.
#[sqlx::test]
async fn test_initialize_system_imports_preset_skills(pool: SqlitePool) {
    let ctx = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let bs = crate::common::factories::bootstrap_system(&app).await;

    let skills = query_all_skills(&ctx).await;

    let skill_ids: Vec<&str> = skills.iter().map(|s| s.po.id.as_str()).collect();
    assert!(
        skill_ids.contains(&"TEMPLATE_PLATFORM_GUIDE"),
        "缺少预置技能：平台使用指南"
    );
    assert!(
        skill_ids.contains(&"TEMPLATE_MEMORY_GUIDE"),
        "缺少预置技能：记忆管理指南"
    );
    assert!(
        skill_ids.contains(&"TEMPLATE_COLLABORATION_GUIDE"),
        "缺少预置技能：Agent 协作指南"
    );

    // 验证 author_id 被替换为实际 owner（B 方案）
    let platform_skill = skills
        .iter()
        .find(|s| s.po.id == "TEMPLATE_PLATFORM_GUIDE")
        .expect("平台使用指南不存在");
    assert_eq!(
        platform_skill.po.author_id, bs.user_id,
        "预置技能 author_id 应替换为实际 owner id"
    );

    // 验证 tags 包含 neural
    assert!(
        platform_skill
            .po
            .parse_tags()
            .contains(&"neural".to_string()),
        "预置技能必须包含 neural tag"
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

    // 验证平台使用指南的 skill.md 文件
    let files = hr::domain()
        .skill_manage()
        .list_skill_files(ctx.clone(), "TEMPLATE_PLATFORM_GUIDE")
        .await
        .expect("查询技能文件失败")
        .unwrap_or_default();

    assert!(!files.is_empty(), "平台使用指南应有 skill.md 文件");

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
        content.contains("# 平台使用指南"),
        "skill.md 内容应包含标题"
    );
    assert!(content.contains("神经工具"), "skill.md 应包含神经工具章节");
    assert!(
        content.contains("search_skill"),
        "skill.md 应提及 search_skill 工具"
    );

    // 验证记忆管理指南
    let memory_files = hr::domain()
        .skill_manage()
        .list_skill_files(ctx.clone(), "TEMPLATE_MEMORY_GUIDE")
        .await
        .expect("查询记忆指南文件失败")
        .unwrap_or_default();
    let memory_md = memory_files
        .iter()
        .find(|f| f.filename == "skill.md")
        .expect("记忆指南缺少 skill.md");
    assert!(
        memory_md
            .content
            .as_ref()
            .unwrap()
            .contains("# 记忆管理指南"),
        "记忆指南内容不正确"
    );

    // 验证协作指南
    let collab_files = hr::domain()
        .skill_manage()
        .list_skill_files(ctx.clone(), "TEMPLATE_COLLABORATION_GUIDE")
        .await
        .expect("查询协作指南文件失败")
        .unwrap_or_default();
    let collab_md = collab_files
        .iter()
        .find(|f| f.filename == "skill.md")
        .expect("协作指南缺少 skill.md");
    assert!(
        collab_md
            .content
            .as_ref()
            .unwrap()
            .contains("# Agent 协作指南"),
        "协作指南内容不正确"
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
    assert_eq!(count_after_first, 3, "第一次初始化后应有 3 个预置技能");

    // 第二次 bootstrap — 应更新而非重复创建
    let bs2 = crate::common::factories::bootstrap_system(&app).await;

    let skills_after_second = query_all_skills(&ctx).await;
    let count_after_second = skills_after_second
        .iter()
        .filter(|s| s.po.id.starts_with("TEMPLATE_"))
        .count();
    assert_eq!(
        count_after_second, 3,
        "第二次初始化后仍应只有 3 个预置技能（idempotent）"
    );

    // author_id 应更新为第二个 owner
    let platform_skill = skills_after_second
        .iter()
        .find(|s| s.po.id == "TEMPLATE_PLATFORM_GUIDE")
        .expect("平台使用指南不存在");
    assert_eq!(
        platform_skill.po.author_id, bs2.user_id,
        "第二次初始化后 author_id 应更新为新的 owner id"
    );
}
