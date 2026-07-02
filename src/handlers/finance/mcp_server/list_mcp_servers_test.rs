use common::api::ListMcpServersRequest;
use sqlx::SqlitePool;


use crate::models::mcp_server::{McpServer, McpServerConfig, McpTransport};
use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;

use super::list_mcp_servers::list_mcp_servers;
use common::error::Result;

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

fn init_test_singletons() {
    let _ = crate::config::init();
    crate::service::dao::init_all();
    crate::service::dal::init_all();
    crate::service::domain::init_all();
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
