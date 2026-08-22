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

    fn stdio_server(name: &str, creator: &str) -> McpServer {
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
                url: Some("https://example.com/mcp?api_key=test-token#frag".to_string()),
                credential_requirements: vec![common::models::CredentialRequirement {
                    kind: common::models::CredentialKind::GenericToken,
                    platform: Some("linear".to_string()),
                    field: None,
                    enhancer: None,
                    binding: common::models::CredentialBinding::Env {
                        name: "LINEAR_API_TOKEN".to_string(),
                    },
                }],
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
        // a2a_callback dao：dal::message_channel 注入依赖
        crate::service::dao::a2a_callback::init();
        // user dao：dal::message_channel 注入飞书凭证引用解析依赖
        crate::service::dao::user::init();

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

        let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);
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
        // 凭据需求声明持久化往返（env/headers 已移除，D14）
        assert_eq!(stored_config.credential_requirements.len(), 1);
        assert_eq!(
            stored_config.credential_requirements[0].platform.as_deref(),
            Some("linear")
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
        // requirements 为非敏感声明：更新往返原样保留（无 REDACTED 占位符语义）
        assert_eq!(persisted_config.credential_requirements.len(), 1);
        assert_eq!(
            persisted_config.credential_requirements[0]
                .platform
                .as_deref(),
            Some("linear")
        );
        assert_eq!(
            persisted_config.url,
            Some("https://example.com/mcp?api_key=test-token#frag".to_string())
        );
    }
}
