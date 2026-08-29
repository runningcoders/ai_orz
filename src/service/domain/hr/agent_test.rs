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

// ==================== get_agent_association_view 单测 ====================

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

/// 验证 install_tool_pack 对 neural 保留标签显式拒绝
#[sqlx::test]
async fn test_install_tool_pack_rejects_neural_tag(pool: SqlitePool) {
    let (domain, ctx, _temp_dir) = init_test_env_with_fs(pool);
    let agent = create_test_agent("NeuralBlockedAgent");
    domain
        .agent_manage()
        .create_agent(ctx.clone(), &agent)
        .await
        .unwrap();

    let err = domain
        .agent_manage()
        .install_tool_pack(ctx.clone(), agent.id(), "neural")
        .await
        .expect_err("neural 标签应被 install_tool_pack 拒绝");
    assert_eq!(
        err.code,
        common::error::ErrorCode::InvalidRequest,
        "应返回 InvalidRequest 错误"
    );
}

/// 验证 tools_overview 三分组互不相交且与 runtime 装配同源（neural → bound → pack）
#[sqlx::test]
async fn test_tools_overview_three_groups_disjoint_and_priority(pool: SqlitePool) {
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
    //    T1 = neural （应进 neural_tools）
    //    T2 = search + neural 交叉（tags=[search,neural]，应归 neural 高优先级，不再进 pack）
    //    T3 = search 单独 （应进 search 工具包）
    //    T4 = dev + search 交叉（通过 agent_tools 显式绑定；因在 neural 中不存在，应进 bound_tools）
    //    T5 = internal （应被整体过滤）
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

    // 4. 安装 search 工具包
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
    let (tools_overview, _) = hr_domain
        .agent_manage()
        .get_agent_association_view(ctx, &agent, true, false)
        .await
        .unwrap();
    let tools_overview = tools_overview.expect("with_tools=true 时应返回 tools_overview");

    // -- 断言 1：内部工具不出现 --
    let all_ids = {
        let mut set = std::collections::BTreeSet::new();
        for t in &tools_overview.neural_tools {
            set.insert(t.id.clone());
        }
        for t in &tools_overview.bound_tools {
            set.insert(t.id.clone());
        }
        for g in &tools_overview.pack_groups {
            for t in &g.tools {
                set.insert(t.id.clone());
            }
        }
        set
    };
    assert!(
        !all_ids.contains(&t5.po.id),
        "internal 工具不应出现在 overview 中"
    );

    // -- 断言 2：neural 组包含 T1、T2（search+neural 因 neural 优先级更高）--
    let neural_ids: std::collections::BTreeSet<_> = tools_overview
        .neural_tools
        .iter()
        .map(|t| t.id.clone())
        .collect();
    assert!(neural_ids.contains(&t1.po.id));
    assert!(neural_ids.contains(&t2.po.id));

    // -- 断言 3：bound 组包含 T4 --
    let bound_ids: std::collections::BTreeSet<_> = tools_overview
        .bound_tools
        .iter()
        .map(|t| t.id.clone())
        .collect();
    assert!(bound_ids.contains(&t4.po.id));
    // 不重复
    for bid in &bound_ids {
        assert!(!neural_ids.contains(bid), "bound 与 neural 互不相交");
    }

    // -- 断言 4：search 工具包组只含 T3（T2 因已在 neural 被剔除，T4 在 bound 被剔除）--
    assert_eq!(
        tools_overview.pack_groups.len(),
        1,
        "search 包正好一个分组（neural 已被 install 拒绝，installed_tags 不含 neural）"
    );
    let search_pack = tools_overview
        .pack_groups
        .iter()
        .find(|g| g.tag == "search")
        .expect("search 分组应存在");
    let pack_ids: std::collections::BTreeSet<_> =
        search_pack.tools.iter().map(|t| t.id.clone()).collect();
    assert_eq!(pack_ids.len(), 1);
    assert!(pack_ids.contains(&t3.po.id));
    // T2 不应出现在 search 包中（neural 优先）
    assert!(!pack_ids.contains(&t2.po.id));
    // T4 不应出现在 search 包中（bound 优先）
    assert!(!pack_ids.contains(&t4.po.id));
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
    // neural 不是技能包名，而是技能个体的标签，单独安装两份神经技能
    domain
        .skill_manage()
        .install_to_agent(ctx.clone(), &s1.po.id, agent.id())
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
    let (_, skills_overview) = domain
        .agent_manage()
        .get_agent_association_view(ctx, &agent, false, true)
        .await
        .unwrap();
    let skills_overview = skills_overview.expect("with_skills=true 时应返回 skills_overview");

    // 神经技能：S1、S2（即使 S2 也有 coding 标签，neural 优先级更高）
    let neural_ids: std::collections::BTreeSet<_> = skills_overview
        .neural_skills
        .iter()
        .map(|s| s.parent_skill_id.clone())
        .collect();
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
    let coding_pack = skills_overview
        .pack_groups
        .iter()
        .find(|g| g.tag == "coding")
        .expect("coding 技能包分组应存在");
    let coding_ids: std::collections::BTreeSet<_> = coding_pack
        .skills
        .iter()
        .map(|s| s.parent_skill_id.clone())
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
    let standalone_ids: std::collections::BTreeSet<_> = skills_overview
        .standalone_skills
        .iter()
        .map(|s| s.parent_skill_id.clone())
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
