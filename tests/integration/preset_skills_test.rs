//! Integration tests for preset skills & builtin tools import on system initialization.
//!
//! Covers:
//! - `initialize_system` imports 5 preset skills (4 neural + 1 project_management) to the shared library
//! - `initialize_system` syncs builtin tools to DB
//! - Preset skills' author_id is replaced with the actual owner user_id
//! - Preset skill files (skill.md) are written and contain expected content
//! - `apply_preset_skills` is idempotent (second bootstrap updates, not duplicates)
//!
//! ## 进程级串行化原因（为什么 4 个用例必须全部互斥，锁窗口含 bootstrap+断言）
//!
//! 集成测试共享同一进程级 Storage OnceLock（单 SQLite 文件），固定 ID 的 Local 组织 /
//! 预置技能（TEMPLATE_*）在所有用例间共享同一行数据：
//!
//! 1. `count=5` / `author_id == bs2.user_id` 这类**快照断言**对「额外写入」零容忍，
//!    并行执行下其他测试先 bootstrap 导入后，会被当前用例的断言误判为「自己导入的」，
//!    造成测试间语义耦合。
//! 2. `bootstrap_system` 内部的 `BOOTSTRAP_MUTEX` 仅串行化 bootstrap 本身，
//!    **断言（query_skills / list_skill_files / 计数）在锁外**，不能避免竞争。
//! 3. 复用路径中的「初始化阶段 check_initialized / 导入中途 Forbidden」等边界条件，
//!    只在完全串行、可预测的初始化顺序下才有稳定语义。
//!
//! 每个用例的第一步即获取此锁，保持到断言结束。模式同：
//! `tests/integration/message_vector_test.rs`（REAL_VECTOR_MUTEX）。
#![allow(clippy::await_holding_lock)]

#[path = "../common/mod.rs"]
mod common;

use crate::common::TestApp;
use ai_orz::service::domain::hr;
use sqlx::SqlitePool;

/// preset_skills 全文件进程级串行互斥锁。
///
/// 保持 `std::sync::Mutex`（非 async）因为获取是立即的，锁期间包含 HTTP handler
/// 调用 + tokio sleep 的异步工作，但 MutexGuard 跨 .await 点持有在这里是安全的：
/// - 单个 tokio worker 线程拿不到其他用例也不会阻塞整个 runtime；
/// - 锁窗口内的工作是 CPU 轻 + I/O，其它任务可在其他线程继续；
/// - 用 `std::sync::Mutex` 免去 `tokio::sync::Mutex` 必须 .await 的句法负担，
///   与 `BOOTSTRAP_MUTEX` 不同（那把锁只保护 bootstrap 获取路径短异步窗口）。
static PRESET_STATE_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    // 全局串行锁：4 个用例共用 Storage + 固定 ID 预置技能 + 单个 Local 组织，
    // 必须整用例（bootstrap→断言）互斥，否则 count=5/author_id 更新等零容忍断言失效。
    // 用 into_inner() 跳过 PoisonError：此锁只做执行顺序栅拦，不保护可变状态。
    let _lock = PRESET_STATE_MUTEX
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let bs = crate::common::factories::bootstrap_system(&app).await;
    // 必须用 bootstrap 返回的真实 admin 身份（含 SuperAdmin role + organization_id），
    // 才能在集成测试共享 DB 时跨用例访问「当前归属于复用 admin 的预置技能」元数据。
    let ctx = bs.build_authenticated_ctx();

    let skills = query_all_skills(&ctx).await;

    let skill_ids: Vec<&str> = skills.iter().map(|s| s.po.id.as_str()).collect();
    assert!(
        skill_ids.contains(&"TEMPLATE_TOOL_MANAGEMENT"),
        "缺少预置技能：工具管理"
    );
    assert!(
        skill_ids.contains(&"TEMPLATE_SKILL_MANAGEMENT"),
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
        .find(|s| s.po.id == "TEMPLATE_TOOL_MANAGEMENT")
        .expect("工具管理技能不存在");
    assert_eq!(
        tool_skill.po.author_id, bs.user_id,
        "预置技能 author_id 应替换为实际 owner id"
    );

    // 验证神经技能 tags 包含 neural
    assert!(
        tool_skill.po.parse_tags().contains(&"neural".to_string()),
        "神经技能必须包含 neural tag"
    );

    // 验证项目管理技能不含 neural（按需加载）
    let pm_skill = skills
        .iter()
        .find(|s| s.po.id == "TEMPLATE_PROJECT_MANAGEMENT")
        .expect("项目管理技能不存在");
    assert!(
        !pm_skill.po.parse_tags().contains(&"neural".to_string()),
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
    let _lock = PRESET_STATE_MUTEX
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let bs = crate::common::factories::bootstrap_system(&app).await;
    // 本测试断言内置工具同步结果；此处同样用 bootstrap 构建的认证 ctx，
    // 以与其他用例保持一致且避免 DB 共享造成的查询权限异常。
    let ctx = bs.build_authenticated_ctx();

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
    let _lock = PRESET_STATE_MUTEX
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;
    let bs = crate::common::factories::bootstrap_system(&app).await;

    // 用 bootstrap 的真实身份构造 ctx（user_id=技能作者 + user_role=SuperAdmin）：
    // list_skill_files 内部会走 ensure_skill_access（管理员 / 作者 / Agent 创建者，
    // 三条件任一放行），必须匹配上才能读他人创建的预置技能文件。
    let ctx = bs.build_authenticated_ctx();

    // 验证工具基础的 skill.md 文件
    let files = hr::domain()
        .skill_manage()
        .list_skill_files(ctx.clone(), "TEMPLATE_TOOL_MANAGEMENT")
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

    assert!(content.contains("# 工具管理"), "skill.md 内容应包含标题");
    assert!(
        content.contains("get_tool_call_entry") || content.contains("query_tool_call_entries"),
        "skill.md 应提及工具调用追溯工具（get_tool_call_entry 或 query_tool_call_entries）"
    );
    assert!(
        content.contains("ToolCallResult"),
        "skill.md 应提及异步工具调用结果消息（ToolCallResult）"
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
        memory_md.content.as_ref().unwrap().contains("# 记忆认知"),
        "记忆认知内容不正确"
    );
    // 记忆认知应含「用户偏好沉淀」专节（组织级共享画像约定）
    let memory_content = memory_md.content.as_ref().unwrap();
    assert!(
        memory_content.contains("用户偏好沉淀"),
        "记忆认知应含用户偏好沉淀专节"
    );
    assert!(
        memory_content.contains("user_preference"),
        "用户偏好沉淀应约定 user_preference tag（tag 只表种类）"
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
        comm_md.content.as_ref().unwrap().contains("# 协作沟通"),
        "协作沟通内容不正确"
    );
    // 协作沟通行为准则应含「留意用户偏好」条目
    assert!(
        comm_md.content.as_ref().unwrap().contains("留意用户偏好"),
        "协作沟通行为准则应含留意用户偏好条目"
    );
}

/// apply_preset_skills should be idempotent: second bootstrap updates, not duplicates.
#[sqlx::test]
async fn test_preset_skills_idempotent(pool: SqlitePool) {
    let _lock = PRESET_STATE_MUTEX
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    let _ = crate::common::init_full_test_env(pool.clone()).await;
    let app = TestApp::new(pool).await;

    // 第一次 bootstrap — 创建预置技能
    let bs1 = crate::common::factories::bootstrap_system(&app).await;
    let ctx = bs1.build_authenticated_ctx();

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
        .find(|s| s.po.id == "TEMPLATE_TOOL_MANAGEMENT")
        .expect("工具管理技能不存在");
    assert_eq!(
        tool_skill.po.author_id, bs2.user_id,
        "第二次初始化后 author_id 应更新为新的 owner id"
    );
}
