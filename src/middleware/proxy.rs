//! 反向代理（网关）场景的请求头解析
//!
//! 部署在 Caddy / Nginx / Traefik 之后时，TCP 对端恒为网关，必须靠转发头还原
//! 客户端真实 IP 与协议。安全前提：**只有 `server.trust_proxy = true` 才信任这些头**
//! —— 直连暴露场景下客户端可任意伪造 `X-Forwarded-*`，忽略即安全。
//!
//! 解析规则见 [`client_ip`] 的文档注释：多级代理链路由最外层网关负责重写该头。

use axum::http::HeaderMap;
use common::config::AppConfig;

/// 解析真实客户端 IP（仅在 `server.trust_proxy` 开启时返回 `X-Forwarded-For` 的值）
///
/// 取 `X-Forwarded-For` **最右**一项：网关（nginx `$proxy_add_x_forwarded_for` /
/// Caddy `reverse_proxy`）会把直连对端追加到逗号列表末尾，而左侧条目是客户端自报内容、
/// 不可信。若链路有 N 级代理，应由最外层网关重写该头为单一可信值。
pub fn client_ip(config: &AppConfig, headers: &HeaderMap) -> Option<String> {
    if !config.server.trust_proxy {
        return None;
    }
    headers
        .get(X_FORWARDED_FOR)?
        .to_str()
        .ok()?
        .rsplit(',')
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// 读取 `X-Forwarded-Proto`（网关终止 TLS 后的原始协议）
///
/// 与 [`client_ip`] 不同，此函数不做 trust_proxy 判断——是否采信由调用方
/// （[`common::config::ServerConfig::cookie_secure`]）决定，便于单测。
pub fn forwarded_proto(headers: &HeaderMap) -> Option<&str> {
    headers.get(X_FORWARDED_PROTO)?.to_str().ok()
}

const X_FORWARDED_FOR: &str = "x-forwarded-for";
const X_FORWARDED_PROTO: &str = "x-forwarded-proto";

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderName;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(HeaderName::from_static(k), v.parse().unwrap());
        }
        h
    }

    /// AppConfig 未实现 Default，用最小 TOML 构造（其余段走 serde default）
    fn config(trust_proxy: bool) -> AppConfig {
        toml::from_str(&format!("[server]\ntrust_proxy = {trust_proxy}\n")).unwrap()
    }

    #[test]
    fn client_ip_ignored_when_trust_proxy_off() {
        let h = headers(&[("x-forwarded-for", "1.2.3.4")]);
        assert_eq!(client_ip(&config(false), &h), None);
    }

    #[test]
    fn client_ip_takes_rightmost_entry() {
        let h = headers(&[("x-forwarded-for", "spoofed, 10.0.0.1, 203.0.113.7")]);
        assert_eq!(
            client_ip(&config(true), &h).as_deref(),
            Some("203.0.113.7"),
            "最右一项才是网关追加的直连对端，左侧条目可被客户端伪造"
        );
    }

    #[test]
    fn client_ip_handles_single_entry_and_empty() {
        assert_eq!(
            client_ip(
                &config(true),
                &headers(&[("x-forwarded-for", "198.51.100.2")])
            )
            .as_deref(),
            Some("198.51.100.2")
        );
        assert_eq!(
            client_ip(&config(true), &headers(&[("x-forwarded-for", "  ")])),
            None,
            "空白值不应记录为客户端 IP"
        );
        assert_eq!(client_ip(&config(true), &HeaderMap::new()), None);
    }

    #[test]
    fn forwarded_proto_reads_header() {
        let h = headers(&[("x-forwarded-proto", "https")]);
        assert_eq!(forwarded_proto(&h), Some("https"));
        assert_eq!(forwarded_proto(&HeaderMap::new()), None);
    }
}
