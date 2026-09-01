//! HR Domain Agent 管理单元测试

use super::{CreateSkillParams, HrDomain, domain};
use crate::models::agent::{Agent, AgentPo};
use crate::models::skill::{Skill, SkillPo};
use crate::pkg::RequestContext;
use common::enums::AgentStatus;
use common::enums::SkillStatus;
use common::enums::skill::SkillAuthorType;
use sqlx::SqlitePool;
use tempfile::TempDir;
use uuid::Uuid;

fn new_ctx(user_id: &str, pool: sqlx::SqlitePool) -> RequestContext {
    crate::pkg::request_context_test_support::new_test_ctx(user_id, pool)
}

/// 初始化 HR Domain 所有依赖
/// 初始化顺序：dao -> dal -> domain
fn init_test_env(pool: SqlitePool) -> (std::sync::Arc<dyn HrDomain>, RequestContext) {
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
    (domain, ctx)
}

/// 创建测试 Agent
fn create_test_agent(name: &str) -> Agent {
    let agent_po = AgentPo::new(
        name.to_string(),
        vec!["worker".to_string()],
        "A helpful agent".to_string(),
        vec!["coding".to_string()],
        "A helpful agent that can code".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    Agent::from_po(agent_po)
}

#[sqlx::test]
async fn test_create_and_find_by_id(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let agent = create_test_agent("TestAgent");

    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    let found: Option<Agent> = domain
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await
        .unwrap();
    assert_eq!(found.unwrap().name(), "TestAgent");
}

#[sqlx::test]
async fn test_list_agents(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    for i in 0..3 {
        let agent = create_test_agent(&format!("Agent{}", i));
        domain
            .agent_manage()
            .create_agent(ctx.clone(), &agent)
            .await
            .unwrap();
    }

    let agents: Vec<Agent> = domain.agent_manage().list_agents(ctx).await.unwrap();
    assert_eq!(agents.len(), 3);
}

#[sqlx::test]
async fn test_update_agent(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let agent = create_test_agent("Original");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    let mut updated = agent.clone();
    updated.po.name = "Updated".to_string();
    domain
        .agent_manage()
        .update_agent(new_ctx("editor", pool), &updated)
        .await
        .unwrap();

    let found: Option<Agent> = domain
        .agent_manage()
        .get_agent(ctx, updated.id(), Default::default())
        .await
        .unwrap();
    assert_eq!(found.unwrap().name(), "Updated");
}

#[sqlx::test]
async fn test_delete_agent(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let agent = create_test_agent("ToDelete");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    domain
        .agent_manage()
        .delete_agent(ctx.clone(), &agent)
        .await
        .unwrap();
    let found: Option<Agent> = domain
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await
        .unwrap();
    assert!(found.is_none());
}

#[sqlx::test]
async fn test_transition_status_persists_valid_agent_lifecycle(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let mut agent = create_test_agent("LifecycleAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    domain
        .agent_manage()
        .transition_status(ctx.clone(), &mut agent, AgentStatus::PendingOnboard)
        .await
        .unwrap();

    assert_eq!(agent.po.status, AgentStatus::PendingOnboard);

    let found = domain
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await
        .unwrap()
        .expect("transitioned agent should be readable");

    assert_eq!(found.po.status, AgentStatus::PendingOnboard);
}

#[sqlx::test]
async fn test_transition_status_rejects_invalid_agent_lifecycle(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let mut agent = create_test_agent("InvalidLifecycleAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    let result = domain
        .agent_manage()
        .transition_status(ctx.clone(), &mut agent, AgentStatus::Onboarded)
        .await;

    assert!(result.is_err());
    assert_eq!(agent.po.status, AgentStatus::Interviewing);

    let found = domain
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await
        .unwrap()
        .expect("rejected transition should keep agent readable");

    assert_eq!(found.po.status, AgentStatus::Interviewing);
}

#[sqlx::test]
async fn test_onboard_installs_project_management_tag(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let mut agent = create_test_agent("OnboardAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // Interviewing → PendingOnboard → Onboarded
    domain
        .agent_manage()
        .transition_status(ctx.clone(), &mut agent, AgentStatus::PendingOnboard)
        .await
        .unwrap();
    domain
        .agent_manage()
        .transition_status(ctx.clone(), &mut agent, AgentStatus::Onboarded)
        .await
        .unwrap();

    // Verify installed_tags contains "project_management"
    assert!(
        agent
            .po
            .get_installed_tags()
            .contains(&"project_management".to_string())
    );

    // Verify persisted
    let found = domain
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await
        .unwrap()
        .expect("onboarded agent should be readable");

    assert!(
        found
            .po
            .get_installed_tags()
            .contains(&"project_management".to_string())
    );
}

#[sqlx::test]
async fn test_non_onboard_transition_does_not_install_tag(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let mut agent = create_test_agent("NonOnboardAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // Interviewing → PendingOnboard (NOT Onboarded)
    domain
        .agent_manage()
        .transition_status(ctx.clone(), &mut agent, AgentStatus::PendingOnboard)
        .await
        .unwrap();

    // Verify installed_tags is empty
    assert!(agent.po.get_installed_tags().is_empty());

    // Verify persisted
    let found = domain
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await
        .unwrap()
        .expect("agent should be readable");

    assert!(found.po.get_installed_tags().is_empty());
}

#[sqlx::test]
async fn test_install_tool_pack_installs_tag_idempotently(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let agent = create_test_agent("ToolPackAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // Install once
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "data_analysis")
        .await
        .unwrap();

    // Install same tag again (idempotent)
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "data_analysis")
        .await
        .unwrap();

    // Verify installed_tags contains exactly one "data_analysis"
    let installed = domain
        .agent_manage()
        .list_installed_tool_packs(ctx.clone(), agent.id())
        .await
        .unwrap();

    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0], "data_analysis");

    // Verify persisted
    let found = domain
        .agent_manage()
        .get_agent(ctx, agent.id(), Default::default())
        .await
        .unwrap()
        .expect("agent should be readable");

    assert_eq!(found.po.get_installed_tags().len(), 1);
    assert_eq!(found.po.get_installed_tags()[0], "data_analysis");
}

#[sqlx::test]
async fn test_uninstall_tool_pack_removes_tag_idempotently(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let agent = create_test_agent("UninstallAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // Install two tags
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "data_analysis")
        .await
        .unwrap();
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "project_management")
        .await
        .unwrap();

    // Uninstall one
    domain
        .agent_manage()
        .uninstall_tool_pack(ctx.clone(), agent.id(), "data_analysis")
        .await
        .unwrap();

    let installed = domain
        .agent_manage()
        .list_installed_tool_packs(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0], "project_management");

    // Uninstall same tag again (idempotent - no error, no change)
    domain
        .agent_manage()
        .uninstall_tool_pack(ctx.clone(), agent.id(), "data_analysis")
        .await
        .unwrap();

    let installed = domain
        .agent_manage()
        .list_installed_tool_packs(ctx, agent.id())
        .await
        .unwrap();
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0], "project_management");
}

#[sqlx::test]
async fn test_list_installed_tool_packs_returns_all_tags(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool.clone());

    let agent = create_test_agent("ListPacksAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // Initially empty
    let installed = domain
        .agent_manage()
        .list_installed_tool_packs(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert!(installed.is_empty());

    // Install multiple tags
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "data_analysis")
        .await
        .unwrap();
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "project_management")
        .await
        .unwrap();
    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "coding")
        .await
        .unwrap();

    let installed = domain
        .agent_manage()
        .list_installed_tool_packs(ctx, agent.id())
        .await
        .unwrap();
    assert_eq!(installed.len(), 3);
    assert!(installed.contains(&"data_analysis".to_string()));
    assert!(installed.contains(&"project_management".to_string()));
    assert!(installed.contains(&"coding".to_string()));
}

#[sqlx::test]
async fn test_install_tool_pack_returns_error_for_nonexistent_agent(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    let result = domain
        .agent_manage()
        .install_tool_pack(ctx, "nonexistent-agent-id", "data_analysis")
        .await;

    assert!(result.is_err());
}

// ==================== 技能包安装/卸载/列表测试 ====================

/// 初始化测试环境（带文件系统支持，用于技能包安装测试）
fn init_test_env_with_fs(
    pool: SqlitePool,
) -> (std::sync::Arc<dyn HrDomain>, RequestContext, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path().to_path_buf();

    unsafe {
        std::env::set_var("AI_ORZ_BASE_PATH", base_path.to_str().unwrap());
    }

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

/// 创建已发布技能（带指定 tag）
fn create_published_skill_with_tag(name: &str, tag: &str) -> Skill {
    let skill_po = SkillPo::new(
        format!(
            "{}--{}",
            name.to_lowercase().replace(" ", "-"),
            Uuid::new_v4()
        ),
        name.to_string(),
        format!("A test skill for {}", tag),
        vec![tag.to_string()],
        "coding".to_string(),
        String::new(),
        "admin".to_string(),
        SkillAuthorType::User,
        format!("skills/{}", name.to_lowercase().replace(" ", "-")),
    );
    let mut skill = Skill::from_po(skill_po);
    skill.po.status = SkillStatus::Published;
    skill
}

#[sqlx::test]
async fn test_install_skill_pack(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env_with_fs(pool);

    // 创建 Agent
    let agent = create_test_agent("SkillPackAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // 创建 2 个带 "coding" tag 的已发布技能
    let skill1 = create_published_skill_with_tag("CodingSkill1", "coding");
    let skill2 = create_published_skill_with_tag("CodingSkill2", "coding");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill1))
        .await
        .unwrap();
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill2))
        .await
        .unwrap();

    // 安装技能包
    let count = domain
        .agent_manage()
        .install_skill_pack(ctx.clone(), agent.id(), "coding")
        .await
        .unwrap();

    // 验证返回成功安装数量
    assert_eq!(count, 2);

    // 验证 tag 已记录
    let packs = domain
        .agent_manage()
        .list_installed_skill_packs(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert_eq!(packs.len(), 1);
    assert_eq!(packs[0], "coding");

    // 验证技能副本已创建
    let agent_skills = domain
        .skill_manage()
        .list_for_agent(ctx, agent.id())
        .await
        .unwrap();
    assert_eq!(agent_skills.len(), 2);
}

#[sqlx::test]
async fn test_install_skill_pack_idempotent(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env_with_fs(pool);

    let agent = create_test_agent("IdempotentAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    let skill = create_published_skill_with_tag("UniqueSkill", "writing");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    // 第一次安装
    let count1 = domain
        .agent_manage()
        .install_skill_pack(ctx.clone(), agent.id(), "writing")
        .await
        .unwrap();
    assert_eq!(count1, 1);

    // 第二次安装同一 tag（幂等跳过）
    let count2 = domain
        .agent_manage()
        .install_skill_pack(ctx.clone(), agent.id(), "writing")
        .await
        .unwrap();
    assert_eq!(count2, 0);

    // 验证 tag 只记录一次
    let packs = domain
        .agent_manage()
        .list_installed_skill_packs(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert_eq!(packs.len(), 1);

    // 验证技能副本只有 1 个（install_to_agent 幂等）
    let agent_skills = domain
        .skill_manage()
        .list_for_agent(ctx, agent.id())
        .await
        .unwrap();
    assert_eq!(agent_skills.len(), 1);
}

#[sqlx::test]
async fn test_uninstall_skill_pack(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env_with_fs(pool);

    let agent = create_test_agent("UninstallSkillPackAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    let skill = create_published_skill_with_tag("PersistSkill", "analysis");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    // 安装技能包
    domain
        .agent_manage()
        .install_skill_pack(ctx.clone(), agent.id(), "analysis")
        .await
        .unwrap();

    // 验证已安装
    let packs = domain
        .agent_manage()
        .list_installed_skill_packs(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert_eq!(packs.len(), 1);

    // 卸载技能包
    domain
        .agent_manage()
        .uninstall_skill_pack(ctx.clone(), agent.id(), "analysis", false)
        .await
        .unwrap();

    // 验证 tag 已移除
    let packs = domain
        .agent_manage()
        .list_installed_skill_packs(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert!(packs.is_empty());

    // 验证技能副本保留
    let agent_skills = domain
        .skill_manage()
        .list_for_agent(ctx, agent.id())
        .await
        .unwrap();
    assert_eq!(agent_skills.len(), 1);
}

#[sqlx::test]
async fn test_list_installed_skill_packs(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env_with_fs(pool);

    let agent = create_test_agent("ListSkillPacksAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // 初始为空
    let packs = domain
        .agent_manage()
        .list_installed_skill_packs(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert!(packs.is_empty());

    // 创建不同 tag 的已发布技能
    let skill1 = create_published_skill_with_tag("Skill1", "coding");
    let skill2 = create_published_skill_with_tag("Skill2", "writing");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill1))
        .await
        .unwrap();
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill2))
        .await
        .unwrap();

    // 安装多个技能包
    domain
        .agent_manage()
        .install_skill_pack(ctx.clone(), agent.id(), "coding")
        .await
        .unwrap();
    domain
        .agent_manage()
        .install_skill_pack(ctx.clone(), agent.id(), "writing")
        .await
        .unwrap();

    // 验证列表
    let packs = domain
        .agent_manage()
        .list_installed_skill_packs(ctx, agent.id())
        .await
        .unwrap();
    assert_eq!(packs.len(), 2);
    assert!(packs.contains(&"coding".to_string()));
    assert!(packs.contains(&"writing".to_string()));
}

#[sqlx::test]
async fn test_reinstall_skill_pack_updates_existing_copy(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env_with_fs(pool);

    let agent = create_test_agent("ReinstallAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // 创建已发布技能并安装
    let skill = create_published_skill_with_tag("ReinstallSkill", "coding");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();
    domain
        .agent_manage()
        .install_skill_pack(ctx.clone(), agent.id(), "coding")
        .await
        .unwrap();

    // 验证初始安装
    let agent_skills = domain
        .skill_manage()
        .list_for_agent(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert_eq!(agent_skills.len(), 1);
    let original_name = agent_skills[0].po.name.clone();

    // 更新源技能名称
    let mut updated_source = skill.clone();
    updated_source.po.name = "UpdatedReinstallSkill".to_string();
    domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            super::UpdateSkillParams {
                skill: &updated_source,
                imports: Vec::new(),
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .unwrap();

    // 重新安装技能包
    let count = domain
        .agent_manage()
        .reinstall_skill_pack(ctx.clone(), agent.id(), "coding")
        .await
        .unwrap();
    assert_eq!(count, 1);

    // 验证副本名称已更新（而非创建新副本）
    let agent_skills = domain
        .skill_manage()
        .list_for_agent(ctx, agent.id())
        .await
        .unwrap();
    assert_eq!(agent_skills.len(), 1, "重装不应创建新副本");
    assert_eq!(
        agent_skills[0].po.name, "UpdatedReinstallSkill",
        "副本名称应已更新"
    );
    assert_ne!(agent_skills[0].po.name, original_name, "名称应已变化");
}

#[sqlx::test]
async fn test_sync_agent_packs_fills_missing_base_and_new_skills(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env_with_fs(pool);

    let agent = create_test_agent("SyncPacksAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // ① 场景准备：
    // - "coding" 技能包已手动安装（1 个已发布技能 → 1 个副本）
    // - "neural" 基础技能包已发布但未安装（模拟存量 Agent 缺基础包）
    // - 另发布 1 个 "coding" 新技能，Agent 尚未拥有（模拟包内新增）
    let c1 = create_published_skill_with_tag("SyncCoding1", "coding");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&c1))
        .await
        .unwrap();
    domain
        .agent_manage()
        .install_skill_pack(ctx.clone(), agent.id(), "coding")
        .await
        .unwrap();

    let n1 = create_published_skill_with_tag("SyncNeural1", "neural");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&n1))
        .await
        .unwrap();

    let c2 = create_published_skill_with_tag("SyncCoding2", "coding");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&c2))
        .await
        .unwrap();

    // ② 执行同步
    let resp = domain
        .agent_manage()
        .sync_agent_packs(ctx.clone(), agent.id())
        .await
        .unwrap();

    // 测试环境无已启用工具 → 工具包补装为空
    assert!(resp.installed_tool_tags.is_empty());
    // 缺失的基础技能包 neural 被补装；coding 已安装故不在补装列表
    assert_eq!(resp.installed_skill_packs, vec!["neural".to_string()]);
    // coding 包检测到新增技能（SyncCoding2）→ 重装补全
    assert_eq!(resp.refreshed_skill_packs, vec!["coding".to_string()]);

    // ③ 验证副本：neural 1 个 + coding 2 个
    let agent_skills = domain
        .skill_manage()
        .list_for_agent(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert_eq!(agent_skills.len(), 3);
    let copy_parents: std::collections::HashSet<String> = agent_skills
        .iter()
        .map(|s| s.po.parent_skill_id.clone())
        .collect();
    assert!(copy_parents.contains(&n1.po.id));
    assert!(copy_parents.contains(&c1.po.id));
    assert!(copy_parents.contains(&c2.po.id));

    // ④ 幂等验证：再次同步应无任何变更
    let resp2 = domain
        .agent_manage()
        .sync_agent_packs(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert!(resp2.installed_tool_tags.is_empty());
    assert!(resp2.installed_skill_packs.is_empty());
    assert!(resp2.refreshed_skill_packs.is_empty());

    // ⑤ 不存在的 Agent 返回 NotFound
    let err = domain
        .agent_manage()
        .sync_agent_packs(ctx, "nonexistent-agent-id")
        .await
        .unwrap_err();
    assert!(matches!(err.code, common::error::ErrorCode::NotFound));
}

/// 创建指定角色的 Onboarded 测试 Agent（走正常流程：create_agent Interviewing → 入职）
async fn create_onboarded_agent(
    ctx: RequestContext,
    domain: &std::sync::Arc<dyn HrDomain>,
    name: &str,
    roles: Vec<&str>,
) -> String {
    let po = AgentPo::new(
        name.to_string(),
        roles.into_iter().map(|s| s.to_string()).collect(),
        "test agent".to_string(),
        vec!["chat".to_string()],
        "test soul".to_string(),
        "provider-id-1".to_string(),
        "admin".to_string(),
    );
    let agent = Agent::from_po(po);
    let id = agent.po.id.clone();
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();
    let mut agent = domain
        .agent_manage()
        .get_agent(
            ctx.clone(),
            &id,
            crate::service::dal::agent::AgentFetchOptions::default(),
        )
        .await
        .unwrap()
        .unwrap();
    domain
        .agent_manage()
        .transition_status(ctx.clone(), &mut agent, AgentStatus::PendingOnboard)
        .await
        .unwrap();
    domain
        .agent_manage()
        .transition_status(ctx, &mut agent, AgentStatus::Onboarded)
        .await
        .unwrap();
    id
}

/// 渐进式角色匹配：全匹配（tier1）优先于子串层级（tier3）
#[sqlx::test]
async fn test_resolve_agent_progressive_role_tiers(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    // 子串层级 agent：feishu_reception 包含 reception 子串，但无精确命中
    let substring_id =
        create_onboarded_agent(ctx.clone(), &domain, "飞书前台", vec!["feishu_reception"]).await;
    // 无命中 agent
    let worker_id = create_onboarded_agent(ctx.clone(), &domain, "普通员工", vec!["worker"]).await;
    // 全匹配 agent：reception 精确命中（创建最晚，created_at 最大）
    let full_id = create_onboarded_agent(ctx.clone(), &domain, "通用前台", vec!["reception"]).await;

    // 按 reception 匹配：应选中全匹配 agent（即使 created_at 更晚）
    let found = domain
        .resolve_agent(
            ctx.clone(),
            common::api::AgentMatchCriteria::by_role("reception"),
        )
        .await
        .unwrap()
        .expect("应解析到 Agent");
    assert_eq!(found.po.id, full_id, "tier1 全匹配应优先");

    // 全匹配为空（无 reception 精确命中）时，应靠子串层级命中 feishu_reception，
    // 而不是回退到完全无关的 worker（tier3 > 0 分 fallback）
    let full_agent = domain
        .agent_manage()
        .get_agent(
            ctx.clone(),
            &found.po.id,
            crate::service::dal::agent::AgentFetchOptions::default(),
        )
        .await
        .unwrap()
        .expect("全匹配 agent 应存在");
    domain
        .agent_manage()
        .delete_agent(ctx.clone(), &full_agent)
        .await
        .unwrap();
    let found = domain
        .resolve_agent(
            ctx.clone(),
            common::api::AgentMatchCriteria::by_role("reception"),
        )
        .await
        .unwrap()
        .expect("应解析到 Agent");
    assert_eq!(found.po.id, substring_id, "tier3 子串层级应命中");
    assert_ne!(found.po.id, worker_id, "不应回退到完全无关的 agent");
}

/// 多角色条件：tier2 部分精确（仅命中一个角色）应低于 tier1 全匹配所有角色
#[sqlx::test]
async fn test_resolve_agent_partial_vs_full_all_roles(pool: SqlitePool) {
    let (domain, ctx) = init_test_env(pool);

    // 只命中一个角色的 agent（tier2 部分精确）
    let _single_id = create_onboarded_agent(ctx.clone(), &domain, "前台", vec!["reception"]).await;
    // 命中全部两个角色的 agent（tier1 全匹配）
    let both_id =
        create_onboarded_agent(ctx.clone(), &domain, "全能前台", vec!["reception", "hr"]).await;

    let found = domain
        .resolve_agent(
            ctx.clone(),
            common::api::AgentMatchCriteria::by_roles(vec![
                "reception".to_string(),
                "hr".to_string(),
            ]),
        )
        .await
        .unwrap()
        .expect("应解析到 Agent");
    assert_eq!(found.po.id, both_id, "tier1 全匹配所有角色应优先");
}

// ==================== get_agent_association_groups 单测 ====================

/// 创建一个启用状态的 Tool（management 路径），方便测试工具分组
fn create_enabled_tool(name: &str, tags: Vec<&str>) -> crate::models::tool::Tool {
    use crate::models::tool::{Tool, ToolPo};
    use common::enums::ToolProtocol;
    use serde_json::json;
    // 注意：这里不能用 ToolProtocol::Builtin —— Builtin 工具需在内置注册表中存在，
    // 否则 tool_dal::get_by_id 会因 assemble_core_tool 失败而返回 None（表现为
    // "Tool not found"），导致后续的 bind_tool_to_agent 失败。
    // 使用 Http 协议时该分支会回落为 from_po_for_management，可正常读取。
    // 协议类型不影响本文件被测的工具/技能分组逻辑。
    let po = ToolPo::new(
        String::new(),
        name.to_string(),
        format!("Tool {}", name),
        ToolProtocol::Http,
        // HttpToolConfig 的 method / url 为必填，否则 create_tool 校验会拒绝
        json!({"method": "POST", "url": "https://example.test/tool"}),
        Some(json!({"type":"object","properties":{}})),
        tags.into_iter().map(|s| s.to_string()).collect(),
        Some("admin".to_string()),
    );
    Tool::from_po_for_management(po)
}

/// 创建 FinanceDomain（复用 tool_provider_test 模式）用于工具创建
fn init_finance_env(
    _pool: SqlitePool,
) -> std::sync::Arc<dyn crate::service::domain::finance::FinanceDomain> {
    let _ = crate::config::init();

    // 一次性初始化全部 DAO。
    // 原因：finance::domain() 构造需要 dal::message_channel，而它会拉起
    // lark/wechat/slack/email/webhook/a2a_callback/user/user_credential 一串依赖，
    // 逐个补齐极易遗漏（缺任一都会在首次访问时 panic）。init_all 内部为
    // OnceLock::set 语义，重复调用安全。
    crate::service::dao::init_all();

    // 初始化 DAL：同样用全量入口，内部已按依赖顺序排列
    // （lark dal 依赖 message_channel + agent + user dal，排在最末）。
    crate::service::dal::init_all();

    crate::service::domain::finance::init();
    crate::service::domain::finance::domain()
}

/// 验证 install_tool_pack 现在允许安装神经工具包
/// （每个 Agent 显式持有 neural 工具绑定，无需加载侧再兜底）
#[sqlx::test]
async fn test_install_tool_pack_allows_neural_tag(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env_with_fs(pool);
    let agent = create_test_agent("NeuralToolAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // 发布一个 neural 工具，确保可安装
    let _t = create_enabled_tool("NeuralToolX", vec!["neural"]);

    domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "neural")
        .await
        .expect("neural 现在应允许通过工具包安装");

    // 已写入 installed_tags
    let packs = domain
        .agent_manage()
        .list_installed_tool_packs(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert!(
        packs.contains(&"neural".to_string()),
        "neural 应已进入 installed_tags"
    );
}

/// 验证 install_skill_pack 现在允许安装神经技能（每个 Agent 拥有自己的神经技能副本以便自我演进）
///
/// 设计背景：之前 neural 被 install_skill_pack 显式拒绝，导致神经技能从未以副本形式进入
/// Agent 目录，既无法加载也无法自我演进。现改为允许安装，并在 create_agent 时默认安装。
#[sqlx::test]
async fn test_install_skill_pack_allows_neural_tag(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env_with_fs(pool);
    let agent = create_test_agent("NeuralSkillAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // 发布一份带 neural 标签的技能
    let n = create_published_skill_with_tag("AgentNeural", "neural");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&n))
        .await
        .unwrap();
    let published_ids: std::collections::HashSet<String> = domain
        .skill_manage()
        .list_published_by_tag(ctx.clone(), "neural")
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.po.id)
        .collect();

    // 直接安装 neural 技能包，验证副本确实被装入且不再被拒绝
    let count = domain
        .agent_manage()
        .install_skill_pack(ctx.clone(), agent.id(), "neural")
        .await
        .expect("neural 现在应允许通过技能包安装");
    assert_eq!(count, 1, "应安装 1 份神经技能副本");

    // Agent 目录下应有对应副本（parent 指向发布的神经技能）
    let copies = domain
        .skill_manage()
        .list_for_agent(ctx.clone(), agent.id())
        .await
        .unwrap();
    assert!(
        copies
            .iter()
            .any(|c| published_ids.contains(&c.po.parent_skill_id)),
        "Agent 应持有神经技能副本"
    );
}

/// 验证 tools_overview 分组互不相交（neural 作为普通包 tag 与 search 并列；
/// 每个工具只进首个匹配包组，internal 被过滤，避免出现空包 / 重复计数）
#[sqlx::test]
async fn test_tools_overview_pack_groups_disjoint_and_priority(pool: SqlitePool) {
    let temp_dir = TempDir::new().unwrap();
    unsafe {
        std::env::set_var("AI_ORZ_BASE_PATH", temp_dir.path().to_str().unwrap());
    }
    crate::config::init().unwrap();

    let (hr_domain, ctx) = {
        crate::service::dao::agent::init();
        crate::service::dao::tool::init();
        crate::service::dao::skill::init();
        crate::service::dao::tool_call::init();
        crate::service::dao::model_provider::init();
        crate::service::dao::cortex::init();
        crate::service::dal::agent::init();
        crate::service::dal::tool::init();
        crate::service::dal::skill::init();
        crate::service::dal::model_provider::init();
        super::init();
        (domain(), new_ctx("admin", pool.clone()))
    };
    let finance = init_finance_env(pool.clone());

    // 1. 创建 Agent
    let agent = create_test_agent("OverviewAgent");
    hr_domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // 2. 创建工具：
    //    T1 = neural （应进 neural 工具包）
    //    T2 = search + neural 交叉（tags=[search,neural]，归入首个匹配的包组；neural 先装则归 neural）
    //    T3 = search 单独 （应进 search 工具包）
    //    T4 = dev + search 交叉（绑定到 agent；带 search tag，应进 search 工具包）
    //    T5 = internal （应被整体过滤，不进入任何分组）
    let t1 = create_enabled_tool("NeuralTool1", vec!["neural"]);
    let t2 = create_enabled_tool("SearchNeuralTool", vec!["search", "neural"]);
    let t3 = create_enabled_tool("SearchPackOnly", vec!["search"]);
    let t4 = create_enabled_tool("DevSearchBound", vec!["dev", "search"]);
    let t5 = create_enabled_tool("InternalHidden", vec!["internal", "neural"]);
    for t in [&t1, &t2, &t3, &t4, &t5] {
        finance
            .tool_provider_manage()
            .create_tool(ctx.clone(), t)
            .await
            .unwrap();
    }

    // 3. 将 T4 绑定到 Agent（agent_tools 关联）
    finance
        .tool_provider_manage()
        .bind_tool_to_agent(ctx.clone(), agent.id(), &t4.po.id)
        .await
        .unwrap();

    // 4. 安装工具包：先装 neural（保证 T2 归入 neural 组），再装 search
    hr_domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "neural")
        .await
        .unwrap();
    hr_domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "search")
        .await
        .unwrap();

    // 5. 获取 Agent 并装配视图
    let agent = hr_domain
        .agent_manage()
        .get_agent(ctx.clone(), agent.id(), Default::default())
        .await
        .unwrap()
        .unwrap();
    let (tool_groups, _) = hr_domain
        .agent_manage()
        .get_agent_association_groups(ctx, &agent, true, false)
        .await
        .unwrap();
    let groups = tool_groups.expect("with_tools=true 时应返回工具分组");

    // -- 断言 1：内部工具不出现在任何分组 --
    let all_pack_ids: std::collections::BTreeSet<String> = groups
        .pack_groups
        .iter()
        .flat_map(|g| g.tool_ids.iter().cloned())
        .collect();
    assert!(
        !all_pack_ids.contains(&t5.po.id),
        "internal 工具不应出现在包分组中"
    );
    assert!(
        !groups.bound_ids.contains(&t5.po.id),
        "internal 工具不应出现在 bound 分组中"
    );

    // -- 断言 2：neural 工具包组包含 T1、T2（T2 因 neural 先装而优先归入 neural）--
    let neural_group = groups
        .pack_groups
        .iter()
        .find(|g| g.tag == "neural")
        .expect("neural 分组应作为普通包组存在");
    let neural_ids: std::collections::BTreeSet<_> = neural_group.tool_ids.iter().cloned().collect();
    assert!(neural_ids.contains(&t1.po.id));
    assert!(neural_ids.contains(&t2.po.id));

    // -- 断言 3：search 工具包组包含 T3、T4（T2 已归 neural 不再重复，T4 带 search tag 进包）--
    let search_group = groups
        .pack_groups
        .iter()
        .find(|g| g.tag == "search")
        .expect("search 分组应存在");
    let search_ids: std::collections::BTreeSet<_> = search_group.tool_ids.iter().cloned().collect();
    assert!(search_ids.contains(&t3.po.id));
    assert!(search_ids.contains(&t4.po.id));
    // T2 不应出现在 search 包中（已归 neural）
    assert!(!search_ids.contains(&t2.po.id));
    // internal 工具 T5 不应出现在 search 包中
    assert!(!search_ids.contains(&t5.po.id));

    // -- 断言 4：工具分组互不相交，无重复（每个工具至多出现在一个包组内，避免数量翻倍）--
    let mut seen_any: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for g in &groups.pack_groups {
        for id in &g.tool_ids {
            assert!(
                seen_any.insert(id.clone()),
                "工具 {id} 不应在多个包组重复出现（导致关系图/列表数量翻倍）"
            );
        }
    }
    // 应有两个包组：neural 与 search
    assert_eq!(groups.pack_groups.len(), 2, "neural 与 search 各一个包组");
    // neural 不再单列分组，工具侧 neural_ids 应为空
    assert!(
        groups.neural_ids.is_empty(),
        "工具侧 neural_ids 已并入 pack_groups，应为空"
    );
    // T4 带 search tag 已进 search 包组，故 bound 为空
    assert!(
        groups.bound_ids.is_empty(),
        "T4 归属 search 包组，bound 应为空"
    );
}

/// 验证 skills_overview 三分组互不相交（neural → pack → standalone 优先级）
#[sqlx::test]
async fn test_skills_overview_three_groups_disjoint(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env_with_fs(pool);
    let agent = create_test_agent("SkillOverviewAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    // 创建 4 份已发布技能并安装
    // S1 = neural（神经技能）
    // S2 = neural + coding（因 neural 优先级，应归 neural 组，不进入 coding pack 组）
    // S3 = coding（coding pack 组）
    // S4 = 无任何 tag（standalone）
    let s1 = create_published_skill_with_tag("NeuralSkill", "neural");
    let mut s2 = create_published_skill_with_tag("NeuralCodingSkill", "coding");
    // 给 S2 追加上 neural tag：重新构造 PO tags
    let mut s2_po = s2.po.clone();
    use serde_json::json;
    s2_po.tags = json!(["neural", "coding"]).to_string();
    s2 = Skill::from_po(s2_po);
    let s3 = create_published_skill_with_tag("CodingOnly", "coding");
    let s4 = {
        let skill_po = SkillPo::new(
            format!("standalone--{}", Uuid::new_v4()),
            "StandaloneSkill".to_string(),
            "A skill without any pack tag".to_string(),
            vec![],
            "misc".to_string(),
            String::new(),
            "admin".to_string(),
            SkillAuthorType::User,
            "skills/standalone".to_string(),
        );
        let mut skill = Skill::from_po(skill_po);
        skill.po.status = SkillStatus::Published;
        skill
    };
    for s in [&s1, &s2, &s3, &s4] {
        domain
            .skill_manage()
            .create_skill(ctx.clone(), CreateSkillParams::from_skill(s))
            .await
            .unwrap();
    }

    // 安装 coding 技能包 + 单独安装 standalone + neural skill
    domain
        .agent_manage()
        .install_skill_pack(ctx.clone(), agent.id(), "coding")
        .await
        .unwrap();
    // neural 不是技能包名，而是技能个体的标签，单独安装神经技能副本
    // （S2 同时带 neural+coding，必须显式安装副本，否则移除加载兜底后将只由 neural 组可见）
    domain
        .skill_manage()
        .install_to_agent(ctx.clone(), &s1.po.id, agent.id())
        .await
        .unwrap();
    domain
        .skill_manage()
        .install_to_agent(ctx.clone(), &s2.po.id, agent.id())
        .await
        .unwrap();
    domain
        .skill_manage()
        .install_to_agent(ctx.clone(), &s4.po.id, agent.id())
        .await
        .unwrap();

    let agent = domain
        .agent_manage()
        .get_agent(ctx.clone(), agent.id(), Default::default())
        .await
        .unwrap()
        .unwrap();
    let (_, skill_groups) = domain
        .agent_manage()
        .get_agent_association_groups(ctx.clone(), &agent, false, true)
        .await
        .unwrap();
    let groups = skill_groups.expect("with_skills=true 时应返回技能分组");

    // 分组产出的是「Agent 目录下安装副本」的 ID；通过 parent_skill_id
    // 映射回原始技能 ID（S1-S4 为原始技能，安装副本的 parent 指向它们）。
    let agent_skills = domain
        .skill_manage()
        .list_for_agent(ctx, agent.id())
        .await
        .unwrap();
    let source_of = |copy_id: &str| -> String {
        agent_skills
            .iter()
            .find(|s| s.po.id == copy_id)
            .map(|s| s.po.parent_skill_id.clone())
            .unwrap_or_else(|| copy_id.to_string())
    };

    // 神经技能：S1、S2（即使 S2 也有 coding 标签，neural 优先级更高）
    let neural_ids: std::collections::BTreeSet<_> =
        groups.neural_ids.iter().map(|id| source_of(id)).collect();
    assert!(
        neural_ids.contains(&s1.po.id),
        "S1 神经技能应在 neural 组，实际={:?}",
        neural_ids
    );
    assert!(
        neural_ids.contains(&s2.po.id),
        "S2 neural+coding 应优先归 neural 组"
    );

    // coding 技能包分组：只有 S3（S2 已被 neural 组拿走）
    let coding_pack = groups
        .pack_groups
        .iter()
        .find(|g| g.tag == "coding")
        .expect("coding 技能包分组应存在");
    let coding_ids: std::collections::BTreeSet<_> = coding_pack
        .skill_ids
        .iter()
        .map(|id| source_of(id))
        .collect();
    assert!(coding_ids.contains(&s3.po.id));
    assert!(
        !coding_ids.contains(&s2.po.id),
        "S2 不应重复进入 coding 包组"
    );
    for cid in &coding_ids {
        assert!(!neural_ids.contains(cid), "coding 包与 neural 互不相交");
    }

    // standalone：S4
    let standalone_ids: std::collections::BTreeSet<_> = groups
        .standalone_ids
        .iter()
        .map(|id| source_of(id))
        .collect();
    assert!(
        standalone_ids.contains(&s4.po.id),
        "S4 应在 standalone，实际={:?}",
        standalone_ids
    );
    for sid in &standalone_ids {
        assert!(!neural_ids.contains(sid), "standalone 与 neural 互不相交");
        assert!(
            !coding_ids.contains(sid),
            "standalone 与 coding pack 互不相交"
        );
    }
}
