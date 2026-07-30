//! HR Domain Agent 管理单元测试

use super::{HrDomain, domain};
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
        .create_skill(ctx.clone(), &skill1)
        .await
        .unwrap();
    domain
        .skill_manage()
        .create_skill(ctx.clone(), &skill2)
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
        .create_skill(ctx.clone(), &skill)
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
        .create_skill(ctx.clone(), &skill)
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
        .create_skill(ctx.clone(), &skill1)
        .await
        .unwrap();
    domain
        .skill_manage()
        .create_skill(ctx.clone(), &skill2)
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
        .create_skill(ctx.clone(), &skill)
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
                file_writes: Vec::new(),
                file_deletes: Vec::new(),
                file_imports: Vec::new(),
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
