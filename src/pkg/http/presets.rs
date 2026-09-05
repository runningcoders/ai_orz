//! 出站 HTTP 客户端预设
//!
//! 业务侧常用组合的命名入口：业务层只声明「要哪种客户端」，
//! 不再感知 `reqwest` 的具体配置细节。需要更细控制时用
//! [`HttpClientOptions`] 的 builder 方法叠加。

use super::client::{HttpClientOptions, MAX_TIMEOUT, MAX_TIMEOUT_MS, RedirectPolicy};
use common::error::{Error, Result};
use std::net::SocketAddr;
use std::time::Duration;

/// LLM 推理调用超时（毫秒）：120s
///
/// 推理响应普遍较慢（长上下文、工具调用循环），显著长于一般出站调用。
pub const LLM_TIMEOUT_MS: u64 = 120_000;
/// LLM 推理调用超时
pub const LLM_TIMEOUT: Duration = Duration::from_millis(LLM_TIMEOUT_MS);

/// 联邦出站调用超时（毫秒）：30s
pub const FEDERATION_TIMEOUT_MS: u64 = 30_000;
/// 联邦出站调用超时
pub const FEDERATION_TIMEOUT: Duration = Duration::from_millis(FEDERATION_TIMEOUT_MS);

/// 一般出站调用：管理面 / webhook / 三方 API（30s）
///
/// 适用于超时不敏感的一次性请求；默认值即为此预设。
pub fn outbound() -> HttpClientOptions {
    HttpClientOptions::new()
}

/// LLM 推理调用（120s）
pub fn llm() -> HttpClientOptions {
    HttpClientOptions::new().timeout(LLM_TIMEOUT)
}

/// SSRF 防护出站：DNS pinning + 禁止重定向 + 禁用系统代理
///
/// 用于目标地址来自用户/工具配置的场景。三项缺一不可：
/// - **DNS pinning**：防 DNS rebinding（校验时与请求时解析到同一地址）
/// - **禁止重定向**：防 302 绕过地址校验跳转内网
/// - **禁用代理**：代理会绕过 DNS pinning
///
/// 确需走系统代理时用 [`HttpClientOptions::use_proxy`] 显式声明。
pub fn ssrf_guarded(
    host: impl Into<String>,
    pinned: Vec<SocketAddr>,
    timeout: Duration,
) -> HttpClientOptions {
    HttpClientOptions::new()
        .timeout(timeout)
        .no_proxy()
        .no_redirect()
        .resolve_to_addrs(host, pinned)
}

/// 按毫秒构造带校验的超时选项
///
/// 区间外（0 或 > [`MAX_TIMEOUT_MS`]）直接报错，用于配置驱动的超时必须显式合法的场景。
pub fn with_timeout_ms(timeout_ms: u64) -> Result<HttpClientOptions> {
    if timeout_ms == 0 || timeout_ms > MAX_TIMEOUT_MS {
        return Err(Error::bad_request(format!(
            "invalid http timeout_ms: {timeout_ms} (must be 1..={MAX_TIMEOUT_MS})"
        )));
    }
    Ok(HttpClientOptions::new().timeout_ms(timeout_ms))
}

/// 构造指定超时：`None` 或零值回落 [`DEFAULT_TIMEOUT`]，超过 [`MAX_TIMEOUT`] 截断
pub fn with_timeout(timeout: Option<Duration>) -> HttpClientOptions {
    match timeout {
        Some(timeout) if timeout.as_nanos() > 0 => {
            HttpClientOptions::new().timeout(timeout.min(MAX_TIMEOUT))
        }
        _ => HttpClientOptions::new(),
    }
}

/// 重定向策略便捷构造：`max` 为 0 表示禁止重定向
pub fn redirect_policy(max: usize) -> RedirectPolicy {
    if max == 0 {
        RedirectPolicy::None
    } else {
        RedirectPolicy::Limited(max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pkg::http::DEFAULT_TIMEOUT;

    #[test]
    fn llm_preset_is_longer_than_outbound() {
        assert!(llm().effective_timeout() > outbound().effective_timeout());
        assert_eq!(llm().effective_timeout(), LLM_TIMEOUT);
        assert_eq!(outbound().effective_timeout(), DEFAULT_TIMEOUT);
    }

    #[test]
    fn ssrf_preset_is_hardened() {
        let opts = ssrf_guarded("example.com", Vec::new(), Duration::from_secs(5));
        assert!(opts.no_proxy);
        assert_eq!(opts.redirect, RedirectPolicy::None);
        assert_eq!(opts.effective_timeout(), Duration::from_secs(5));
    }

    #[test]
    fn with_timeout_ms_rejects_out_of_range() {
        assert!(with_timeout_ms(0).is_err());
        assert!(with_timeout_ms(MAX_TIMEOUT_MS + 1).is_err());
        assert!(with_timeout_ms(1).is_ok());
    }

    #[test]
    fn redirect_policy_zero_means_none() {
        assert_eq!(redirect_policy(0), RedirectPolicy::None);
        assert_eq!(redirect_policy(3), RedirectPolicy::Limited(3));
    }
}
