//! Tool Provider 配置管理测试
//!
//! 工具提供商配置的 CRUD 测试，属于财务领域

#[cfg(test)]
mod tests {
    use crate::models::tool::Tool;
    use crate::service::domain::finance;
    use crate::service::dal::tool::ToolDal;
    use crate::service::dal::model_provider::ModelProviderDal;
    use crate::service::dal::message_channel::MessageChannelDal;
    use crate::service::dal::brain::BrainDal;
    use crate::pkg::RequestContext;
    use std::sync::Arc;

    /// 创建测试用的 Finance Domain
    fn create_test_domain() -> Arc<dyn finance::FinanceDomain> {
        // 使用测试环境初始化各 DAO
        let model_provider_dal = crate::service::dal::model_provider::new_for_test();
        let message_channel_dal = crate::service::dal::message_channel::new_for_test();
        let tool_dal = crate::service::dal::tool::new_for_test();
        let brain_dal = crate::service::dal::brain::new_for_test();

        finance::new(model_provider_dal, message_channel_dal, tool_dal, brain_dal)
    }

    fn create_test_ctx() -> RequestContext {
        RequestContext::default()
    }

    #[tokio::test]
    async fn test_list_tools() {
        let domain = create_test_domain();
        let ctx = create_test_ctx();

        let result = domain.tool_provider_manage().list_tools(ctx).await;
        assert!(result.is_ok(), "list_tools should succeed");
    }

    #[tokio::test]
    async fn test_get_tool_not_found() {
        let domain = create_test_domain();
        let ctx = create_test_ctx();

        let result = domain.tool_provider_manage().get_tool(ctx, "non-existent-id").await;
        assert!(result.is_ok(), "get_tool should succeed even if not found");
        assert!(result.unwrap().is_none(), "should return None for non-existent tool");
    }

    #[tokio::test]
    async fn test_query_tools() {
        let domain = create_test_domain();
        let ctx = create_test_ctx();

        let query = crate::service::dao::tool::ToolQuery::default();
        let result = domain.tool_provider_manage().query_tools(ctx, query).await;
        assert!(result.is_ok(), "query_tools should succeed");
    }
}
