//! Model Provider Domain 单元测试

#[cfg(test)]
mod tests {
    use crate::models::model_provider::{ModelProvider, ModelProviderPo};
    use common::enums::{ModelCapability, ProviderType};
    use std::assert_matches;

    #[test]
    fn test_create_model_provider_po() {
        // 测试创建 ModelProviderPo 测试构造是否正常
        let provider = ModelProviderPo::new(
            "测试OpenAI".to_string(),
            ProviderType::OpenAI,
            ModelCapability::Agent,
            "gpt-3.5-turbo".to_string(),
            "***".to_string(),
            None,
            Some("测试用".to_string()),
            "test-user-1".to_string(),
        );

        assert_eq!(provider.name, "测试OpenAI".to_string());
        assert_matches!(provider.provider_type, ProviderType::OpenAI);
        assert_matches!(provider.capability, ModelCapability::Agent);
        assert_eq!(provider.model_name, "gpt-3.5-turbo".to_string());
        assert_eq!(provider.api_key, "***".to_string());
        assert_eq!(provider.base_url, None);
        assert_eq!(provider.description, Some("测试用".to_string()));
        assert_eq!(provider.created_by, "test-user-1".to_string());
        assert_eq!(provider.modified_by, "test-user-1".to_string());
        assert!(provider.created_at > 0);
        assert!(provider.updated_at > 0);
    }

    #[test]
    fn test_create_model_provider_from_po() {
        // 测试从 PO 转换为 Model
        let po = ModelProviderPo::new(
            "测试DeepSeek".to_string(),
            ProviderType::DeepSeek,
            ModelCapability::Agent,
            "deepseek-chat".to_string(),
            "***".to_string(),
            None,
            Some("DeepSeek 官方API".to_string()),
            "test-user-1".to_string(),
        );

        let model = ModelProvider::from_po(po);
        assert_eq!(model.po.name, "测试DeepSeek".to_string());
        assert_matches!(model.po.provider_type, ProviderType::DeepSeek);
    }

    #[test]
    fn test_create_model_provider_with_base_url() {
        // 测试带有自定义 Base URL 的创建
        let po = ModelProviderPo::new(
            "自定义OpenAI".to_string(),
            ProviderType::Custom,
            ModelCapability::Agent,
            "gpt-4".to_string(),
            "sk-custom".to_string(),
            Some("https://my-proxy.example.com/v1".to_string()),
            Some("自定义代理".to_string()),
            "test-user-1".to_string(),
        );

        assert_eq!(
            po.base_url,
            Some("https://my-proxy.example.com/v1".to_string())
        );
        let model = ModelProvider::from_po(po);
        assert_eq!(
            model.po.base_url,
            Some("https://my-proxy.example.com/v1".to_string())
        );
    }

    #[test]
    fn test_all_provider_types_serialize() {
        // 测试所有 ProviderType 都能正常构造
        let types = vec![
            ProviderType::OpenAI,
            ProviderType::Custom,
            ProviderType::DeepSeek,
            ProviderType::Doubao,
            ProviderType::Qwen,
            ProviderType::Ollama,
        ];

        // 只是确保能构造不panic，实际序列化正常
        for t in types {
            let _po = ModelProviderPo::new(
                "test".to_string(),
                t,
                ModelCapability::Agent,
                "model".to_string(),
                "key".to_string(),
                None,
                Some("".to_string()),
                "user".to_string(),
            );
        }

        // 如果走到这里就成功了
    }
}

// ==================== Decision（小脑）单启用守卫测试 ====================
// 口径（B1'-2 v2）：create 已有启用者时静默降级 Disabled（首个直接 Normal）；
// update 启用新记录时静默自动停用旧启用者（无 409 / 无 switch modal / 无重建，
// 与 AMan「随时切换无需任何额外操作」口径一致）。

#[cfg(test)]
mod decision_single_enable_guard_tests {
    use crate::models::model_provider::{ModelProvider, ModelProviderPo};
    use crate::pkg::RequestContext;
    use crate::service::domain::finance;
    use common::enums::{ModelCapability, ModelProviderStatus, ProviderType};
    use sqlx::SqlitePool;

    async fn init_test_env(
        pool: SqlitePool,
    ) -> (
        std::sync::Arc<dyn finance::FinanceDomain>,
        std::sync::Arc<dyn crate::service::dao::model_provider::ModelProviderDao>,
        RequestContext,
    ) {
        // 统一业务层初始化（config + ToolCallLogger + dao/dal/domain init_all）
        crate::pkg::request_context_test_support::init_service_for_test();

        let domain = finance::new(
            crate::service::dal::model_provider::dal(),
            crate::service::dal::message_channel::dal(),
            crate::service::dal::mcp_server::dal(),
            crate::service::dal::mcp_tool::dal(),
            crate::service::dal::tool::dal(),
            crate::service::dal::brain::dal(),
            crate::service::dal::attachment::dal(),
        );
        let dao = crate::service::dao::model_provider::dao();
        let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user-001", pool);
        (domain, dao, ctx)
    }

    fn decision_provider(name: &str, status: ModelProviderStatus) -> ModelProvider {
        let mut po = ModelProviderPo::new(
            name.to_string(),
            ProviderType::Jev,
            ModelCapability::Decision,
            "jev-latest".to_string(),
            "sk-test".to_string(),
            None,
            None,
            "test".to_string(),
        );
        po.status = status;
        ModelProvider::from_po(po)
    }

    /// 首个 Decision 直接 Normal 启用；已有启用者时新创建静默降级 Disabled
    #[sqlx::test]
    async fn test_decision_create_demotes_when_enabled_exists(pool: SqlitePool) {
        let (domain, dao, ctx) = init_test_env(pool).await;

        // 首个小脑：直接 Normal
        let first = decision_provider("小脑-A", ModelProviderStatus::Normal);
        domain
            .model_provider_manage()
            .create_model_provider(ctx.clone(), &first)
            .await
            .unwrap();
        let saved = dao
            .find_by_id(ctx.clone(), first.po.id.as_str())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            saved.status,
            ModelProviderStatus::Normal,
            "首个小脑应直接启用"
        );

        // 第二个小脑：创建不阻塞，但静默降级 Disabled
        let second = decision_provider("小脑-B", ModelProviderStatus::Normal);
        domain
            .model_provider_manage()
            .create_model_provider(ctx.clone(), &second)
            .await
            .unwrap();
        let saved_second = dao
            .find_by_id(ctx.clone(), second.po.id.as_str())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            saved_second.status,
            ModelProviderStatus::Disabled,
            "已有启用者时新创建必须降级 Disabled"
        );

        // 唯一启用者仍是 first
        let enabled = dao
            .find_enabled_decision_provider(ctx.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(enabled.id, first.po.id);
    }

    /// update 启用备用小脑时，旧启用者被静默自动停用（切换零额外操作）
    #[sqlx::test]
    async fn test_decision_update_auto_disables_previous(pool: SqlitePool) {
        let (domain, dao, ctx) = init_test_env(pool).await;

        // 场景 1：首个启用 + 备用停用（create 守卫路径）
        let first = decision_provider("小脑-A", ModelProviderStatus::Normal);
        domain
            .model_provider_manage()
            .create_model_provider(ctx.clone(), &first)
            .await
            .unwrap();
        let backup = decision_provider("小脑-B", ModelProviderStatus::Normal);
        domain
            .model_provider_manage()
            .create_model_provider(ctx.clone(), &backup)
            .await
            .unwrap();

        // 切换：update backup → Normal，first 应被静默自动停用
        let mut enable_backup = backup.clone();
        enable_backup.po.status = ModelProviderStatus::Normal;
        domain
            .model_provider_manage()
            .update_model_provider(ctx.clone(), &enable_backup)
            .await
            .unwrap();

        let first_after = dao
            .find_by_id(ctx.clone(), first.po.id.as_str())
            .await
            .unwrap()
            .unwrap();
        let backup_after = dao
            .find_by_id(ctx.clone(), backup.po.id.as_str())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            backup_after.status,
            ModelProviderStatus::Normal,
            "切换目标必须启用"
        );
        assert_eq!(
            first_after.status,
            ModelProviderStatus::Disabled,
            "旧启用者必须被静默自动停用（无 409、无 switch modal）"
        );

        // 唯一启用者=backup
        let enabled = dao
            .find_enabled_decision_provider(ctx.clone())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(enabled.id, backup.po.id);
    }

    /// update 自身保持 Normal（重复保存已启用者）不误伤：同一条记录不禁用自己
    #[sqlx::test]
    async fn test_decision_update_same_record_keeps_enabled(pool: SqlitePool) {
        let (domain, dao, ctx) = init_test_env(pool).await;

        let first = decision_provider("小脑-A", ModelProviderStatus::Normal);
        domain
            .model_provider_manage()
            .create_model_provider(ctx.clone(), &first)
            .await
            .unwrap();

        // 重复保存自身（改描述）应保持启用
        let mut renamed = first.clone();
        renamed.po.name = "小脑-A-改名".to_string();
        renamed.po.status = ModelProviderStatus::Normal;
        domain
            .model_provider_manage()
            .update_model_provider(ctx.clone(), &renamed)
            .await
            .unwrap();

        let saved = dao
            .find_by_id(ctx.clone(), first.po.id.as_str())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            saved.status,
            ModelProviderStatus::Normal,
            "同记录更新不得自停用"
        );
        assert_eq!(saved.name, "小脑-A-改名");
    }

    /// 停用启用者（update → Disabled）合法且不触发守卫副作用
    #[sqlx::test]
    async fn test_decision_update_to_disabled_is_allowed(pool: SqlitePool) {
        let (domain, dao, ctx) = init_test_env(pool).await;

        let first = decision_provider("小脑-A", ModelProviderStatus::Normal);
        domain
            .model_provider_manage()
            .create_model_provider(ctx.clone(), &first)
            .await
            .unwrap();

        let mut off = first.clone();
        off.po.status = ModelProviderStatus::Disabled;
        domain
            .model_provider_manage()
            .update_model_provider(ctx.clone(), &off)
            .await
            .unwrap();

        let saved = dao
            .find_by_id(ctx.clone(), first.po.id.as_str())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(saved.status, ModelProviderStatus::Disabled);
        assert!(
            dao.find_enabled_decision_provider(ctx.clone())
                .await
                .unwrap()
                .is_none(),
            "停用后无启用者（get_default 返回 None 为合法态）"
        );
    }
}
