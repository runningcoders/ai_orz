//! HTTP 安全工具函数
//!
//! 从 `tool_registry::tool_security` 提取的公共 HTTP 安全逻辑，
//! 供 `tool_security`（再导出）、`utils::fetch_remote_content`、Domain 层共享。
//!
//! 包含：SSRF 防护（IP 黑名单 + DNS 解析校验）、响应大小限制、敏感头脱敏。

use anyhow::anyhow;
use common::error::Result;
use std::net::{IpAddr, SocketAddr};
use tokio::net::lookup_host;

/// 默认最大响应大小：1MB
pub const DEFAULT_RESPONSE_MAX_BYTES: usize = 1024 * 1024;
/// 硬最大响应大小：10MB
pub const HARD_RESPONSE_MAX_BYTES: usize = 10 * 1024 * 1024;
/// 默认请求超时：30s
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
/// 硬最大请求超时：10 分钟
pub const HARD_TIMEOUT_MS: u64 = 600_000;

/// 检查 host 是否为本地网络地址（SSRF 防护）
pub fn is_local_network_host(host: &str) -> bool {
    let normalized = normalize_domain(host);
    if normalized == "localhost" {
        return true;
    }

    normalized.parse::<IpAddr>().is_ok_and(is_local_network_ip)
}

/// 检查 IP 地址是否为本地网络地址（SSRF 防护）
pub fn is_local_network_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_unspecified()
                || octets[0] == 0
                || (octets[0] == 100 && (octets[1] & 0b1100_0000) == 64)
                || (octets[0] == 169 && octets[1] == 254)
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
                || (octets[0] == 192 && octets[1] == 168)
                || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
                || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
                || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
                || octets[0] >= 224
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_local_network_ip(IpAddr::V4(mapped));
            }

            let segments = ip.segments();
            let is_nat64_well_known = segments[0] == 0x0064 && segments[1] == 0xff9b;
            let is_teredo = segments[0] == 0x2001 && segments[1] == 0x0000;
            let is_6to4 = segments[0] == 0x2002;

            ip.is_loopback()
                || ip.is_unspecified()
                || (segments[0] & 0xff00) == 0xff00
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] & 0xffc0) == 0xfec0
                || (segments[0] == 0x2001 && segments[1] == 0x0db8)
                || is_nat64_well_known
                || is_teredo
                || is_6to4
        }
    }
}

/// 规范化域名
pub fn normalize_domain(domain: &str) -> String {
    domain
        .trim()
        .trim_matches(['[', ']'])
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

/// 检查域名是否匹配配置模式
pub fn domain_matches(host: &str, configured_domain: &str) -> bool {
    let host = normalize_domain(host);
    let configured_domain = normalize_domain(configured_domain);
    if configured_domain.is_empty() {
        return false;
    }

    host == configured_domain || host.ends_with(&format!(".{}", configured_domain))
}

/// 校验目标 URL 并解析地址（SSRF 防护）
///
/// 返回 DNS pinning 用的 SocketAddr 列表。
pub async fn validate_target_url(
    allow_local_network: Option<bool>,
    allowed_domains: Option<&Vec<String>>,
    blocked_domains: Option<&Vec<String>>,
    url: &reqwest::Url,
) -> Result<Vec<SocketAddr>> {
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(anyhow!("unsupported url scheme: {}", scheme).into());
    }

    let host = normalize_domain(
        url.host_str()
            .ok_or_else(|| anyhow!("url host is required"))?,
    );

    if host.is_empty() {
        return Err(anyhow!("url host is required").into());
    }

    if let Some(blocked_domains) = blocked_domains
        && blocked_domains
            .iter()
            .any(|domain| domain_matches(&host, domain))
    {
        return Err(anyhow!("blocked http domain").into());
    }

    if is_local_network_host(&host) && allow_local_network != Some(true) {
        return Err(anyhow!("local network http target requires allow_local_network=true").into());
    }

    if let Some(allowed_domains) = allowed_domains
        && !allowed_domains
            .iter()
            .any(|domain| domain_matches(&host, domain))
    {
        return Err(anyhow!("http domain is not allowed").into());
    }

    let addresses =
        validate_resolved_addresses(&host, url.port_or_known_default(), allow_local_network)
            .await?;

    Ok(addresses)
}

/// 校验 DNS 解析后的 IP 地址（SSRF 防护）
async fn validate_resolved_addresses(
    host: &str,
    port: Option<u16>,
    allow_local_network: Option<bool>,
) -> Result<Vec<SocketAddr>> {
    let Some(port) = port else {
        return Ok(Vec::new());
    };

    let lookup_host_name = host.trim_matches(['[', ']']);
    let addresses: Vec<SocketAddr> = lookup_host((lookup_host_name, port))
        .await
        .map_err(|_| anyhow!("failed to resolve http target host"))
        .map_err(common::error::Error::from)?
        .collect();

    if allow_local_network != Some(true)
        && addresses
            .iter()
            .any(|address| is_local_network_ip(address.ip()))
    {
        return Err(anyhow!(
            "resolved local network http target requires allow_local_network=true"
        )
        .into());
    }

    Ok(addresses)
}

/// 检查 header 名称是否敏感
///
/// 委托 common 单点实现（`is_sensitive_credential_name`），与前端表单预检同源零漂移
pub fn is_sensitive_header(name: &str) -> bool {
    common::models::is_sensitive_credential_name(name)
}

/// 脱敏响应头
pub fn sanitize_response_headers(headers: &reqwest::header::HeaderMap) -> serde_json::Value {
    use serde_json::{Map, Value};
    let mut sanitized = Map::new();
    for (name, value) in headers {
        let key = name.as_str().to_string();
        let value = if is_sensitive_header(name.as_str()) {
            Value::String("[REDACTED]".to_string())
        } else {
            Value::String(value.to_str().unwrap_or("<non-utf8>").to_string())
        };
        sanitized.insert(key, value);
    }
    Value::Object(sanitized)
}

/// 读取响应体（带大小限制）
pub async fn read_limited_response_body(
    response: &mut reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    if let Some(content_length) = response.content_length()
        && content_length > max_bytes as u64
    {
        return Err(anyhow!(
            "http response too large: {} bytes exceeds limit {}",
            content_length,
            max_bytes
        )
        .into());
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow!("http response read failed"))?
    {
        if bytes.len() + chunk.len() > max_bytes {
            return Err(
                anyhow!("http response too large: exceeds limit {} bytes", max_bytes).into(),
            );
        }
        bytes.extend_from_slice(&chunk);
    }

    Ok(bytes)
}
