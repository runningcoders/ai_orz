//! HTTP 客户端统一构建入口
//!
//! 提供 [`HttpClientOptions`] 声明式选项与 [`build_client`] 单一构建函数。
//! 业务层只声明诉求，不再手写 `reqwest::Client::builder()`。

use common::error::{Error, Result};
use std::net::SocketAddr;
use std::time::Duration;

/// 默认请求超时（毫秒）：30s
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
/// 硬上限请求超时（毫秒）：10 分钟
pub const MAX_TIMEOUT_MS: u64 = 600_000;
/// 默认请求超时
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(DEFAULT_TIMEOUT_MS);
/// 硬上限请求超时
pub const MAX_TIMEOUT: Duration = Duration::from_millis(MAX_TIMEOUT_MS);

/// 统一 User-Agent（`ai-orz/<version>`）
pub const USER_AGENT: &str = concat!("ai-orz/", env!("CARGO_PKG_VERSION"));

/// 重定向策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RedirectPolicy {
    /// reqwest 默认（最多 10 次）
    #[default]
    Default,
    /// 禁止重定向（SSRF 防护场景：重定向可绕过地址校验）
    None,
    /// 限制重定向次数
    Limited(usize),
}

/// 出站 HTTP 客户端选项
///
/// 所有字段都有默认值，且默认组合是安全的（30s 超时 + 统一 UA）。
#[derive(Debug, Clone, Default)]
pub struct HttpClientOptions {
    /// 请求超时；`None` 或零值时回落 [`DEFAULT_TIMEOUT`]
    pub timeout: Option<Duration>,
    /// 禁用系统代理（SSRF 防护场景：代理会绕过 DNS pinning）
    pub no_proxy: bool,
    /// 重定向策略
    pub redirect: RedirectPolicy,
    /// DNS pinning：将 host 固定解析到给定地址列表（防 DNS rebinding）
    pub resolve_to_addrs: Option<(String, Vec<SocketAddr>)>,
}

impl HttpClientOptions {
    /// 创建默认选项（30s 超时）
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置请求超时（零值按未指定处理）
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 设置请求超时（毫秒；零值按未指定处理）
    pub fn timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout = Some(Duration::from_millis(timeout_ms));
        self
    }

    /// 禁用系统代理
    pub fn no_proxy(mut self) -> Self {
        self.no_proxy = true;
        self
    }

    /// 显式启用系统代理（默认即启用，仅用于从 [`crate::pkg::http::presets::ssrf_guarded`]
    /// 回退到代理模式）
    ///
    /// 注意：走代理会绕过 DNS pinning，仅在明确信任代理且目标非内网时使用。
    pub fn use_proxy(mut self) -> Self {
        self.no_proxy = false;
        self
    }

    /// 设置重定向策略
    pub fn redirect(mut self, policy: RedirectPolicy) -> Self {
        self.redirect = policy;
        self
    }

    /// 禁止重定向
    pub fn no_redirect(mut self) -> Self {
        self.redirect = RedirectPolicy::None;
        self
    }

    /// DNS pinning：将 host 固定解析到给定地址列表
    pub fn resolve_to_addrs(mut self, host: impl Into<String>, addrs: Vec<SocketAddr>) -> Self {
        self.resolve_to_addrs = Some((host.into(), addrs));
        self
    }

    /// 有效超时：未指定或零值 → [`DEFAULT_TIMEOUT`]；超过 [`MAX_TIMEOUT`] → 截断
    ///
    /// 保证返回值恒在 `1ms..=MAX_TIMEOUT` 区间，杜绝「无超时客户端」。
    pub fn effective_timeout(&self) -> Duration {
        match self.timeout {
            Some(timeout) if timeout.as_nanos() > 0 => timeout.min(MAX_TIMEOUT),
            _ => DEFAULT_TIMEOUT,
        }
    }

    /// 构建客户端
    pub fn build(&self) -> Result<reqwest::Client> {
        build_client(self)
    }
}

/// 构建 HTTP 客户端（全项目唯一入口）
///
/// # 契约
///
/// - 超时恒为 [`HttpClientOptions::effective_timeout`]（永不无限）
/// - 恒定携带 [`USER_AGENT`]
/// - SSRF 相关的地址校验不在此处，见 [`crate::pkg::http::ssrf`]
pub fn build_client(opts: &HttpClientOptions) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(opts.effective_timeout())
        .user_agent(USER_AGENT);

    match opts.redirect {
        RedirectPolicy::Default => {}
        RedirectPolicy::None => {
            builder = builder.redirect(reqwest::redirect::Policy::none());
        }
        RedirectPolicy::Limited(max) => {
            builder = builder.redirect(reqwest::redirect::Policy::limited(max));
        }
    }

    if opts.no_proxy {
        builder = builder.no_proxy();
    }

    if let Some((host, addrs)) = &opts.resolve_to_addrs
        && !addrs.is_empty()
    {
        builder = builder.resolve_to_addrs(host.as_str(), addrs.as_slice());
    }

    builder
        .build()
        .map_err(|e| Error::internal(format!("构建 HTTP 客户端失败: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_options_always_have_timeout() {
        let opts = HttpClientOptions::new();
        assert_eq!(opts.effective_timeout(), DEFAULT_TIMEOUT);
    }

    #[test]
    fn zero_timeout_falls_back_to_default() {
        let opts = HttpClientOptions::new().timeout_ms(0);
        assert_eq!(opts.effective_timeout(), DEFAULT_TIMEOUT);
    }

    #[test]
    fn excessive_timeout_is_clamped() {
        let opts = HttpClientOptions::new().timeout(MAX_TIMEOUT + Duration::from_secs(1));
        assert_eq!(opts.effective_timeout(), MAX_TIMEOUT);
    }

    #[test]
    fn custom_timeout_is_preserved() {
        let opts = HttpClientOptions::new().timeout_ms(1_500);
        assert_eq!(opts.effective_timeout(), Duration::from_millis(1_500));
    }

    #[test]
    fn build_succeeds_with_defaults() {
        assert!(HttpClientOptions::new().build().is_ok());
    }

    #[test]
    fn build_succeeds_with_ssrf_shape() {
        let opts = HttpClientOptions::new()
            .no_proxy()
            .no_redirect()
            .resolve_to_addrs("example.com", Vec::new());
        assert!(opts.build().is_ok());
    }

    #[test]
    fn user_agent_is_versioned() {
        assert!(USER_AGENT.starts_with("ai-orz/"));
        assert!(USER_AGENT.len() > "ai-orz/".len());
    }
}
