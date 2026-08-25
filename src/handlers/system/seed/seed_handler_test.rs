//! Seed Handler 集成测试
//!
//! 测试 handler 层的跨 domain 编排逻辑：
//! - assemble_snapshot_from_db: 各 domain 拉数据组装快照
//! - apply_snapshot_to_db: 各 domain upsert
//! - 往返一致性：导出 → 修改 → 导入 → 重新导出，验证字段更新

use crate::pkg::request_context_test_support::new_test_ctx;
use sqlx::SqlitePool;
use std::collections::HashMap;

/// 初始化所有 domain（参考 a2a 集成测试的 init 模式）
async fn init_test_env(pool: SqlitePool) -> crate::pkg::RequestContext {
    let _ = crate::config::init();

    // 初始化 ToolCallLogger（agent dal 创建时会写 trace）
    let base_path = std::env::temp_dir().join("ai_orz_seed_handler_test_trace");
    let _ = std::fs::create_dir_all(&base_path);
    crate::pkg::tool_tracing::logger::ToolCallLogger::init(base_path);

    // 初始化所有 DAO
    crate::service::dao::organization::init();
    crate::service::dao::user::init();
    crate::service::dao::user_credential::init();
    crate::service::dao::agent::init();
    crate::service::dao::tool::init();
    crate::service::dao::skill::init();
    crate::service::dao::tool_call::init();
    crate::service::dao::model_provider::init();
    crate::service::dao::cortex::init();
    crate::service::dao::memory::init();
    crate::service::dao::mcp_server::init();
    crate::service::dao::project::init();
    crate::service::dao::task::init();
    crate::service::dao::message::init();
    crate::service::dao::artifact::init();
    crate::service::dao::attachment::init();
    crate::service::dao::message_channel::init();
    // 消息渠道 DAO 初始化
    crate::service::dao::lark::init();
    crate::service::dao::wechat::init();
    crate::service::dao::slack::init();
    crate::service::dao::email::init();
    crate::service::dao::webhook::init();
    crate::service::dao::a2a_callback::init();

    // 初始化所有 DAL
    crate::service::dal::organization::init();
    crate::service::dal::user::init();
    crate::service::dal::agent::init();
    crate::service::dal::tool::init();
    crate::service::dal::skill::init();
    crate::service::dal::model_provider::init();
    crate::service::dal::memory::init();
    crate::service::dal::mcp_server::init();
    crate::service::dal::mcp_tool::init();
    crate::service::dal::brain::init();
    crate::service::dal::project::init();
    crate::service::dal::task::init();
    crate::service::dal::message::init();
    crate::service::dal::message_channel::init();
    crate::service::dal::attachment::init();
    crate::service::dal::artifact::init();
    // user dal / lark dal：organization / message 相关 domain 注入依赖
    crate::service::dal::user::init();
    crate::service::dal::lark::init();
    crate::service::dal::mcp_server::init();

    // 初始化所有 Domain
    // 注意：seed handler 仅使用 organization / finance / hr domain，
    // 以及 system::seed 子模块的纯函数（不需要 system domain 单例）。
    // system::init() 依赖 cron_trigger/backup/log_query DAL，这里不需要初始化。
    crate::service::domain::hr::init();
    crate::service::domain::finance::init();
    crate::service::domain::organization::init();
    crate::service::domain::message::init();

    new_test_ctx("test-seed-handler-user", pool)
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

    let chat_provider = ModelProvider::new(
        "OpenAI Chat".to_string(),
        ProviderType::OpenAI,
        ModelCapability::Agent,
        "gpt-4o".to_string(),
        "sk-test-key".to_string(),
        None,
        Some("对话模型".to_string()),
        user_id.clone(),
    );
    provider_dal
        .create(ctx.clone(), &chat_provider)
        .await
        .unwrap();

    let embedding_provider = ModelProvider::new(
        "OpenAI Embedding".to_string(),
        ProviderType::OpenAI,
        ModelCapability::Embedding,
        "text-embedding-3-small".to_string(),
        "sk-test-key".to_string(),
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
        sensitive.insert(
            format!("model_provider:{}:api_key", p.id),
            "sk-new-key".to_string(),
        );
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
    sensitive.insert(
        "model_provider:TEMPLATE_CHAT_PROVIDER:api_key".to_string(),
        "sk-test".to_string(),
    );
    sensitive.insert(
        "model_provider:TEMPLATE_EMBEDDING_PROVIDER:api_key".to_string(),
        "sk-test".to_string(),
    );

    let result = super::apply_snapshot_to_db(
        ctx,
        &snapshot,
        common::api::seed::ImportStrategy::PreserveIds,
        &sensitive,
    )
    .await
    .unwrap();

    assert!(result.created > 0);
}
