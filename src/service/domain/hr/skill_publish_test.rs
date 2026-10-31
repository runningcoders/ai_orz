//! 技能发布入口（`publish_skill`）的契约测试。
//!
//! 发布从 `update_skill` 里拆出来，是为了把「状态跃迁 + 回写副作用」收进单一入口。
//! 本文件固化拆分后必须成立的四类性质：
//! - **正路**：Draft → Published，Agent 自有根技能额外回写作者 Agent 的工作副本；
//! - **幂等**：重复发布不报错、不重复回写；
//! - **入口唯一**：`update_skill` 拒绝「非 Published → Published」的跃迁，但仍允许
//!   对**已是 Published** 的技能刷新内容（系统 seed 的预置同步依赖这一点）；
//! - **边界**：Agent 上下文、Agent 安装副本、Expired 技能一律拒绝。

use super::skill_test::{create_test_skill, init_test_env};
use super::{CreateSkillParams, UpdateSkillParams};
use crate::pkg::RequestContext;
use common::enums::SkillStatus;
use common::enums::skill::SkillAuthorType;
use sqlx::SqlitePool;

/// 正路：Draft → Published，状态真正落库。
#[sqlx::test]
async fn publish_transitions_draft_to_published(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let skill = create_test_skill("PublishDraft");
    assert_eq!(skill.po.status, SkillStatus::Draft, "新建技能默认应为草稿");

    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    let published = domain
        .skill_manage()
        .publish_skill(ctx.clone(), skill.id())
        .await
        .expect("草稿发布应成功");
    assert_eq!(published.po.status, SkillStatus::Published);

    let reloaded = domain
        .skill_manage()
        .get_skill(ctx, skill.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.po.status, SkillStatus::Published, "发布必须落库");
}

/// 正路副作用：Agent 自有根技能发布后，为作者 Agent 回写一条 Draft 工作副本。
#[sqlx::test]
async fn publish_agent_root_skill_writes_back_work_copy(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let mut skill = create_test_skill("AgentRootPublish");
    skill.po.author_type = SkillAuthorType::Agent;
    skill.po.author_id = "agent-pub".to_string();
    skill.po.parent_skill_id = String::new();

    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    // 发布由「Agent 创建者」在人侧执行（Admin 旁路），而不是 Agent 上下文
    let human_ctx = ctx
        .to_builder()
        .user_id("creator".to_string())
        .user_role(1)
        .build();
    domain
        .skill_manage()
        .publish_skill(human_ctx, skill.id())
        .await
        .expect("Agent 根技能应可由其创建者发布");

    let copies = domain
        .skill_manage()
        .list_for_agent(ctx, "agent-pub")
        .await
        .unwrap();
    let copy = copies
        .iter()
        .find(|s| s.po.parent_skill_id == skill.id())
        .expect("发布回写应为作者 Agent 生成本地工作副本");
    assert_eq!(copy.po.status, SkillStatus::Draft, "回写副本应为 Draft");
    assert_eq!(copy.po.author_id, "agent-pub");
}

/// 幂等：重复发布不报错，也不产生第二份回写副本。
#[sqlx::test]
async fn publish_is_idempotent(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let mut skill = create_test_skill("IdempotentPublish");
    skill.po.author_type = SkillAuthorType::Agent;
    skill.po.author_id = "agent-idem".to_string();

    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    // 作者是 Agent：发布由人侧执行（Admin 旁路）
    let publisher = ctx
        .to_builder()
        .user_id("publisher".to_string())
        .user_role(1)
        .build();
    domain
        .skill_manage()
        .publish_skill(publisher.clone(), skill.id())
        .await
        .unwrap();
    let again = domain
        .skill_manage()
        .publish_skill(publisher, skill.id())
        .await
        .expect("重复发布应幂等成功");
    assert_eq!(again.po.status, SkillStatus::Published);

    let copies = domain
        .skill_manage()
        .list_for_agent(ctx, "agent-idem")
        .await
        .unwrap();
    assert_eq!(
        copies
            .iter()
            .filter(|s| s.po.parent_skill_id == skill.id())
            .count(),
        1,
        "重复发布不应产生第二份回写副本"
    );
}

/// 边界①：Agent 上下文即使作者是自身，也不允许发布（发布属共享库治理）。
#[sqlx::test]
async fn publish_rejects_agent_context(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let mut skill = create_test_skill("AgentSelfPublish");
    skill.po.author_type = SkillAuthorType::Agent;
    skill.po.author_id = "agent-self".to_string();

    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    let agent_ctx: RequestContext = ctx.to_builder().agent_id("agent-self".to_string()).build();
    let err = domain
        .skill_manage()
        .publish_skill(agent_ctx, skill.id())
        .await
        .expect_err("Agent 上下文必须被拒绝");
    assert!(
        err.to_string().contains("不允许发布"),
        "报错应说明 Agent 不能发布: {err}"
    );

    let reloaded = domain
        .skill_manage()
        .get_skill(ctx, skill.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        reloaded.po.status,
        SkillStatus::Draft,
        "被拒绝的发布不得改动状态"
    );
}

/// 边界②：Agent 安装副本（parent 非空）不能发布，必须回到根技能。
#[sqlx::test]
async fn publish_rejects_agent_install_copy(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let mut skill = create_test_skill("AgentCopyPublish");
    skill.po.author_type = SkillAuthorType::Agent;
    skill.po.author_id = "agent-copy".to_string();
    skill.po.parent_skill_id = "root-skill-id".to_string();

    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    // 作者是 Agent 且当前 ctx 既非作者也非创建者：用 Admin 旁路，确保测的是
    // 「安装副本不能发布」而不是先被权限摔掉。
    let admin = ctx
        .to_builder()
        .user_id("publisher".to_string())
        .user_role(1)
        .build();
    let err = domain
        .skill_manage()
        .publish_skill(admin, skill.id())
        .await
        .expect_err("安装副本不能发布");
    assert!(
        err.to_string().contains("安装副本"),
        "报错应指出这是安装副本: {err}"
    );
}

/// 边界③：Expired 必须先 restore 回 Draft，不允许直接复活成 Published。
#[sqlx::test]
async fn publish_rejects_expired_skill(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let skill = create_test_skill("ExpiredPublish");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    let mut expired = skill.clone();
    expired.po.status = SkillStatus::Expired;
    expired.po.updated_at += 1;
    domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &expired,
                imports: Vec::new(),
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .unwrap();

    let err = domain
        .skill_manage()
        .publish_skill(ctx, skill.id())
        .await
        .expect_err("过期技能不能直接发布");
    assert!(
        err.to_string().contains("restore_skill"),
        "报错应指引先恢复: {err}"
    );
}

/// 入口唯一：`update_skill` 拒绝 Draft → Published 的跃迁（否则可绕过回写）。
#[sqlx::test]
async fn update_skill_rejects_publish_transition(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let skill = create_test_skill("UpdateCantPublish");
    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();

    let mut target = skill.clone();
    target.po.status = SkillStatus::Published;
    target.po.updated_at += 1;
    let err = domain
        .skill_manage()
        .update_skill(
            ctx.clone(),
            UpdateSkillParams {
                skill: &target,
                imports: Vec::new(),
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .expect_err("update 不允许把技能改为 Published");
    assert!(
        err.to_string().contains("publish_skill"),
        "报错应指引改用发布接口: {err}"
    );

    let reloaded = domain
        .skill_manage()
        .get_skill(ctx, skill.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        reloaded.po.status,
        SkillStatus::Draft,
        "被拒绝的跃迁不得改动状态"
    );
}

/// 但 `update_skill` 必须继续允许**已是 Published** 的技能刷新内容/元数据。
///
/// 系统 seed 的预置技能同步走 `update_skill`，技能本来就是 Published（状态不变）；
/// 若按「值」拦而不是按「跃迁」拦，整条预置同步链路会直接断掉。
#[sqlx::test]
async fn update_skill_still_allows_content_refresh_on_published_skill(pool: SqlitePool) {
    let (domain, ctx, _temp) = init_test_env(pool.clone());
    let mut skill = create_test_skill("PublishedRefresh");
    skill.po.author_type = SkillAuthorType::Agent;
    skill.po.author_id = "agent-refresh".to_string();

    domain
        .skill_manage()
        .create_skill(ctx.clone(), CreateSkillParams::from_skill(&skill))
        .await
        .unwrap();
    // 作者是 Agent：走 Admin 旁路发布（用户侧治理动作）
    let publisher = ctx
        .to_builder()
        .user_id("publisher".to_string())
        .user_role(1)
        .build();
    domain
        .skill_manage()
        .publish_skill(publisher.clone(), skill.id())
        .await
        .unwrap();

    let mut refreshed = skill.clone();
    refreshed.po.status = SkillStatus::Published; // 状态不变
    refreshed.po.name = "PublishedRefreshV2".to_string();
    refreshed.po.updated_at += 1;
    domain
        .skill_manage()
        .update_skill(
            publisher.clone(),
            UpdateSkillParams {
                skill: &refreshed,
                imports: Vec::new(),
                file_deletes: Vec::new(),
                remote_source: None,
            },
        )
        .await
        .expect("已发布技能的内容刷新不应被发布守卫拦住");

    let reloaded = domain
        .skill_manage()
        .get_skill(publisher, skill.id())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(reloaded.po.name, "PublishedRefreshV2");
    assert_eq!(reloaded.po.status, SkillStatus::Published);
}
