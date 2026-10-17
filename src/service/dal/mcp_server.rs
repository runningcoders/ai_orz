//! MCP Server DAL.
//!
//! Business-facing MCP Server management layer. DAO remains responsible only for
//! persistence of `McpServerPo`; this DAL exposes the `McpServer` business entity
//! to upper layers and performs minimal configuration validation.

use crate::models::mcp_server::{
    McpServer, McpServerPo, McpServerQuery, McpServerStatus, McpTransport,
};
use crate::pkg::RequestContext;
use crate::pkg::http::is_local_network_host;
use crate::service::dao::mcp_server::McpServerDao;
use crate::service::dao::tool_call::{self, McpToolCallDao};
use async_trait::async_trait;
use common::api::PagedResult;
use common::error::{Result, bail_err};
use std::sync::{Arc, OnceLock};

static MCP_SERVER_DAL: OnceLock<Arc<dyn McpServerDal + Send + Sync>> = OnceLock::new();

/// Get global MCP Server DAL.
pub fn dal() -> Arc<dyn McpServerDal + Send + Sync> {
    MCP_SERVER_DAL.get().cloned().unwrap()
}

/// Initialize global MCP Server DAL using global DAO singletons.
pub fn init() {
    use crate::service::dao::mcp_server;
    let _ = MCP_SERVER_DAL.set(new(mcp_server::dao(), tool_call::mcp_dao()));
}

/// Create MCP Server DAL with explicit dependencies.
pub fn new(
    mcp_server_dao: Arc<dyn McpServerDao + Send + Sync>,
    mcp_tool_call_dao: Arc<dyn McpToolCallDao + Send + Sync>,
) -> Arc<dyn McpServerDal + Send + Sync> {
    Arc::new(McpServerDalImpl {
        mcp_server_dao,
        mcp_tool_call_dao,
    })
}

#[async_trait]
pub trait McpServerDal: Send + Sync {
    async fn create(&self, ctx: RequestContext, server: &McpServer) -> Result<()>;

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<McpServer>>;

    async fn query(
        &self,
        ctx: RequestContext,
        query: McpServerQuery,
    ) -> Result<PagedResult<McpServer>>;

    async fn update(&self, ctx: RequestContext, server: &McpServer) -> Result<()>;

    async fn set_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: McpServerStatus,
    ) -> Result<()>;

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;
}

pub struct McpServerDalImpl {
    mcp_server_dao: Arc<dyn McpServerDao + Send + Sync>,
    mcp_tool_call_dao: Arc<dyn McpToolCallDao + Send + Sync>,
}

#[async_trait]
impl McpServerDal for McpServerDalImpl {
    async fn create(&self, ctx: RequestContext, server: &McpServer) -> Result<()> {
        validate_mcp_server_po(&server.po)?;
        let mut po = server.po.clone();
        let now = common::constants::utils::current_timestamp_ms();
        let uid = ctx.caller_id();
        po.created_at = now;
        po.updated_at = now;
        po.created_by = uid.clone();
        po.updated_by = uid;
        self.mcp_server_dao.insert(ctx, &po).await
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<McpServer>> {
        Ok(self
            .mcp_server_dao
            .find_by_id(ctx, id)
            .await?
            .map(McpServer::from_po))
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: McpServerQuery,
    ) -> Result<PagedResult<McpServer>> {
        Ok(self
            .mcp_server_dao
            .query(ctx, query)
            .await?
            .map(McpServer::from_po))
    }

    async fn update(&self, ctx: RequestContext, server: &McpServer) -> Result<()> {
        validate_mcp_server_po(&server.po)?;
        let mut po = server.po.clone();
        po.touch(Some(ctx.uid()));
        self.mcp_server_dao.update(ctx, &po).await?;
        self.mcp_tool_call_dao.invalidate_mcp_server(&server.po.id);
        Ok(())
    }

    async fn set_status(
        &self,
        ctx: RequestContext,
        id: &str,
        status: McpServerStatus,
    ) -> Result<()> {
        self.mcp_server_dao.set_status(ctx, id, status).await?;
        self.mcp_tool_call_dao.invalidate_mcp_server(id);
        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        self.mcp_server_dao.delete(ctx, id).await?;
        self.mcp_tool_call_dao.invalidate_mcp_server(id);
        Ok(())
    }
}

fn validate_mcp_server_po(po: &McpServerPo) -> Result<()> {
    let config = po.config();
    match po.transport {
        McpTransport::Stdio => {
            if config
                .command
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                bail_err!(
                    InvalidRequest,
                    "MCP stdio server {} requires command",
                    po.id
                );
            }
        }
        McpTransport::StreamableHttp => {
            // 方案 A（AMan 拍板）：URL 校验 + SSRF 软防护替代硬拒绝。
            // DNS 解析级校验属 runtime pkg/http client 职责，管理面仅做静态软防护。
            let raw_url = config.url.as_deref().unwrap_or_default().trim();
            let Ok(url) = reqwest::Url::parse(raw_url) else {
                bail_err!(
                    InvalidRequest,
                    "MCP streamable_http endpoint URL is invalid: expected an absolute URL such as https://host/path"
                );
            };
            if url.scheme() != "https" {
                bail_err!(
                    InvalidRequest,
                    "MCP streamable_http endpoint only supports https URLs (got scheme \"{}\")",
                    url.scheme()
                );
            }
            let Some(host) = url.host_str() else {
                bail_err!(
                    InvalidRequest,
                    "MCP streamable_http endpoint URL requires a host (e.g. https://host/path)"
                );
            };
            if host.trim().is_empty() {
                bail_err!(
                    InvalidRequest,
                    "MCP streamable_http endpoint URL requires a non-empty host"
                );
            }
            if is_local_network_host(host) {
                bail_err!(
                    InvalidRequest,
                    "MCP streamable_http endpoint must not target localhost or private/internal network addresses"
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::mcp_server::McpServerConfig;

    /// 构造 StreamableHttp 形态的 `McpServerPo`（仅配置级，不涉持久化）。
    fn streamable_http_po(url: Option<&str>) -> McpServerPo {
        let config = McpServerConfig {
            url: url.map(str::to_string),
            ..McpServerConfig::default_streamable_http()
        };
        McpServerPo::new(
            "validate-http-server".to_string(),
            "validate-http-server".to_string(),
            McpTransport::StreamableHttp,
            config,
            Some("test-user".to_string()),
        )
    }

    /// 断言校验拒绝且错误码为 InvalidRequest，返回错误消息供文案断言。
    fn validate_message(po: &McpServerPo) -> String {
        let err = validate_mcp_server_po(po)
            .expect_err("validation must reject this streamable_http server");
        assert_eq!(err.code_enum(), common::error::ErrorCode::InvalidRequest);
        err.msg
    }

    #[test]
    fn validate_accepts_public_https_streamable_http_endpoint() {
        // query 串（可能携带凭据形态参数）不影响校验语义。
        let po = streamable_http_po(Some("https://mcp.amap.com/mcp?key=<AMAP_KEY>"));
        validate_mcp_server_po(&po).expect("public https endpoint should pass validation");
    }

    #[test]
    fn validate_rejects_non_https_scheme_without_echoing_url() {
        for url in ["http://mcp.amap.com/mcp", "ftp://mcp.example.com/mcp"] {
            let po = streamable_http_po(Some(url));
            let msg = validate_message(&po);
            assert!(msg.contains("https"), "msg should mention https: {msg}");
            // 错误消息纪律：不得回显完整 URL（query 可能携带凭据）。
            assert!(!msg.contains(url), "msg must not echo the URL: {msg}");
        }
    }

    #[test]
    fn validate_rejects_local_and_private_hosts_ssrf_guard() {
        for url in [
            "https://localhost/mcp",
            "https://127.0.0.1/mcp",
            "https://192.168.1.1/mcp",
            "https://10.0.0.7/mcp",
            "https://[::1]/mcp",
        ] {
            let po = streamable_http_po(Some(url));
            let msg = validate_message(&po);
            assert!(
                msg.contains("local"),
                "msg should mention local network guard: {msg}"
            );
            // 错误消息纪律：不得回显完整 URL 或 host 原文。
            assert!(!msg.contains(url), "msg must not echo the URL: {msg}");
        }
    }

    #[test]
    fn validate_rejects_unparseable_or_hostless_url() {
        for url in [None, Some(""), Some("not a url")] {
            let po = streamable_http_po(url);
            let msg = validate_message(&po);
            assert!(
                msg.contains("invalid"),
                "msg should describe the URL problem: {msg}"
            );
            if let Some(url) = url
                && !url.is_empty()
            {
                assert!(!msg.contains(url), "msg must not echo the URL: {msg}");
            }
        }
        // host 缺失（EmptyHost）在 URL 解析层即被拒绝：https 特殊 scheme 强制 host 非空。
        let po = streamable_http_po(Some("https://"));
        let msg = validate_message(&po);
        assert!(
            msg.contains("invalid"),
            "msg should describe the URL problem: {msg}"
        );
    }
}
