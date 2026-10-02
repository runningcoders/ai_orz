//! Seed Handler 集成测试
//!
//! 测试 handler 层的跨 domain 编排逻辑：
//! - assemble_snapshot_from_db: 各 domain 拉数据组装快照
//! - apply_snapshot_to_db: 各 domain upsert
//! - 往返一致性：导出 → 修改 → 导入 → 重新导出，验证字段更新

use crate::pkg::request_context_test_support::{init_service_for_test, new_test_ctx};
use common::enums::UserRole;
use sqlx::SqlitePool;
use std::collections::HashMap;

/// 初始化所有 domain（统一业务层初始化入口）
async fn init_test_env(pool: SqlitePool) -> crate::pkg::RequestContext {
    // 统一业务层初始化（config + ToolCallLogger + dao/dal/domain init_all）
    init_service_for_test();

    // 注意：Seed Handler 在生产中始终由管理员或 System 启动器调用（含创建/覆盖 TEMPLATE 内容），
    // 因此测试 ctx 直接赋予 SuperAdmin 角色，避免 HR 域资源级权限守卫拒绝写入跨用户的 TEMPLATE_* 技能等。
    let mut ctx = new_test_ctx("test-seed-handler-user", pool);
    ctx.user_role = Some(UserRole::SuperAdmin as i32);
    ctx
}

/// 准备测试数据：1 个组织 + 1 个 SuperAdmin + 1 个 chat provider + 1 个 embedding provider + 1 个 Agent
async fn prepare_test_data(ctx: &crate::pkg::RequestContext) -> String {
    use crate::models::agent::{Agent, AgentPo};
    use crate::models::model_provider::ModelProvider;
    use crate::models::organization::OrganizationPo;
    use crate::models::user::UserPo;
    use common::enums::{AgentStatus, ModelCapability, ProviderType, UserRole};

    let org_dal = crate::service::dal::organization::dal();
    let user_dal = crate::service::dal::user::dal();
    let provider_dal = crate::service::dal::model_provider::dal();
    let agent_dal = crate::service::dal::agent::dal();

    let org_id = "TESTORG0001".to_string();
    let org = OrganizationPo::new(
        org_id.clone(),
        "测试组织".to_string(),
        "测试用组织".to_string(),
        None,
        org_id.clone(),
    );
    org_dal.create(ctx.clone(), &org).await.unwrap();

    let user_id = "TESTUSER000000001".to_string();
    let user = UserPo::new(
        user_id.clone(),
        org_id.clone(),
        "admin".to_string(),
        "管理员".to_string(),
        "admin@test.com".to_string(),
        "hashed_pwd".to_string(),
        UserRole::SuperAdmin,
        user_id.clone(),
    );
    user_dal.create(ctx.clone(), &user).await.unwrap();

    let mut chat_provider = ModelProvider::new(
        "OpenAI Chat".to_string(),
        ProviderType::OpenAI,
        ModelCapability::Agent,
        "gpt-4o".to_string(),
        String::new(),
        None,
        Some("对话模型".to_string()),
        user_id.clone(),
    );
    // 对话模型必须带上下文长度：seed 导入会拒绝缺失该字段的快照
    // （validate_provider_context_length），测试数据需与之对齐。
    chat_provider
        .po
        .set_config(&crate::models::model_provider::ModelProviderConfig {
            max_context_length: Some(128_000),
            ..Default::default()
        });
    provider_dal
        .create(ctx.clone(), &chat_provider)
        .await
        .unwrap();

    let embedding_provider = ModelProvider::new(
        "OpenAI Embedding".to_string(),
        ProviderType::OpenAI,
        ModelCapability::Embedding,
        "text-embedding-3-small".to_string(),
        String::new(),
        None,
        Some("向量模型".to_string()),
        user_id.clone(),
    );
    provider_dal
        .create(ctx.clone(), &embedding_provider)
        .await
        .unwrap();

    let mut agent_po = AgentPo::new(
        "前台 Agent".to_string(),
        vec!["feishu_reception".to_string()],
        "前台接待".to_string(),
        vec!["chat".to_string()],
        "测试灵魂".to_string(),
        chat_provider.po.id.clone(),
        user_id.clone(),
    );
    agent_po.id = format!("reception-{}", uuid::Uuid::now_v7());
    agent_po.status = AgentStatus::Onboarded;
    let agent = Agent::from_po(agent_po);
    agent_dal.create(ctx.clone(), &agent).await.unwrap();

    org_id
}

#[sqlx::test]
async fn test_assemble_snapshot_from_db_returns_valid_structure(pool: SqlitePool) {
    let ctx = init_test_env(pool).await;
    let org_id = prepare_test_data(&ctx).await;

    let snapshot = super::assemble_snapshot_from_db(ctx, &org_id, Some("测试".to_string()))
        .await
        .unwrap();

    assert_eq!(
        snapshot.version,
        crate::service::domain::system::seed::defs::SeedSnapshot::CURRENT_VERSION
    );
    assert_eq!(snapshot.organization.id, org_id);
    assert_eq!(snapshot.users.len(), 1);
    assert_eq!(snapshot.model_providers.len(), 2);
    assert_eq!(snapshot.agents.len(), 1);
    // 词表段接线：测试库无词表数据 → 空词表（export_lexicon 查询走通即通过）
    assert!(snapshot.ontology.is_empty());
    assert_eq!(
        snapshot.users[0].password_ref,
        crate::service::domain::system::seed::defs::PENDING_INPUT
    );
}

#[sqlx::test]
async fn test_apply_snapshot_with_preserve_ids_round_trip(pool: SqlitePool) {
    let ctx = init_test_env(pool).await;
    let org_id = prepare_test_data(&ctx).await;

    // 导出
    let snapshot = super::assemble_snapshot_from_db(ctx.clone(), &org_id, None)
        .await
        .unwrap();

    // 提供敏感字段
    let mut sensitive = HashMap::new();
    for u in &snapshot.users {
        sensitive.insert(
            format!("user:{}:password", u.id),
            "new_hashed_pwd".to_string(),
        );
    }
    for p in &snapshot.model_providers {
        sensitive.insert(format!("model_provider:{}:api_key", p.id), String::new());
    }

    // 修改快照模拟配置更新
    let mut modified = snapshot.clone();
    modified.agents[0].name = "修改后的 Agent".to_string();

    // 导入
    let result = super::apply_snapshot_to_db(
        ctx,
        &modified,
        common::api::seed::ImportStrategy::PreserveIds,
        &sensitive,
    )
    .await
    .unwrap();

    // apply_snapshot_to_db 对快照中每个已存在的实体执行 upsert，
    // 因此 1 user + 2 providers + 1 agent = 4 个 updated（不是仅 Agent）
    assert_eq!(result.updated, 4);
    assert_eq!(result.created, 0);
}

#[sqlx::test]
async fn test_apply_snapshot_dry_run_returns_diff_without_writing(pool: SqlitePool) {
    let ctx = init_test_env(pool).await;
    let org_id = prepare_test_data(&ctx).await;

    let snapshot = super::assemble_snapshot_from_db(ctx.clone(), &org_id, None)
        .await
        .unwrap();

    let result = super::apply_snapshot_to_db(
        ctx,
        &snapshot,
        common::api::seed::ImportStrategy::DryRun,
        &HashMap::new(),
    )
    .await
    .unwrap();

    assert!(result.diff.is_some());
    assert_eq!(result.created, 0); // DryRun 不写入
}

#[sqlx::test]
async fn test_apply_default_template_creates_template_entities(pool: SqlitePool) {
    let ctx = init_test_env(pool).await;
    // 注意：默认模板的 organization_id="TEMPLATE_ORG"，需要先创建组织
    use crate::models::organization::OrganizationPo;
    let org = OrganizationPo::new(
        "TEMPLATE_ORG".to_string(),
        "模板组织".to_string(),
        "测试".to_string(),
        None,
        "TEMPLATE_ORG".to_string(),
    );
    crate::service::dal::organization::dal()
        .create(ctx.clone(), &org)
        .await
        .unwrap();

    let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();

    let mut sensitive = HashMap::new();
    sensitive.insert(
        "user:TEMPLATE_ADMIN:password".to_string(),
        "hashed".to_string(),
    );
    // 模板 provider 的 api_key 注入空字符串：测试环境无真实模型凭据，
    // 空 key 会在 cortex DAO 层被前置校验快速拒绝（ConfigInvalid），
    // 向量化作为弱依赖降级，不影响实体创建主流程，也避免发起真实 HTTP 导致长时挂起。
    sensitive.insert(
        "model_provider:TEMPLATE_CHAT_PROVIDER:api_key".to_string(),
        String::new(),
    );
    sensitive.insert(
        "model_provider:TEMPLATE_EMBEDDING_PROVIDER:api_key".to_string(),
        String::new(),
    );

    let result = super::apply_snapshot_to_db(
        ctx.clone(),
        &snapshot,
        common::api::seed::ImportStrategy::PreserveIds,
        &sensitive,
    )
    .await
    .unwrap();

    assert!(result.created > 0);

    // 模板里的对话模型必须把 config 一并落库（含 max_context_length）。
    // 漏掉这步会让 seed 导入的 Provider 退化成 threshold=0 —— ContextOverflowPolicy
    // 恒不命中，Agent 永不压缩上下文。
    let created_provider = crate::service::domain::finance::domain()
        .model_provider_manage()
        .get_model_provider(ctx, "TEMPLATE_CHAT_PROVIDER")
        .await
        .unwrap()
        .expect("TEMPLATE_CHAT_PROVIDER 应已创建");
    assert_eq!(
        created_provider.po.config().max_context_length,
        Some(128_000),
        "默认模板的对话模型应带上 max_context_length"
    );
}

/// 导入快照时，对话类模型缺 `max_context_length` 必须 fail-fast 拒绝。
///
/// 缺该字段的 Provider 落库后 threshold=0 → Agent 永不压缩上下文，等同模型信息不完整，
/// 因此与"缺敏感字段"同级拦截。
#[sqlx::test]
async fn test_apply_snapshot_rejects_chat_provider_without_context_length(pool: SqlitePool) {
    use common::enums::ModelCapability;

    let ctx = init_test_env(pool).await;
    let org_id = prepare_test_data(&ctx).await;

    let mut snapshot = super::assemble_snapshot_from_db(ctx.clone(), &org_id, None)
        .await
        .unwrap();

    // 抹掉对话模型的 config，模拟外部快照漏填
    for p in &mut snapshot.model_providers {
        if p.capability == ModelCapability::Agent as i32 {
            p.config = "{}".to_string();
        }
    }

    // 注意：此处 sensitive_values 为空——快照中的实体均已存在，敏感字段校验应放行，
    // 从而让失败原因唯一地落在上下文长度校验上。
    let err = super::apply_snapshot_to_db(
        ctx,
        &snapshot,
        common::api::seed::ImportStrategy::PreserveIds,
        &HashMap::new(),
    )
    .await
    .unwrap_err();

    assert!(
        err.to_string().contains("max_context_length"),
        "应因缺少 max_context_length 被拒绝，实际错误: {}",
        err
    );
}

/// 预置 Agent 同步：缺失新建（不绑模型，停留 Incubating）→ 在用跳过/覆盖（模型绑定保留）
/// → 软删恢复（模型绑定保留本地现值，不被 seed 占位符冲掉）
#[sqlx::test]
async fn test_apply_single_preset_agent(pool: SqlitePool) {
    use crate::handlers::system::seed::{PresetAgentAction, apply_single_preset_agent};
    use crate::service::domain::finance;
    use crate::service::domain::hr;
    use common::enums::AgentStatus;

    let ctx = init_test_env(pool).await;
    let _org_id = prepare_test_data(&ctx).await;

    let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
    let agent_def = snapshot
        .agents
        .iter()
        .find(|a| a.id == "TEMPLATE_RECEPTION_AGENT")
        .expect("seed 应包含 TEMPLATE_RECEPTION_AGENT");

    // 1. 完全缺失 + OnlyMissing → 新建；模型绑定不在同步范围内 → 不绑模型，停留 Incubating
    let action = apply_single_preset_agent(ctx.clone(), agent_def, true)
        .await
        .unwrap();
    assert_eq!(action, PresetAgentAction::Created);
    let agent = hr::domain()
        .agent_manage()
        .get_agent(ctx.clone(), &agent_def.id, Default::default())
        .await
        .unwrap()
        .expect("新建的预置 Agent 应可见");
    assert_eq!(agent.po.status, AgentStatus::Incubating);
    assert!(
        agent.po.model_provider_id.is_empty(),
        "同步不负责模型绑定，新建的 Agent 不应自动绑模型"
    );

    // 2. 在用 + OnlyMissing → 跳过
    let action = apply_single_preset_agent(ctx.clone(), agent_def, true)
        .await
        .unwrap();
    assert_eq!(action, PresetAgentAction::Skipped);

    // 给该 Agent 配一个本地模型（模拟用户自行配置），验证后续覆盖/恢复都不冲掉它
    let chat_provider = finance::domain()
        .model_provider_manage()
        .list_model_providers(ctx.clone())
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.po.capability == common::enums::ModelCapability::Agent)
        .expect("prepare_test_data 应已创建 chat provider");
    let mut with_model = agent.clone();
    with_model.po.model_provider_id = chat_provider.po.id.clone();
    hr::domain()
        .agent_manage()
        .update_agent(ctx.clone(), &with_model)
        .await
        .unwrap();

    // 3. 在用 + Overwrite → 基础身份字段对齐 seed；生命周期状态与模型绑定不动
    let action = apply_single_preset_agent(ctx.clone(), agent_def, false)
        .await
        .unwrap();
    assert_eq!(action, PresetAgentAction::Updated);
    let agent = hr::domain()
        .agent_manage()
        .get_agent(ctx.clone(), &agent_def.id, Default::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(agent.po.name, agent_def.name);
    assert_eq!(agent.po.soul, agent_def.soul);
    assert_eq!(agent.po.status, AgentStatus::Incubating);
    assert_eq!(
        agent.po.model_provider_id, chat_provider.po.id,
        "覆盖不应冲掉用户本地配置的模型"
    );

    // 4. 软删（误删）+ OnlyMissing → 恢复；状态回 seed 定义，模型绑定保留本地现值
    hr::domain()
        .agent_manage()
        .delete_agent(ctx.clone(), &agent)
        .await
        .unwrap();
    assert!(
        hr::domain()
            .agent_manage()
            .get_agent(ctx.clone(), &agent_def.id, Default::default())
            .await
            .unwrap()
            .is_none(),
        "软删后 get_agent 应不可见"
    );
    let action = apply_single_preset_agent(ctx.clone(), agent_def, true)
        .await
        .unwrap();
    assert_eq!(action, PresetAgentAction::Restored);
    let agent = hr::domain()
        .agent_manage()
        .get_agent(ctx.clone(), &agent_def.id, Default::default())
        .await
        .unwrap()
        .expect("恢复后的预置 Agent 应可见");
    assert_eq!(
        agent.po.status,
        AgentStatus::from_i32(agent_def.status),
        "恢复后状态应回到 seed 定义（PendingOnboard）"
    );
    assert_eq!(agent.po.name, agent_def.name);
    assert_eq!(
        agent.po.model_provider_id, chat_provider.po.id,
        "恢复不应冲掉软删行原有的模型绑定"
    );
}

/// 预置词表同步守卫①：Overwrite 覆写生效且永不触碰边界成立
///
/// 覆盖路径逐点断言（TL 裁定口径）：业务字段（display_name / direction）按 seed 覆写；
/// term_key / id / status / created_at 保持本地原值；退役行整体跳过不覆写不复活；
/// updated 计数 = 执行覆写动作的条目数（非实际字段变化数）；preview retired_count
/// 冒烟对账。随 seed 的 21 关系词全量入库后，后续 OnlyMissing 复调 created=0
/// 幂等性同链验证。
#[sqlx::test]
async fn test_sync_preset_ontology_overwrite_strategy(pool: SqlitePool) {
    use crate::handlers::system::seed::sync_preset_ontology::{
        preview_preset_ontology, sync_preset_ontology,
    };
    use crate::service::domain::hr;
    use common::api::ontology::{PresetOntologySyncStrategy, SyncPresetOntologyRequest};

    let ctx = init_test_env(pool).await;
    let _org_id = prepare_test_data(&ctx).await;

    // 注入一版本地值（模拟管理页本地修改），为后续 Overwrite 断言提供基线
    let domain = hr::domain();
    let class_domain = domain.ontology_domain();
    class_domain
        .create_class(
            ctx.clone(),
            &crate::models::ontology::OntologyClass::from_po(
                crate::models::ontology::OntologyClassPo::new(
                    "agent",
                    "本地展示名 Agent",
                    "本地描述（将被 seed 覆写）",
                    "[]",
                ),
            ),
        )
        .await
        .unwrap();

    // Overwrite 全量注入 seed
    let report = sync_preset_ontology(
        ctx.clone(),
        SyncPresetOntologyRequest {
            strategy: PresetOntologySyncStrategy::Overwrite,
        },
    )
    .await
    .unwrap();
    assert!(report.created > 0, "缺失条目应被新建");
    assert!(
        report.updated > 0,
        "本地已有条目应执行覆写动作（updated 计数口径=动作数）"
    );

    // 本地修改被 seed 值覆写 + term_key / created_at / id 保持本地原值
    let class = crate::service::dal::ontology::dal()
        .find_class_by_term_key(ctx.clone(), "agent")
        .await
        .unwrap()
        .expect("agent 应已入库");
    assert_eq!(class.po.display_name, "智能体");
    assert_eq!(class.po.description, "具备大脑与认知能力的自主执行实体");

    // 关系词 direction 随 seed 落库（related=undirected / contains=directed）
    let related = crate::service::dal::ontology::dal()
        .find_relation_type_by_term_key(ctx.clone(), "related")
        .await
        .unwrap()
        .expect("related 应已入库");
    assert_eq!(related.po.direction, "undirected");
    let contains = crate::service::dal::ontology::dal()
        .find_relation_type_by_term_key(ctx.clone(), "contains")
        .await
        .unwrap()
        .expect("contains 应已入库");
    assert_eq!(contains.po.direction, "directed");
    // seed 展示名同步覆写（contains 本地原为「包含」，仅补缺注入不产生差异）
    assert_eq!(contains.po.display_name, "包含");

    // 退役行整体跳过：retire 后再次 Overwrite，updated 不含该行且状态保持 Retired
    hr::domain()
        .ontology_domain()
        .retire_relation_type(ctx.clone(), &related.po.id)
        .await
        .unwrap();
    let before = crate::service::dal::ontology::dal()
        .find_relation_type_by_term_key(ctx.clone(), "related")
        .await
        .unwrap()
        .unwrap();
    let report2 = sync_preset_ontology(
        ctx.clone(),
        SyncPresetOntologyRequest {
            strategy: PresetOntologySyncStrategy::Overwrite,
        },
    )
    .await
    .unwrap();
    let after = crate::service::dal::ontology::dal()
        .find_relation_type_by_term_key(ctx.clone(), "related")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.po.status, common::enums::OntologyStatus::Retired);
    assert_eq!(after.po.display_name, before.po.display_name);
    assert_eq!(after.po.direction, before.po.direction);

    // preview retired_count 冒烟：seed 21 词中仅 related 退役
    let preview = preview_preset_ontology(ctx.clone(), Default::default())
        .await
        .unwrap();
    assert_eq!(preview.retired_count, 1);

    // 同链幂等性：同一策略复调，created=0（缺失条目上一轮已建齐）
    assert_eq!(report2.created, 0, "重复 Overwrite 不应重复新建");
}

/// 预置词表同步守卫②：缺省（不传 strategy）与历史行为逐字节一致
///
/// serde default=OnlyMissing 兼容守卫：空请求体走仅补缺路径，已存在条目
/// 跳过、无覆写动作（updated=0），审计与响应计数形态与历史一致。
#[sqlx::test]
async fn test_sync_preset_ontology_default_is_only_missing(pool: SqlitePool) {
    use crate::handlers::system::seed::sync_preset_ontology::sync_preset_ontology;
    use common::api::ontology::SyncPresetOntologyRequest;

    let ctx = init_test_env(pool).await;
    let _org_id = prepare_test_data(&ctx).await;

    // 空请求体（serde default 生效）→ 仅补缺注入 seed
    let report = sync_preset_ontology(ctx.clone(), SyncPresetOntologyRequest::default())
        .await
        .unwrap();
    assert!(report.created > 0);
    assert_eq!(report.updated, 0, "缺省策略不得产生覆写动作");
    assert!(report.skipped == 0, "空库仅补缺首轮无跳过");
    assert_eq!(
        report.created + report.skipped,
        report.total,
        "created + skipped 应对账 seed 总数"
    );

    // 复调幂等：全部计入 skipped，created=updated=0
    let report2 = sync_preset_ontology(ctx.clone(), SyncPresetOntologyRequest::default())
        .await
        .unwrap();
    assert_eq!(report2.created, 0);
    assert_eq!(report2.updated, 0);
    assert_eq!(report2.skipped, report2.total);
}
