//! 技能安装、副本与发布回写用例

use super::test_support::{init_test, make_m2_skill_po, new_ctx};
use crate::models::skill::{Skill, SkillPo};
use crate::service::dao::skill::SkillQuery;
use common::enums::skill::{SkillAuthorType, SkillStatus};
use common::error::Result;
use sqlx::SqlitePool;

/// 测试安装技能到 Agent（创建私有副本）
#[sqlx::test]
async fn test_install_to_agent(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建一个 Published 的源技能（共享库技能）
    let source_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", source_id);
    let mut source_po = SkillPo::new(
        source_id.clone(),
        "shared-skill".to_string(),
        "A shared skill for all agents".to_string(),
        vec!["AI Agent".to_string()],
        "shared".to_string(),
        "".to_string(),
        "system-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    source_po.status = SkillStatus::Published;
    skill_dal.create(ctx.clone(), &source_po).await?;

    // 写一点主内容
    skill_dal.write_main_content(&source_po, "# Shared Skill\n\nFor all agents.")?;

    // 安装到 Agent
    let agent_id = "agent-123";
    let installed = skill_dal
        .install_to_agent(ctx.clone(), &source_id, agent_id)
        .await?;

    // 验证：创建了新的独立副本
    assert_ne!(installed.po.id, source_id);
    assert_eq!(installed.po.author_id, agent_id);
    // 安装后变为 Draft（Agent 私有副本）
    assert_eq!(installed.po.status, SkillStatus::Draft);

    // DAL 返回完整 Skill 实体，包含安装后副本的文件列表
    assert!(!installed.files.is_empty());

    // 验证文件已复制
    let installed_content = skill_dal.read_main_content(&installed.po)?;
    assert!(!installed_content.is_empty());
    assert!(installed_content.contains("Shared Skill"));

    Ok(())
}

/// 测试安装技能到 Agent 的幂等性：同一技能安装两次，第二次不创建新副本
#[sqlx::test]
async fn test_install_to_agent_idempotent(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建一个 Published 的源技能（共享库技能）
    let source_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", source_id);
    let mut source_po = SkillPo::new(
        source_id.clone(),
        "shared-skill-idempotent".to_string(),
        "A shared skill for idempotent install test".to_string(),
        vec!["AI Agent".to_string()],
        "shared".to_string(),
        "".to_string(),
        "system-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    source_po.status = SkillStatus::Published;
    skill_dal.create(ctx.clone(), &source_po).await?;

    // 写一点主内容
    skill_dal.write_main_content(&source_po, "# Shared Skill Idempotent\n\nFor all agents.")?;

    let agent_id = "agent-idempotent";

    // 第一次安装
    let installed_first = skill_dal
        .install_to_agent(ctx.clone(), &source_id, agent_id)
        .await?;
    let first_installed_id = installed_first.po.id.clone();

    // 验证第一次安装成功
    assert_ne!(first_installed_id, source_id);
    assert_eq!(installed_first.po.author_id, agent_id);
    assert_eq!(installed_first.po.parent_skill_id, source_id);
    assert_eq!(installed_first.po.status, SkillStatus::Draft);

    // 第二次安装同一技能到同一 Agent：应跳过创建，返回已有副本
    let installed_second = skill_dal
        .install_to_agent(ctx.clone(), &source_id, agent_id)
        .await?;

    // 验证：返回的是同一个副本（ID 相同），没有创建新副本
    assert_eq!(installed_second.po.id, first_installed_id);
    assert_eq!(installed_second.po.author_id, agent_id);
    assert_eq!(installed_second.po.parent_skill_id, source_id);
    assert_eq!(installed_second.po.status, SkillStatus::Draft);

    // 验证：DAL 仍然返回完整 Skill 实体（含文件列表）
    assert!(!installed_second.files.is_empty());

    // 验证：Agent 名下 parent_skill_id = source_id 的技能只有 1 条
    use crate::service::dao::skill::SkillQuery;
    let agent_skills = skill_dal
        .query(
            ctx.clone(),
            SkillQuery {
                author_id: Some(agent_id.to_string()),
                parent_skill_id: Some(source_id.clone()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(agent_skills.items.len(), 1, "重复安装不应创建新副本");
    assert_eq!(agent_skills.items[0].po.id, first_installed_id);

    // 额外验证：另一个 Agent 安装同一源技能仍然会创建新副本（不同 Agent 互不影响）
    let other_agent_id = "agent-other";
    let installed_other = skill_dal
        .install_to_agent(ctx.clone(), &source_id, other_agent_id)
        .await?;
    assert_ne!(installed_other.po.id, first_installed_id);
    assert_eq!(installed_other.po.author_id, other_agent_id);

    Ok(())
}

/// Agent 安装「自己创建的原始技能」：技能本就在自己名下（author_id = agent_id），
/// 不应再副本化产生 parent_skill_id 指向自己的冗余行（否则提示词会出现两份同名技能）。
#[sqlx::test]
async fn test_install_to_agent_self_created_skill(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // Agent 自己创建的 Published 原始技能（parent_skill_id = ""）
    let self_skill_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("agents/agent-self/skills/{}/", self_skill_id);
    let mut self_po = SkillPo::new(
        self_skill_id.clone(),
        "self-created-skill".to_string(),
        "A skill created by the agent itself".to_string(),
        vec!["AI Agent".to_string()],
        "shared".to_string(),
        "".to_string(),
        "agent-self".to_string(),
        SkillAuthorType::Agent,
        content_path,
    );
    self_po.status = SkillStatus::Published;
    skill_dal.create(ctx.clone(), &self_po).await?;
    skill_dal.write_main_content(&self_po, "# Self Created Skill\n\nMade by the agent.")?;

    // Agent 安装自己的技能：应幂等返回源技能本体，不创建副本
    let installed = skill_dal
        .install_to_agent(ctx.clone(), &self_skill_id, "agent-self")
        .await?;
    assert_eq!(installed.po.id, self_skill_id);
    assert_eq!(installed.po.parent_skill_id, "");
    assert_eq!(installed.po.status, SkillStatus::Published);
    assert!(!installed.files.is_empty());

    // Agent 名下没有任何指向自己的副本行
    use crate::service::dao::skill::SkillQuery;
    let copies = skill_dal
        .query(
            ctx.clone(),
            SkillQuery {
                author_id: Some("agent-self".to_string()),
                parent_skill_id: Some(self_skill_id.clone()),
                ..Default::default()
            },
        )
        .await?;
    assert!(copies.items.is_empty(), "自装自建技能不应产生冗余副本");

    Ok(())
}

/// 回归测试：软删除（Expired）的旧副本不能被当作「已安装」。
///
/// 场景复现：Agent 卸载技能包（delete_copies=true）产生软删除副本后，
/// 再次安装同一技能包 —— 按新策略原地复用 Expired 副本 ID，
/// 执行状态重置（Expired→Draft）+ 元数据覆盖 + 文件字节级 diff 覆盖写，
/// 避免 (agent, parent_skill) 组合产生重复行堆积。
#[sqlx::test]
async fn test_install_to_agent_after_soft_delete_creates_fresh_copy(
    pool: SqlitePool,
) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 创建一个 Published 的源技能（共享库技能）
    let source_id = uuid::Uuid::now_v7().to_string();
    let content_path = format!("skills/{}/", source_id);
    let mut source_po = SkillPo::new(
        source_id.clone(),
        "shared-skill-reinstall".to_string(),
        "A shared skill for reinstall-after-soft-delete test".to_string(),
        vec!["AI Agent".to_string()],
        "shared".to_string(),
        "".to_string(),
        "system-author".to_string(),
        SkillAuthorType::User,
        content_path,
    );
    source_po.status = SkillStatus::Published;
    skill_dal.create(ctx.clone(), &source_po).await?;
    skill_dal.write_main_content(&source_po, "# Shared Skill Reinstall\n\nContent v1.")?;

    let agent_id = "agent-reinstall";

    // 第一次安装 → 得到 Draft 副本
    let installed_first = skill_dal
        .install_to_agent(ctx.clone(), &source_id, agent_id)
        .await?;
    let first_copy_id = installed_first.po.id.clone();
    assert_eq!(installed_first.po.status, SkillStatus::Draft);

    // 模拟卸载删除副本：软删除（status → Expired，目录同步移除）
    skill_dal.delete(ctx.clone(), &first_copy_id).await?;

    // 再次安装同一源技能：原地复用 Expired 副本（ID 不变），
    // 状态恢复为 Draft，并覆盖元数据/文件
    let installed_second = skill_dal
        .install_to_agent(ctx.clone(), &source_id, agent_id)
        .await?;
    assert_eq!(
        installed_second.po.id, first_copy_id,
        "软删除后重装应原地复用 Expired 副本 ID，避免 (agent, parent_skill) 行堆积"
    );
    assert_eq!(installed_second.po.status, SkillStatus::Draft);
    assert_eq!(installed_second.po.parent_skill_id, source_id);
    assert_eq!(installed_second.po.author_id, agent_id);
    // 元数据以源为准（name 从 source_po 同步）
    assert_eq!(installed_second.po.name, source_po.name);
    // 主文件应被恢复（删除时删了目录，重装应重建）
    let main_text = skill_dal.read_main_content(&installed_second.po)?;
    assert!(
        main_text.contains("Content v1."),
        "重装应恢复源技能主文件内容"
    );

    // find_agent_skill_copies 只返回有效副本（排除 Expired），复用后应正好 1 条
    let copies = skill_dal
        .find_agent_skill_copies(ctx.clone(), agent_id, std::slice::from_ref(&source_id))
        .await?;
    assert_eq!(
        copies.len(),
        1,
        "复用 Expired 恢复为 Draft 后应仅 1 条有效副本"
    );
    assert_eq!(copies[0].po.id, installed_second.po.id);
    assert_ne!(copies[0].po.status, SkillStatus::Expired);

    // 第三次安装仍幂等：不新建，ID 保持一致
    let installed_third = skill_dal
        .install_to_agent(ctx.clone(), &source_id, agent_id)
        .await?;
    assert_eq!(installed_third.po.id, installed_second.po.id);

    Ok(())
}

/// M2 修复 A：list_for_agent 只保留「安装副本 或 非正式发布」——
/// Agent 历史创建并发布的共享库根技能不再被计为 Agent 直接持有。
#[sqlx::test]
async fn test_list_for_agent_excludes_published_root_skill(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // ① 历史脏数据形态：Agent 创建并发布的共享库根技能（应被排除）
    let published_root_id = uuid::Uuid::now_v7().to_string();
    let published_root = make_m2_skill_po(
        &published_root_id,
        "agent-published-root",
        "agent-1",
        SkillAuthorType::Agent,
        "",
        vec!["search".to_string()],
        SkillStatus::Published,
    );
    skill_dal.create(ctx.clone(), &published_root).await?;

    // ② 安装副本（应保留）
    let copy_id = uuid::Uuid::now_v7().to_string();
    let copy = make_m2_skill_po(
        &copy_id,
        "agent-copy",
        "agent-1",
        SkillAuthorType::Agent,
        "source-skill-x",
        vec!["search".to_string()],
        SkillStatus::Draft,
    );
    skill_dal.create(ctx.clone(), &copy).await?;

    // ③ 自有草稿根技能（应保留）
    let draft_root_id = uuid::Uuid::now_v7().to_string();
    let draft_root = make_m2_skill_po(
        &draft_root_id,
        "agent-draft-root",
        "agent-1",
        SkillAuthorType::Agent,
        "",
        vec![],
        SkillStatus::Draft,
    );
    skill_dal.create(ctx.clone(), &draft_root).await?;

    // 干扰项：其他 Agent 的技能（author 过滤应排除）
    let other = make_m2_skill_po(
        &uuid::Uuid::now_v7().to_string(),
        "other-agent-copy",
        "agent-2",
        SkillAuthorType::Agent,
        "source-skill-y",
        vec![],
        SkillStatus::Draft,
    );
    skill_dal.create(ctx.clone(), &other).await?;

    let skills = skill_dal.list_for_agent(ctx, "agent-1").await?;
    let ids: Vec<&str> = skills.iter().map(|s| s.po.id.as_str()).collect();

    assert!(
        !ids.contains(&published_root_id.as_str()),
        "正式发布版根技能不应被 Agent 直接持有"
    );
    assert!(ids.contains(&copy_id.as_str()), "安装副本应保留");
    assert!(ids.contains(&draft_root_id.as_str()), "自有草稿应保留");
    assert_eq!(skills.len(), 2, "Agent 持有恰好 2 条：安装副本 + 自有草稿");

    Ok(())
}

/// M2 修复 B：list_published_by_tag 排除安装副本——
/// Published 状态的 Agent 副本（历史脏数据）不能作为共享库安装源。
#[sqlx::test]
async fn test_list_published_by_tag_excludes_copies(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 共享库正式发布根技能（tag: m2tag）
    let root_id = uuid::Uuid::now_v7().to_string();
    let root = make_m2_skill_po(
        &root_id,
        "m2-published-root",
        "user-a",
        SkillAuthorType::User,
        "",
        vec!["m2tag".to_string()],
        SkillStatus::Published,
    );
    skill_dal.create(ctx.clone(), &root).await?;

    // Published 状态的 Agent 安装副本（tag: m2tag）——应被排除
    let copy_id = uuid::Uuid::now_v7().to_string();
    let copy = make_m2_skill_po(
        &copy_id,
        "m2-published-copy",
        "agent-9",
        SkillAuthorType::Agent,
        &root_id,
        vec!["m2tag".to_string()],
        SkillStatus::Published,
    );
    skill_dal.create(ctx.clone(), &copy).await?;

    // Draft 根技能（同 tag、非 Published，也应排除）
    let draft_id = uuid::Uuid::now_v7().to_string();
    let draft = make_m2_skill_po(
        &draft_id,
        "m2-draft-root",
        "user-b",
        SkillAuthorType::User,
        "",
        vec!["m2tag".to_string()],
        SkillStatus::Draft,
    );
    skill_dal.create(ctx.clone(), &draft).await?;

    let published = skill_dal.list_published_by_tag(ctx, "m2tag").await?;
    let ids: Vec<&str> = published.iter().map(|s| s.po.id.as_str()).collect();

    assert!(ids.contains(&root_id.as_str()), "正式发布根技能应保留");
    assert!(
        !ids.contains(&copy_id.as_str()),
        "Published 安装副本不应作为安装源"
    );
    assert!(
        !ids.contains(&draft_id.as_str()),
        "非 Published 根技能不应出现"
    );

    Ok(())
}

// ==================== N2 发布回写自身技能用例 ====================

/// N2 修复：publish_writeback_copy 主路径——
/// 已发布源技能 → 为作者 Agent 生成 Draft 副本（parent=源 id / author=Agent /
/// 文件拷贝 / 向量索引补偿可查）；同时锁定 N5 复用的幂等语义（重复调用不产生
/// 第二条副本）与源缺失边界（Ok(None) 不报错）。
#[sqlx::test]
async fn test_publish_writeback_copy_creates_agent_draft_copy(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    // 已发布源技能（Agent 自有根技能发布后的共享库形态）
    let source_id = uuid::Uuid::now_v7().to_string();
    let source = make_m2_skill_po(
        &source_id,
        "doubao-search",
        "agent-1",
        SkillAuthorType::Agent,
        "",
        vec!["search".to_string()],
        SkillStatus::Published,
    );
    skill_dal.create(ctx.clone(), &source).await?;
    // 源写入 skill.md，验证副本文件全量拷贝
    skill_dal.write_main_content(&source, "# Doubao Search\n正文")?;

    // ===== 主路径：回写生成副本 =====
    let copy = skill_dal
        .publish_writeback_copy(ctx.clone(), &source_id, "agent-1")
        .await?
        .expect("首次回写应生成副本");
    assert_ne!(copy.po.id, source_id, "副本应为新 id");
    assert_eq!(copy.po.status, SkillStatus::Draft, "副本应为 Draft");
    assert_eq!(copy.po.parent_skill_id, source_id, "副本 parent 应指向源");
    assert_eq!(copy.po.author_id, "agent-1");
    assert!(matches!(copy.po.author_type, SkillAuthorType::Agent));
    assert!(
        copy.files.iter().any(|f| f.filename == "skill.md"),
        "副本应携带拷贝的 skill.md 文件"
    );

    // 向量索引补偿：副本可查到索引（Mock embed 恒可用）
    let copy_hash = skill_dal
        .get_vector_content_hash(ctx.clone(), &copy.po.id)
        .await?;
    assert!(copy_hash.is_some(), "回写副本应有向量索引补偿");

    // ===== 幂等语义（N5 补偿重放依赖）：重复调用不产生第二条副本 =====
    let second = skill_dal
        .publish_writeback_copy(ctx.clone(), &source_id, "agent-1")
        .await?;
    assert!(second.is_none(), "重复回写应幂等跳过");
    let copies = skill_dal
        .query(
            ctx.clone(),
            SkillQuery {
                author_id: Some("agent-1".to_string()),
                parent_skill_id: Some(source_id.clone()),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(copies.items.len(), 1, "重复回写不得产生第二条副本");

    // ===== 边界：源不存在 → Ok(None) 不报错（发布主流程不被阻断的前提） =====
    let missing = skill_dal
        .publish_writeback_copy(ctx, "nonexistent-source-id", "agent-1")
        .await?;
    assert!(missing.is_none(), "源缺失应返回 Ok(None)");

    Ok(())
}

/// N2 修复：Expired 副本不参与幂等查重——旧副本过期后发布回写允许重建新副本
/// （与 install_to_agent「过期副本原地恢复」的可用性语义一致）。
#[sqlx::test]
async fn test_publish_writeback_copy_rebuilds_after_expired(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("test-user", pool);

    let source_id = uuid::Uuid::now_v7().to_string();
    let source = make_m2_skill_po(
        &source_id,
        "tavily-search",
        "agent-2",
        SkillAuthorType::Agent,
        "",
        vec!["search".to_string()],
        SkillStatus::Published,
    );
    skill_dal.create(ctx.clone(), &source).await?;

    // 第一次回写 → 副本生成
    let copy1 = skill_dal
        .publish_writeback_copy(ctx.clone(), &source_id, "agent-2")
        .await?
        .expect("首次回写应生成副本");

    // 副本置为 Expired（模拟过期清理）
    let mut expired_po = copy1.po.clone();
    expired_po.status = SkillStatus::Expired;
    skill_dal
        .update(
            ctx.clone(),
            &Skill {
                po: expired_po,
                files: vec![],
                search_match: None,
            },
        )
        .await?;

    // 再次回写：Expired 副本不参与查重 → 允许重建
    let copy2 = skill_dal
        .publish_writeback_copy(ctx.clone(), &source_id, "agent-2")
        .await?
        .expect("Expired 副本不应阻断重建");
    assert_ne!(copy2.po.id, copy1.po.id, "重建应产生新副本 id");
    assert_eq!(copy2.po.status, SkillStatus::Draft);

    Ok(())
}

// ==================== 向量路接线护栏 ====================
