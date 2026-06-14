//! Tool Provider 配置管理测试
//!
//! 工具提供商配置的 CRUD 测试，属于财务领域

#[cfg(test)]
mod tests {
    use crate::models::tool::{Tool, ToolPo};
    use crate::pkg::RequestContext;
    use crate::service::domain::finance;
    use common::enums::ToolProtocol;
    use sqlx::SqlitePool;

    async fn init_test_env(
        pool: SqlitePool,
    ) -> (std::sync::Arc<dyn finance::FinanceDomain>, RequestContext) {
        // 初始化依赖的 DAO
        crate::service::dao::tool::init();
        crate::service::dao::tool_call::init();
        crate::service::dao::cortex::init();
        crate::service::dao::model_provider::init();
        crate::service::dao::message_channel::init();
        crate::service::dao::lark::init();
        crate::service::dao::wechat::init();
        crate::service::dao::slack::init();
        crate::service::dao::email::init();
        crate::service::dao::webhook::init();

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

        let result = domain
            .tool_provider_manage()
            .get_tool(ctx, "non-existent-id")
            .await;
        assert!(result.is_ok(), "get_tool should succeed even if not found");
        assert!(
            result.unwrap().is_none(),
            "should return None for non-existent tool"
        );
    }

    #[sqlx::test]
    async fn test_delete_tool(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;

        let po = ToolPo::new(
            "".to_string(),
            "domain-delete-http-tool".to_string(),
            "Domain delete tool".to_string(),
            ToolProtocol::Http,
            serde_json::json!({"endpoint":"https://example.com/tool"}),
            None,
            vec![],
            Some("test-user".to_string()),
        );
        let tool = Tool::from_po_for_management(po.clone());
        domain
            .tool_provider_manage()
            .create_tool(ctx.clone(), &tool)
            .await
            .unwrap();

        let stored = domain
            .tool_provider_manage()
            .get_tool(ctx.clone(), &po.id)
            .await
            .unwrap()
            .expect("created tool should be readable for management");

        let result = domain
            .tool_provider_manage()
            .delete_tool(ctx.clone(), &stored)
            .await;
        assert!(result.is_ok(), "delete_tool should succeed: {:?}", result);

        let got = domain
            .tool_provider_manage()
            .get_tool(ctx.clone(), &po.id)
            .await
            .unwrap();
        assert!(got.is_none(), "deleted tool should not be found");
    }
}
