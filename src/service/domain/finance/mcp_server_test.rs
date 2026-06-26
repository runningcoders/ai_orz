//! MCP Server provider management tests.
//!
//! MCP Server is an external capability provider and belongs to Finance Domain.

#[cfg(test)]
mod tests {
    use crate::models::mcp_server::{
        McpServer, McpServerConfig, McpTransport, REDACTED_CONFIG_VALUE,
    };
    use crate::pkg::RequestContext;
    use crate::service::domain::finance;
    use sqlx::SqlitePool;
    use std::collections::BTreeMap;

    fn stdio_server(name: &str, creator: &str) -> McpServer {
        let mut env = BTreeMap::new();
        env.insert("MCP_API_TOKEN".to_string(), "test-token".to_string());
        let mut headers = BTreeMap::new();
        headers.insert("Authorization".to_string(), "Bearer test-token".to_string());

        McpServer::new(
            "".to_string(),
            name.to_string(),
            McpTransport::Stdio,
            McpServerConfig {
                command: Some("npx".to_string()),
                args: vec![
                    "-y".to_string(),
                    "@modelcontextprotocol/server-memory".to_string(),
                ],
                env,
                url: Some("https://example.com/mcp?api_key=test-token#frag".to_string()),
                headers,
                ..McpServerConfig::default_stdio()
            },
            Some(creator.to_string()),
        )
    }

    async fn init_test_env(
        pool: SqlitePool,
    ) -> (std::sync::Arc<dyn finance::FinanceDomain>, RequestContext) {
        crate::config::init().unwrap();

        crate::service::dao::tool::init();
        crate::service::dao::tool_call::init();
        crate::service::dao::cortex::init();
        crate::service::dao::attachment::init();
        crate::service::dao::model_provider::init();
        crate::service::dao::message_channel::init();
        crate::service::dao::mcp_server::init();
        crate::service::dao::lark::init();
        crate::service::dao::wechat::init();
        crate::service::dao::slack::init();
        crate::service::dao::email::init();
        crate::service::dao::webhook::init();

        crate::service::dal::tool::init();
        crate::service::dal::model_provider::init();
        crate::service::dal::message_channel::init();
        crate::service::dal::mcp_server::init();
        crate::service::dal::mcp_tool::init();
        crate::service::dal::brain::init();
        crate::service::dal::attachment::init();

        let domain = finance::new(
            crate::service::dal::model_provider::dal(),
            crate::service::dal::message_channel::dal(),
            crate::service::dal::mcp_server::dal(),
            crate::service::dal::mcp_tool::dal(),
            crate::service::dal::tool::dal(),
            crate::service::dal::brain::dal(),
            crate::service::dal::attachment::dal(),
        );

        let ctx = RequestContext::new_simple("test-user", pool);
        (domain, ctx)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn finance_domain_creates_and_gets_mcp_server(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;
        let server = stdio_server("domain-memory", "test-user");
        let server_id = server.po.id.clone();

        domain
            .mcp_server_manage()
            .create_mcp_server(ctx.clone(), &server)
            .await
            .expect("create MCP server should succeed through finance domain");

        let stored = domain
            .mcp_server_manage()
            .get_mcp_server(ctx.clone(), &server_id)
            .await
            .expect("get MCP server should succeed")
            .expect("created MCP server should exist");

        assert_eq!(stored.po.id, server_id);
        assert_eq!(stored.po.name, "domain-memory");
        assert_eq!(stored.po.transport, McpTransport::Stdio);
        let stored_config = stored.po.config();
        assert_eq!(stored_config.command, Some("npx".to_string()));
        assert_eq!(
            stored_config.env.get("MCP_API_TOKEN"),
            Some(&REDACTED_CONFIG_VALUE.to_string())
        );
        assert_eq!(
            stored_config.headers.get("Authorization"),
            Some(&REDACTED_CONFIG_VALUE.to_string())
        );
        assert_eq!(
            stored_config.url,
            Some(format!(
                "https://example.com/mcp?{}#frag",
                REDACTED_CONFIG_VALUE
            ))
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn finance_domain_update_preserves_redacted_secret_placeholders(pool: SqlitePool) {
        let (domain, ctx) = init_test_env(pool).await;
        let server = stdio_server("domain-memory", "test-user");
        let server_id = server.po.id.clone();

        domain
            .mcp_server_manage()
            .create_mcp_server(ctx.clone(), &server)
            .await
            .expect("create MCP server should succeed through finance domain");

        let mut redacted = domain
            .mcp_server_manage()
            .get_mcp_server(ctx.clone(), &server_id)
            .await
            .expect("get MCP server should succeed")
            .expect("created MCP server should exist");
        redacted.po.name = "domain-memory-updated".to_string();

        domain
            .mcp_server_manage()
            .update_mcp_server(ctx.clone(), &redacted)
            .await
            .expect("round-trip update with redacted placeholders should preserve secrets");

        let persisted = crate::service::dal::mcp_server::dal()
            .find_by_id(ctx.clone(), &server_id)
            .await
            .expect("dal get should succeed")
            .expect("server should still exist");
        let persisted_config = persisted.po.config();
        assert_eq!(
            persisted_config.env.get("MCP_API_TOKEN"),
            Some(&"test-token".to_string())
        );
        assert_eq!(
            persisted_config.headers.get("Authorization"),
            Some(&"Bearer test-token".to_string())
        );
        assert_eq!(
            persisted_config.url,
            Some("https://example.com/mcp?api_key=test-token#frag".to_string())
        );
    }
}
