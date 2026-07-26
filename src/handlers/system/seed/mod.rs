//! Seed 配置迁移 HTTP 接口
//!
//! 路由层 `require_role_middleware(UserRole::Admin)` 已确保 Admin/SuperAdmin 可进入
//! 高危操作（load/apply-default/delete）在 handler 内部二次校验 SuperAdmin
//!
//! Handler 层职责：
//! 1. 编排各 domain 拉取数据组装 SeedSnapshot（导出）
//! 2. 编排各 domain 完成 upsert（导入）
//! 3. 调用 seed 模块的纯函数（diff、validate、resolve）完成算法部分

pub mod apply_default;
pub mod delete_file;
pub mod diff;
pub mod diff_files;
pub mod get_default;
pub mod get_file;
pub mod list;
pub mod load;
pub mod save;

pub use apply_default::apply_default_handler;
pub use delete_file::delete_seed_file_handler;
pub use diff::diff_handler;
pub use diff_files::diff_files_handler;
pub use get_default::get_default_handler;
pub use get_file::get_seed_file_handler;
pub use list::list_seeds_handler;
pub use load::load_seed_handler;
pub use save::save_seed_handler;

use std::collections::HashMap;
use common::error::{Error, Result};
use crate::pkg::RequestContext;
use crate::service::domain::system::seed::defs::*;

/// 校验当前用户是否为 SuperAdmin
fn check_super_admin(ctx: &RequestContext) -> Result<()> {
    let user_role = ctx
        .user_role()
        .map(common::enums::UserRole::from_i32)
        .unwrap_or(common::enums::UserRole::Member);
    if !common::enums::UserRole::has_permission(user_role, common::enums::UserRole::SuperAdmin) {
        return Err(Error::forbidden("权限不足，仅 SuperAdmin 可执行此操作"));
    }
    Ok(())
}

/// 统计 diff 列表中 New 条目数量
fn count_new<T>(entries: &[DiffEntry<T>]) -> usize {
    entries.iter().filter(|e| matches!(e, DiffEntry::New { .. })).count()
}

/// 统计 diff 列表中 Updated 条目数量
fn count_updated<T>(entries: &[DiffEntry<T>]) -> usize {
    entries.iter().filter(|e| matches!(e, DiffEntry::Updated { .. })).count()
}

/// 从当前 DB 组装 SeedSnapshot（编排各 domain）
///
/// 调用 organization / user / finance / hr domain 拉取实体，
/// 转换为 SeedSnapshot 结构。敏感字段全部填 PENDING_INPUT。
pub async fn assemble_snapshot_from_db(
    ctx: RequestContext,
    org_id: &str,
    description: Option<String>,
) -> Result<SeedSnapshot> {
    use crate::service::domain::{finance, hr, organization};

    // 1. 组织
    let org = organization::domain()
        .organization_manage()
        .get_by_id(ctx.clone(), org_id)
        .await?
        .ok_or_else(|| Error::not_found(format!("组织不存在: {}", org_id)))?;

    let organization_def = OrganizationDef {
        id: org.id.clone(),
        name: org.name.clone(),
        description: org.description.clone(),
        base_url: org.base_url.clone(),
        status: org.status.to_i32(),
        scope: org.scope.to_i32(),
    };

    // 2. 用户
    let users = organization::domain()
        .user_manage()
        .find_by_organization_id(ctx.clone(), org_id)
        .await?;
    let user_defs: Vec<UserDef> = users.into_iter().map(|u| UserDef {
        id: u.id.clone(),
        organization_id: u.organization_id.clone(),
        username: u.username.clone(),
        display_name: u.display_name.clone(),
        email: u.email.clone(),
        password_ref: PENDING_INPUT.to_string(),
        role: u.role.to_i32(),
        status: u.status.to_i32(),
    }).collect();

    // 3. ModelProvider
    let providers = finance::domain()
        .model_provider_manage()
        .list_model_providers(ctx.clone())
        .await?;
    let provider_defs: Vec<ModelProviderDef> = providers.into_iter().map(|p| ModelProviderDef {
        id: p.po.id.clone(),
        name: p.po.name.clone(),
        provider_type: p.po.provider_type.to_i32(),
        model_name: p.po.model_name.clone(),
        capability: p.po.capability.to_i32(),
        api_key_ref: PENDING_INPUT.to_string(),
        base_url: p.po.base_url.clone(),
        description: p.po.description.clone(),
        config: p.po.config.clone(),
        status: p.po.status.to_i32(),
    }).collect();

    // 4. Agent
    let agents = hr::domain()
        .agent_manage()
        .list_agents(ctx.clone())
        .await?;
    let agent_defs: Vec<AgentDef> = agents.into_iter().map(|a| AgentDef {
        id: a.po.id.clone(),
        name: a.po.name.clone(),
        roles: a.po.get_roles(),
        description: a.po.description.clone(),
        capabilities: a.po.get_capabilities(),
        soul: a.po.soul.clone(),
        model_provider_id: a.po.model_provider_id.clone(),
        runtime_config: a.po.runtime_config.clone(),
        status: a.po.status.to_i32(),
        kind: a.po.kind.to_i32(),
    }).collect();

    // 5. Skill
    let skills = hr::domain()
        .skill_manage()
        .query_skills(ctx.clone(), Default::default())
        .await?;
    let skill_defs: Vec<SkillDef> = skills.items.into_iter().map(|s| SkillDef {
        id: s.po.id.clone(),
        name: s.po.name.clone(),
        description: s.po.description.clone(),
        tags: s.po.parse_tags(),
        category: s.po.category.clone(),
        parent_skill_id: s.po.parent_skill_id.clone(),
        author_id: s.po.author_id.clone(),
        author_type: s.po.author_type.to_i32(),
        status: s.po.status.to_i32(),
        content_path: s.po.content_path.clone(),
    }).collect();

    Ok(SeedSnapshot {
        version: SeedSnapshot::CURRENT_VERSION.to_string(),
        generated_at: common::constants::utils::current_timestamp(),
        description,
        source_organization_id: org_id.to_string(),
        organization: organization_def,
        users: user_defs,
        model_providers: provider_defs,
        agents: agent_defs,
        skills: skill_defs,
    })
}

/// 将快照应用到 DB（编排各 domain upsert）
///
/// 根据 strategy 决定行为：
/// - PreserveIds: 按 ID upsert
/// - RegenerateIds: 生成新 ID（跨组织迁移）
/// - DryRun: 由调用方处理（不调用本函数）
/// - SkipExisting: 仅创建不存在的
///
/// sensitive_values 由前端提供；INHERIT_CURRENT 时调用各 domain 拉当前 DB 值
pub async fn apply_snapshot_to_db(
    ctx: RequestContext,
    snapshot: &SeedSnapshot,
    strategy: common::api::seed::ImportStrategy,
    sensitive_values: &HashMap<String, String>,
) -> Result<common::api::seed::LoadSeedResponse> {
    use crate::service::domain::{finance, hr, organization};
    use common::api::seed::ImportStrategy;

    // 1. DryRun 直接返回 diff（不调用本函数的写入路径）
    if matches!(strategy, ImportStrategy::DryRun) {
        let current = assemble_snapshot_from_db(ctx.clone(), &snapshot.source_organization_id, None).await?;
        let diff = crate::service::domain::system::seed::diff::diff_snapshots(&current, snapshot);
        // DiffEntry<T> 的 T 因实体类型而异，无法直接 chain；分别统计后求和
        let created = count_new(&diff.users)
            + count_new(&diff.model_providers)
            + count_new(&diff.agents)
            + count_new(&diff.skills);
        let updated = count_updated(&diff.users)
            + count_updated(&diff.model_providers)
            + count_updated(&diff.agents)
            + count_updated(&diff.skills);
        return Ok(common::api::seed::LoadSeedResponse {
            created, updated, skipped: 0, diff: Some(serde_json::to_value(&diff)?),
        });
    }

    // 2. 校验敏感字段齐备
    crate::service::domain::system::seed::diff::validate_sensitive_fields(snapshot, sensitive_values)
        .map_err(Error::bad_request)?;

    let mut created = 0usize;
    let mut updated = 0usize;
    let mut skipped = 0usize;

    // 3. 写入用户
    for user_def in &snapshot.users {
        let existing = organization::domain()
            .user_manage()
            .get_user_by_id(ctx.clone(), &user_def.id)
            .await?;

        if existing.is_some() && matches!(strategy, ImportStrategy::SkipExisting) {
            skipped += 1;
            continue;
        }

        // 解析密码（INHERIT_CURRENT 时用 existing 的 password_hash）
        let current_hash = existing.as_ref().map(|u| u.password_hash.as_str());
        let password_hash = crate::service::domain::system::seed::diff::resolve_password(
            &user_def.password_ref,
            &user_def.id,
            sensitive_values,
            current_hash,
        ).map_err(Error::bad_request)?;

        let user_po = crate::models::user::UserPo {
            id: user_def.id.clone(),
            organization_id: user_def.organization_id.clone(),
            username: user_def.username.clone(),
            display_name: user_def.display_name.clone(),
            email: user_def.email.clone(),
            password_hash,
            role: common::enums::UserRole::from_i32(user_def.role),
            status: common::enums::UserStatus::from_i32(user_def.status),
            created_by: "seed_import".to_string(),
            modified_by: "seed_import".to_string(),
            created_at: common::constants::utils::current_timestamp(),
            updated_at: common::constants::utils::current_timestamp(),
        };

        if existing.is_some() {
            organization::domain().user_manage().update_user(ctx.clone(), &user_po).await?;
            updated += 1;
        } else {
            organization::domain().user_manage().create_user(ctx.clone(), user_po).await?;
            created += 1;
        }
    }

    // 4. 写入 ModelProvider
    for provider_def in &snapshot.model_providers {
        let existing = finance::domain()
            .model_provider_manage()
            .get_model_provider_with_options(ctx.clone(), &provider_def.id, Default::default())
            .await?;

        if existing.is_some() && matches!(strategy, ImportStrategy::SkipExisting) {
            skipped += 1;
            continue;
        }

        let current_api_key = existing.as_ref().map(|p| p.po.api_key.clone());
        let api_key = crate::service::domain::system::seed::diff::resolve_api_key(
            &provider_def.api_key_ref,
            &provider_def.id,
            sensitive_values,
            current_api_key.as_deref(),
        ).map_err(Error::bad_request)?;

        let mut provider = crate::models::model_provider::ModelProvider::new(
            provider_def.name.clone(),
            common::enums::ProviderType::from_i32(provider_def.provider_type),
            common::enums::ModelCapability::from_i32(provider_def.capability),
            provider_def.model_name.clone(),
            api_key,
            provider_def.base_url.clone(),
            provider_def.description.clone(),
            "seed_import".to_string(),
        );
        // 覆盖 ID 以保持引用一致
        provider.po.id = provider_def.id.clone();

        if existing.is_some() {
            finance::domain().model_provider_manage().update_model_provider(ctx.clone(), &provider).await?;
            updated += 1;
        } else {
            finance::domain().model_provider_manage().create_model_provider(ctx.clone(), &provider).await?;
            created += 1;
        }
    }

    // 5. 写入 Agent
    for agent_def in &snapshot.agents {
        let existing = hr::domain()
            .agent_manage()
            .get_agent(ctx.clone(), &agent_def.id, Default::default())
            .await?;

        if existing.is_some() && matches!(strategy, ImportStrategy::SkipExisting) {
            skipped += 1;
            continue;
        }

        let target_status = common::enums::AgentStatus::from_i32(agent_def.status);

        let mut agent_po = crate::models::agent::AgentPo::new(
            agent_def.name.clone(),
            agent_def.roles.clone(),
            agent_def.description.clone(),
            agent_def.capabilities.clone(),
            agent_def.soul.clone(),
            agent_def.model_provider_id.clone(),
            "seed_import".to_string(),
        );
        agent_po.id = agent_def.id.clone();
        agent_po.status = common::enums::AgentStatus::from_i32(agent_def.status);
        agent_po.kind = common::enums::AgentKind::from_i32(agent_def.kind);
        agent_po.runtime_config = agent_def.runtime_config.clone();
        let agent = crate::models::agent::Agent::from_po(agent_po);

        if existing.is_some() {
            hr::domain().agent_manage().update_agent(ctx.clone(), &agent).await?;
            updated += 1;
        } else {
            // seed 导入是"数据恢复"语义，需要绕过 hr domain 的"新建必须 Interviewing"校验。
            // 实现方式：先以 Interviewing 创建（满足 hr domain 不变量），再 update 覆写为目标状态。
            let mut interim = agent.clone();
            interim.po.status = common::enums::AgentStatus::Interviewing;
            hr::domain().agent_manage().create_agent(ctx.clone(), &interim).await?;

            if target_status != common::enums::AgentStatus::Interviewing {
                hr::domain().agent_manage().update_agent(ctx.clone(), &agent).await?;
            }
            created += 1;
        }
    }

    // 6. 写入 Skill（仅元数据，文件需要单独处理）
    for skill_def in &snapshot.skills {
        let existing = hr::domain()
            .skill_manage()
            .get_skill(ctx.clone(), &skill_def.id)
            .await?;

        if existing.is_some() && matches!(strategy, ImportStrategy::SkipExisting) {
            skipped += 1;
            continue;
        }

        // Skill 实体没有直接字段，需要通过 SkillPo 构造再 from_po
        let mut skill_po = crate::models::skill::SkillPo::new(
            skill_def.id.clone(),
            skill_def.name.clone(),
            skill_def.description.clone(),
            skill_def.tags.clone(),
            skill_def.category.clone(),
            skill_def.parent_skill_id.clone(),
            skill_def.author_id.clone(),
            common::enums::skill::SkillAuthorType::from(skill_def.author_type),
            skill_def.content_path.clone(),
        );
        // SkillPo::new 不设置 status，按快照覆盖
        skill_po.status = common::enums::SkillStatus::from(skill_def.status);
        let skill = crate::models::skill::Skill::from_po(skill_po);

        if existing.is_some() {
            // Skill update 接口需要 UpdateSkillParams，这里简化为不更新文件
            let params = crate::service::domain::hr::UpdateSkillParams {
                skill: &skill,
                file_writes: vec![],
                file_deletes: vec![],
                file_imports: vec![],
            };
            hr::domain().skill_manage().update_skill(ctx.clone(), params).await?;
            updated += 1;
        } else {
            hr::domain().skill_manage().create_skill(ctx.clone(), &skill).await?;
            created += 1;
        }
    }

    Ok(common::api::seed::LoadSeedResponse {
        created, updated, skipped, diff: None,
    })
}

#[cfg(test)]
mod seed_handler_test;
