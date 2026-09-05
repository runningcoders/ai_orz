//! 远程内容抓取公共函数
//!
//! 封装 HTTPS 抓取 + SSRF 防护 + 大小限制 + 超时控制，
//! 供 `http_fetch` 工具、Seed 预置技能导入、Domain `apply_content_sources` 共享复用。
//!
//! 三方调用方只需提供 URL + 可选配置，无需各自实现安全校验逻辑。

use crate::pkg::http::ssrf::{
    DEFAULT_RESPONSE_MAX_BYTES, HARD_RESPONSE_MAX_BYTES, read_limited_response_body,
    sanitize_response_headers, validate_target_url,
};
use crate::pkg::http::{DEFAULT_TIMEOUT_MS, MAX_TIMEOUT_MS, presets};
use anyhow::anyhow;
use common::error::Result;

/// 远程抓取配置
#[derive(Debug, Clone)]
pub struct FetchOptions {
    /// 请求超时（毫秒），默认 30s，硬上限 10 分钟
    pub timeout_ms: u64,
    /// 最大响应大小（字节），默认 1MB，硬上限 10MB
    pub max_bytes: usize,
    /// 是否允许访问本地网络（SSRF 防护开关），默认 false
    pub allow_local_network: bool,
    /// 允许的域名白名单（None = 不限制）
    pub allowed_domains: Option<Vec<String>>,
    /// 屏蔽的域名黑名单
    pub blocked_domains: Option<Vec<String>>,
    /// 最大重定向次数，0 = 不跟随重定向，默认 5
    pub max_redirects: usize,
    /// 是否禁用代理，默认 true（工具场景安全优先）
    pub no_proxy: bool,
    /// 是否强制 HTTPS（拒绝 HTTP），默认 true
    pub https_only: bool,
}

impl Default for FetchOptions {
    fn default() -> Self {
        Self {
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_bytes: DEFAULT_RESPONSE_MAX_BYTES,
            allow_local_network: false,
            allowed_domains: None,
            blocked_domains: None,
            max_redirects: 5,
            no_proxy: true,
            https_only: true,
        }
    }
}

impl FetchOptions {
    /// 硬上限钳制
    fn clamped(&self) -> Self {
        Self {
            timeout_ms: self.timeout_ms.min(MAX_TIMEOUT_MS),
            max_bytes: self.max_bytes.min(HARD_RESPONSE_MAX_BYTES),
            ..self.clone()
        }
    }
}

/// 远程内容抓取结果
#[derive(Debug)]
pub struct FetchResult {
    /// 响应体 bytes
    pub bytes: Vec<u8>,
    /// Content-Type
    pub content_type: Option<String>,
    /// HTTP 状态码
    pub status: u16,
    /// 响应头（脱敏后）
    pub headers: serde_json::Value,
}

/// 抓取远程 HTTPS 内容
///
/// 核心公共函数：HTTPS 强制 + SSRF 防护（DNS pinning）+ 超时 + 大小限制。
///
/// `http_fetch` 工具、Seed 导入、Domain `apply_content_sources` 均委托此函数。
pub async fn fetch_remote_content(url: &str, options: &FetchOptions) -> Result<FetchResult> {
    let opts = options.clamped();

    // 1. 解析 URL
    let parsed_url: reqwest::Url = url.parse().map_err(|_| anyhow!("invalid URL: {}", url))?;

    // 1b. HTTPS 强制校验
    if opts.https_only && parsed_url.scheme() != "https" {
        return Err(anyhow!(
            "only HTTPS URLs are allowed for security reasons, got '{}'",
            parsed_url.scheme()
        )
        .into());
    }

    // 2. SSRF 校验 + DNS 解析
    let pinned_addresses = validate_target_url(
        Some(opts.allow_local_network),
        opts.allowed_domains.as_ref(),
        opts.blocked_domains.as_ref(),
        &parsed_url,
    )
    .await?;

    // 3. 构建带 DNS pinning 的 client
    let host = parsed_url
        .host_str()
        .ok_or_else(|| anyhow!("URL host is required"))?;

    // SSRF 预设：DNS pinning + 重定向策略 + 默认禁代理
    let mut client_options = presets::ssrf_guarded(
        host,
        pinned_addresses,
        std::time::Duration::from_millis(opts.timeout_ms),
    )
    .redirect(presets::redirect_policy(opts.max_redirects));

    if !opts.no_proxy {
        client_options = client_options.use_proxy();
    }

    let client = client_options.build()?;

    // 4. 发送请求
    let mut response = client
        .get(parsed_url)
        .send()
        .await
        .map_err(|e| anyhow!("HTTP request failed for {}: {}", url, e))?;

    let status = response.status().as_u16();
    let headers = sanitize_response_headers(response.headers());
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // 5. 读取响应体（带大小限制）
    let bytes = read_limited_response_body(&mut response, opts.max_bytes).await?;

    Ok(FetchResult {
        bytes,
        content_type,
        status,
        headers,
    })
}

/// 抓取远程内容并返回 UTF-8 字符串
///
/// 便捷封装：`fetch_remote_content` + `String::from_utf8_lossy`
pub async fn fetch_remote_text(url: &str, options: &FetchOptions) -> Result<String> {
    let result = fetch_remote_content(url, options).await?;
    Ok(String::from_utf8_lossy(&result.bytes).into_owned())
}
