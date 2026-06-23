//! Tool Provider 配置管理测试
//!
//! 工具提供商配置的 CRUD 测试，属于财务领域

#[cfg(test)]
mod tests {
    use crate::models::tool::{Tool, ToolPo};
    use crate::pkg::RequestContext;
    use crate::service::domain::finance;
    use common::enums::{ControlMode, ToolProtocol};
    use serde_json::{Value, json};
    use sqlx::SqlitePool;

    fn valid_http_config() -> Value {
        json!({
            "method": "GET",
            "url": "https://api.example.com/search",
            "timeout_ms": 1000,
            "response_max_bytes": 1024
        })
    }

    fn http_management_tool(name: &str) -> Tool {
        let po = ToolPo::new(
            String::new(),
            name.to_string(),
            "Domain HTTP tool".to_string(),
            ToolProtocol::Http,
            valid_http_config(),
            None,
            vec![],
            Some("test-user".to_string()),
        );
        Tool::from_po_for_management(po)
    }

    async fn init_test_env(
        pool: SqlitePool,
    ) -> (std::sync::Arc<dyn finance::FinanceDomain>, RequestContext) {
        crate::config::init().unwrap();

        // 初始化依赖的 DAO
        crate::service::dao::tool::init();
        crate::service::dao::tool_call::init();
        crate::service::dao::cortex::init();
        crate::service::dao::attachment::init();
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
        crate::service::dal::attachment::init();

        // 创建 Domain
        let domain = finance::new(
            crate::service::dal::model_provider::dal(),
            crate::service::dal::message_channel::dal(),
            crate::service::dal::tool::dal(),
            crate::service::dal::brain::dal(),
            crate::service::dal::attachment::dal(),
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

        let tool = http_management_tool("domain-delete-http-tool");
        let tool_id = tool.po.id.clone();
        domain
            .tool_provider_manage()
            .create_tool(ctx.clone(), &tool)
            .await
            .unwrap();

        let stored = domain
            .tool_provider_manage()
            .get_tool(ctx.clone(), &tool_id)
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
            .get_tool(ctx.clone(), &tool_id)
            .await
            .unwrap();
        assert!(got.is_none(), "deleted tool should not be found");
    }

    #[sqlx::test]
    async fn test_create_http_tool_rejects_auto_control_mode(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;
        let mut tool = http_management_tool("domain-http-auto-rejected");
        tool.po.control_mode = ControlMode::Auto;

        let result = domain
            .tool_provider_manage()
            .create_tool(ctx.clone(), &tool)
            .await;

        assert!(result.is_err(), "HTTP tools should be manual-only");
        let error = result.unwrap_err().to_string();
        assert!(error.contains("HTTP Tool"));
        assert!(error.contains("Manual"));
    }

    #[sqlx::test]
    async fn test_update_http_tool_rejects_auto_control_mode(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;
        let tool = http_management_tool("domain-http-update-auto-rejected");
        let tool_id = tool.po.id.clone();
        domain
            .tool_provider_manage()
            .create_tool(ctx.clone(), &tool)
            .await
            .unwrap();

        let mut stored = domain
            .tool_provider_manage()
            .get_tool(ctx.clone(), &tool_id)
            .await
            .unwrap()
            .expect("created tool should be readable for management");
        stored.po.control_mode = ControlMode::Auto;

        let result = domain
            .tool_provider_manage()
            .update_tool(ctx.clone(), &stored)
            .await;

        assert!(result.is_err(), "HTTP tools should reject Auto on update");
        let error = result.unwrap_err().to_string();
        assert!(error.contains("HTTP Tool"));
        assert!(error.contains("Manual"));
    }

    #[sqlx::test]
    async fn test_create_http_tool_rejects_invalid_config_before_persist(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;
        let mut tool = http_management_tool("domain-http-invalid-config");
        tool.po.config = json!({
            "method": "PUT",
            "url": "https://api.example.com/search"
        });
        let tool_id = tool.po.id.clone();

        let result = domain
            .tool_provider_manage()
            .create_tool(ctx.clone(), &tool)
            .await;

        assert!(result.is_err(), "invalid HTTP config should be rejected");
        let stored = domain
            .tool_provider_manage()
            .get_tool(ctx.clone(), &tool_id)
            .await
            .unwrap();
        assert!(stored.is_none(), "invalid HTTP tool must not be persisted");
    }
}
