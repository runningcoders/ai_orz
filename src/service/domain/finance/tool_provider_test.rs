//! Tool Provider 配置管理测试
//!
//! 工具提供商配置的 CRUD 测试，属于财务领域

#[cfg(test)]
mod tests {
    use crate::service::domain::finance;
    use crate::pkg::RequestContext;
    use sqlx::SqlitePool;

    async fn init_test_env(
        pool: SqlitePool,
    ) -> (
        std::sync::Arc<dyn finance::FinanceDomain>,
        RequestContext,
    ) {
        // 初始化依赖的 DAO
        crate::service::dao::tool::init();
        crate::service::dao::tool_call::init();
        crate::service::dao::cortex::init();
        crate::service::dao::model_provider::init();
        crate::service::dao::message_channel::init();

        // 初始化 DAL
        crate::service::dal::tool::init();
        crate::service::dal::model_provider::init();
        crate::service::dal::message_channel::init();
        crate::service::dal::brain::init();

        // 创建 Domain
        let domain = finance::new(
            crate::service::dal::model_provider::dal(),
            crate::service::dal::message_channel::dal(),
            crate::service::dal::tool::dal(),
            crate::service::dal::brain::dal(),
        );

        let ctx = RequestContext::new_simple("test-user", pool);

        (domain, ctx)
    }

    #[sqlx::test]
    async fn test_list_tools(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;

        let result = domain.tool_provider_manage().list_tools(ctx).await;
        assert!(result.is_ok(), "list_tools should succeed");
    }

    #[sqlx::test]
    async fn test_get_tool_not_found(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;

        let result = domain.tool_provider_manage().get_tool(ctx, "non-existent-id").await;
        assert!(result.is_ok(), "get_tool should succeed even if not found");
        assert!(result.unwrap().is_none(), "should return None for non-existent tool");
    }

    #[sqlx::test]
    async fn test_query_tools(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;

        let query = crate::service::dao::tool::ToolQuery::default();
        let result = domain.tool_provider_manage().query_tools(ctx, query).await;
        assert!(result.is_ok(), "query_tools should succeed");
    }
}
