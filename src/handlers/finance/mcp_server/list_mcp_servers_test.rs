use common::api::ListMcpServersRequest;
use sqlx::SqlitePool;
use std::sync::Once;

use crate::models::mcp_server::{McpServer, McpServerConfig, McpTransport};
use crate::pkg::tool_tracing::logger::ToolCallLogger;
use crate::service::domain::finance::domain;

use super::list_mcp_servers::list_mcp_servers;
use common::error::Result;

fn init_test_singletons() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let base_path = std::env::temp_dir().join(format!(
            "ai_orz_mcp_server_handler_tests_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&base_path)
            .expect("MCP server handler test trace base path should be created");
        ToolCallLogger::init(base_path);

        let _ = crate::config::init();
        crate::service::dao::init_all();
        crate::service::dal::init_all();
        crate::service::domain::init_all();
    });
}

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
            ..McpServerConfig::default_stdio()
        },
        Some(creator.to_string()),
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn list_mcp_servers_returns_total_matching_query_not_page_size(
    pool: SqlitePool,
) -> Result<()> {
    init_test_singletons();
    let ctx = crate::pkg::request_context_test_support::new_test_ctx("test-user", pool);

    for name in ["page-a", "page-b", "page-c"] {
        let server = stdio_server(name, "test-user");
        domain()
            .mcp_server_manage()
            .create_mcp_server(ctx.clone(), &server)
            .await?;
    }

    let response = list_mcp_servers(
        ctx,
        ListMcpServersRequest {
            transport: Some(common::enums::McpTransport::Stdio),
            pagination: common::api::PaginationParams {
                limit: Some(1),
                offset: Some(1),
            },
            ..Default::default()
        },
    )
    .await?;

    assert_eq!(response.servers.len(), 1);
    assert_eq!(response.total, 3);

    Ok(())
}
