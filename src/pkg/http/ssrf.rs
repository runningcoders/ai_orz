//! HTTP 出站安全：SSRF 防护、响应大小限制、敏感头脱敏
//!
//! 原 `pkg/utils/http_security`，随 HTTP 基建统一迁入 `pkg/http`，
//! 与客户端构建内聚（安全校验 + 连接参数必须配套使用，拆在两处易漏配）。
//!
//! 包含：SSRF 防护（IP 黑名单 + DNS 解析校验）、响应大小限制、敏感头脱敏。
//! 超时相关常量见 [`super::client`]。

use anyhow::anyhow;
use common::error::Result;
use std::net::{IpAddr, SocketAddr};
use tokio::net::lookup_host;

/// 默认最大响应大小：1MB
pub const DEFAULT_RESPONSE_MAX_BYTES: usize = 1024 * 1024;
/// 硬最大响应大小：10MB
pub const HARD_RESPONSE_MAX_BYTES: usize = 10 * 1024 * 1024;

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
/// 返回 DNS pinning 用的 SocketAddr 列表，需配合
/// [`super::presets::ssrf_guarded`] 使用才构成完整防护。
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

/// 云元数据端点：任何情况下都不允许 Agent 浏览器访问
///
/// 这些地址没有合法的浏览器测试用途，是 SSRF 的头号目标。即使管理员
/// 开启了 `allow_local_network`（为测试本机 dev server），它们仍被拒绝。
const CLOUD_METADATA_HOSTS: &[&str] = &[
    "169.254.169.254", // AWS / GCP / Azure IMDS
    "169.254.170.2",   // AWS ECS / Fargate 凭证端点
    "100.100.100.200", // 阿里云 ECS 元数据
    "metadata.google.internal",
    "metadata.goog",
    "fd00:ec2::254", // AWS IMDS IPv6
];

/// Agent 浏览器 `open` URL 的**字面**检查（SSRF 纵深防御，非完整边界）
///
/// 只做无网络 I/O 的静态检查，明确不解析 DNS——浏览器随后会自行解析与
/// 跟随重定向，入口解析给出的"安全"结论对后续请求无效（DNS rebinding、
/// 302、JS fetch、点击跳转都绕过这里）。完整边界依赖 agent-browser 的
/// `--allowed-domains`（浏览器级 containment）与部署层 egress 隔离。
///
/// 拦截规则：
/// 1. scheme 仅 http / https。本地文件 `file://` 由 browser 工具层改用 fs 作用域
///    校验函数（`tool_security::fs::resolve_and_validate_path`）单独处理，不走这里
/// 2. 云元数据端点无条件拒绝，不受 `allow_local_network` 影响
/// 3. localhost / 字面内网地址：仅当 `allow_local_network == false` 时拒绝
///    （开启后供本机/内网页面测试使用；此为管理员工具级开关，Agent 不可覆盖）
pub fn validate_browser_open_url(raw: &str, allow_local_network: bool) -> Result<()> {
    let url = reqwest::Url::parse(raw)
        .map_err(|e| anyhow!("URL 格式无效：{e}（请使用 http:// 或 https:// 开头的绝对地址）"))?;
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(anyhow!("不允许的 URL 协议「{scheme}」：仅支持 http / https / file").into());
    }
    let host = normalize_domain(url.host_str().ok_or_else(|| anyhow!("URL 缺少 host"))?);
    if host.is_empty() {
        return Err(anyhow!("URL 缺少 host").into());
    }
    if CLOUD_METADATA_HOSTS.contains(&host.as_str()) {
        return Err(anyhow!(
            "禁止访问云元数据端点「{host}」：该地址是 SSRF 高风险目标，任何配置下都不允许"
        )
        .into());
    }
    if !allow_local_network && is_local_network_host(&host) {
        return Err(anyhow!(
            "禁止访问本地/内网地址「{host}」：如需用浏览器测试本机或内网页面，请在工具配置页开启 allow_local_network"
        )
        .into());
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_open_allows_public_http() {
        assert!(validate_browser_open_url("https://example.com/path?q=1", false).is_ok());
        assert!(validate_browser_open_url("http://example.com:8080/", false).is_ok());
    }

    #[test]
    fn browser_open_rejects_non_http_scheme() {
        for url in [
            "file:///etc/passwd",
            "javascript:alert(1)",
            "data:text/html,x",
        ] {
            assert!(
                validate_browser_open_url(url, true).is_err(),
                "must reject non-http scheme: {url}"
            );
        }
    }

    #[test]
    fn browser_open_rejects_cloud_metadata_even_when_local_allowed() {
        for host in [
            "169.254.169.254",
            "169.254.170.2",
            "100.100.100.200",
            "metadata.google.internal",
        ] {
            assert!(
                validate_browser_open_url(&format!("http://{host}/latest/meta-data/"), true)
                    .is_err(),
                "metadata endpoint must always be blocked: {host}"
            );
        }
    }

    #[test]
    fn browser_open_local_network_respects_switch() {
        for host in ["localhost", "127.0.0.1", "192.168.1.10", "10.0.0.5"] {
            let url = format!("http://{host}:3000/");
            assert!(
                validate_browser_open_url(&url, false).is_err(),
                "local target must be blocked by default: {url}"
            );
            assert!(
                validate_browser_open_url(&url, true).is_ok(),
                "local target must pass when explicitly allowed: {url}"
            );
        }
    }
}
